use alltokens_core::model::{Provider, UsageRecord};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::paths;
use super::Collector;

/// Cursor tokscale cache 格式
/// 路径 (含 WSL):
///   - ~/.config/tokscale/cursor-cache/
///   - ~/.cursor/
///   - ~/Library/Application Support/Cursor/
#[derive(Debug, Deserialize, Serialize)]
struct CursorUsageEntry {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(rename = "prompt_tokens", default)]
    input_tokens: Option<u64>,
    #[serde(rename = "completion_tokens", default)]
    output_tokens: Option<u64>,
    #[serde(rename = "cache_read_tokens", default)]
    cache_read_tokens: Option<u64>,
    #[serde(rename = "total_tokens", default)]
    total_tokens: Option<u64>,
    #[serde(default)]
    cost: Option<f64>,
    #[serde(default)]
    latency_ms: Option<u64>,
}

pub struct CursorCollector {
    data_dirs: Vec<PathBuf>,
}

impl CursorCollector {
    pub fn new() -> Self {
        let candidates = [
            ".config/tokscale/cursor-cache",
            ".cursor",
            "Library/Application Support/Cursor",
        ];
        let data_dirs = paths::find_paths(&candidates);
        Self { data_dirs }
    }

    /// Override data directories (for tests and dry-run tooling).
    #[doc(hidden)]
    pub fn with_dirs(data_dirs: Vec<PathBuf>) -> Self {
        Self { data_dirs }
    }

    fn scan_json_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for dir in &self.data_dirs {
            walkdir::WalkDir::new(dir).max_depth(3).into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    let path = e.path();
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    (ext == "json" || ext == "jsonl")
                        && path.file_name().and_then(|n| n.to_str())
                            .map(|n| n.contains("usage") || n.contains("token") || n.contains("cost"))
                            .unwrap_or(false)
                })
                .for_each(|e| files.push(e.path().to_path_buf()));
        }
        files
    }

    fn parse_file(&self, path: &Path, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        let content = std::fs::read_to_string(path)?;
        let mut records = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('[') { continue; }
            if let Ok(entry) = serde_json::from_str::<CursorUsageEntry>(line) {
                if let Some(record) = self.entry_to_record(entry, path, since, Some(line.to_string())) {
                    records.push(record);
                }
            }
        }

        if records.is_empty() {
            if let Ok(entries) = serde_json::from_str::<Vec<CursorUsageEntry>>(&content) {
                for entry in entries {
                    let raw = serde_json::to_string(&entry).ok();
                    if let Some(record) = self.entry_to_record(entry, path, since, raw) {
                        records.push(record);
                    }
                }
            }
        }

        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(records)
    }

    fn entry_to_record(
        &self,
        entry: CursorUsageEntry,
        path: &Path,
        since: Option<DateTime<Utc>>,
        raw_json: Option<String>,
    ) -> Option<UsageRecord> {
        let timestamp = entry.timestamp.as_ref()
            .and_then(|ts| DateTime::parse_from_rfc3339(ts).or_else(|_| DateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.fZ")).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        if let Some(since) = since { if timestamp <= since { return None; } }

        let model = entry.model.unwrap_or_default();
        let provider = Provider::from_url_and_model("", &model);
        let input = entry.input_tokens.unwrap_or(0);
        let output = entry.output_tokens.unwrap_or(0);
        let cache_read = entry.cache_read_tokens.unwrap_or(0);

        Some(UsageRecord {
            id: None, timestamp, collector: "cursor".to_string(), tool: Some("Cursor".to_string()),
            provider: provider.name().to_string(), model,
            input_tokens: input, output_tokens: output, reasoning_tokens: 0, cache_read_tokens: cache_read, cache_creation_tokens: 0,
            total_tokens: entry.total_tokens.unwrap_or(input + output + cache_read),
            cost_usd: entry.cost.unwrap_or(0.0), cost_cny: 0.0,
            latency_ms: entry.latency_ms, is_stream: false, status_code: None,
            session_id: None, request_id: None,
            source_file: Some(path.to_string_lossy().to_string()), raw_json, notes: None,
        })
    }
}

#[async_trait]
impl Collector for CursorCollector {
    fn id(&self) -> &str { "cursor" }
    fn name(&self) -> &str { "Cursor" }
    fn is_available(&self) -> bool { !self.data_dirs.is_empty() }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        let files = self.scan_json_files();
        let mut all_records = Vec::new();
        for file in files {
            match self.parse_file(&file, since) {
                Ok(records) => all_records.extend(records),
                Err(e) => tracing::warn!("Failed to parse {}: {e}", file.display()),
            }
        }
        all_records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(all_records)
    }

    fn watch_paths(&self) -> Vec<PathBuf> { self.data_dirs.clone() }
}

impl CursorCollector {
    /// Dry-run probe: list data paths, file counts, and sample record count.
    pub fn probe(&self) -> Result<super::probe::BasicProbeResult> {
        let files = self.scan_json_files();
        let data_paths: Vec<String> = self
            .data_dirs
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        let sample_records = super::probe::collect_sample_count(self);
        Ok(super::probe::build_basic_probe_result(
            "cursor",
            "Cursor",
            self.is_available(),
            data_paths,
            files.len(),
            sample_records,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn collect_from_token_usage_fixture() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("cursor-usage.jsonl"),
            r#"{"timestamp":"2026-07-10T12:00:00Z","model":"gpt-4o","prompt_tokens":1000,"completion_tokens":500,"cache_read_tokens":100,"total_tokens":1600,"cost":0.015}
{"timestamp":"2026-07-10T13:00:00Z","model":"claude-sonnet-4-20250514","prompt_tokens":800,"completion_tokens":400,"total_tokens":1200}"#,
        )
        .unwrap();

        let collector = CursorCollector::with_dirs(vec![dir.path().to_path_buf()]);
        let records = collector.collect(None).await.unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].input_tokens, 1000);
        assert_eq!(records[0].cache_read_tokens, 100);
        assert_eq!(records[1].model, "claude-sonnet-4-20250514");
    }
}
