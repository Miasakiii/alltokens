use alltokens_core::model::{Provider, UsageRecord};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use super::paths;
use super::Collector;

/// Claude Code 使用记录 JSON 格式 (legacy 独立 usage 文件)
/// 路径:
///   - ~/.claude/projects/*/usage/*.jsonl
///   - ~/.claude/usage/*.jsonl
///   - WSL: /mnt/c/Users/<user>/.claude/...
#[derive(Debug, Deserialize)]
struct ClaudeUsageEntry {
    #[serde(rename = "timestamp", default)]
    timestamp: Option<String>,
    #[serde(rename = "model", default)]
    model: Option<String>,
    #[serde(rename = "inputTokens", default)]
    input_tokens: Option<u64>,
    #[serde(rename = "outputTokens", default)]
    output_tokens: Option<u64>,
    #[serde(rename = "cacheCreationInputTokens", default)]
    cache_creation_tokens: Option<u64>,
    #[serde(rename = "cacheReadInputTokens", default)]
    cache_read_tokens: Option<u64>,
    #[serde(rename = "costUSD", default)]
    cost_usd: Option<f64>,
    #[serde(rename = "durationMs", default)]
    duration_ms: Option<u64>,
    #[serde(rename = "sessionId", default)]
    session_id: Option<String>,
    #[serde(rename = "requestId", default)]
    request_id: Option<String>,
    #[serde(rename = "isStreaming", default)]
    is_streaming: Option<bool>,
    #[serde(rename = "statusCode", default)]
    status_code: Option<u16>,
}

/// Claude Code 真实会话 JSONL 格式 (2025+ 版本)
/// 文件名为 UUID.jsonl，存储在 ~/.claude/projects/<project>/
/// 每行 type=assistant 包含 message.usage 嵌套的 token 用量
#[derive(Debug, Deserialize)]
struct ClaudeSessionLine {
    #[serde(rename = "type", default)]
    line_type: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(rename = "sessionId", default)]
    session_id: Option<String>,
    #[serde(default)]
    message: Option<ClaudeMessage>,
}

