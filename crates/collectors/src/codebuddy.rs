use alltokens_core::model::{Provider, UsageRecord};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::Collector;

/// CodeBuddy (腾讯 AI 编程助手) 使用数据
/// 扩展 ID: Tencent.codebuddy-code 或类似
/// 数据存储在 VS Code globalStorage 或独立目录
#[derive(Debug, Deserialize, Serialize)]
struct CodeBuddyUsage {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(rename = "promptTokens", default)]
    prompt_tokens: Option<u64>,
    #[serde(rename = "completionTokens", default)]
    completion_tokens: Option<u64>,
    #[serde(rename = "inputTokens", default)]
    input_tokens: Option<u64>,
    #[serde(rename = "outputTokens", default)]
    output_tokens: Option<u64>,
    #[serde(rename = "cacheReadTokens", default)]
    cache_read_tokens: Option<u64>,
    #[serde(rename = "totalTokens", default)]
    total_tokens: Option<u64>,
    #[serde(default)]
    cost: Option<f64>,
    #[serde(default)]
    latency_ms: Option<u64>,
    #[serde(rename = "sessionId", default)]
    session_id: Option<String>,
}

pub struct CodeBuddyCollector {
    data_dirs: Vec<PathBuf>,
}

impl CodeBuddyCollector {
    pub fn new() -> Self {
        let mut dirs = Vec::new();
        if let Some(home) = dirs::home_dir() {
            // VS Code globalStorage
            let vscode_dirs = [
                home.join(".config").join("Code").join("User").join("globalStorage"),
                home.join("Library").join("Application Support").join("Code").join("User").join("globalStorage"),
            ];
            for vscode_dir in &vscode_dirs {
                if vscode_dir.exists() {
                    // 扫描所有可能的 CodeBuddy 扩展目录
                    if let Ok(entries) = std::fs::read_dir(vscode_dir) {
                        for entry in entries.flatten() {
                            let name = entry.file_name().to_string_lossy().to_string();
                            if name.contains("codebuddy") || name.contains("tencent") {
                                dirs.push(entry.path());
                            }
                        }
                    }
                }
            }
            // 独立目录
            let home_dirs = [
                home.join(".codebuddy"),
                home.join(".config").join("codebuddy"),
                home.join(".tencent").join("codebuddy"),
            ];
            for path in &home_dirs {
                if path.exists() { dirs.push(path.clone()); }
            }
        }
        Self { data_dirs: dirs }
    }
}

#[async_trait]
impl Collector for CodeBuddyCollector {
    fn id(&self) -> &str { "codebuddy" }
    fn name(&self) -> &str { "CodeBuddy" }
    fn is_available(&self) -> bool { !self.data_dirs.is_empty() }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        let mut records = Vec::new();
        for dir in &self.data_dirs {
            for entry in walkdir::WalkDir::new(dir).max_depth(4).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext != "json" && ext != "jsonl" { continue; }
                let Ok(content) = std::fs::read_to_string(path) else { continue };

                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() { continue; }
                    if let Ok(usage) = serde_json::from_str::<CodeBuddyUsage>(line) {
                        if let Some(record) = usage_to_record(&usage, path, since, Some(line.to_string())) {
                            records.push(record);
                        }
                    }
                }
                // 也尝试 JSON 数组
                if let Ok(usages) = serde_json::from_str::<Vec<CodeBuddyUsage>>(&content) {
                    for usage in &usages {
                        let raw = serde_json::to_string(usage).ok();
                        if let Some(record) = usage_to_record(usage, path, since, raw) {
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

fn usage_to_record(
    usage: &CodeBuddyUsage,
    path: &Path,
    since: Option<DateTime<Utc>>,
    raw_json: Option<String>,
) -> Option<UsageRecord> {
    let timestamp = usage.timestamp.as_ref()
        .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);
    if let Some(since) = since { if timestamp <= since { return None; } }

    let model = usage.model.clone().unwrap_or_default();
    if model.is_empty() { return None; }
    let provider = Provider::from_url_and_model("", &model);
    let input = usage.prompt_tokens.or(usage.input_tokens).unwrap_or(0);
    let output = usage.completion_tokens.or(usage.output_tokens).unwrap_or(0);
    let cache_read = usage.cache_read_tokens.unwrap_or(0);

    Some(UsageRecord {
        id: None, timestamp, collector: "codebuddy".to_string(), tool: Some("CodeBuddy".to_string()),
        provider: provider.name().to_string(), model,
        input_tokens: input, output_tokens: output, reasoning_tokens: 0, cache_read_tokens: cache_read, cache_creation_tokens: 0,
        total_tokens: usage.total_tokens.unwrap_or(input + output + cache_read),
        cost_usd: usage.cost.unwrap_or(0.0), cost_cny: 0.0,
        latency_ms: usage.latency_ms, is_stream: false, status_code: None,
        session_id: usage.session_id.clone(), request_id: None,
        source_file: Some(path.to_string_lossy().to_string()), raw_json, notes: None,
    })
}
