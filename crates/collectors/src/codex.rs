use alltokens_core::model::{Provider, UsageRecord};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::paths;
use super::Collector;

const NOTE_DETAILED: &str = "source_quality:detailed";
const NOTE_COARSE: &str = "source_quality:coarse";

/// Codex session JSON format (legacy per-item usage).
/// Path: ~/.codex/sessions/*.json (含 WSL)
#[derive(Debug, Deserialize)]
struct CodexSession {
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    items: Vec<CodexItem>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CodexItem {
    #[serde(rename = "type", default)]
    #[allow(dead_code)]
    item_type: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<CodexUsage>,
    #[serde(default)]
    timestamp: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CodexUsage {
    #[serde(rename = "input_tokens", default)]
    input_tokens: Option<u64>,
    #[serde(rename = "output_tokens", default)]
    output_tokens: Option<u64>,
    #[serde(rename = "input_tokens_details", default)]
    input_details: Option<InputDetails>,
    #[serde(rename = "total_tokens", default)]
    total_tokens: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct InputDetails {
    #[serde(rename = "cached_tokens", default)]
    cached_tokens: Option<u64>,
}

/// Rollout JSONL line: session_meta | turn_context | event_msg/token_count
#[derive(Debug, Deserialize)]
struct RolloutLine {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(rename = "type", default)]
    line_type: Option<String>,
    #[serde(default)]
    payload: Option<RolloutPayload>,
}

#[derive(Debug, Deserialize)]
struct RolloutPayload {
    #[serde(rename = "type", default)]
    payload_type: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    info: Option<TokenCountInfo>,
}

#[derive(Debug, Deserialize)]
struct TokenCountInfo {
    #[serde(rename = "total_token_usage", default)]
    total_token_usage: Option<TokenUsageSnapshot>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TokenUsageSnapshot {
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    reasoning_output_tokens: u64,
    total_tokens: u64,
}

impl<'de> Deserialize<'de> for TokenUsageSnapshot {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(rename = "input_tokens", default)]
            input_tokens: Option<u64>,
            #[serde(rename = "cached_input_tokens", default)]
            cached_input_tokens: Option<u64>,
            #[serde(rename = "output_tokens", default)]
            output_tokens: Option<u64>,
            #[serde(rename = "reasoning_output_tokens", default)]
            reasoning_output_tokens: Option<u64>,
            #[serde(rename = "total_tokens", default)]
            total_tokens: Option<u64>,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(Self {
            input_tokens: raw.input_tokens.unwrap_or(0),
            cached_input_tokens: raw.cached_input_tokens.unwrap_or(0),
            output_tokens: raw.output_tokens.unwrap_or(0),
            reasoning_output_tokens: raw.reasoning_output_tokens.unwrap_or(0),
            total_tokens: raw.total_tokens.unwrap_or(0),
        })
    }
}

/// SQLite thread row (coarse fallback).
struct SqliteThread {
    id: String,
    tokens_used: u64,
    updated_at: Option<String>,
    model: Option<String>,
}

/// Probe summary for `alltokens probe codex`.
#[derive(Debug, Clone, Serialize)]
pub struct CodexProbeResult {
    pub codex_roots: Vec<String>,
    pub jsonl_files: usize,
    pub session_json_files: usize,
    pub sqlite_paths: Vec<String>,
    pub detailed_records: usize,
    pub coarse_records: usize,
    pub sessions_with_detailed: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota: Option<alltokens_core::model::CodexQuotaSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_error: Option<String>,
}

pub struct CodexCollector {
    codex_roots: Vec<PathBuf>,
}

impl CodexCollector {
    pub fn new() -> Self {
        Self {
            codex_roots: find_codex_roots(),
        }
    }

    /// Override Codex data roots (for tests and probe tooling).
    #[doc(hidden)]
    pub fn with_roots(codex_roots: Vec<PathBuf>) -> Self {
        Self { codex_roots }
    }

