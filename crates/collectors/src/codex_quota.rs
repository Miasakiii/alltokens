//! Codex app-server quota collection via JSON-RPC `account/rateLimits/read`.
//!
//! Windows are classified by `windowDurationMins` (300 = 5h, 10080 = 7d), not by
//! primary/secondary slot order (codexU v1.0.4 pattern).

use alltokens_core::model::{
    CodexQuotaSnapshot, CodexQuotaWindow, CodexQuotaWindowKind,
};
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::time::timeout;

pub const WINDOW_5H_MINS: i64 = 300;
pub const WINDOW_7D_MINS: i64 = 7 * 24 * 60; // 10080

const INIT_TIMEOUT: Duration = Duration::from_secs(30);
const RPC_TIMEOUT: Duration = Duration::from_secs(15);
const SOURCE: &str = "codex app-server account/rateLimits/read";

/// Classifies rate-limit windows by duration instead of primary/secondary order.
pub struct CodexRateLimitNormalizer;

impl CodexRateLimitNormalizer {
    /// Parse a JSON-RPC or bare `account/rateLimits/read` payload into a snapshot.
    pub fn normalize(value: &Value) -> Option<CodexQuotaSnapshot> {
        let rate_limits = find_rate_limit_snapshot(value)?;
        let fetched_at = Utc::now();
        let plan_type = read_string(rate_limits, &["planType", "plan_type"]);
        let rate_limit_reached = read_string(
            rate_limits,
            &["rateLimitReachedType", "rate_limit_reached_type"],
        )
        .is_some();

        let mut five_hour = None;
        let mut seven_day = None;

        for window_value in collect_window_values(rate_limits) {
            let window = parse_window(&window_value)?;
            match window.kind {
                CodexQuotaWindowKind::FiveHour => {
                    five_hour = Some(merge_window(five_hour.take(), window));
                }
                CodexQuotaWindowKind::SevenDay => {
                    seven_day = Some(merge_window(seven_day.take(), window));
                }
                CodexQuotaWindowKind::Other => {}
            }
        }

        if five_hour.is_none() && seven_day.is_none() && !rate_limit_reached {
            return None;
        }

        Some(CodexQuotaSnapshot {
            fetched_at,
            source: SOURCE.to_string(),
            plan_type,
            five_hour,
            seven_day,
            rate_limit_reached,
        })
    }
}

fn merge_window(existing: Option<CodexQuotaWindow>, incoming: CodexQuotaWindow) -> CodexQuotaWindow {
    match existing {
        Some(prev) if prev.used_percent.is_some() => prev,
        _ => incoming,
    }
}

fn collect_window_values(snapshot: &Value) -> Vec<Value> {
    let mut windows = Vec::new();
    for key in ["primary", "secondary"] {
        if let Some(window) = snapshot.get(key).filter(|v| !v.is_null()) {
            windows.push(window.clone());
        }
    }
    windows
}

fn parse_window(value: &Value) -> Option<CodexQuotaWindow> {
    let window_duration_mins =
        read_i64(value, &["windowDurationMins", "window_duration_mins", "windowMinutes"]);
    let kind = classify_window(window_duration_mins);

    let used_percent = read_i64(value, &["usedPercent", "used_percent"])
        .map(|v| v.clamp(0, 100) as i32);
    let remaining_percent = used_percent.map(|used| (100 - used).clamp(0, 100));
    let resets_at = read_i64(value, &["resetsAt", "resets_at"]);

    if used_percent.is_none() && resets_at.is_none() && window_duration_mins.is_none() {
        return None;
    }

    Some(CodexQuotaWindow {
        kind,
        used_percent,
        remaining_percent,
        window_duration_mins,
        resets_at,
    })
}

fn classify_window(window_duration_mins: Option<i64>) -> CodexQuotaWindowKind {
    match window_duration_mins {
        Some(WINDOW_5H_MINS) => CodexQuotaWindowKind::FiveHour,
        Some(WINDOW_7D_MINS) => CodexQuotaWindowKind::SevenDay,
        Some(_) => CodexQuotaWindowKind::Other,
        None => CodexQuotaWindowKind::Other,
    }
}

