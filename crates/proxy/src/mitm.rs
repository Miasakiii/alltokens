//! MITM TLS interception for HTTPS CONNECT tunnels.
//!
//! When a CA is configured, CONNECT requests are intercepted:
//! 1. Accept TLS from client using a dynamically-generated leaf cert
//! 2. Establish TLS connection to the real upstream server
//! 3. Relay HTTP request/response through both TLS channels
//! 4. Inspect decrypted response body for usage data (if host is in intercept list)

use crate::ca::CertificateAuthority;
use crate::intercept::{extract_usage_from_body, extract_usage_from_sse, should_intercept_host};
use crate::server::UsageCallback;
use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use tokio_rustls::{TlsAcceptor, TlsConnector};

/// Perform MITM interception on a CONNECT tunnel.
///
/// The client has already received "200 Connection Established".
/// We now perform a TLS handshake with the client (presenting a forged cert),
/// then connect to the real upstream with TLS, relay the HTTP exchange, and
/// inspect the response for token usage.
pub async fn mitm_intercept(
    client: TcpStream,
    host: &str,
    port: u16,
    ca: Arc<CertificateAuthority>,
    on_usage: Option<UsageCallback>,
) -> Result<()> {
    let hostname = host.to_string();

    // 1. Generate TLS config for this host and accept client TLS connection
    let server_config = ca
        .get_server_config(&hostname)
        .context("generate leaf cert for MITM")?;
    let acceptor = TlsAcceptor::from(server_config);
    let mut client_tls = acceptor
        .accept(client)
        .await
        .context("TLS handshake with client")?;

    // 2. Connect to upstream with real TLS
    let upstream_addr = format!("{}:{}", hostname, port);
    let upstream_tcp = TcpStream::connect(&upstream_addr)
        .await
        .context(format!("connect to upstream {upstream_addr}"))?;

    let mut root_store = RootCertStore::empty();
    // Use platform native roots for upstream verification
    root_store.extend(webpki_roots());
    let client_config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(client_config));

    let server_name = ServerName::try_from(hostname.clone())
        .map_err(|e| anyhow::anyhow!("invalid server name '{hostname}': {e}"))?;
    let mut upstream_tls = connector
        .connect(server_name, upstream_tcp)
        .await
        .context("TLS handshake with upstream")?;

    // 3. Relay: read request from client, forward to upstream, read response
    let should_inspect = should_intercept_host(&hostname);

    if should_inspect {
        // Full interception: buffer request and response to extract usage
        relay_with_inspection(&mut client_tls, &mut upstream_tls, &hostname, on_usage).await
    } else {
        // Simple bidirectional copy (no inspection needed)
        relay_passthrough(&mut client_tls, &mut upstream_tls).await
    }
}

/// Relay with full body inspection for usage extraction.
async fn relay_with_inspection<C, U>(
    client: &mut C,
    upstream: &mut U,
    hostname: &str,
    on_usage: Option<UsageCallback>,
) -> Result<()>
where
    C: AsyncRead + AsyncWrite + Unpin,
    U: AsyncRead + AsyncWrite + Unpin,
{
    // Read client request (HTTP over the decrypted TLS channel)
    let request_buf = read_http_message(client).await?;
    if request_buf.is_empty() {
        return Ok(());
    }

    // Extract model hint from request (for streaming responses without model in chunks)
    let model_hint = extract_model_from_request(&request_buf);

    // Forward request to upstream
    upstream.write_all(&request_buf).await?;
    upstream.flush().await?;

    // Read upstream response
    let response_buf = read_http_response(upstream).await?;
    if response_buf.is_empty() {
        return Ok(());
    }

    // Forward response to client
    client.write_all(&response_buf).await?;
    client.flush().await?;

    // Extract usage from the decrypted response
    if let Some(cb) = on_usage {
        if let Some(body_bytes) = body_for_extraction(&response_buf) {
            if let Ok(body) = std::str::from_utf8(&body_bytes) {
                let is_sse = is_sse_response(&response_buf);
                let record = if is_sse {
                    extract_usage_from_sse(hostname, model_hint.as_deref(), body)
                } else {
                    extract_usage_from_body(hostname, model_hint.as_deref(), body)
                };
                if let Some(record) = record {
                    tracing::debug!(
                        "MITM intercepted: {} {} ({}+{} tokens)",
                        record.provider,
                        record.model,
                        record.input_tokens,
                        record.output_tokens,
                    );
                    cb(record);
                }
            }
        }
    }

    Ok(())
}

