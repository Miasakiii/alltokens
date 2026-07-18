use alltokens_core::model::{Provider, UsageRecord};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use super::paths;
use super::Collector;

/// OpenCode 使用数据
/// 路径:
///   - ~/.local/share/opencode/ (session JSON/SQLite)
///   - ~/.config/opencode/
///   - WSL: /mnt/c/Users/<user>/.local/share/opencode/
///
/// OpenCode 的 session 文件通常是 JSONL 格式
#[derive(Debug, Deserialize)]
struct OpenCodeEntry {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(rename = "inputTokens", default)]
    input_tokens: Option<u64>,
    #[serde(rename = "outputTokens", default)]
    output_tokens: Option<u64>,
    #[serde(rename = "prompt_tokens", default)]
    prompt_tokens: Option<u64>,
    #[serde(rename = "completion_tokens", default)]
    completion_tokens: Option<u64>,
    #[serde(rename = "cacheReadInputTokens", default)]
    cache_read_tokens: Option<u64>,
    #[serde(rename = "cache_read_tokens", default)]
    cache_read_tokens_alt: Option<u64>,
    #[serde(rename = "totalTokens", default)]
    total_tokens: Option<u64>,
    #[serde(rename = "total_tokens", default)]
    total_tokens_alt: Option<u64>,
    #[serde(rename = "costUSD", default)]
    cost_usd: Option<f64>,
    #[serde(default)]
    cost: Option<f64>,
    #[serde(rename = "sessionId", default)]
    session_id: Option<String>,
    #[serde(rename = "durationMs", default)]
    duration_ms: Option<u64>,
    #[serde(rename = "isStreaming", default)]
    is_streaming: Option<bool>,
}

pub struct OpenCodeCollector {
    data_paths: Vec<PathBuf>,
}

impl OpenCodeCollector {
    pub fn new() -> Self {
        let candidates = [
            ".local/share/opencode",
            ".config/opencode",
            ".opencode",
            "Library/Application Support/opencode",
        ];
        let data_paths = paths::find_paths(&candidates);
        Self { data_paths }
    }

    /// Override data paths (for tests and probe tooling).
    #[doc(hidden)]
    pub fn with_paths(data_paths: Vec<PathBuf>) -> Self {
        Self { data_paths }
    }