fn find_rate_limit_snapshot(value: &Value) -> Option<&Value> {
    if looks_like_rate_limit_snapshot(value) {
        return Some(value);
    }

    for key in [
        "rateLimits",
        "rate_limits",
        "result",
        "data",
    ] {
        if let Some(nested) = value.get(key) {
            if let Some(found) = find_rate_limit_snapshot(nested) {
                return Some(found);
            }
        }
    }

    if let Some(map) = value.get("rateLimitsByLimitId").or(value.get("rate_limits_by_limit_id")) {
        if let Some(codex) = map.get("codex") {
            if let Some(found) = find_rate_limit_snapshot(codex) {
                return Some(found);
            }
        }
        if let Some((_, entry)) = map.as_object()?.iter().next() {
            if let Some(found) = find_rate_limit_snapshot(entry) {
                return Some(found);
            }
        }
    }

    None
}

fn looks_like_rate_limit_snapshot(value: &Value) -> bool {
    value.get("primary").is_some()
        || value.get("secondary").is_some()
        || value.get("limitId").is_some()
        || value.get("limit_id").is_some()
}

fn read_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn read_i64(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| value.get(*key)).and_then(|v| {
        v.as_i64()
            .or_else(|| v.as_f64().map(|f| f.round() as i64))
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })
}

/// JSON-RPC client for `codex app-server`.
pub struct CodexAppServerClient {
    command: PathBuf,
    args: Vec<String>,
}

impl CodexAppServerClient {
    pub fn new() -> Self {
        Self {
            command: PathBuf::from("codex"),
            args: vec!["app-server".to_string()],
        }
    }

    /// Override binary and args (for tests).
    #[doc(hidden)]
    pub fn with_command(command: PathBuf, args: Vec<String>) -> Self {
        Self { command, args }
    }

    pub async fn fetch_quota_snapshot(&self) -> Result<CodexQuotaSnapshot> {
        let response = self.read_rate_limits().await?;
        CodexRateLimitNormalizer::normalize(&response)
            .ok_or_else(|| anyhow!("no recognizable rate limit windows in app-server response"))
    }