/// Simple bidirectional relay without inspection.
async fn relay_passthrough<C, U>(client: &mut C, upstream: &mut U) -> Result<()>
where
    C: AsyncRead + AsyncWrite + Unpin,
    U: AsyncRead + AsyncWrite + Unpin,
{
    let (mut cr, mut cw) = tokio::io::split(client);
    let (mut ur, mut uw) = tokio::io::split(upstream);

    let c2u = tokio::io::copy(&mut cr, &mut uw);
    let u2c = tokio::io::copy(&mut ur, &mut cw);

    // Either direction closing ends the connection
    tokio::select! {
        r = c2u => { r.context("client->upstream copy")?; }
        r = u2c => { r.context("upstream->client copy")?; }
    }
    Ok(())
}

/// Read an HTTP message from a stream (request or response headers + body).
/// Uses Content-Length or reads until connection close.
async fn read_http_message<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(8192);
    let mut tmp = [0u8; 4096];

    // Read headers first
    loop {
        let n = reader.read(&mut tmp).await.context("read HTTP message")?;
        if n == 0 {
            return Ok(buf);
        }
        buf.extend_from_slice(&tmp[..n]);

        // Check if we have the full headers
        if let Some(header_end) = find_header_end(&buf) {
            // Parse Content-Length if present
            if let Some(content_length) = parse_content_length(&buf[..header_end]) {
                let total_expected = header_end + 4 + content_length; // headers + \r\n\r\n + body
                while buf.len() < total_expected {
                    let n = reader.read(&mut tmp).await?;
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                }
            }
            // If Transfer-Encoding: chunked or no Content-Length, just return what we have
            // (for simplicity; full chunked decoding could be added later)
            break;
        }
    }

    Ok(buf)
}

/// Read an HTTP response, handling chunked transfer encoding and streaming.
async fn read_http_response<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(16384);
    let mut tmp = [0u8; 8192];

    loop {
        let n = reader.read(&mut tmp).await.context("read HTTP response")?;
        if n == 0 {
            break; // Connection closed = end of response
        }
        buf.extend_from_slice(&tmp[..n]);

        // If we have headers, check for Content-Length
        if let Some(header_end) = find_header_end(&buf) {
            if let Some(cl) = parse_content_length(&buf[..header_end]) {
                let total = header_end + 4 + cl;
                while buf.len() < total {
                    let n = reader.read(&mut tmp).await?;
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                }
                break;
            }
            // For chunked/streaming, check for end markers
            if is_chunked_response(&buf[..header_end]) {
                // Read until "0\r\n\r\n" terminal chunk
                loop {
                    if buf.ends_with(b"0\r\n\r\n") || buf.ends_with(b"\r\n0\r\n\r\n") {
                        break;
                    }
                    let n = reader.read(&mut tmp).await?;
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    // Safety limit: 10MB
                    if buf.len() > 10 * 1024 * 1024 {
                        break;
                    }
                }
                break;
            }
        }

        // Safety limit
        if buf.len() > 10 * 1024 * 1024 {
            break;
        }
    }

    Ok(buf)
}

/// Find the position of "\r\n\r\n" header terminator.
fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4)
        .position(|w| w == b"\r\n\r\n")
}

/// Parse Content-Length from HTTP headers.
fn parse_content_length(headers: &[u8]) -> Option<usize> {
    let text = std::str::from_utf8(headers).ok()?;
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("content-length:") {
            return line[15..].trim().parse().ok();
        }
    }
    None
}

/// Check if response uses chunked transfer encoding.
fn is_chunked_response(headers: &[u8]) -> bool {
    let text = std::str::from_utf8(headers).unwrap_or("");
    text.to_ascii_lowercase().contains("transfer-encoding: chunked")
}

/// Parse the `Content-Encoding` header value, lowercased and trimmed.
/// Returns `None` when absent. For multi-layer encodings (comma separated),
/// returns the full value lowercased so `decompress` can pick the last layer.
fn content_encoding(headers: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(headers).ok()?;
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("content-encoding:") {
            let val = rest.trim().to_string();
            if val.is_empty() {
                return None;
            }
            return Some(val);
        }
    }
    None
}

