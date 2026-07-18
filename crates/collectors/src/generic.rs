use alltokens_core::model::{Provider, UsageRecord};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use super::Collector;

/// Kimi CLI (Moonshot AI) 使用数据
/// 路径: ~/.kimi/sessions/ 或 ~/.config/kimi/
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct KimiSession {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    messages: Vec<KimiMessage>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct KimiMessage {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<KimiUsage>,
    #[serde(default)]
    timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct KimiUsage {
    #[serde(rename = "prompt_tokens", default)]
    prompt_tokens: Option<u64>,
    #[serde(rename = "completion_tokens", default)]
    completion_tokens: Option<u64>,
    #[serde(rename = "total_tokens", default)]
    total_tokens: Option<u64>,
}

pub struct KimiCollector {
    data_dirs: Vec<PathBuf>,
}

impl KimiCollector {
    pub fn new() -> Self {
        let mut dirs = Vec::new();
        if let Some(home) = dirs::home_dir() {
            let candidates = [
                home.join(".kimi"),
                home.join(".config").join("kimi"),
                home.join(".local").join("share").join("kimi"),
            ];
            for path in &candidates {
                if path.exists() { dirs.push(path.clone()); }
            }
        }
        Self { data_dirs: dirs }
    }
}

#[async_trait]
impl Collector for KimiCollector {
    fn id(&self) -> &str { "kimi" }
    fn name(&self) -> &str { "Kimi CLI" }
    fn is_available(&self) -> bool { !self.data_dirs.is_empty() }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        collect_session_json(&self.data_dirs, "kimi", "Kimi CLI", since)
    }
    fn watch_paths(&self) -> Vec<PathBuf> { self.data_dirs.clone() }
}

/// Qwen CLI (通义千问) 使用数据
pub struct QwenCollector {
    data_dirs: Vec<PathBuf>,
}

impl QwenCollector {
    pub fn new() -> Self {
        let mut dirs = Vec::new();
        if let Some(home) = dirs::home_dir() {
            let candidates = [
                home.join(".qwen"),
                home.join(".config").join("qwen"),
                home.join(".tongyi"),
            ];
            for path in &candidates {
                if path.exists() { dirs.push(path.clone()); }
            }
        }
        Self { data_dirs: dirs }
    }
}

#[async_trait]
impl Collector for QwenCollector {
    fn id(&self) -> &str { "qwen" }
    fn name(&self) -> &str { "Qwen CLI" }
    fn is_available(&self) -> bool { !self.data_dirs.is_empty() }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        collect_session_json(&self.data_dirs, "qwen", "Qwen CLI", since)
    }
    fn watch_paths(&self) -> Vec<PathBuf> { self.data_dirs.clone() }
}

/// Trae (字节跳动 AI IDE) 使用数据
pub struct TraeCollector {
    data_dirs: Vec<PathBuf>,
}

impl TraeCollector {
    pub fn new() -> Self {
        let mut dirs = Vec::new();
        if let Some(home) = dirs::home_dir() {
            let candidates = [
                home.join(".trae"),
                home.join(".config").join("trae"),
                home.join(".local").join("share").join("trae"),
                // macOS
                home.join("Library").join("Application Support").join("Trae"),
            ];
            for path in &candidates {
                if path.exists() { dirs.push(path.clone()); }
            }
        }
        Self { data_dirs: dirs }
    }
}

#[async_trait]
impl Collector for TraeCollector {
    fn id(&self) -> &str { "trae" }
    fn name(&self) -> &str { "Trae" }
    fn is_available(&self) -> bool { !self.data_dirs.is_empty() }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        collect_session_json(&self.data_dirs, "trae", "Trae", since)
    }
    fn watch_paths(&self) -> Vec<PathBuf> { self.data_dirs.clone() }
}

/// Qoder 使用数据
pub struct QoderCollector {
    data_dirs: Vec<PathBuf>,
}

impl QoderCollector {
    pub fn new() -> Self {
        let mut dirs = Vec::new();
        if let Some(home) = dirs::home_dir() {
            let candidates = [
                home.join(".qoder"),
                home.join(".config").join("qoder"),
            ];
            for path in &candidates {
                if path.exists() { dirs.push(path.clone()); }
            }
        }
        Self { data_dirs: dirs }
    }
}

