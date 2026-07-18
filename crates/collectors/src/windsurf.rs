use alltokens_core::model::{Provider, UsageRecord};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::Collector;

/// Windsurf 使用数据
/// 路径: ~/.local/share/windsurf/ (Linux)
///       ~/Library/Application Support/Windsurf/ (macOS)
#[derive(Debug, Deserialize, Serialize)]
struct WindsurfUsage {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(rename = "prompt_tokens", default)]
    prompt_tokens: Option<u64>,
    #[serde(rename = "completion_tokens", default)]
    completion_tokens: Option<u64>,
    #[serde(rename = "cache_read_tokens", default)]
    cache_read_tokens: Option<u64>,
    #[serde(rename = "total_tokens", default)]
    total_tokens: Option<u64>,
    #[serde(default)]
    cost: Option<f64>,
}

pub struct WindsurfCollector {
    data_dir: Option<PathBuf>,
}

impl WindsurfCollector {
    pub fn new() -> Self {
        let data_dir = {
            #[cfg(target_os = "windows")]
            {
                dirs::data_dir().map(|d| d.join("Windsurf")).filter(|p| p.exists())
            }
            #[cfg(not(target_os = "windows"))]
            {
                dirs::home_dir().and_then(|home| {
                    #[cfg(target_os = "macos")]
                    let path = home.join("Library").join("Application Support").join("Windsurf");
                    #[cfg(target_os = "linux")]
                    let path = home.join(".local").join("share").join("windsurf");
                    if path.exists() { Some(path) } else { None }
                })
            }
        };

        Self { data_dir }
    }

    /// Override data directory (for tests and probe tooling).
    #[doc(hidden)]
    pub fn with_dir(data_dir: Option<PathBuf>) -> Self {
        Self { data_dir }
    }

    fn count_data_files(&self) -> usize {
        let Some(dir) = &self.data_dir else {
            return 0;
        };
        walkdir::WalkDir::new(dir)
            .max_depth(3)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                (ext == "json" || ext == "jsonl" || ext == "db")
                    && path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.contains("usage") || n.contains("token") || n.contains("session"))
                        .unwrap_or(false)
            })
            .count()
    }
}

#[async_trait]
impl Collector for WindsurfCollector {
    fn id(&self) -> &str {
        "windsurf"
    }

    fn name(&self) -> &str {
        "Windsurf"
    }

    fn is_available(&self) -> bool {
        self.data_dir.is_some()
    }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        let dir = match &self.data_dir {
            Some(d) => d,
            None => return Ok(Vec::new()),
        };

        let mut records = Vec::new();

        for entry in walkdir::WalkDir::new(dir)
            .max_depth(3)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                (ext == "json" || ext == "jsonl" || ext == "db")
                    && path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.contains("usage") || n.contains("token") || n.contains("session"))
                        .unwrap_or(false)
            })
        {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

            if ext == "db" {
                // SQLite DB - 尝试读取
                if let Ok(db_records) = parse_windsurf_db(path, since) {
                    records.extend(db_records);
                }
                continue;
            }

            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // JSONL
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(usage) = serde_json::from_str::<WindsurfUsage>(line) {
                    if let Some(record) = windsurf_usage_to_record(usage, path, since, Some(line.to_string())) {
                        records.push(record);
                    }
                }
            }

            // JSON array
            if let Ok(usages) = serde_json::from_str::<Vec<WindsurfUsage>>(&content) {
                for usage in usages {
                    let raw = serde_json::to_string(&usage).ok();
                    if let Some(record) = windsurf_usage_to_record(usage, path, since, raw) {
                        records.push(record);
                    }
                }
            }
        }

        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(records)
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        self.data_dir.iter().cloned().collect()
    }
}

impl WindsurfCollector {
    /// Dry-run probe: list data paths, file counts, and sample record count.
    pub fn probe(&self) -> Result<super::probe::BasicProbeResult> {
        let data_paths: Vec<String> = self
            .data_dir
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        let sample_records = super::probe::collect_sample_count(self);
        Ok(super::probe::build_basic_probe_result(
            "windsurf",
            "Windsurf",
            self.is_available(),
            data_paths,
            self.count_data_files(),
            sample_records,
        ))
    }
}