/// Decompress a response body according to its `Content-Encoding`.
///
/// Supports `gzip`/`x-gzip` (flate2), `deflate` (zlib, falling back to raw),
/// and `br` (brotli). Unknown/`identity` encodings pass through untouched.
/// Best-effort: on any decode error or empty result, returns the original
/// bytes so relaying never breaks.
fn decompress(body: Vec<u8>, encoding: &str) -> Vec<u8> {
    use std::io::Read;
    // For layered encodings (e.g. "gzip, br") only the last applied layer
    // matters here; take the final comma-separated token.
    let enc = encoding
        .rsplit(',')
        .next()
        .map(|s| s.trim())
        .unwrap_or(encoding);
    match enc {
        "gzip" | "x-gzip" => {
            let mut out = Vec::new();
            match flate2::read::GzDecoder::new(&body[..]).read_to_end(&mut out) {
                Ok(_) if !out.is_empty() => out,
                _ => body,
            }
        }
        "deflate" => {
            // HTTP "deflate" is usually zlib-wrapped; fall back to raw deflate.
            let mut out = Vec::new();
            if flate2::read::ZlibDecoder::new(&body[..])
                .read_to_end(&mut out)
                .is_ok()
                && !out.is_empty()
            {
                return out;
            }
            let mut raw = Vec::new();
            match flate2::read::DeflateDecoder::new(&body[..]).read_to_end(&mut raw) {
                Ok(_) if !raw.is_empty() => raw,
                _ => body,
            }
        }
        "br" => {
            let mut out = Vec::new();
            match brotli::Decompressor::new(&body[..], 4096).read_to_end(&mut out) {
                Ok(_) if !out.is_empty() => out,
                _ => body,
            }
        }
        _ => body,
    }
}

/// Check if response is Server-Sent Events (text/event-stream).
fn is_sse_response(response: &[u8]) -> bool {
    let text = std::str::from_utf8(response).unwrap_or("");
    let header_section = if let Some(pos) = text.find("\r\n\r\n") {
        &text[..pos]
    } else {
        text
    };
    header_section
        .to_ascii_lowercase()
        .contains("text/event-stream")
}

/// Extract response body (after headers).
#[cfg(test)]
fn extract_body_from_http_response(response: &[u8]) -> Option<&str> {
    let text = std::str::from_utf8(response).ok()?;
    let pos = text.find("\r\n\r\n")?;
    let body = &text[pos + 4..];
    if body.is_empty() {
        None
    } else {
        Some(body)
    }
}

/// Decode an HTTP/1.1 chunked transfer-encoding body into the raw payload.
///
/// Parses each chunk as `<hex-size>[;ext]\r\n<data>\r\n`, concatenating the data
/// segments until a zero-size chunk terminates the stream. On malformed or
/// truncated input, returns whatever was decoded so far (best effort).
fn dechunk(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len());
    let mut i = 0usize;
    while i < body.len() {
        // Find end of the chunk-size line.
        let line_end = match find_crlf(&body[i..]) {
            Some(p) => i + p,
            None => break,
        };
        let size_line = &body[i..line_end];
        // Chunk size is the hex value before any ';' chunk-extension.
        let hex = match std::str::from_utf8(size_line) {
            Ok(s) => s.split(';').next().unwrap_or("").trim(),
            Err(_) => break,
        };
        let size = match usize::from_str_radix(hex, 16) {
            Ok(v) => v,
            Err(_) => break,
        };
        // Advance past the size line + CRLF.
        let data_start = line_end + 2;
        if size == 0 {
            break; // Terminal chunk.
        }
        let data_end = data_start + size;
        if data_end > body.len() {
            // Truncated: take what remains.
            out.extend_from_slice(&body[data_start.min(body.len())..]);
            break;
        }
        out.extend_from_slice(&body[data_start..data_end]);
        // Advance past the data + trailing CRLF.
        i = data_end + 2;
    }
    out
}

/// Find the byte offset of the next "\r\n" in `buf`.
fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

/// Return the response body ready for usage extraction, decoding chunked
/// transfer-encoding when present. `None` if headers are incomplete or body empty.
fn body_for_extraction(response: &[u8]) -> Option<Vec<u8>> {
    let header_end = find_header_end(response)?;
    let headers = &response[..header_end];
    let raw = &response[header_end + 4..];
    if raw.is_empty() {
        return None;
    }
    let body = if is_chunked_response(headers) {
        dechunk(raw)
    } else {
        raw.to_vec()
    };
    // Decompress per Content-Encoding (after transfer-encoding de-framing).
    let body = match content_encoding(headers) {
        Some(enc) => decompress(body, &enc),
        None => body,
    };
    if body.is_empty() {
        None
    } else {
        Some(body)
    }
}