    fn collect_inner(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        let mut records = Vec::new();
        let mut detailed_sessions = HashSet::new();

        for root in &self.codex_roots {
            let session_json = collect_session_json_files(root, since)?;
            for record in &session_json {
                if let Some(sid) = &record.session_id {
                    detailed_sessions.insert(sid.clone());
                }
            }
            records.extend(session_json);

            let jsonl_records = collect_rollout_jsonl(root, since)?;
            for record in &jsonl_records {
                if let Some(sid) = &record.session_id {
                    detailed_sessions.insert(sid.clone());
                }
            }
            records.extend(jsonl_records);
        }

        records.extend(collect_sqlite_fallback(
            &self.codex_roots,
            since,
            &detailed_sessions,
        )?);

        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(records)
    }

    /// Dry-run probe: list sources and record counts without persisting.
    pub fn probe(&self) -> Result<CodexProbeResult> {
        self.probe_with_quota(true)
    }

    /// Probe with optional live quota fetch from `codex app-server`.
    pub fn probe_with_quota(&self, include_quota: bool) -> Result<CodexProbeResult> {
        let records = self.collect_inner(None)?;
        let detailed = records
            .iter()
            .filter(|r| r.notes.as_deref() == Some(NOTE_DETAILED))
            .count();
        let coarse = records
            .iter()
            .filter(|r| r.notes.as_deref() == Some(NOTE_COARSE))
            .count();
        let sessions_with_detailed = records
            .iter()
            .filter(|r| r.notes.as_deref() == Some(NOTE_DETAILED))
            .filter_map(|r| r.session_id.clone())
            .collect::<HashSet<_>>()
            .len();

        let mut jsonl_files = 0usize;
        let mut session_json_files = 0usize;
        let mut sqlite_paths = Vec::new();

        for root in &self.codex_roots {
            jsonl_files += count_rollout_jsonl_files(root);
            session_json_files += count_session_json_files(root);
            let db = root.join("state_5.sqlite");
            if db.exists() {
                sqlite_paths.push(db.to_string_lossy().to_string());
            }
        }

        let (quota, quota_error) = if include_quota {
            match tokio::runtime::Runtime::new() {
                Ok(rt) => match rt.block_on(super::codex_quota::fetch_codex_quota()) {
                    Ok(snapshot) => (Some(snapshot), None),
                    Err(e) => (None, Some(e.to_string())),
                },
                Err(e) => (None, Some(e.to_string())),
            }
        } else {
            (None, None)
        };

        Ok(CodexProbeResult {
            codex_roots: self
                .codex_roots
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect(),
            jsonl_files,
            session_json_files,
            sqlite_paths,
            detailed_records: detailed,
            coarse_records: coarse,
            sessions_with_detailed,
            quota,
            quota_error,
        })
    }
}

fn find_codex_roots() -> Vec<PathBuf> {
    paths::find_paths(&[".codex"])
}

fn parse_timestamp(ts: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(ts)
        .or_else(|_| DateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.f%:z"))
        .or_else(|_| DateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.fZ"))
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn delta_snapshot(prev: &TokenUsageSnapshot, curr: &TokenUsageSnapshot) -> TokenUsageSnapshot {
    TokenUsageSnapshot {
        input_tokens: curr.input_tokens.saturating_sub(prev.input_tokens),
        cached_input_tokens: curr.cached_input_tokens.saturating_sub(prev.cached_input_tokens),
        output_tokens: curr.output_tokens.saturating_sub(prev.output_tokens),
        reasoning_output_tokens: curr
            .reasoning_output_tokens
            .saturating_sub(prev.reasoning_output_tokens),
        total_tokens: curr.total_tokens.saturating_sub(prev.total_tokens),
    }
}

fn snapshot_to_record(
    delta: &TokenUsageSnapshot,
    timestamp: DateTime<Utc>,
    model: &str,
    session_id: Option<String>,
    source_file: &Path,
    raw_json: Option<String>,
) -> Option<UsageRecord> {
    if delta.total_tokens == 0
        && delta.input_tokens == 0
        && delta.output_tokens == 0
        && delta.cached_input_tokens == 0
    {
        return None;
    }

    let uncached_input = delta.input_tokens.saturating_sub(delta.cached_input_tokens);
    let provider = Provider::from_url_and_model("", model);
    let total = if delta.total_tokens > 0 {
        delta.total_tokens
    } else {
        uncached_input + delta.cached_input_tokens + delta.output_tokens
    };

    Some(UsageRecord {
        id: None,
        timestamp,
        collector: "codex".to_string(),
        tool: Some("Codex".to_string()),
        provider: provider.name().to_string(),
        model: model.to_string(),
        input_tokens: uncached_input,
        output_tokens: delta.output_tokens,
        reasoning_tokens: delta.reasoning_output_tokens,
        cache_read_tokens: delta.cached_input_tokens,
        cache_creation_tokens: 0,
        total_tokens: total,
        cost_usd: 0.0,
        cost_cny: 0.0,
        latency_ms: None,
        is_stream: false,
        status_code: None,
        session_id,
        request_id: None,
        source_file: Some(source_file.to_string_lossy().to_string()),
        raw_json,
        notes: Some(NOTE_DETAILED.to_string()),
    })
}

/// Parse rollout / archived JSONL with cumulative total_token_usage delta algorithm.
pub(crate) fn parse_rollout_jsonl_content(
    content: &str,
    source_file: &Path,
    since: Option<DateTime<Utc>>,
) -> Vec<UsageRecord> {
    let mut records = Vec::new();
    let mut session_id: Option<String> = None;
    let mut model = String::new();
    let mut last_total = TokenUsageSnapshot::default();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let entry: RolloutLine = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let line_type = entry.line_type.as_deref().unwrap_or("");
        let payload = match &entry.payload {
            Some(p) => p,
            None => continue,
        };

        match line_type {
            "session_meta" => {
                if let Some(id) = &payload.id {
                    session_id = Some(id.clone());
                    last_total = TokenUsageSnapshot::default();
                }
            }
            "turn_context" => {
                if let Some(m) = &payload.model {
                    model = m.clone();
                }
            }
            "event_msg" if payload.payload_type.as_deref() == Some("token_count") => {
                let Some(info) = &payload.info else { continue };
                let Some(curr) = &info.total_token_usage else { continue };

                let timestamp = entry
                    .timestamp
                    .as_deref()
                    .and_then(parse_timestamp)
                    .unwrap_or_else(Utc::now);

                if let Some(since) = since {
                    if timestamp <= since {
                        last_total = curr.clone();
                        continue;
                    }
                }

                let delta = delta_snapshot(&last_total, curr);
                last_total = curr.clone();

                if let Some(record) = snapshot_to_record(
                    &delta,
                    timestamp,
                    &model,
                    session_id.clone(),
                    source_file,
                    Some(line.to_string()),
                ) {
                    records.push(record);
                }
            }
            _ => {
                let timestamp = entry
                    .timestamp
                    .as_deref()
                    .and_then(parse_timestamp)
                    .unwrap_or_else(Utc::now);
                records.extend(invocation_records_from_line(
                    line,
                    timestamp,
                    session_id.clone(),
                    source_file,
                    since,
                ));
            }
        }
    }

    records
}

fn invocation_records_from_line(
    line: &str,
    timestamp: DateTime<Utc>,
    session_id: Option<String>,
    source_file: &Path,
    since: Option<DateTime<Utc>>,
) -> Vec<UsageRecord> {
    if let Some(since) = since {
        if timestamp <= since {
            return Vec::new();
        }
    }

    let mut records = Vec::new();
    for name in alltokens_core::invocation::extract_tool_names_from_json(line) {
        records.push(UsageRecord {
            id: None,
            timestamp,
            collector: "codex".to_string(),
            tool: Some(name),
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
            session_id: session_id.clone(),
            request_id: None,
            source_file: Some(source_file.to_string_lossy().to_string()),
            raw_json: Some(line.to_string()),
            notes: Some(alltokens_core::invocation::NOTE_INVOCATION_TOOL.to_string()),
        });
    }
    for name in alltokens_core::invocation::extract_skill_names_from_json(line) {
        records.push(UsageRecord {
            id: None,
            timestamp,
            collector: "codex".to_string(),
            tool: Some(name),
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
            session_id: session_id.clone(),
            request_id: None,
            source_file: Some(source_file.to_string_lossy().to_string()),
            raw_json: Some(line.to_string()),
            notes: Some(alltokens_core::invocation::NOTE_INVOCATION_SKILL.to_string()),
        });
    }
    records
}

fn is_rollout_jsonl(path: &Path) -> bool {
    if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
        return false;
    }
    let path_str = path.to_string_lossy();
    if path_str.contains("archived_sessions") {
        return true;
    }
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with("rollout-"))
        .unwrap_or(false)
}

fn collect_rollout_jsonl(root: &Path, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
    let mut records = Vec::new();
    let sessions_dir = root.join("sessions");
    let archived_dir = root.join("archived_sessions");

    for base in [&sessions_dir, &archived_dir] {
        if !base.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(base)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| is_rollout_jsonl(e.path()))
        {
            let path = entry.path();
            let Ok(content) = std::fs::read_to_string(path) else {
                continue;
            };
            records.extend(parse_rollout_jsonl_content(&content, path, since));
        }
    }
    Ok(records)
}

