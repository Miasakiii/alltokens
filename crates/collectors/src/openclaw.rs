use alltokens_core::model::{Provider, UsageRecord};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use super::paths;
use super::Collector;

/// OpenClaw agent 日志格式
/// 路径 (含 WSL):
///   - ~/.openclaw/agents/*/logs/*.jsonl
///   - /mnt/c/Users/<user>/.openclaw/agents/...
#[derive(Debug, Deserialize)]
struct OpenClawLogEntry {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(rename = "input_tokens", default)]
    input_tokens: Option<u64>,
    #[serde(rename = "output_tokens", default)]
    output_tokens: Option<u64>,
    #[serde(rename = "cache_read_tokens", default)]
    cache_read_tokens: Option<u64>,
    #[serde(rename = "cache_creation_tokens", default)]
    cache_creation_tokens: Option<u64>,
    #[serde(rename = "total_tokens", default)]
    total_tokens: Option<u64>,
    #[serde(default)]
    cost: Option<f64>,
    #[serde(rename = "request_id", default)]
    request_id: Option<String>,
    #[serde(rename = "session_key", default)]
    session_key: Option<String>,
}

pub struct OpenClawCollector {
    data_dirs: Vec<PathBuf>,
}

impl OpenClawCollector {
    pub fn new() -> Self {
        let candidates = [".openclaw"];
        let mut data_dirs = Vec::new();
        for base in paths::home_dirs() {
            for candidate in &candidates {
                let dir = base.join(candidate);
                if dir.exists() {
                    data_dirs.push(dir);
                }
            }
        }
        Self { data_dirs }
    }
}

#[async_trait]
impl Collector for OpenClawCollector {
    fn id(&self) -> &str { "openclaw" }
    fn name(&self) -> &str { "OpenClaw" }
    fn is_available(&self) -> bool { !self.data_dirs.is_empty() }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        let mut records = Vec::new();
        for base in &self.data_dirs {
            let agents_dir = base.join("agents");
            if !agents_dir.exists() { continue; }
            for entry in walkdir::WalkDir::new(&agents_dir).max_depth(4).into_iter().filter_map(|e| e.ok())
                .filter(|e| e.path().extension().and_then(|ext| ext.to_str()).map(|ext| ext == "jsonl" || ext == "json").unwrap_or(false))
            {
                let path = entry.path();
                let Ok(content) = std::fs::read_to_string(path) else { continue };
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() { continue; }
                    if let Ok(log_entry) = serde_json::from_str::<OpenClawLogEntry>(line) {
                        if let Some(record) = log_entry_to_record(log_entry, path, since, Some(line.to_string())) {
                            records.push(record);
                        }
                    }
                }
            }
        }
        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(records)
    }

    fn watch_paths(&self) -> Vec<PathBuf> { self.data_dirs.iter().map(|d| d.join("agents")).collect() }
}

fn log_entry_to_record(
    entry: OpenClawLogEntry,
    path: &Path,
    since: Option<DateTime<Utc>>,
    raw_json: Option<String>,
) -> Option<UsageRecord> {
    let timestamp = entry.timestamp.as_ref()
        .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);
    if let Some(since) = since { if timestamp <= since { return None; } }

    let model = entry.model.unwrap_or_default();
    if model.is_empty() { return None; }
    let provider = Provider::from_url_and_model("", &model);
    let input = entry.input_tokens.unwrap_or(0);
    let output = entry.output_tokens.unwrap_or(0);
    let cache_read = entry.cache_read_tokens.unwrap_or(0);
    let cache_creation = entry.cache_creation_tokens.unwrap_or(0);

    Some(UsageRecord {
        id: None, timestamp, collector: "openclaw".to_string(), tool: Some("OpenClaw".to_string()),
        provider: provider.name().to_string(), model,
        input_tokens: input, output_tokens: output, reasoning_tokens: 0, cache_read_tokens: cache_read, cache_creation_tokens: cache_creation,
        total_tokens: entry.total_tokens.unwrap_or(input + output + cache_read + cache_creation),
        cost_usd: entry.cost.unwrap_or(0.0), cost_cny: 0.0, latency_ms: None, is_stream: false, status_code: None,
        session_id: entry.session_key, request_id: entry.request_id,
        source_file: Some(path.to_string_lossy().to_string()), raw_json, notes: None,
    })
}