fn windsurf_usage_to_record(
    usage: WindsurfUsage,
    path: &Path,
    since: Option<DateTime<Utc>>,
    raw_json: Option<String>,
) -> Option<UsageRecord> {
    let timestamp = usage
        .timestamp
        .as_ref()
        .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
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
    let input = usage.prompt_tokens.unwrap_or(0);
    let output = usage.completion_tokens.unwrap_or(0);
    let cache_read = usage.cache_read_tokens.unwrap_or(0);

    Some(UsageRecord {
        id: None,
        timestamp,
        collector: "windsurf".to_string(),
        tool: Some("Windsurf".to_string()),
        provider: provider.name().to_string(),
        model,
        input_tokens: input,
        output_tokens: output,
        reasoning_tokens: 0,
        cache_read_tokens: cache_read,
        cache_creation_tokens: 0,
        total_tokens: usage.total_tokens.unwrap_or(input + output + cache_read),
        cost_usd: usage.cost.unwrap_or(0.0),
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

fn parse_windsurf_db(path: &Path, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
    use rusqlite::Connection;

    let conn = Connection::open(path)?;
    let mut records = Vec::new();

    // 尝试常见的表名
    for table in &["usage", "requests", "token_usage", "api_calls"] {
        let sql = format!("SELECT * FROM {table} LIMIT 1");
        if conn.execute(&sql, []).is_err() {
            continue;
        }

        // 表存在，尝试读取数据
        let query = format!("SELECT * FROM {table} ORDER BY rowid DESC LIMIT 1000");
        let mut stmt = conn.prepare(&query)?;
        let column_names: Vec<String> = stmt
            .column_names()
            .iter()
            .map(|s| s.to_string())
            .collect();

        let rows = stmt.query_map([], |row| {
            let mut map = serde_json::Map::new();
            for (i, name) in column_names.iter().enumerate() {
                let val: serde_json::Value = match row.get_ref(i)? {
                    rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                    rusqlite::types::ValueRef::Integer(i) => serde_json::Value::Number(i.into()),
                    rusqlite::types::ValueRef::Real(f) => {
                        serde_json::Value::Number(
                            serde_json::Number::from_f64(f).unwrap_or(0.into()),
                        )
                    }
                    rusqlite::types::ValueRef::Text(t) => {
                        serde_json::Value::String(String::from_utf8_lossy(t).to_string())
                    }
                    rusqlite::types::ValueRef::Blob(b) => {
                        serde_json::Value::String(base64_encode(b))
                    }
                };
                map.insert(name.clone(), val);
            }
            Ok(serde_json::Value::Object(map))
        })?;

        for row in rows.flatten() {
            if let Some(record) = json_to_usage_record(row, path, since) {
                records.push(record);
            }
        }
    }

    Ok(records)
}

fn json_to_usage_record(
    val: serde_json::Value,
    path: &Path,
    since: Option<DateTime<Utc>>,
) -> Option<UsageRecord> {
    let obj = val.as_object()?;

    let timestamp = obj
        .get("timestamp")
        .or_else(|| obj.get("created_at"))
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

    let model = obj
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if model.is_empty() {
        return None;
    }

    let provider = Provider::from_url_and_model("", &model);
    let input = json_u64(obj, "input_tokens").or_else(|| json_u64(obj, "prompt_tokens")).unwrap_or(0);
    let output = json_u64(obj, "output_tokens").or_else(|| json_u64(obj, "completion_tokens")).unwrap_or(0);
    let cache_read = json_u64(obj, "cache_read_tokens").or_else(|| json_u64(obj, "cache_reads")).unwrap_or(0);

    Some(UsageRecord {
        id: None,
        timestamp,
        collector: "windsurf".to_string(),
        tool: Some("Windsurf".to_string()),
        provider: provider.name().to_string(),
        model,
        input_tokens: input,
        output_tokens: output,
        reasoning_tokens: 0,
        cache_read_tokens: cache_read,
        cache_creation_tokens: 0,
        total_tokens: input + output + cache_read,
        cost_usd: json_f64(obj, "cost").or_else(|| json_f64(obj, "total_cost")).unwrap_or(0.0),
        cost_cny: 0.0,
        latency_ms: None,
        is_stream: false,
        status_code: None,
        session_id: None,
        request_id: None,
        source_file: Some(path.to_string_lossy().to_string()),
        raw_json: serde_json::to_string(&val).ok(),
        notes: None,
    })
}

fn json_u64(obj: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<u64> {
    obj.get(key).and_then(|v| v.as_u64())
}

fn json_f64(obj: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<f64> {
    obj.get(key).and_then(|v| v.as_f64())
}

fn base64_encode(data: &[u8]) -> String {
    let mut buf = Vec::new();
    // 简单 base64 编码
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        buf.push(CHARS[((triple >> 18) & 0x3F) as usize]);
        buf.push(CHARS[((triple >> 12) & 0x3F) as usize]);
        if chunk.len() > 1 {
            buf.push(CHARS[((triple >> 6) & 0x3F) as usize]);
        } else {
            buf.push(b'=');
        }
        if chunk.len() > 2 {
            buf.push(CHARS[(triple & 0x3F) as usize]);
        } else {
            buf.push(b'=');
        }
    }
    String::from_utf8(buf).unwrap_or_default()
}