#[derive(Debug, Deserialize)]
struct ClaudeMessage {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    role: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<ClaudeMessageUsage>,
    #[serde(default)]
    content: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct ClaudeMessageUsage {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u64>,
}

pub struct ClaudeCodeCollector {
    data_dirs: Vec<PathBuf>,
}

impl ClaudeCodeCollector {
    pub fn new() -> Self {
        let mut data_dirs = Vec::new();

        // 本机 + WSL Windows 路径
        let candidates = [
            ".claude/projects",
            ".claude/usage",
            ".claude/statsig",
        ];
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

    /// Override data directories (for tests and dry-run tooling).
    #[doc(hidden)]
    pub fn with_dirs(data_dirs: Vec<PathBuf>) -> Self {
        Self { data_dirs }
    }

    /// 扫描所有 JSONL usage 文件
    fn scan_usage_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for dir in &self.data_dirs {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        if let Ok(sub_entries) = std::fs::read_dir(&path) {
                            for sub in sub_entries.flatten() {
                                let sub_path = sub.path();
                                // 再递归一层 (projects/<name>/usage/)
                                if sub_path.is_dir() {
                                    if let Ok(sub2) = std::fs::read_dir(&sub_path) {
                                        for s in sub2.flatten() {
                                            let p = s.path();
                                            if is_usage_file(&p) { files.push(p); }
                                        }
                                    }
                                }
                                if is_usage_file(&sub_path) {
                                    files.push(sub_path);
                                }
                            }
                        }
                    } else if is_usage_file(&path) {
                        files.push(path);
                    }
                }
            }
        }
        files
    }

    fn parse_jsonl_file(&self, path: &Path, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        let content = std::fs::read_to_string(path)?;
        let mut records = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }

            // Try the new session format first (2025+ UUID-named files)
            // Only match if the line has a root-level "type" field (session lines always have it)
            if let Ok(session_line) = serde_json::from_str::<ClaudeSessionLine>(line) {
                if session_line.line_type.is_some() {
                    if let Some(record) = self.session_line_to_record(&session_line, path, since, line) {
                        records.push(record);
                        continue;
                    }
                    // Also extract tool/skill invocations from assistant content
                    if session_line.line_type.as_deref() == Some("assistant") {
                        if let Some(msg) = &session_line.message {
                            if let Some(content_arr) = &msg.content {
                                for item in content_arr {
                                    if item.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                                        if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                                            let ts = parse_timestamp_str(session_line.timestamp.as_deref());
                                            if let Some(since) = since { if ts <= since { continue; } }
                                            if name == "Skill" || item.get("input").and_then(|i| i.get("skill")).is_some() {
                                                let skill_name = item.get("input")
                                                    .and_then(|i| i.get("skill"))
                                                    .and_then(|s| s.as_str())
                                                    .unwrap_or(name);
                                                records.push(make_invocation_record(path, line, skill_name, alltokens_core::invocation::NOTE_INVOCATION_SKILL, ts, session_line.session_id.clone()));
                                                continue;
                                            } else {
                                                records.push(make_invocation_record(path, line, name, alltokens_core::invocation::NOTE_INVOCATION_TOOL, ts, session_line.session_id.clone()));
                                                continue;
                                            };
                                        }
                                    }
                                }
                            }
                        }
                    }
                    continue;
                }
            }

            // Fallback: legacy flat usage format
            if let Ok(entry) = serde_json::from_str::<ClaudeUsageEntry>(line) {
                let has_usage = entry.model.as_ref().is_some_and(|m| !m.is_empty())
                    || entry.input_tokens.unwrap_or(0) > 0
                    || entry.output_tokens.unwrap_or(0) > 0
                    || entry.cache_read_tokens.unwrap_or(0) > 0
                    || entry.cache_creation_tokens.unwrap_or(0) > 0
                    || entry.cost_usd.unwrap_or(0.0) > 0.0;
                if !has_usage {
                    append_invocations_from_line(&mut records, path, line, since);
                    continue;
                }

                let timestamp = parse_timestamp_str(entry.timestamp.as_deref());
                if let Some(since) = since { if timestamp <= since { continue; } }

                let model = entry.model.unwrap_or_default();
                let provider = Provider::from_url_and_model("", &model);
                let input = entry.input_tokens.unwrap_or(0);
                let output = entry.output_tokens.unwrap_or(0);
                let cache_read = entry.cache_read_tokens.unwrap_or(0);
                let cache_creation = entry.cache_creation_tokens.unwrap_or(0);

                let mut record = UsageRecord {
                    id: None, timestamp, collector: "claude_code".to_string(), tool: Some("Claude Code".to_string()),
                    provider: provider.name().to_string(), model,
                    input_tokens: input, output_tokens: output, reasoning_tokens: 0, cache_read_tokens: cache_read, cache_creation_tokens: cache_creation,
                    total_tokens: input + output + cache_read + cache_creation,
                    cost_usd: entry.cost_usd.unwrap_or(0.0), cost_cny: 0.0,
                    latency_ms: entry.duration_ms, is_stream: entry.is_streaming.unwrap_or(false),
                    status_code: entry.status_code, session_id: entry.session_id, request_id: entry.request_id,
                    source_file: Some(path.to_string_lossy().to_string()), raw_json: Some(line.to_string()), notes: None,
                };
                if record.cost_usd > 0.0 { record.cost_cny = record.cost_usd * 7.25; }
                records.push(record);
            } else {
                append_invocations_from_line(&mut records, path, line, since);
            }
        }
        Ok(records)
    }

    /// Convert a session line (2025+ format) to UsageRecord if it has token data.
    fn session_line_to_record(
        &self,
        line: &ClaudeSessionLine,
        path: &Path,
        since: Option<DateTime<Utc>>,
        raw: &str,
    ) -> Option<UsageRecord> {
        // Only process assistant messages with usage
        if line.line_type.as_deref() != Some("assistant") {
            return None;
        }
        let msg = line.message.as_ref()?;
        let usage = msg.usage.as_ref()?;

        let input = usage.input_tokens.unwrap_or(0);
        let output = usage.output_tokens.unwrap_or(0);
        let cache_read = usage.cache_read_input_tokens.unwrap_or(0);
        let cache_creation = usage.cache_creation_input_tokens.unwrap_or(0);

        // Skip lines with zero tokens (e.g. partial streaming entries)
        if input == 0 && output == 0 && cache_read == 0 && cache_creation == 0 {
            return None;
        }

        let timestamp = parse_timestamp_str(line.timestamp.as_deref());
        if let Some(since) = since { if timestamp <= since { return None; } }

        let model = msg.model.clone().unwrap_or_default();
        let provider = Provider::from_url_and_model("", &model);

        Some(UsageRecord {
            id: None,
            timestamp,
            collector: "claude_code".to_string(),
            tool: Some("Claude Code".to_string()),
            provider: provider.name().to_string(),
            model,
            input_tokens: input,
            output_tokens: output,
            reasoning_tokens: 0,
            cache_read_tokens: cache_read,
            cache_creation_tokens: cache_creation,
            total_tokens: input + output + cache_read + cache_creation,
            cost_usd: 0.0,
            cost_cny: 0.0,
            latency_ms: None,
            is_stream: false,
            status_code: None,
            session_id: line.session_id.clone(),
            request_id: msg.id.clone(),
            source_file: Some(path.to_string_lossy().to_string()),
            raw_json: Some(raw.to_string()),
            notes: None,
        })
    }
}