fn count_rollout_jsonl_files(root: &Path) -> usize {
    let mut count = 0;
    for sub in ["sessions", "archived_sessions"] {
        let base = root.join(sub);
        if !base.exists() {
            continue;
        }
        count += walkdir::WalkDir::new(&base)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| is_rollout_jsonl(e.path()))
            .count();
    }
    count
}

fn collect_session_json_files(
    root: &Path,
    since: Option<DateTime<Utc>>,
) -> Result<Vec<UsageRecord>> {
    let sessions_dir = root.join("sessions");
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    for entry in walkdir::WalkDir::new(&sessions_dir)
        .max_depth(2)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().and_then(|ext| ext.to_str()) == Some("json")
                && !e
                    .path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("rollout-"))
                    .unwrap_or(false)
        })
    {
        let path = entry.path();
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let session: CodexSession = match serde_json::from_str(&content) {
            Ok(s) => s,
            Err(_) => continue,
        };
        for item in &session.items {
            let Some(usage) = &item.usage else { continue };
            let timestamp = item
                .timestamp
                .as_ref()
                .and_then(|ts| parse_timestamp(ts))
                .unwrap_or_else(Utc::now);
            if let Some(since) = since {
                if timestamp <= since {
                    continue;
                }
            }

            let model = item.model.clone().unwrap_or_default();
            let provider = Provider::from_url_and_model("", &model);
            let input = usage.input_tokens.unwrap_or(0);
            let output = usage.output_tokens.unwrap_or(0);
            let cache_read = usage
                .input_details
                .as_ref()
                .and_then(|d| d.cached_tokens)
                .unwrap_or(0);

            records.push(UsageRecord {
                id: None,
                timestamp,
                collector: "codex".to_string(),
                tool: Some("Codex".to_string()),
                provider: provider.name().to_string(),
                model,
                input_tokens: input,
                output_tokens: output,
                reasoning_tokens: 0,
                cache_read_tokens: cache_read,
                cache_creation_tokens: 0,
                total_tokens: usage
                    .total_tokens
                    .unwrap_or(input + output + cache_read),
                cost_usd: 0.0,
                cost_cny: 0.0,
                latency_ms: None,
                is_stream: false,
                status_code: None,
                session_id: session.session_id.clone(),
                request_id: None,
                source_file: Some(path.to_string_lossy().to_string()),
                raw_json: serde_json::to_string(item).ok(),
                notes: Some(NOTE_DETAILED.to_string()),
            });
        }
    }
    Ok(records)
}