    fn count_data_files(&self) -> usize {
        let mut count = 0usize;
        for dir in &self.data_paths {
            for entry in walkdir::WalkDir::new(dir)
                .max_depth(3)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let ext = entry
                    .path()
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                if ext == "json" || ext == "jsonl" || ext == "db" {
                    count += 1;
                }
            }
        }
        count
    }

    fn parse_dir(&self, dir: &Path, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        let mut records = Vec::new();
        for entry in walkdir::WalkDir::new(dir).max_depth(3).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "json" && ext != "jsonl" && ext != "db" { continue; }

            if ext == "db" {
                if let Ok(r) = self.parse_db(path, since) { records.extend(r); }
                continue;
            }

            let Ok(content) = std::fs::read_to_string(path) else { continue };
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() { continue; }
                if let Some(record) = self.parse_line(line, path, since) {
                    records.push(record);
                }
            }
        }
        Ok(records)
    }

    /// 解析 OpenCode `message` 表：assistant 消息的 `data` JSON 含完整
    /// tokens/cost/model/time（epoch 毫秒）。
    fn parse_message_table(
        &self,
        conn: &rusqlite::Connection,
        db_path: &Path,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<UsageRecord>> {
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='message'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);
        if !exists {
            return Ok(Vec::new());
        }

        // 首次全量导入按页拉取，避免 LIMIT 截断丢掉更老的历史消息。
        let page_size = 5000i64;
        let mut offset = 0i64;
        let mut records = Vec::new();
        loop {
            let mut stmt = conn.prepare(&format!(
                "SELECT id, session_id, time_created, data FROM message
                 ORDER BY time_created DESC LIMIT {page_size} OFFSET {offset}"
            ))?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?;

            let mut page_count = 0usize;
            for row in rows.flatten() {
                page_count += 1;
                let (id, session_id, time_created, data) = row;
                if let Some(r) = message_row_to_record(
                    &id,
                    session_id.as_deref(),
                    time_created,
                    &data,
                    db_path,
                    since,
                ) {
                    records.push(r);
                }
            }

            if page_count < page_size as usize || since.is_some() {
                break;
            }
            offset += page_size;
        }
        Ok(records)
    }

    fn parse_line(&self, line: &str, path: &Path, since: Option<DateTime<Utc>>) -> Option<UsageRecord> {
        let entry: OpenCodeEntry = serde_json::from_str(line).ok()?;
        let timestamp = entry.timestamp.as_ref()
            .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);
        if let Some(since) = since { if timestamp <= since { return None; } }

        let model = entry.model.unwrap_or_default();
        if model.is_empty() { return None; }
        let provider = Provider::from_url_and_model("", &model);
        let input = entry.input_tokens.or(entry.prompt_tokens).unwrap_or(0);
        let output = entry.output_tokens.or(entry.completion_tokens).unwrap_or(0);
        let cache_read = entry.cache_read_tokens.or(entry.cache_read_tokens_alt).unwrap_or(0);
        let total = entry.total_tokens.or(entry.total_tokens_alt);

        Some(UsageRecord {
            id: None, timestamp, collector: "opencode".to_string(), tool: Some("OpenCode".to_string()),
            provider: provider.name().to_string(), model,
            input_tokens: input, output_tokens: output, reasoning_tokens: 0, cache_read_tokens: cache_read, cache_creation_tokens: 0,
            total_tokens: total.unwrap_or(input + output + cache_read),
            cost_usd: entry.cost_usd.or(entry.cost).unwrap_or(0.0),
            cost_cny: 0.0, latency_ms: entry.duration_ms,
            is_stream: entry.is_streaming.unwrap_or(false), status_code: None,
            session_id: entry.session_id, request_id: None,
            source_file: Some(path.to_string_lossy().to_string()), raw_json: Some(line.to_string()), notes: None,
        })
    }

    fn parse_db(&self, db_path: &Path, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        use rusqlite::Connection;
        let conn = Connection::open(db_path)?;
        let mut records = Vec::new();

        // OpenCode ≥ 0.x 把聊天消息存在 `message` 表（JSON blob 在 data 列，
        // assistant 消息带 modelID/providerID/tokens/cost/time）。
        if let Ok(r) = self.parse_message_table(&conn, db_path, since) {
            records.extend(r);
        }

        for table in &["usage", "requests", "sessions", "messages"] {
            let exists: bool = conn
                .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1", [table], |row| row.get::<_, i64>(0))
                .map(|c| c > 0).unwrap_or(false);
            if !exists { continue; }

            let mut stmt = match conn.prepare(&format!("SELECT * FROM {table} ORDER BY rowid DESC LIMIT 5000")) {
                Ok(s) => s, Err(_) => continue,
            };
            let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
            let rows = stmt.query_map([], |row| {
                let mut map = serde_json::Map::new();
                for (i, name) in col_names.iter().enumerate() {
                    let val: serde_json::Value = match row.get_ref(i)? {
                        rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                        rusqlite::types::ValueRef::Integer(v) => serde_json::Value::Number(v.into()),
                        rusqlite::types::ValueRef::Real(f) => serde_json::Value::Number(serde_json::Number::from_f64(f).unwrap_or(0.into())),
                        rusqlite::types::ValueRef::Text(t) => serde_json::Value::String(String::from_utf8_lossy(t).to_string()),
                        rusqlite::types::ValueRef::Blob(_) => serde_json::Value::Null,
                    };
                    map.insert(name.clone(), val);
                }
                Ok(serde_json::Value::Object(map))
            })?;

            for row in rows.flatten() {
                if let Some(record) = self.json_to_record(&row, db_path, since) {
                    records.push(record);
                }
            }
        }
        Ok(records)
    }

    fn json_to_record(&self, val: &serde_json::Value, path: &Path, since: Option<DateTime<Utc>>) -> Option<UsageRecord> {
        let obj = val.as_object()?;
        let timestamp = find_str(obj, &["timestamp", "created_at", "time"])
            .and_then(|ts| DateTime::parse_from_rfc3339(&ts).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);
        if let Some(since) = since { if timestamp <= since { return None; } }

        let model = find_str(obj, &["model"]).unwrap_or_default();
        if model.is_empty() { return None; }
        let provider = Provider::from_url_and_model("", &model);
        let input = find_u64(obj, &["input_tokens", "inputTokens", "prompt_tokens"]).unwrap_or(0);
        let output = find_u64(obj, &["output_tokens", "outputTokens", "completion_tokens"]).unwrap_or(0);
        let cache_read = find_u64(obj, &["cache_read_tokens", "cacheReadInputTokens"]).unwrap_or(0);

        Some(UsageRecord {
            id: None, timestamp, collector: "opencode".to_string(), tool: Some("OpenCode".to_string()),
            provider: provider.name().to_string(), model,
            input_tokens: input, output_tokens: output, reasoning_tokens: 0, cache_read_tokens: cache_read, cache_creation_tokens: 0,
            total_tokens: find_u64(obj, &["total_tokens", "totalTokens"]).unwrap_or(input + output + cache_read),
            cost_usd: find_f64(obj, &["cost_usd", "cost", "costUSD"]).unwrap_or(0.0),
            cost_cny: 0.0, latency_ms: find_u64(obj, &["duration_ms", "durationMs"]),
            is_stream: find_bool(obj, &["is_stream", "isStreaming"]).unwrap_or(false), status_code: None,
            session_id: find_str(obj, &["session_id", "sessionId"]),
            request_id: find_str(obj, &["request_id", "requestId"]),
            source_file: Some(path.to_string_lossy().to_string()), raw_json: serde_json::to_string(val).ok(), notes: None,
        })
    }
}