#[async_trait]
impl Collector for QoderCollector {
    fn id(&self) -> &str { "qoder" }
    fn name(&self) -> &str { "Qoder" }
    fn is_available(&self) -> bool { !self.data_dirs.is_empty() }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        collect_session_json(&self.data_dirs, "qoder", "Qoder", since)
    }
    fn watch_paths(&self) -> Vec<PathBuf> { self.data_dirs.clone() }
}

/// Grok Build 使用数据
pub struct GrokBuildCollector {
    data_dirs: Vec<PathBuf>,
}

impl GrokBuildCollector {
    pub fn new() -> Self {
        let mut dirs = Vec::new();
        if let Some(home) = dirs::home_dir() {
            let candidates = [
                home.join(".grok"),
                home.join(".config").join("grok"),
                home.join("Library").join("Application Support").join("Grok"),
            ];
            for path in &candidates {
                if path.exists() { dirs.push(path.clone()); }
            }
        }
        Self { data_dirs: dirs }
    }
}

#[async_trait]
impl Collector for GrokBuildCollector {
    fn id(&self) -> &str { "grok" }
    fn name(&self) -> &str { "Grok Build" }
    fn is_available(&self) -> bool { !self.data_dirs.is_empty() }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        collect_session_json(&self.data_dirs, "grok", "Grok Build", since)
    }
    fn watch_paths(&self) -> Vec<PathBuf> { self.data_dirs.clone() }
}

/// Antigravity 使用数据
pub struct AntigravityCollector {
    data_dirs: Vec<PathBuf>,
}

impl AntigravityCollector {
    pub fn new() -> Self {
        let mut dirs = Vec::new();
        if let Some(home) = dirs::home_dir() {
            let candidates = [
                home.join(".antigravity"),
                home.join(".config").join("antigravity"),
            ];
            for path in &candidates {
                if path.exists() { dirs.push(path.clone()); }
            }
        }
        Self { data_dirs: dirs }
    }
}

#[async_trait]
impl Collector for AntigravityCollector {
    fn id(&self) -> &str { "antigravity" }
    fn name(&self) -> &str { "Antigravity" }
    fn is_available(&self) -> bool { !self.data_dirs.is_empty() }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        collect_session_json(&self.data_dirs, "antigravity", "Antigravity", since)
    }
    fn watch_paths(&self) -> Vec<PathBuf> { self.data_dirs.clone() }
}

/// Pi (Oh My Pi) 使用数据
pub struct PiCollector {
    data_dirs: Vec<PathBuf>,
}

impl PiCollector {
    pub fn new() -> Self {
        let mut dirs = Vec::new();
        if let Some(home) = dirs::home_dir() {
            let candidates = [
                home.join(".pi"),
                home.join(".config").join("pi"),
                home.join(".omp"),
            ];
            for path in &candidates {
                if path.exists() { dirs.push(path.clone()); }
            }
        }
        Self { data_dirs: dirs }
    }
}

#[async_trait]
impl Collector for PiCollector {
    fn id(&self) -> &str { "pi" }
    fn name(&self) -> &str { "Pi" }
    fn is_available(&self) -> bool { !self.data_dirs.is_empty() }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        collect_session_json(&self.data_dirs, "pi", "Pi", since)
    }
    fn watch_paths(&self) -> Vec<PathBuf> { self.data_dirs.clone() }
}

/// MiMo Code 使用数据
pub struct MiMoCodeCollector {
    data_dirs: Vec<PathBuf>,
}

impl MiMoCodeCollector {
    pub fn new() -> Self {
        let mut dirs = Vec::new();
        if let Some(home) = dirs::home_dir() {
            let candidates = [
                home.join(".mimo-code"),
                home.join(".local").join("share").join("mimocode"),
                home.join(".config").join("mimo-code"),
            ];
            for path in &candidates {
                if path.exists() { dirs.push(path.clone()); }
            }
        }
        Self { data_dirs: dirs }
    }
}

#[async_trait]
impl Collector for MiMoCodeCollector {
    fn id(&self) -> &str { "mimo_code" }
    fn name(&self) -> &str { "MiMo Code" }
    fn is_available(&self) -> bool { !self.data_dirs.is_empty() }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        collect_session_json(&self.data_dirs, "mimo_code", "MiMo Code", since)
    }
    fn watch_paths(&self) -> Vec<PathBuf> { self.data_dirs.clone() }
}