fn count_session_json_files(root: &Path) -> usize {
    let sessions_dir = root.join("sessions");
    if !sessions_dir.exists() {
        return 0;
    }
    walkdir::WalkDir::new(&sessions_dir)
        .max_depth(2)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().and_then(|ext| ext.to_str()) == Some("json")
                && !e
                    .path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("rollout-"))
                    .unwrap_or(false)
        })
        .count()
}

fn read_sqlite_threads(db_path: &Path) -> Result<Vec<SqliteThread>> {
    let conn = Connection::open(db_path)?;
    let mut threads = Vec::new();

    // Schema may vary; try common column names.
    let sql = "SELECT id, tokens_used, updated_at, model FROM threads WHERE tokens_used > 0";
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => {
            // Fallback without model column
            let sql = "SELECT id, tokens_used, updated_at FROM threads WHERE tokens_used > 0";
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt.query_map([], |row| {
                Ok(SqliteThread {
                    id: row.get(0)?,
                    tokens_used: row.get::<_, i64>(1)? as u64,
                    updated_at: row.get(2).ok(),
                    model: None,
                })
            })?;
            for row in rows.flatten() {
                threads.push(row);
            }
            return Ok(threads);
        }
    };

    let rows = stmt.query_map([], |row| {
        Ok(SqliteThread {
            id: row.get(0)?,
            tokens_used: row.get::<_, i64>(1)? as u64,
            updated_at: row.get(2).ok(),
            model: row.get(3).ok(),
        })
    })?;

    for row in rows.flatten() {
        threads.push(row);
    }
    Ok(threads)
}