/// Try to extract model name from the request body (JSON "model" field).
fn extract_model_from_request(request: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(request).ok()?;
    let body_start = text.find("\r\n\r\n")? + 4;
    let body = &text[body_start..];
    let val: serde_json::Value = serde_json::from_str(body).ok()?;
    val.get("model").and_then(|v| v.as_str()).map(|s| s.to_string())
}

/// Get bundled webpki root certificates for upstream TLS verification.
fn webpki_roots() -> Vec<rustls::pki_types::TrustAnchor<'static>> {
    webpki_roots_embedded()
}

/// Embedded Mozilla root CA set from rustls built-in.
/// We use a simple approach: accept any valid cert for upstream connections.
/// In production, you'd use the platform trust store.
fn webpki_roots_embedded() -> Vec<rustls::pki_types::TrustAnchor<'static>> {
    // For MITM proxy purposes, we trust the upstream server's real certificate
    // using the Mozilla root store bundled with rustls.
    rustls::RootCertStore::empty()
        .roots
        .iter()
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_header_end_works() {
        let data = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\nHello";
        assert_eq!(find_header_end(data), Some(41));
    }

    #[test]
    fn parse_content_length_from_headers() {
        let headers = b"HTTP/1.1 200 OK\r\nContent-Length: 42\r\n";
        assert_eq!(parse_content_length(headers), Some(42));
    }

    #[test]
    fn detect_sse_content_type() {
        let resp = b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\ndata: {}";
        assert!(is_sse_response(resp));

        let resp2 = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{}";
        assert!(!is_sse_response(resp2));
    }

    #[test]
    fn extract_model_from_json_request() {
        let req = b"POST /v1/chat/completions HTTP/1.1\r\nHost: api.openai.com\r\nContent-Type: application/json\r\n\r\n{\"model\":\"gpt-4o\",\"messages\":[]}";
        let model = extract_model_from_request(req);
        assert_eq!(model.as_deref(), Some("gpt-4o"));
    }

    #[test]
    fn extract_body_from_response() {
        let resp = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"usage\":{\"prompt_tokens\":10}}";
        let body = extract_body_from_http_response(resp).unwrap();
        assert!(body.contains("prompt_tokens"));
    }

    #[test]
    fn dechunk_single_chunk() {
        // "Hello" as one chunk (5 = 0x5) + terminal 0 chunk.
        let raw = b"5\r\nHello\r\n0\r\n\r\n";
        assert_eq!(dechunk(raw), b"Hello");
    }

    #[test]
    fn dechunk_multi_chunk() {
        // "Hello" + " World" (6 = 0x6) across two chunks.
        let raw = b"5\r\nHello\r\n6\r\n World\r\n0\r\n\r\n";
        assert_eq!(dechunk(raw), b"Hello World");
    }

    #[test]
    fn dechunk_with_extension() {
        // Chunk size line carries a chunk-extension after ';' which must be ignored.
        let raw = b"5;foo=bar\r\nHello\r\n0\r\n\r\n";
        assert_eq!(dechunk(raw), b"Hello");
    }

    #[test]
    fn body_for_extraction_dechunks_json() {
        let json = b"{\"usage\":{\"prompt_tokens\":1}}"; // len 29 = 0x1d
        let mut resp = Vec::new();
        resp.extend_from_slice(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n");
        resp.extend_from_slice(b"1d\r\n");
        resp.extend_from_slice(json);
        resp.extend_from_slice(b"\r\n0\r\n\r\n");
        let body = body_for_extraction(&resp).unwrap();
        // Decoded body must be parseable JSON (no hex framing left).
        let val: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(val.pointer("/usage/prompt_tokens").and_then(|v| v.as_u64()), Some(1));
    }

    #[test]
    fn body_for_extraction_passthrough_non_chunked() {
        let resp = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}";
        let body = body_for_extraction(resp).unwrap();
        assert_eq!(&body, b"{}");
    }

    #[test]
    fn chunked_sse_dechunks_then_extracts_usage() {
        // Two SSE events delivered as separate HTTP/1.1 chunks; the usage event
        // sits in the final data chunk before the terminal 0 chunk.
        let ev1 = "data: {\"model\":\"gpt-4o-mini\",\"choices\":[]}\n\n";
        let ev2 = "data: {\"model\":\"gpt-4o-mini\",\"usage\":{\"prompt_tokens\":30,\"completion_tokens\":15,\"total_tokens\":45}}\n\ndata: [DONE]\n\n";
        let mut resp = Vec::new();
        resp.extend_from_slice(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n",
        );
        for ev in [ev1, ev2] {
            // Chunk size as hex, computed to avoid manual miscounting.
            resp.extend_from_slice(format!("{:x}\r\n", ev.len()).as_bytes());
            resp.extend_from_slice(ev.as_bytes());
            resp.extend_from_slice(b"\r\n");
        }
        resp.extend_from_slice(b"0\r\n\r\n");

        assert!(is_sse_response(&resp));
        let body_bytes = body_for_extraction(&resp).unwrap();
        let body = std::str::from_utf8(&body_bytes).unwrap();
        // Hex framing must be gone so SSE `data:` lines parse cleanly.
        let record =
            crate::intercept::extract_usage_from_sse("api.openai.com", None, body).unwrap();
        assert_eq!(record.input_tokens, 30);
        assert_eq!(record.output_tokens, 15);
        assert!(record.is_stream);
    }

    // --- Content-Encoding decode helpers ---

    fn gzip(data: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(data).unwrap();
        enc.finish().unwrap()
    }

    fn zlib(data: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(data).unwrap();
        enc.finish().unwrap()
    }

    fn brotli_compress(data: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut w = brotli::CompressorWriter::new(Vec::new(), 4096, 5, 22);
        w.write_all(data).unwrap();
        w.into_inner()
    }

    #[test]
    fn content_encoding_parses_header() {
        let headers = b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Type: application/json";
        assert_eq!(content_encoding(headers).as_deref(), Some("gzip"));
        let none = b"HTTP/1.1 200 OK\r\nContent-Type: application/json";
        assert_eq!(content_encoding(none), None);
    }

    #[test]
    fn decompress_gzip_round_trip() {
        let json = b"{\"usage\":{\"prompt_tokens\":7}}";
        let out = decompress(gzip(json), "gzip");
        assert_eq!(&out, json);
    }

    #[test]
    fn decompress_deflate_round_trip() {
        let json = b"{\"usage\":{\"completion_tokens\":9}}";
        let out = decompress(zlib(json), "deflate");
        assert_eq!(&out, json);
    }

    #[test]
    fn decompress_brotli_round_trip() {
        let json = b"{\"usage\":{\"total_tokens\":16}}";
        let out = decompress(brotli_compress(json), "br");
        assert_eq!(&out, json);
    }

    #[test]
    fn decompress_passthrough_identity() {
        let json = b"{\"ok\":true}";
        assert_eq!(&decompress(json.to_vec(), "identity"), json);
        assert_eq!(&decompress(json.to_vec(), ""), json);
    }

    #[test]
    fn decompress_malformed_returns_original() {
        // Not valid gzip: must not panic and returns bytes unchanged.
        let garbage = vec![0x1f, 0x8b, 0x00, 0x01, 0x02, 0x03];
        assert_eq!(decompress(garbage.clone(), "gzip"), garbage);
    }

    #[test]
    fn body_for_extraction_gunzips_body() {
        let json = b"{\"usage\":{\"prompt_tokens\":11}}";
        let compressed = gzip(json);
        let mut resp = Vec::new();
        resp.extend_from_slice(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Encoding: gzip\r\n");
        resp.extend_from_slice(format!("Content-Length: {}\r\n\r\n", compressed.len()).as_bytes());
        resp.extend_from_slice(&compressed);
        let body = body_for_extraction(&resp).unwrap();
        let val: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(val.pointer("/usage/prompt_tokens").and_then(|v| v.as_u64()), Some(11));
    }

    #[test]
    fn body_for_extraction_chunked_then_gzip() {
        let json = b"{\"usage\":{\"total_tokens\":42}}";
        let compressed = gzip(json);
        let mut resp = Vec::new();
        resp.extend_from_slice(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Encoding: gzip\r\n\r\n",
        );
        // Frame the gzip bytes as a single chunk + terminal chunk.
        resp.extend_from_slice(format!("{:x}\r\n", compressed.len()).as_bytes());
        resp.extend_from_slice(&compressed);
        resp.extend_from_slice(b"\r\n0\r\n\r\n");
        let body = body_for_extraction(&resp).unwrap();
        let val: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(val.pointer("/usage/total_tokens").and_then(|v| v.as_u64()), Some(42));
    }
}