/// 通用 session JSON 解析器
/// 适用于 Kimi / Qwen / Trae / Qoder / Grok / Antigravity / Pi / MiMo Code
/// 格式: { messages: [{ model, usage, timestamp }] } 或 JSONL
pub fn collect_session_json(
    dirs: &[PathBuf],
    collector_id: &str,
    tool_name: &str,
    since: Option<DateTime<Utc>>,
) -> Result<Vec<UsageRecord>> {
    let mut records = Vec::new();
    for dir in dirs {
        for entry in walkdir::WalkDir::new(dir).max_depth(3).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "json" && ext != "jsonl" { continue; }
            let Ok(content) = std::fs::read_to_string(path) else { continue };

            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() { continue; }
                if let Some(record) = parse_generic_usage_line(line, path, collector_id, tool_name, since) {
                    records.push(record);
                }
            }
        }
    }
    records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    Ok(records)
}

/// 通用 usage JSON 行解析
fn parse_generic_usage_line(
    line: &str,
    path: &Path,
    collector_id: &str,
    tool_name: &str,
    since: Option<DateTime<Utc>>,
) -> Option<UsageRecord> {
    let val: serde_json::Value = serde_json::from_str(line).ok()?;
    let obj = val.as_object()?;

    // 尝试提取 usage 字段 (可能在顶层或嵌套在 message 中)
    let usage_obj = obj.get("usage").and_then(|v| v.as_object())
        .or_else(|| obj.get("message").and_then(|m| m.get("usage")).and_then(|v| v.as_object()));

    let input = usage_obj
        .and_then(|u| u.get("prompt_tokens").or_else(|| u.get("input_tokens")))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output = usage_obj
        .and_then(|u| u.get("completion_tokens").or_else(|| u.get("output_tokens")))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_read = usage_obj
        .and_then(|u| u.get("cached_tokens").or_else(|| u.get("cache_read_tokens")))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let total = usage_obj
        .and_then(|u| u.get("total_tokens"))
        .and_then(|v| v.as_u64());

    if input == 0 && output == 0 {
        return None;
    }

    let model = obj.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if model.is_empty() { return None; }

    let timestamp = obj.get("timestamp")
        .or_else(|| obj.get("created_at"))
        .or_else(|| obj.get("time"))
        .and_then(|v| v.as_str())
        .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    if let Some(since) = since { if timestamp <= since { return None; } }

    let provider = Provider::from_url_and_model("", &model);
    let cost = obj.get("cost").and_then(|v| v.as_f64()).unwrap_or(0.0);

    Some(UsageRecord {
        id: None, timestamp, collector: collector_id.to_string(), tool: Some(tool_name.to_string()),
        provider: provider.name().to_string(), model,
        input_tokens: input, output_tokens: output, reasoning_tokens: 0, cache_read_tokens: cache_read, cache_creation_tokens: 0,
        total_tokens: total.unwrap_or(input + output + cache_read),
        cost_usd: cost, cost_cny: 0.0, latency_ms: None, is_stream: false, status_code: None,
        session_id: obj.get("session_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
        request_id: obj.get("request_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
        source_file: Some(path.to_string_lossy().to_string()), raw_json: Some(line.to_string()), notes: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn parse_openai_style_usage_fields() {
        let line = r#"{"model":"gpt-4o","timestamp":"2026-07-10T12:00:00Z","usage":{"prompt_tokens":1500,"completion_tokens":800,"total_tokens":2300}}"#;
        let record = parse_generic_usage_line(line, Path::new("test.json"), "kimi", "Kimi CLI", None).unwrap();
        assert_eq!(record.input_tokens, 1500);
        assert_eq!(record.output_tokens, 800);
        assert_eq!(record.total_tokens, 2300);
        assert_eq!(record.model, "gpt-4o");
        assert_eq!(record.collector, "kimi");
    }

    #[test]
    fn parse_nested_message_usage() {
        let line = r#"{"model":"qwen-plus","message":{"usage":{"input_tokens":500,"output_tokens":200}},"timestamp":"2026-07-10T12:00:00Z"}"#;
        let record = parse_generic_usage_line(line, Path::new("session.jsonl"), "qwen", "Qwen CLI", None).unwrap();
        assert_eq!(record.input_tokens, 500);
        assert_eq!(record.output_tokens, 200);
        assert_eq!(record.model, "qwen-plus");
    }

    #[test]
    fn parse_cache_read_tokens() {
        let line = r#"{"model":"deepseek-chat","timestamp":"2026-07-10T12:00:00Z","usage":{"input_tokens":1000,"output_tokens":400,"cached_tokens":300}}"#;
        let record = parse_generic_usage_line(line, Path::new("log.json"), "trae", "Trae", None).unwrap();
        assert_eq!(record.cache_read_tokens, 300);
        assert_eq!(record.total_tokens, 1700);
    }

    #[test]
    fn skips_lines_without_token_usage() {
        let line = r#"{"model":"gpt-4o","timestamp":"2026-07-10T12:00:00Z","usage":{"prompt_tokens":0,"completion_tokens":0}}"#;
        assert!(parse_generic_usage_line(line, Path::new("empty.json"), "qoder", "Qoder", None).is_none());
    }

    #[test]
    fn respects_since_filter() {
        let line = r#"{"model":"gpt-4o","timestamp":"2026-07-09T12:00:00Z","usage":{"prompt_tokens":100,"completion_tokens":50}}"#;
        let since = DateTime::parse_from_rfc3339("2026-07-10T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert!(parse_generic_usage_line(line, Path::new("old.json"), "kimi", "Kimi CLI", Some(since)).is_none());
    }

    #[test]
    fn collect_kimi_fixture_dir() {
        let dir = tempfile::tempdir().unwrap();
        let session = dir.path().join("sessions").join("s1.jsonl");
        std::fs::create_dir_all(session.parent().unwrap()).unwrap();
        std::fs::write(
            &session,
            r#"{"model":"moonshot-v1-8k","timestamp":"2026-07-10T12:00:00Z","usage":{"prompt_tokens":1200,"completion_tokens":600,"total_tokens":1800}}
{"model":"moonshot-v1-32k","timestamp":"2026-07-10T13:00:00Z","usage":{"prompt_tokens":800,"completion_tokens":400}}"#,
        )
        .unwrap();

        let records = collect_session_json(&[dir.path().to_path_buf()], "kimi", "Kimi CLI", None).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].model, "moonshot-v1-8k");
        assert_eq!(records[0].input_tokens, 1200);
        assert_eq!(records[1].output_tokens, 400);
    }

    #[test]
    fn collect_qwen_fixture_dir() {
        let dir = tempfile::tempdir().unwrap();
        let session = dir.path().join("session.json");
        std::fs::write(
            &session,
            r#"{"model":"qwen-plus","timestamp":"2026-07-10T14:00:00Z","usage":{"input_tokens":2000,"output_tokens":900,"cached_tokens":150}}"#,
        )
        .unwrap();

        let records = collect_session_json(&[dir.path().to_path_buf()], "qwen", "Qwen CLI", None).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].cache_read_tokens, 150);
        assert_eq!(records[0].provider, "Qwen");
    }

    #[test]
    fn collect_trae_qoder_fixture_dirs() {
        let trae_dir = tempfile::tempdir().unwrap();
        std::fs::write(
            trae_dir.path().join("log.json"),
            r#"{"model":"deepseek-chat","timestamp":"2026-07-10T15:00:00Z","usage":{"prompt_tokens":500,"completion_tokens":250}}"#,
        )
        .unwrap();

        let qoder_dir = tempfile::tempdir().unwrap();
        std::fs::write(
            qoder_dir.path().join("usage.jsonl"),
            r#"{"model":"gpt-4o","message":{"usage":{"input_tokens":100,"output_tokens":50}},"timestamp":"2026-07-10T16:00:00Z"}"#,
        )
        .unwrap();

        let trae = collect_session_json(&[trae_dir.path().to_path_buf()], "trae", "Trae", None).unwrap();
        let qoder = collect_session_json(&[qoder_dir.path().to_path_buf()], "qoder", "Qoder", None).unwrap();
        assert_eq!(trae.len(), 1);
        assert_eq!(qoder.len(), 1);
        assert_eq!(trae[0].model, "deepseek-chat");
        assert_eq!(qoder[0].model, "gpt-4o");
    }
}