fn collect_sqlite_fallback(
    roots: &[PathBuf],
    since: Option<DateTime<Utc>>,
    detailed_sessions: &HashSet<String>,
) -> Result<Vec<UsageRecord>> {
    let mut records = Vec::new();
    let mut seen_threads: HashMap<String, bool> = HashMap::new();

    for root in roots {
        let db_path = root.join("state_5.sqlite");
        if !db_path.exists() {
            continue;
        }

        let threads = match read_sqlite_threads(&db_path) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("Failed to read {}: {e}", db_path.display());
                continue;
            }
        };

        for thread in threads {
            if detailed_sessions.contains(&thread.id) {
                continue;
            }
            if seen_threads.contains_key(&thread.id) {
                continue;
            }
            seen_threads.insert(thread.id.clone(), true);

            let timestamp = thread
                .updated_at
                .as_deref()
                .and_then(parse_timestamp)
                .unwrap_or_else(Utc::now);

            if let Some(since) = since {
                if timestamp <= since {
                    continue;
                }
            }

            let model = thread.model.unwrap_or_default();
            let provider = Provider::from_url_and_model("", &model);

            records.push(UsageRecord {
                id: None,
                timestamp,
                collector: "codex".to_string(),
                tool: Some("Codex".to_string()),
                provider: provider.name().to_string(),
                model,
                input_tokens: thread.tokens_used,
                output_tokens: 0,
                reasoning_tokens: 0,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                total_tokens: thread.tokens_used,
                cost_usd: 0.0,
                cost_cny: 0.0,
                latency_ms: None,
                is_stream: false,
                status_code: None,
                session_id: Some(thread.id),
                request_id: None,
                source_file: Some(db_path.to_string_lossy().to_string()),
                raw_json: None,
                notes: Some(NOTE_COARSE.to_string()),
            });
        }
    }

    Ok(records)
}

#[async_trait]
impl Collector for CodexCollector {
    fn id(&self) -> &str {
        "codex"
    }
    fn name(&self) -> &str {
        "Codex"
    }
    fn is_available(&self) -> bool {
        !self.codex_roots.is_empty()
    }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        self.collect_inner(since)
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        self.codex_roots.clone()
    }
}

#[async_trait]
impl super::QuotaProvider for CodexCollector {
    fn provider_id(&self) -> &str {
        "codex"
    }