fn parse_timestamp_str(ts: Option<&str>) -> DateTime<Utc> {
    ts.and_then(|ts| {
        DateTime::parse_from_rfc3339(ts)
            .or_else(|_| DateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.f%:z"))
            .or_else(|_| DateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.fZ"))
            .ok()
    })
    .map(|dt| dt.with_timezone(&Utc))
    .unwrap_or_else(Utc::now)
}

fn make_invocation_record(
    path: &Path,
    raw: &str,
    name: &str,
    note: &str,
    timestamp: DateTime<Utc>,
    session_id: Option<String>,
) -> UsageRecord {
    UsageRecord {
        id: None,
        timestamp,
        collector: "claude_code".to_string(),
        tool: Some(name.to_string()),
        provider: String::new(),
        model: String::new(),
        input_tokens: 0,
        output_tokens: 0,
        reasoning_tokens: 0,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        total_tokens: 0,
        cost_usd: 0.0,
        cost_cny: 0.0,
        latency_ms: None,
        is_stream: false,
        status_code: None,
        session_id,
        request_id: None,
        source_file: Some(path.to_string_lossy().to_string()),
        raw_json: Some(raw.to_string()),
        notes: Some(note.to_string()),
    }
}

fn append_invocations_from_line(
    records: &mut Vec<UsageRecord>,
    path: &Path,
    line: &str,
    since: Option<DateTime<Utc>>,
) {
    for name in alltokens_core::invocation::extract_tool_names_from_json(line) {
        if let Some(record) = invocation_record(
            path,
            line,
            &name,
            alltokens_core::invocation::NOTE_INVOCATION_TOOL,
            since,
        ) {
            records.push(record);
        }
    }
    for name in alltokens_core::invocation::extract_skill_names_from_json(line) {
        if let Some(record) = invocation_record(
            path,
            line,
            &name,
            alltokens_core::invocation::NOTE_INVOCATION_SKILL,
            since,
        ) {
            records.push(record);
        }
    }
}

fn is_usage_file(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some("jsonl") | Some("json") => {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Match legacy "usage"/"stats"/"messages" files
            if name.contains("usage") || name.contains("stats") || name.contains("messages") {
                return true;
            }
            // Match UUID-named session JSONL files (2025+ format)
            // UUID format: 8-4-4-4-12 hex chars with .jsonl extension
            if name.ends_with(".jsonl") {
                let stem = &name[..name.len() - 6];
                if stem.len() >= 32 && stem.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

fn invocation_record(
    path: &Path,
    line: &str,
    name: &str,
    note: &str,
    since: Option<DateTime<Utc>>,
) -> Option<UsageRecord> {
    let timestamp = serde_json::from_str::<serde_json::Value>(line)
        .ok()
        .and_then(|v| {
            v.get("timestamp")
                .and_then(|t| t.as_str())
                .and_then(|ts| {
                    DateTime::parse_from_rfc3339(ts)
                        .or_else(|_| DateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.f%:z"))
                        .or_else(|_| DateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.fZ"))
                        .ok()
                })
                .map(|dt| dt.with_timezone(&Utc))
        })
        .unwrap_or_else(Utc::now);

    if let Some(since) = since {
        if timestamp <= since {
            return None;
        }
    }

    Some(UsageRecord {
        id: None,
        timestamp,
        collector: "claude_code".to_string(),
        tool: Some(name.to_string()),
        provider: String::new(),
        model: String::new(),
        input_tokens: 0,
        output_tokens: 0,
        reasoning_tokens: 0,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        total_tokens: 0,
        cost_usd: 0.0,
        cost_cny: 0.0,
        latency_ms: None,
        is_stream: false,
        status_code: None,
        session_id: None,
        request_id: None,
        source_file: Some(path.to_string_lossy().to_string()),
        raw_json: Some(line.to_string()),
        notes: Some(note.to_string()),
    })
}

#[async_trait]
impl Collector for ClaudeCodeCollector {
    fn id(&self) -> &str { "claude_code" }
    fn name(&self) -> &str { "Claude Code" }
    fn is_available(&self) -> bool { !self.data_dirs.is_empty() }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        let files = self.scan_usage_files();
        let mut all_records = Vec::new();
        for file in files {
            match self.parse_jsonl_file(&file, since) {
                Ok(records) => all_records.extend(records),
                Err(e) => tracing::warn!("Failed to parse {}: {e}", file.display()),
            }
        }
        all_records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(all_records)
    }

    fn watch_paths(&self) -> Vec<PathBuf> { self.data_dirs.clone() }
}

/// Probe summary for `alltokens probe claude`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClaudeProbeResult {
    pub data_dirs: Vec<String>,
    pub usage_files: usize,
    pub snapshot_paths: Vec<String>,
    pub quota: Option<alltokens_core::model::ClaudeQuotaSnapshot>,
    pub quota_error: Option<String>,
}

impl ClaudeCodeCollector {
    /// Dry-run probe: list transcript sources and optional statusLine quota.
    pub fn probe(&self) -> Result<ClaudeProbeResult> {
        self.probe_with_quota(true)
    }

    pub fn probe_with_quota(&self, include_quota: bool) -> Result<ClaudeProbeResult> {
        let usage_files = self.scan_usage_files().len();
        let snapshot_paths = super::claude_quota::statusline_snapshot_candidates()
            .into_iter()
            .filter(|p| p.exists())
            .map(|p| p.to_string_lossy().into_owned())
            .collect();

        let (quota, quota_error) = if include_quota {
            match super::claude_quota::read_claude_quota_snapshot() {
                Ok(snapshot) => (Some(snapshot), None),
                Err(e) => (None, Some(e.to_string())),
            }
        } else {
            (None, None)
        };

        Ok(ClaudeProbeResult {
            data_dirs: self
                .data_dirs
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            usage_files,
            snapshot_paths,
            quota,
            quota_error,
        })
    }
}

#[async_trait]
impl super::ClaudeQuotaProvider for ClaudeCodeCollector {
    fn provider_id(&self) -> &str {
        "claude_code"
    }

    async fn fetch_claude_quota(&self) -> Result<Option<alltokens_core::model::ClaudeQuotaSnapshot>> {
        match super::claude_quota::fetch_claude_quota().await {
            Ok(snapshot) => Ok(Some(snapshot)),
            Err(e) => {
                tracing::warn!("Claude quota fetch failed: {e}");
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_jsonl_line() {
        let line = r#"{"timestamp":"2026-07-10T12:00:00Z","model":"claude-sonnet-4-20250514","inputTokens":1500,"outputTokens":800,"cacheCreationInputTokens":0,"cacheReadInputTokens":200,"costUSD":0.0123,"durationMs":2500,"sessionId":"sess_123","isStreaming":true}"#;
        let entry: ClaudeUsageEntry = serde_json::from_str(line).unwrap();
        assert_eq!(entry.model.unwrap(), "claude-sonnet-4-20250514");
        assert_eq!(entry.input_tokens.unwrap(), 1500);
    }

    #[tokio::test]
    async fn collect_from_usage_fixture_dir() {
        let dir = tempfile::tempdir().unwrap();
        let usage_dir = dir.path().join("usage");
        std::fs::create_dir_all(&usage_dir).unwrap();
        std::fs::write(
            usage_dir.join("usage-session.jsonl"),
            r#"{"timestamp":"2026-07-10T12:00:00Z","model":"claude-sonnet-4-20250514","inputTokens":1500,"outputTokens":800,"cacheReadInputTokens":200,"costUSD":0.0123,"sessionId":"sess_123"}
{"timestamp":"2026-07-10T13:00:00Z","model":"claude-haiku-3.5","inputTokens":400,"outputTokens":100,"cacheReadInputTokens":0,"costUSD":0.001}"#,
        )
        .unwrap();

        let collector = ClaudeCodeCollector::with_dirs(vec![dir.path().to_path_buf()]);
        let records = collector.collect(None).await.unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].input_tokens, 1500);
        assert_eq!(records[0].cache_read_tokens, 200);
        assert_eq!(records[1].model, "claude-haiku-3.5");
    }

    #[tokio::test]
    async fn collect_tool_invocations_from_transcript_lines() {
        let dir = tempfile::tempdir().unwrap();
        let usage_dir = dir.path().join("usage");
        std::fs::create_dir_all(&usage_dir).unwrap();
        std::fs::write(
            usage_dir.join("messages-session.jsonl"),
            r#"{"timestamp":"2026-07-10T12:00:00Z","type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{}}]}}
{"timestamp":"2026-07-10T12:01:00Z","type":"assistant","message":{"content":[{"type":"tool_use","name":"Skill","input":{"skill":"canvas"}}]}}"#,
        )
        .unwrap();

        let collector = ClaudeCodeCollector::with_dirs(vec![dir.path().to_path_buf()]);
        let records = collector.collect(None).await.unwrap();
        assert_eq!(records.len(), 2);
        assert!(records.iter().any(|r| r.tool.as_deref() == Some("Bash")));
        assert!(records.iter().any(|r| r.tool.as_deref() == Some("canvas")));
        assert!(records.iter().all(|r| r.total_tokens == 0));
    }
}