    pub async fn read_rate_limits(&self) -> Result<Value> {
        let mut child = Command::new(&self.command)
            .args(&self.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("failed to spawn {}", self.command.display()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("codex app-server stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("codex app-server stdout unavailable"))?;

        let result = timeout(
            INIT_TIMEOUT,
            handshake_and_read_rate_limits(stdin, stdout),
        )
        .await
        .context("codex app-server timed out")??;

        let _ = child.kill().await;
        Ok(result)
    }
}

impl Default for CodexAppServerClient {
    fn default() -> Self {
        Self::new()
    }
}

async fn handshake_and_read_rate_limits(
    mut stdin: ChildStdin,
    stdout: ChildStdout,
) -> Result<Value> {
    let mut reader = BufReader::new(stdout);

    let init_params = serde_json::json!({
        "clientInfo": {
            "name": "alltokens",
            "title": "AllTokens",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "capabilities": {}
    });
    let init_response = rpc_request(&mut stdin, &mut reader, 1, "initialize", init_params).await?;
    if init_response.get("error").is_some() {
        return Err(anyhow!(
            "initialize failed: {}",
            init_response
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error")
        ));
    }

    rpc_notification(&mut stdin, "initialized").await?;

    let limits_response =
        rpc_request(&mut stdin, &mut reader, 2, "account/rateLimits/read", Value::Null).await?;
    if let Some(error) = limits_response.get("error") {
        return Err(anyhow!(
            "account/rateLimits/read failed: {}",
            error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error")
        ));
    }

    limits_response
        .get("result")
        .cloned()
        .ok_or_else(|| anyhow!("account/rateLimits/read returned no result"))
}

#[derive(Debug, Serialize)]
struct RpcRequest<'a> {
    jsonrpc: &'static str,
    id: u64,
    method: &'a str,
    params: Value,
}

#[derive(Debug, Serialize)]
struct RpcNotification<'a> {
    jsonrpc: &'static str,
    method: &'a str,
}

async fn rpc_request(
    stdin: &mut ChildStdin,
    reader: &mut BufReader<ChildStdout>,
    id: u64,
    method: &str,
    params: Value,
) -> Result<Value> {
    let request = RpcRequest {
        jsonrpc: "2.0",
        id,
        method,
        params,
    };
    write_line(stdin, &serde_json::to_string(&request)?).await?;

    loop {
        let line = read_line(reader).await?;
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = serde_json::from_str(&line)
            .with_context(|| format!("invalid JSON-RPC line: {line}"))?;
        if message.get("method").is_some() && message.get("id").is_none() {
            continue;
        }
        let response_id = message.get("id").and_then(|v| v.as_u64());
        if response_id == Some(id) {
            return Ok(message);
        }
    }
}

async fn rpc_notification(stdin: &mut ChildStdin, method: &str) -> Result<()> {
    let notification = RpcNotification {
        jsonrpc: "2.0",
        method,
    };
    write_line(stdin, &serde_json::to_string(&notification)?).await
}

async fn write_line(stdin: &mut ChildStdin, line: &str) -> Result<()> {
    timeout(RPC_TIMEOUT, async {
        stdin.write_all(line.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        Ok::<(), std::io::Error>(())
    })
    .await
    .context("write to codex app-server timed out")??;
    Ok(())
}

async fn read_line(reader: &mut BufReader<ChildStdout>) -> Result<String> {
    let mut line = String::new();
    timeout(RPC_TIMEOUT, reader.read_line(&mut line))
        .await
        .context("read from codex app-server timed out")??;
    Ok(line)
}

/// Fetch quota from the default `codex app-server` subprocess.
pub async fn fetch_codex_quota() -> Result<CodexQuotaSnapshot> {
    CodexAppServerClient::new().fetch_quota_snapshot().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture(name: &str) -> Value {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/codex")
            .join(name);
        let raw = fs::read_to_string(path).unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    #[test]
    fn normalizer_maps_standard_primary_secondary_order() {
        let value = fixture("rate_limits_standard.json");
        let snapshot = CodexRateLimitNormalizer::normalize(&value).unwrap();

        let five = snapshot.five_hour.unwrap();
        assert_eq!(five.used_percent, Some(25));
        assert_eq!(five.remaining_percent, Some(75));
        assert_eq!(five.window_duration_mins, Some(WINDOW_5H_MINS));

        let seven = snapshot.seven_day.unwrap();
        assert_eq!(seven.used_percent, Some(18));
        assert_eq!(seven.remaining_percent, Some(82));
        assert_eq!(seven.window_duration_mins, Some(WINDOW_7D_MINS));

        assert_eq!(snapshot.plan_type.as_deref(), Some("plus"));
        assert!(!snapshot.rate_limit_reached);
    }

    #[test]
    fn normalizer_classifies_by_duration_not_slot_order() {
        let value = fixture("rate_limits_swapped.json");
        let snapshot = CodexRateLimitNormalizer::normalize(&value).unwrap();

        let five = snapshot.five_hour.unwrap();
        assert_eq!(five.used_percent, Some(25));
        assert_eq!(five.remaining_percent, Some(75));

        let seven = snapshot.seven_day.unwrap();
        assert_eq!(seven.used_percent, Some(18));
        assert_eq!(seven.remaining_percent, Some(82));
    }

    #[test]
    fn normalizer_parses_json_rpc_result_wrapper() {
        let inner = fixture("rate_limits_standard.json");
        let wrapped = serde_json::json!({ "jsonrpc": "2.0", "id": 2, "result": inner });
        let snapshot = CodexRateLimitNormalizer::normalize(&wrapped).unwrap();
        assert!(snapshot.five_hour.is_some());
        assert!(snapshot.seven_day.is_some());
    }

    #[test]
    fn classify_window_constants() {
        assert_eq!(
            classify_window(Some(WINDOW_5H_MINS)),
            CodexQuotaWindowKind::FiveHour
        );
        assert_eq!(
            classify_window(Some(WINDOW_7D_MINS)),
            CodexQuotaWindowKind::SevenDay
        );
        assert_eq!(classify_window(Some(60)), CodexQuotaWindowKind::Other);
    }
}
