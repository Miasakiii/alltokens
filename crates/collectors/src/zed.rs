use alltokens_core::model::{Provider, UsageRecord};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

use super::Collector;

/// Zed 编辑器 thread 数据
/// 路径: ~/.local/share/zed/threads/threads.db (SQLite)
pub struct ZedCollector {
    db_path: Option<PathBuf>,
}

impl ZedCollector {
    pub fn new() -> Self {
        let db_path = if let Some(home) = dirs::home_dir() {
            let path = home.join(".local").join("share").join("zed").join("threads").join("threads.db");
            if path.exists() {
                Some(path)
            } else {
                // macOS
                let mac_path = home
                    .join("Library")
                    .join("Application Support")
                    .join("Zed")
                    .join("threads")
                    .join("threads.db");
                if mac_path.exists() {
                    Some(mac_path)
                } else {
                    None
                }
            }
        } else {
            None
        };

        Self { db_path }
    }

    fn query_usage(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        let db_path = match &self.db_path {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };

        let conn = Connection::open(db_path)?;
        let mut records = Vec::new();

        // 尝试查询 threads 表中的 usage 数据
        // Zed 的 DB schema 可能随版本变化，我们做自适应查询
        let tables = get_table_names(&conn)?;

        for table in &tables {
            if !table.contains("thread") && !table.contains("message") && !table.contains("usage") {
                continue;
            }

            let columns = get_column_names(&conn, table)?;

            // 检查是否有 token 相关列
            let has_tokens = columns.iter().any(|c| c.contains("token") || c.contains("usage"));
            if !has_tokens {
                continue;
            }

            let sql = format!("SELECT * FROM {table} ORDER BY rowid DESC LIMIT 1000");
            let mut stmt = match conn.prepare(&sql) {
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
                        rusqlite::types::ValueRef::Real(f) => {
                            serde_json::Value::Number(serde_json::Number::from_f64(f).unwrap_or(0.into()))
                        }
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

    fn json_to_record(
        &self,
        val: &serde_json::Value,
        path: &Path,
        since: Option<DateTime<Utc>>,
    ) -> Option<UsageRecord> {
        let obj = val.as_object()?;

        let timestamp = obj
            .get("created_at")
            .or_else(|| obj.get("timestamp"))
            .or_else(|| obj.get("time"))
            .and_then(|v| v.as_str())
            .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        if let Some(since) = since {
            if timestamp <= since {
                return None;
            }
        }

        let model = find_string_field(obj, &["model", "model_id", "model_name"]).unwrap_or_default();
        if model.is_empty() {
            return None;
        }

        let provider = Provider::from_url_and_model("", &model);
        let input = find_u64_field(obj, &["input_tokens", "prompt_tokens", "tokens_in"]).unwrap_or(0);
        let output = find_u64_field(obj, &["output_tokens", "completion_tokens", "tokens_out"]).unwrap_or(0);
        let cache_read = find_u64_field(obj, &["cache_read_tokens", "cache_reads"]).unwrap_or(0);

        Some(UsageRecord {
            id: None,
            timestamp,
            collector: "zed".to_string(),
            tool: Some("Zed".to_string()),
            provider: provider.name().to_string(),
            model,
            input_tokens: input,
            output_tokens: output,
            reasoning_tokens: 0,
            cache_read_tokens: cache_read,
            cache_creation_tokens: 0,
            total_tokens: input + output + cache_read,
            cost_usd: find_f64_field(obj, &["cost", "total_cost"]).unwrap_or(0.0),
            cost_cny: 0.0,
            latency_ms: None,
            is_stream: false,
            status_code: None,
            session_id: find_string_field(obj, &["session_id", "thread_id"]),
            request_id: None,
            source_file: Some(path.to_string_lossy().to_string()),
            raw_json: serde_json::to_string(val).ok(),
            notes: None,
        })
    }
}

fn get_table_names(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'")?;
    let names = stmt.query_map([], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    Ok(names)
}

fn get_column_names(conn: &Connection, table: &str) -> Result<Vec<String>> {
    let sql = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&sql)?;
    let names = stmt.query_map([], |row| row.get::<_, String>(1))?.collect::<Result<Vec<_>, _>>()?;
    Ok(names)
}

fn find_string_field(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(v) = obj.get(*key) {
            if let Some(s) = v.as_str() {
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            }
        }
    }
    None
}

fn find_u64_field(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Some(v) = obj.get(*key) {
            if let Some(n) = v.as_u64() {
                return Some(n);
            }
        }
    }
    None
}

fn find_f64_field(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Some(v) = obj.get(*key) {
            if let Some(f) = v.as_f64() {
                return Some(f);
            }
        }
    }
    None
}

#[async_trait]
impl Collector for ZedCollector {
    fn id(&self) -> &str {
        "zed"
    }

    fn name(&self) -> &str {
        "Zed"
    }

    fn is_available(&self) -> bool {
        self.db_path.is_some()
    }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        self.query_usage(since)
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        self.db_path.iter().cloned().collect()
    }
}