#[async_trait]
impl Collector for OpenCodeCollector {
    fn id(&self) -> &str { "opencode" }
    fn name(&self) -> &str { "OpenCode" }
    fn is_available(&self) -> bool { !self.data_paths.is_empty() }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        let mut records = Vec::new();
        for dir in &self.data_paths {
            match self.parse_dir(dir, since) {
                Ok(r) => records.extend(r),
                Err(e) => tracing::warn!("Failed to parse {}: {e}", dir.display()),
            }
        }
        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(records)
    }

    fn watch_paths(&self) -> Vec<PathBuf> { self.data_paths.clone() }
}

impl OpenCodeCollector {
    /// Dry-run probe: list data paths, file counts, and sample record count.
    pub fn probe(&self) -> Result<super::probe::BasicProbeResult> {
        let data_paths: Vec<String> = self
            .data_paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        let sample_records = super::probe::collect_sample_count(self);
        Ok(super::probe::build_basic_probe_result(
            "opencode",
            "OpenCode",
            self.is_available(),
            data_paths,
            self.count_data_files(),
            sample_records,
        ))
    }
}

/// 将 OpenCode `message` 表的一行（assistant 消息）转为 UsageRecord。
/// data JSON 真实形态：
/// `{ "role":"assistant", "modelID":"MiniMax-M3", "providerID":"minimax",
///    "tokens":{"total":56539,"input":10693,"output":2796,"reasoning":0,
///              "cache":{"write":0,"read":43050}},
///    "cost":0.009, "time":{"created":1780402916045,"completed":1780403037339} }`
fn message_row_to_record(
    id: &str,
    session_id: Option<&str>,
    time_created_ms: i64,
    data: &str,
    path: &Path,
    since: Option<DateTime<Utc>>,
) -> Option<UsageRecord> {
    let val: serde_json::Value = serde_json::from_str(data).ok()?;
    let obj = val.as_object()?;
    if obj.get("role").and_then(|v| v.as_str()) != Some("assistant") {
        return None;
    }
    let tokens = obj.get("tokens")?.as_object()?;
    let input = tokens.get("input").and_then(|v| v.as_u64()).unwrap_or(0);
    let output = tokens.get("output").and_then(|v| v.as_u64()).unwrap_or(0);
    let reasoning = tokens.get("reasoning").and_then(|v| v.as_u64()).unwrap_or(0);
    let (cache_read, cache_creation) = tokens
        .get("cache")
        .and_then(|c| c.as_object())
        .map(|c| {
            (
                c.get("read").and_then(|v| v.as_u64()).unwrap_or(0),
                c.get("write").and_then(|v| v.as_u64()).unwrap_or(0),
            )
        })
        .unwrap_or((0, 0));
    if input == 0 && output == 0 && cache_read == 0 {
        return None;
    }

    let model = obj
        .get("modelID")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if model.is_empty() {
        return None;
    }
    let provider = obj
        .get("providerID")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Provider::from_url_and_model("", &model).name().to_string());

    // time.created（epoch 毫秒）优先，缺失时回退到行级 time_created。
    let created_ms = obj
        .get("time")
        .and_then(|t| t.get("created"))
        .and_then(|v| v.as_i64())
        .unwrap_or(time_created_ms);
    let timestamp = DateTime::from_timestamp_millis(created_ms).unwrap_or_else(Utc::now);
    if let Some(since) = since {
        if timestamp <= since {
            return None;
        }
    }

    let latency_ms = obj.get("time").and_then(|t| {
        let completed = t.get("completed").and_then(|v| v.as_i64())?;
        let created = t.get("created").and_then(|v| v.as_i64())?;
        Some((completed - created).max(0) as u64)
    });

    Some(UsageRecord {
        id: None,
        timestamp,
        collector: "opencode".to_string(),
        tool: Some("OpenCode".to_string()),
        provider,
        model,
        input_tokens: input,
        output_tokens: output,
        reasoning_tokens: reasoning,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_creation,
        total_tokens: tokens
            .get("total")
            .and_then(|v| v.as_u64())
            .unwrap_or(input + output + reasoning + cache_read + cache_creation),
        cost_usd: obj.get("cost").and_then(|v| v.as_f64()).unwrap_or(0.0),
        cost_cny: 0.0,
        latency_ms,
        is_stream: false,
        status_code: None,
        session_id: session_id
            .map(|s| s.to_string())
            .or_else(|| obj.get("sessionID").and_then(|v| v.as_str()).map(String::from)),
        request_id: Some(id.to_string()),
        source_file: Some(path.to_string_lossy().to_string()),
        raw_json: Some(data.to_string()),
        notes: None,
    })
}

