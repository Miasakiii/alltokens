//! HTTP forward proxy with optional MITM TLS interception.
//!
//! When a `CertificateAuthority` is provided, CONNECT tunnels to known API hosts
//! are intercepted via MITM: the proxy terminates TLS with the client (using a
//! dynamically-generated leaf certificate) and opens a separate TLS connection to
//! the upstream, allowing full HTTP inspection of encrypted traffic.
//!
//! Without a CA, CONNECT tunnels are passed through transparently (no inspection).

use crate::ca::CertificateAuthority;
use crate::intercept::{extract_usage_from_body, should_intercept_host};
use crate::mitm::mitm_intercept;
use alltokens_core::model::UsageRecord;
use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

pub type UsageCallback = Arc<dyn Fn(UsageRecord) + Send + Sync>;

/// Handle one inbound client connection.
pub async fn handle_connection(
    mut client: TcpStream,
    on_usage: Option<UsageCallback>,
    ca: Option<Arc<CertificateAuthority>>,
) -> Result<()> {
    let mut buf = [0u8; 4096];
    let n = client.read(&mut buf).await.context("read request")?;
    if n == 0 {
        return Ok(());
    }
    let request = String::from_utf8_lossy(&buf[..n]);
    let first_line = request.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");

    if method.eq_ignore_ascii_case("CONNECT") {
        handle_connect(client, target, on_usage, ca).await
    } else if method == "GET" || method == "POST" {
        relay_http(client, &request, target, on_usage).await
    } else {
        let resp = "HTTP/1.1 405 Method Not Allowed\r\nConnection: close\r\n\r\n";
        client.write_all(resp.as_bytes()).await?;
        Ok(())
    }
}

/// Handle a CONNECT request: either MITM-intercept (if CA available and host is
/// in the intercept list) or tunnel passthrough.
async fn handle_connect(
    mut client: TcpStream,
    target: &str,
    on_usage: Option<UsageCallback>,
    ca: Option<Arc<CertificateAuthority>>,
) -> Result<()> {
    let (host, port) = parse_connect_target(target);

    // Decide: MITM or passthrough?
    let do_mitm = ca.is_some() && should_intercept_host(&host);

    if do_mitm {
        // Send 200 to client, then perform MITM interception
        client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await?;
        tracing::debug!("MITM intercepting CONNECT to {host}:{port}");
        mitm_intercept(client, &host, port, ca.unwrap(), on_usage).await
    } else {
        // Plain tunnel passthrough (no inspection)
        tunnel_passthrough(client, target).await
    }
}

/// Parse CONNECT target "host:port" into components.
fn parse_connect_target(target: &str) -> (String, u16) {
    if let Some((host, port_str)) = target.rsplit_once(':') {
        let port = port_str.parse().unwrap_or(443);
        (host.to_string(), port)
    } else {
        (target.to_string(), 443)
    }
}

/// Plain tunnel: connect to upstream and bidirectionally copy bytes.
async fn tunnel_passthrough(mut client: TcpStream, target: &str) -> Result<()> {
    let upstream = TcpStream::connect(target)
        .await
        .context(format!("connect to {target}"))?;
    client
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await?;

    let (mut cr, mut cw) = client.into_split();
    let (mut ur, mut uw) = upstream.into_split();

    let c2u = tokio::io::copy(&mut cr, &mut uw);
    let u2c = tokio::io::copy(&mut ur, &mut cw);
    tokio::try_join!(c2u, u2c)?;
    Ok(())
}

async fn relay_http(
    mut client: TcpStream,
    request: &str,
    target: &str,
    on_usage: Option<UsageCallback>,
) -> Result<()> {
    let host = parse_host_header(request).unwrap_or_else(|| target.to_string());
    let upstream_addr = if target.starts_with("http://") {
        target.trim_start_matches("http://").to_string()
    } else {
        host.clone()
    };

    let mut upstream = TcpStream::connect(&upstream_addr)
        .await
        .context(format!("connect to {upstream_addr}"))?;
    upstream.write_all(request.as_bytes()).await?;

    let mut response = Vec::new();
    upstream.read_to_end(&mut response).await?;
    client.write_all(&response).await?;

    if should_intercept_host(&host) {
        if let Some(body) = split_http_body(&response) {
            if let Some(body_str) = std::str::from_utf8(body).ok() {
                if let Some(record) = extract_usage_from_body(&host, None, body_str) {
                    if let Some(cb) = on_usage {
                        cb(record);
                    }
                }
            }
        }
    }

    Ok(())
}

fn parse_host_header(request: &str) -> Option<String> {
    for line in request.lines().skip(1) {
        if line.is_empty() {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("host:") {
            return Some(line[5..].trim().to_string());
        }
    }
    None
}

fn split_http_body(response: &[u8]) -> Option<&[u8]> {
    let text = std::str::from_utf8(response).ok()?;
    text.find("\r\n\r\n").map(|i| &response[i + 4..])
}

/// Run the proxy until the listener is closed.
pub async fn run_proxy(
    listen_addr: SocketAddr,
    on_usage: Option<UsageCallback>,
    ca: Option<Arc<CertificateAuthority>>,
) -> Result<()> {
    let listener = TcpListener::bind(listen_addr)
        .await
        .context(format!("bind {listen_addr}"))?;

    if ca.is_some() {
        tracing::info!("Proxy listening on {listen_addr} (MITM TLS enabled for known API hosts)");
    } else {
        tracing::info!("Proxy listening on {listen_addr} (forward-only, no MITM)");
    }

    loop {
        let (client, peer) = listener.accept().await?;
        let cb = on_usage.clone();
        let ca_clone = ca.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(client, cb, ca_clone).await {
                tracing::debug!("Connection from {peer} ended: {e}");
            }
        });
    }
}

/// Shared handle for proxy lifecycle (start/stop from CLI).
pub struct ProxyHandle {
    shutdown: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl ProxyHandle {
    pub fn new() -> Self {
        Self {
            shutdown: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn start(
        &self,
        listen_addr: SocketAddr,
        on_usage: Option<UsageCallback>,
        ca: Option<Arc<CertificateAuthority>>,
    ) -> Result<()> {
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        *self.shutdown.lock().await = Some(tx);

        let listener = TcpListener::bind(listen_addr)
            .await
            .context(format!("bind {listen_addr}"))?;
        tracing::info!("Proxy listening on {listen_addr}");

        loop {
            tokio::select! {
                _ = &mut rx => break,
                accept = listener.accept() => {
                    let (client, peer) = accept?;
                    let cb = on_usage.clone();
                    let ca_clone = ca.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(client, cb, ca_clone).await {
                            tracing::debug!("Connection from {peer} ended: {e}");
                        }
                    });
                }
            }
        }
        Ok(())
    }

    pub async fn stop(&self) {
        if let Some(tx) = self.shutdown.lock().await.take() {
            let _ = tx.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_host_from_request() {
        let req = "POST /v1/chat/completions HTTP/1.1\r\nHost: api.openai.com\r\n\r\n{}";
        assert_eq!(parse_host_header(req).as_deref(), Some("api.openai.com"));
    }

    #[test]
    fn split_body_from_http_response() {
        let resp = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"usage\":{}}";
        let body = split_http_body(resp).unwrap();
        assert_eq!(body, b"{\"usage\":{}}");
    }
}
