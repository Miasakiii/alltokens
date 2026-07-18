use alltokens_core::model::{Provider, UsageRecord};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use super::Collector;

/// GitHub Copilot 使用数据
/// OpenTelemetry 格式，存储在 ~/.copilot/otel/
/// 路径: ~/.copilot/otel/*.json 或 ~/.config/github-copilot/
#[derive(Debug, Deserialize)]
struct CopilotUsage {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(rename = "prompt_tokens", default)]
    prompt_tokens: Option<u64>,
    #[serde(rename = "completion_tokens", default)]
    completion_tokens: Option<u64>,
    #[serde(rename = "total_tokens", default)]
    total_tokens: Option<u64>,
}

pub struct CopilotCollector {
    data_dirs: Vec<PathBuf>,
}

impl CopilotCollector {
    pub fn new() -> Self {
        let mut dirs = Vec::new();
        if let Some(home) = dirs::home_dir() {
            let candidates = [
                home.join(".copilot").join("otel"),
                home.join(".config").join("github-copilot"),
                home.join(".local").join("share").join("github-copilot"),
            ];
            for path in &candidates {
                if path.exists() { dirs.push(path.clone()); }
            }
        }
        Self { data_dirs: dirs }
    }
}

#[async_trait]
impl Collector for CopilotCollector {
    fn id(&self) -> &str { "copilot" }
    fn name(&self) -> &str { "GitHub Copilot" }
    fn is_available(&self) -> bool { !self.data_dirs.is_empty() }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        let mut records = Vec::new();
        for dir in &self.data_dirs {
            for entry in walkdir::WalkDir::new(dir).max_depth(3).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext != "json" && ext != "jsonl" { continue; }
                let Ok(content) = std::fs::read_to_string(path) else { continue };
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() { continue; }
                    if let Ok(usage) = serde_json::from_str::<CopilotUsage>(line) {
                        if let Some(record) = copilot_to_record(&usage, path, since, Some(line.to_string())) {
                            records.push(record);
                        }
                    }
                }
            }
        }
        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(records)
    }

    fn watch_paths(&self) -> Vec<PathBuf> { self.data_dirs.clone() }
}

fn copilot_to_record(
    usage: &CopilotUsage,
    path: &Path,
    since: Option<DateTime<Utc>>,
    raw_json: Option<String>,
) -> Option<UsageRecord> {
    let timestamp = usage.timestamp.as_ref()
        .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);
    if let Some(since) = since { if timestamp <= since { return None; } }

    let model = usage.model.clone().unwrap_or_else(|| "gpt-4o".to_string());
    let provider = Provider::from_url_and_model("", &model);
    let input = usage.prompt_tokens.unwrap_or(0);
    let output = usage.completion_tokens.unwrap_or(0);

    Some(UsageRecord {
        id: None, timestamp, collector: "copilot".to_string(), tool: Some("GitHub Copilot".to_string()),
        provider: provider.name().to_string(), model,
        input_tokens: input, output_tokens: output, reasoning_tokens: 0, cache_read_tokens: 0, cache_creation_tokens: 0,
        total_tokens: usage.total_tokens.unwrap_or(input + output),
        cost_usd: 0.0, cost_cny: 0.0, latency_ms: None, is_stream: false, status_code: None,
        session_id: None, request_id: None,
        source_file: Some(path.to_string_lossy().to_string()), raw_json, notes: None,
    })
}
