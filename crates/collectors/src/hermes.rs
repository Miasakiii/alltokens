use alltokens_core::model::{Provider, UsageRecord};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use super::paths;
use super::Collector;

/// Hermes Agent 使用数据
/// 路径:
///   - $HERMES_HOME/state.db (SQLite)
///   - ~/.hermes/state.db
///   - WSL: /mnt/c/Users/<user>/.hermes/state.db
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct HermesUsage {
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
    #[serde(rename = "session_id", default)]
    session_id: Option<String>,
}

pub struct HermesCollector {
    data_paths: Vec<PathBuf>,
}

impl HermesCollector {
    pub fn new() -> Self {
        let mut data_paths = Vec::new();

        // 环境变量
        if let Ok(hermes_home) = std::env::var("HERMES_HOME") {
            let db = PathBuf::from(hermes_home).join("state.db");
            if db.exists() { data_paths.push(db); }
        }

        // 标准路径 + WSL 路径
        let candidates = [
            ".hermes/state.db",
            ".hermes/usage.db",
            ".config/hermes/state.db",
            ".local/share/hermes/state.db",
        ];
        data_paths.extend(paths::find_paths(&candidates));

        Self { data_paths }
    }

    fn parse_db(&self, db_path: &Path, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        use rusqlite::Connection;
        let conn = Connection::open(db_path)?;
        let mut records = Vec::new();

        // 尝试多种表名
        for table in &["usage", "requests", "api_calls", "messages", "conversations"] {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get::<_, i64>(0),
                )
                .map(|c| c > 0)
                .unwrap_or(false);
            if !exists { continue; }

            let mut stmt = match conn.prepare(&format!("SELECT * FROM {table} ORDER BY rowid DESC LIMIT 5000")) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

            let rows = stmt.query_map([], |row| {
                let mut map = serde_json::Map::new();
                for (i, name) in col_names.iter().enumerate() {
                    let val: serde_json::Value = match row.get_ref(i)? {
                        rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                        rusqlite::types::ValueRef::Integer(v) => serde_json::Value::Number(v.into()),
                        rusqlite::types::ValueRef::Real(f) => serde_json::Value::Number(
                            serde_json::Number::from_f64(f).unwrap_or(0.into()),
                        ),
                        rusqlite::types::ValueRef::Text(t) => {
                            serde_json::Value::String(String::from_utf8_lossy(t).to_string())
                        }
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

        let model = find_str(obj, &["model", "model_name"]).unwrap_or_default();
        if model.is_empty() { return None; }
        let provider = Provider::from_url_and_model("", &model);
        let input = find_u64(obj, &["input_tokens", "prompt_tokens"]).unwrap_or(0);
        let output = find_u64(obj, &["output_tokens", "completion_tokens"]).unwrap_or(0);
        let cache_read = find_u64(obj, &["cache_read_tokens", "cache_reads"]).unwrap_or(0);
        let cache_creation = find_u64(obj, &["cache_creation_tokens", "cache_writes"]).unwrap_or(0);

        Some(UsageRecord {
            id: None, timestamp, collector: "hermes".to_string(), tool: Some("Hermes".to_string()),
            provider: provider.name().to_string(), model,
            input_tokens: input, output_tokens: output, reasoning_tokens: 0, cache_read_tokens: cache_read, cache_creation_tokens: cache_creation,
            total_tokens: find_u64(obj, &["total_tokens"]).unwrap_or(input + output + cache_read + cache_creation),
            cost_usd: find_f64(obj, &["cost", "total_cost"]).unwrap_or(0.0),
            cost_cny: 0.0, latency_ms: None, is_stream: false, status_code: None,
            session_id: find_str(obj, &["session_id"]),
            request_id: find_str(obj, &["request_id"]),
            source_file: Some(path.to_string_lossy().to_string()), raw_json: serde_json::to_string(val).ok(), notes: None,
        })
    }
}

#[async_trait]
impl Collector for HermesCollector {
    fn id(&self) -> &str { "hermes" }
    fn name(&self) -> &str { "Hermes" }
    fn is_available(&self) -> bool { !self.data_paths.is_empty() }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        let mut records = Vec::new();
        for db_path in &self.data_paths {
            if db_path.exists() {
                match self.parse_db(db_path, since) {
                    Ok(r) => records.extend(r),
                    Err(e) => tracing::warn!("Failed to parse {}: {e}", db_path.display()),
                }
            }
        }
        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(records)
    }

    fn watch_paths(&self) -> Vec<PathBuf> { self.data_paths.clone() }
}

fn find_str(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(v) = obj.get(*key) {
            if let Some(s) = v.as_str() { if !s.is_empty() { return Some(s.to_string()); } }
        }
    }
    None
}
fn find_u64(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Some(v) = obj.get(*key) { if let Some(n) = v.as_u64() { return Some(n); } }
    }
    None
}
fn find_f64(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Some(v) = obj.get(*key) { if let Some(f) = v.as_f64() { return Some(f); } }
    }
    None
}
