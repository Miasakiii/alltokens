use alltokens_core::model::{Provider, UsageRecord};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::Collector;

/// ZCode (智谱 AI IDE) 使用数据
/// 路径: ~/.zcode/projects/ 或 ~/.config/zcode/
/// 数据格式: session JSON 文件
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ZCodeSession {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    messages: Vec<ZCodeMessage>,
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
struct ZCodeMessage {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<ZCodeUsage>,
    #[serde(default)]
    timestamp: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ZCodeUsage {
    #[serde(rename = "prompt_tokens", default)]
    prompt_tokens: Option<u64>,
    #[serde(rename = "completion_tokens", default)]
    completion_tokens: Option<u64>,
    #[serde(rename = "total_tokens", default)]
    total_tokens: Option<u64>,
    #[serde(default)]
    cache: Option<ZCodeCache>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ZCodeCache {
    #[serde(rename = "cached_tokens", default)]
    cached_tokens: Option<u64>,
}

pub struct ZCodeCollector {
    data_dirs: Vec<PathBuf>,
}

impl ZCodeCollector {
    pub fn new() -> Self {
        let mut dirs = Vec::new();
        if let Some(home) = dirs::home_dir() {
            let candidates = [
                home.join(".zcode"),
                home.join(".config").join("zcode"),
                home.join(".local").join("share").join("zcode"),
            ];
            for path in &candidates {
                if path.exists() {
                    dirs.push(path.clone());
                }
            }
        }
        Self { data_dirs: dirs }
    }
}

#[async_trait]
impl Collector for ZCodeCollector {
    fn id(&self) -> &str { "zcode" }
    fn name(&self) -> &str { "ZCode" }
    fn is_available(&self) -> bool { !self.data_dirs.is_empty() }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        let mut records = Vec::new();
        for dir in &self.data_dirs {
            for entry in walkdir::WalkDir::new(dir).max_depth(3).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext != "json" { continue; }
                let Ok(content) = std::fs::read_to_string(path) else { continue };

                // 尝试解析为 session JSON
                if let Ok(session) = serde_json::from_str::<ZCodeSession>(&content) {
                    for msg in &session.messages {
                        let raw = serde_json::to_string(msg).ok();
                        if let Some(record) = msg_to_record(msg, path, since, raw) {
                            records.push(record);
                        }
                    }
                }
                // 尝试 JSONL
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() { continue; }
                    if let Ok(msg) = serde_json::from_str::<ZCodeMessage>(line) {
                        if let Some(record) = msg_to_record(&msg, path, since, Some(line.to_string())) {
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

fn msg_to_record(
    msg: &ZCodeMessage,
    path: &Path,
    since: Option<DateTime<Utc>>,
    raw_json: Option<String>,
) -> Option<UsageRecord> {
    let usage = msg.usage.as_ref()?;
    let timestamp = msg.timestamp.as_ref()
        .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);
    if let Some(since) = since { if timestamp <= since { return None; } }

    let model = msg.model.clone().unwrap_or_default();
    if model.is_empty() { return None; }
    let provider = Provider::from_url_and_model("", &model);
    let input = usage.prompt_tokens.unwrap_or(0);
    let output = usage.completion_tokens.unwrap_or(0);
    let cache_read = usage.cache.as_ref().and_then(|c| c.cached_tokens).unwrap_or(0);

    Some(UsageRecord {
        id: None, timestamp, collector: "zcode".to_string(), tool: Some("ZCode".to_string()),
        provider: provider.name().to_string(), model,
        input_tokens: input, output_tokens: output, reasoning_tokens: 0, cache_read_tokens: cache_read, cache_creation_tokens: 0,
        total_tokens: usage.total_tokens.unwrap_or(input + output + cache_read),
        cost_usd: 0.0, cost_cny: 0.0, latency_ms: None, is_stream: false, status_code: None,
        session_id: None, request_id: None,
        source_file: Some(path.to_string_lossy().to_string()), raw_json, notes: None,
    })
}
