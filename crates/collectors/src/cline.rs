use alltokens_core::model::{Provider, UsageRecord};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::Collector;

/// Cline / Roo Code / Kilo Code VS Code 扩展的 usage 数据
/// 存储在 VS Code globalStorage 中
/// 路径:
///   - Cline: ~/.config/Code/User/globalStorage/saoudrizwan.claude-dev/
///   - Roo Code: ~/.config/Code/User/globalStorage/rooveterinaryinc.roo-cline/
///   - Kilo Code: ~/.config/Code/User/globalStorage/kilocode.kilo-code/
///
/// 数据格式: 任务级 JSON 文件，包含 token 使用统计
#[derive(Debug, Deserialize, Serialize)]
struct TaskUsage {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(rename = "tokensIn", default)]
    tokens_in: Option<u64>,
    #[serde(rename = "tokensOut", default)]
    tokens_out: Option<u64>,
    #[serde(rename = "cacheWrites", default)]
    cache_writes: Option<u64>,
    #[serde(rename = "cacheReads", default)]
    cache_reads: Option<u64>,
    #[serde(rename = "totalCost", default)]
    total_cost: Option<f64>,
}

/// Cline 系列扩展的通用采集器
pub struct ClineCollector {
    tool_name: String,
    collector_id: String,
    extension_dirs: Vec<PathBuf>,
}

impl ClineCollector {
    pub fn new_cline() -> Self {
        Self::new_with_ids(
            "Cline",
            "cline",
            &["saoudrizwan.claude-dev"],
        )
    }

    pub fn new_roo_code() -> Self {
        Self::new_with_ids(
            "Roo Code",
            "roo_code",
            &["rooveterinaryinc.roo-cline"],
        )
    }

    pub fn new_kilo_code() -> Self {
        Self::new_with_ids(
            "Kilo Code",
            "kilo_code",
            &["kilocode.kilo-code"],
        )
    }

    fn new_with_ids(tool_name: &str, collector_id: &str, extension_ids: &[&str]) -> Self {
        let mut dirs = Vec::new();

        if let Some(home) = dirs::home_dir() {
            // Linux: ~/.config/Code/User/globalStorage/
            let vscode_linux = home.join(".config").join("Code").join("User").join("globalStorage");
            // macOS: ~/Library/Application Support/Code/User/globalStorage/
            let vscode_mac = home
                .join("Library")
                .join("Application Support")
                .join("Code")
                .join("User")
                .join("globalStorage");
            // Windows: %APPDATA%/Code/User/globalStorage/
            let vscode_win = dirs::config_dir()
                .map(|d| d.join("Code").join("User").join("globalStorage"));

            for base in [Some(vscode_linux), Some(vscode_mac), vscode_win].into_iter().flatten() {
                for ext_id in extension_ids {
                    let ext_dir = base.join(ext_id);
                    if ext_dir.exists() {
                        dirs.push(ext_dir);
                    }
                }
            }
        }

        Self {
            tool_name: tool_name.to_string(),
            collector_id: collector_id.to_string(),
            extension_dirs: dirs,
        }
    }

    fn scan_tasks(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        let mut records = Vec::new();

        for dir in &self.extension_dirs {
            // Cline/Roo 存储任务数据在 tasks/ 或直接在目录下
            for entry in walkdir::WalkDir::new(dir)
                .max_depth(3)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext != "json" {
                    continue;
                }

                let content = match std::fs::read_to_string(path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                // 尝试解析为单条 usage 记录
                if let Ok(usage) = serde_json::from_str::<TaskUsage>(&content) {
                    let raw = serde_json::to_string(&usage).ok();
                    if let Some(record) = self.usage_to_record(usage, path, since, raw) {
                        records.push(record);
                    }
                }

                // 尝试解析为 JSON 数组
                if let Ok(usages) = serde_json::from_str::<Vec<TaskUsage>>(&content) {
                    for usage in usages {
                        let raw = serde_json::to_string(&usage).ok();
                        if let Some(record) = self.usage_to_record(usage, path, since, raw) {
                            records.push(record);
                        }
                    }
                }

                // 尝试 JSONL 格式
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    if let Ok(usage) = serde_json::from_str::<TaskUsage>(line) {
                        if let Some(record) = self.usage_to_record(usage, path, since, Some(line.to_string())) {
                            records.push(record);
                        }
                    }
                }
            }
        }

        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(records)
    }

    fn usage_to_record(
        &self,
        usage: TaskUsage,
        path: &Path,
        since: Option<DateTime<Utc>>,
        raw_json: Option<String>,
    ) -> Option<UsageRecord> {
        let timestamp = usage
            .timestamp
            .as_ref()
            .and_then(|ts| {
                DateTime::parse_from_rfc3339(ts)
                    .or_else(|_| DateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.fZ"))
                    .ok()
            })
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        if let Some(since) = since {
            if timestamp <= since {
                return None;
            }
        }

        let model = usage.model.unwrap_or_default();
        if model.is_empty() {
            return None;
        }

        let provider = Provider::from_url_and_model("", &model);
        let input = usage.tokens_in.unwrap_or(0);
        let output = usage.tokens_out.unwrap_or(0);
        let cache_read = usage.cache_reads.unwrap_or(0);
        let cache_creation = usage.cache_writes.unwrap_or(0);

        Some(UsageRecord {
            id: None,
            timestamp,
            collector: self.collector_id.clone(),
            tool: Some(self.tool_name.clone()),
            provider: provider.name().to_string(),
            model,
            input_tokens: input,
            output_tokens: output,
            reasoning_tokens: 0,
            cache_read_tokens: cache_read,
            cache_creation_tokens: cache_creation,
            total_tokens: input + output + cache_read + cache_creation,
            cost_usd: usage.total_cost.unwrap_or(0.0),
            cost_cny: 0.0,
            latency_ms: None,
            is_stream: false,
            status_code: None,
            session_id: None,
            request_id: None,
            source_file: Some(path.to_string_lossy().to_string()),
            raw_json,
            notes: None,
        })
    }
}

#[async_trait]
impl Collector for ClineCollector {
    fn id(&self) -> &str {
        &self.collector_id
    }

    fn name(&self) -> &str {
        &self.tool_name
    }

    fn is_available(&self) -> bool {
        !self.extension_dirs.is_empty()
    }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        self.scan_tasks(since)
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        self.extension_dirs.clone()
    }
}