fn find_str(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(v) = obj.get(*key) { if let Some(s) = v.as_str() { if !s.is_empty() { return Some(s.to_string()); } } }
    }
    None
}
fn find_u64(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<u64> {
    for key in keys { if let Some(v) = obj.get(*key) { if let Some(n) = v.as_u64() { return Some(n); } } }
    None
}
fn find_f64(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<f64> {
    for key in keys { if let Some(v) = obj.get(*key) { if let Some(f) = v.as_f64() { return Some(f); } } }
    None
}
fn find_bool(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<bool> {
    for key in keys { if let Some(v) = obj.get(*key) { if let Some(b) = v.as_bool() { return Some(b); } } }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// OpenCode ≥ 0.x 真实形态：`message` 表（id/session_id/time_created
    /// epoch 毫秒 + data JSON blob），assistant 消息带 tokens/cost/model。
    #[tokio::test]
    async fn collects_from_message_table_real_shape() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE message (
                id TEXT, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT
            );",
        )
        .unwrap();
        let assistant = r#"{"parentID":"msg_x","role":"assistant","mode":"build","agent":"build","cost":0.0091461,"tokens":{"total":56539,"input":10693,"output":2796,"reasoning":12,"cache":{"write":7,"read":43050}},"modelID":"MiniMax-M3","providerID":"minimax","time":{"created":1780402916045,"completed":1780403037339},"finish":"stop"}"#;
        let user = r#"{"role":"user","time":{"created":1780402916039},"agent":"build"}"#;
        conn.execute(
            "INSERT INTO message VALUES ('msg_a1','sess-1',1780402916045,1780403037339,?1)",
            [assistant],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message VALUES ('msg_u1','sess-1',1780402916039,1780402916039,?1)",
            [user],
        )
        .unwrap();
        drop(conn);

        let collector = OpenCodeCollector::with_paths(vec![dir.path().to_path_buf()]);
        let records = collector.collect(None).await.unwrap();
        assert_eq!(records.len(), 1, "只有 assistant 消息产生记录");

        let r = &records[0];
        assert_eq!(r.model, "MiniMax-M3");
        assert_eq!(r.provider, "minimax");
        assert_eq!(r.input_tokens, 10693);
        assert_eq!(r.output_tokens, 2796);
        assert_eq!(r.reasoning_tokens, 12);
        assert_eq!(r.cache_read_tokens, 43050);
        assert_eq!(r.cache_creation_tokens, 7);
        assert_eq!(r.total_tokens, 56539);
        assert!((r.cost_usd - 0.0091461).abs() < 1e-9);
        assert_eq!(r.latency_ms, Some(1_780_403_037_339u64 - 1_780_402_916_045u64));
        assert_eq!(r.session_id.as_deref(), Some("sess-1"));
        assert_eq!(r.request_id.as_deref(), Some("msg_a1"));
        assert_eq!(
            r.timestamp,
            DateTime::from_timestamp_millis(1_780_402_916_045).unwrap()
        );

        // since 过滤（epoch 毫秒）
        let since = DateTime::from_timestamp_millis(1_780_402_916_046).unwrap();
        assert!(collector.collect(Some(since)).await.unwrap().is_empty());
    }
}