    async fn fetch_quota(&self) -> Result<Option<alltokens_core::model::CodexQuotaSnapshot>> {
        match super::codex_quota::fetch_codex_quota().await {
            Ok(snapshot) => Ok(Some(snapshot)),
            Err(e) => {
                tracing::warn!("Codex quota fetch failed: {e}");
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_snapshot_avoids_double_counting_cumulative_totals() {
        let prev = TokenUsageSnapshot {
            input_tokens: 1000,
            cached_input_tokens: 200,
            output_tokens: 500,
            reasoning_output_tokens: 100,
            total_tokens: 1500,
        };
        let curr = TokenUsageSnapshot {
            input_tokens: 1500,
            cached_input_tokens: 400,
            output_tokens: 800,
            reasoning_output_tokens: 150,
            total_tokens: 2300,
        };
        let delta = delta_snapshot(&prev, &curr);
        assert_eq!(delta.input_tokens, 500);
        assert_eq!(delta.cached_input_tokens, 200);
        assert_eq!(delta.output_tokens, 300);
        assert_eq!(delta.total_tokens, 800);
    }

    #[test]
    fn parse_rollout_jsonl_computes_per_event_deltas() {
        let content = r#"{"type":"session_meta","payload":{"id":"sess-abc","timestamp":"2026-07-10T10:00:00Z"}}
{"type":"turn_context","payload":{"model":"gpt-4o"}}
{"timestamp":"2026-07-10T10:01:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":200,"output_tokens":500,"total_tokens":1500}}}}
{"timestamp":"2026-07-10T10:02:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1500,"cached_input_tokens":400,"output_tokens":800,"total_tokens":2300}}}}"#;

        let records = parse_rollout_jsonl_content(content, Path::new("rollout-test.jsonl"), None);
        assert_eq!(records.len(), 2);

        assert_eq!(records[0].input_tokens, 800); // 1000 - 200 cached
        assert_eq!(records[0].cache_read_tokens, 200);
        assert_eq!(records[0].output_tokens, 500);
        assert_eq!(records[0].total_tokens, 1500);
        assert_eq!(records[0].model, "gpt-4o");
        assert_eq!(records[0].session_id.as_deref(), Some("sess-abc"));
        assert_eq!(records[0].notes.as_deref(), Some(NOTE_DETAILED));

        assert_eq!(records[1].input_tokens, 300); // delta input 500 - cached delta 200
        assert_eq!(records[1].cache_read_tokens, 200);
        assert_eq!(records[1].output_tokens, 300);
        assert_eq!(records[1].total_tokens, 800);
    }

    #[test]
    fn parse_rollout_skips_zero_deltas() {
        let content = r#"{"type":"session_meta","payload":{"id":"sess-dup"}}
{"timestamp":"2026-07-10T10:01:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":500,"output_tokens":200,"total_tokens":700}}}}
{"timestamp":"2026-07-10T10:01:05Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":500,"output_tokens":200,"total_tokens":700}}}}"#;

        let records = parse_rollout_jsonl_content(content, Path::new("rollout-dup.jsonl"), None);
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn parse_rollout_extracts_tool_invocations() {
        let content = r#"{"type":"session_meta","payload":{"id":"sess-tools"}}
{"timestamp":"2026-07-10T10:01:00Z","type":"event_msg","payload":{"type":"function_call","name":"shell","input":{}}}
{"timestamp":"2026-07-10T10:02:00Z","type":"event_msg","payload":{"type":"function_call","name":"shell","input":{}}}"#;

        let records = parse_rollout_jsonl_content(content, Path::new("rollout-tools.jsonl"), None);
        let invocations: Vec<_> = records
            .iter()
            .filter(|r| r.notes.as_deref() == Some(alltokens_core::invocation::NOTE_INVOCATION_TOOL))
            .collect();
        assert_eq!(invocations.len(), 2);
        assert!(invocations.iter().all(|r| r.tool.as_deref() == Some("shell")));
    }

    #[tokio::test]
    async fn collect_from_fixture_tree() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex");
        let collector = CodexCollector::with_roots(vec![root]);
        let records = collector.collect(None).await.unwrap();

        let detailed: Vec<_> = records
            .iter()
            .filter(|r| r.notes.as_deref() == Some(NOTE_DETAILED))
            .collect();
        assert!(!detailed.is_empty());
        assert!(detailed.iter().any(|r| r.model == "gpt-4o"));
    }

    #[test]
    fn sqlite_fallback_marks_coarse_quality() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state_5.sqlite");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, tokens_used INTEGER, updated_at TEXT, model TEXT);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO threads (id, tokens_used, updated_at, model) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                "thread-coarse-1",
                4200_i64,
                "2026-07-10T12:00:00Z",
                "gpt-4o-mini"
            ],
        )
        .unwrap();

        let records = collect_sqlite_fallback(
            &[dir.path().to_path_buf()],
            None,
            &HashSet::new(),
        )
        .unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].total_tokens, 4200);
        assert_eq!(records[0].notes.as_deref(), Some(NOTE_COARSE));
        assert_eq!(records[0].session_id.as_deref(), Some("thread-coarse-1"));
    }
}
