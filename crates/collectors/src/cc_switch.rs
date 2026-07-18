use alltokens_core::model::{Provider, UsageRecord};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use std::path::PathBuf;

use super::Collector;

/// cc-switch 使用记录导入器
/// cc-switch 作为 API 中继，天然记录所有经过的 API 调用
/// 其 SQLite DB 中有完整的 usage 数据
///
/// DB 位置: ~/.cc-switch/cc-switch.db 或类似路径
/// 表: api_requests (与 AllTokens 类似的数据结构)
pub struct CcSwitchCollector {
    db_path: Option<PathBuf>,
}

impl CcSwitchCollector {
    pub fn new() -> Self {
        let db_path = find_cc_switch_db();
        Self { db_path }
    }

    /// Override the DB path (for tests and probe tooling).
    #[doc(hidden)]
    pub fn with_db_path(db_path: PathBuf) -> Self {
        Self {
            db_path: Some(db_path),
        }
    }
}

fn find_cc_switch_db() -> Option<PathBuf> {
    let home = dirs::home_dir()?;

    // 常见路径
    let candidates = [
        home.join(".cc-switch").join("cc-switch.db"),
        home.join(".cc-switch").join("data.db"),
        home.join(".cc-switch").join("app.db"),
        home.join(".config").join("cc-switch").join("cc-switch.db"),
        // macOS
        home.join("Library")
            .join("Application Support")
            .join("cc-switch")
            .join("cc-switch.db"),
        // Tauri app data
        dirs::data_dir()
            .map(|d| d.join("cc-switch").join("cc-switch.db"))
            .unwrap_or_default(),
    ];

    for path in &candidates {
        if path.exists() {
            return Some(path.clone());
        }
    }

    None
}

#[async_trait]
impl Collector for CcSwitchCollector {
    fn id(&self) -> &str {
        "cc_switch"
    }

    fn name(&self) -> &str {
        "cc-switch"
    }

    fn is_available(&self) -> bool {
        self.db_path.is_some()
    }

    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>> {
        let db_path = match &self.db_path {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };

        let conn = Connection::open(db_path)?;
        let mut records = Vec::new();

        // cc-switch 的表结构
        // 尝试多种可能的表名（新版 cc-switch 使用 proxy_request_logs）
        for table in &["api_requests", "usage_records", "requests", "request_logs", "proxy_request_logs"] {
            if let Ok(r) = query_cc_switch_table(&conn, table, since) {
                records.extend(r);
            }
        }

        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(records)
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        self.db_path.iter().cloned().collect()
    }
}

fn query_cc_switch_table(
    conn: &Connection,
    table: &str,
    since: Option<DateTime<Utc>>,
) -> Result<Vec<UsageRecord>> {
    // 检查表是否存在
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !exists {
        return Ok(Vec::new());
    }

    let columns = get_columns(conn, table)?;

    // 构建查询
    let mut sql = format!("SELECT * FROM {table}");
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    // 时间过滤。注意列类型：INTEGER 列（如 proxy_request_logs.created_at，
    // epoch 秒）必须按整数比较——SQLite 类型序中 TEXT > INTEGER，直接用
    // RFC3339 字符串比较会恒真、导致每次全量重复导入。
    let time_col = find_column(&columns, &["timestamp", "created_at", "time", "date"]);
    if let (Some((col, decl_type)), Some(since)) = (&time_col, since) {
        sql.push_str(&format!(" WHERE {col} >= ?1"));
        if decl_type.to_uppercase().contains("INT") {
            params.push(Box::new(since.timestamp()));
        } else {
            params.push(Box::new(since.to_rfc3339()));
        }
    }

    // 首次全量导入（since=None）按页拉取，避免 LIMIT 截断丢掉更老的历史
    // 记录；增量导入仍单页（新数据远少于一页）。
    let page_size = 5000i64;
    let mut offset = 0i64;
    let mut records = Vec::new();
    loop {
        let page_sql = format!("{sql} ORDER BY rowid DESC LIMIT {page_size} OFFSET {offset}");
        let mut stmt = conn.prepare(&page_sql)?;
        let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
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

        let mut page_count = 0usize;
        for row in rows.flatten() {
            page_count += 1;
            if let Some(record) = cc_switch_json_to_record(&row) {
                records.push(record);
            }
        }

        if page_count < page_size as usize || since.is_some() {
            break;
        }
        offset += page_size;
    }

    Ok(records)
}

fn cc_switch_json_to_record(val: &serde_json::Value) -> Option<UsageRecord> {
    let obj = val.as_object()?;

    let timestamp = find_timestamp(obj, &["timestamp", "created_at", "time", "date"])
        .unwrap_or_else(Utc::now);

    let model = find_str(obj, &["model", "model_name"]).unwrap_or_default();
    if model.is_empty() {
        return None;
    }

    // provider_id 可能是 "_session" 之类的占位符（session 导入源），此时
    // 回退到按模型名识别 provider。
    let provider_name = find_str(obj, &["provider", "provider_name", "api_provider", "provider_id"]);
    let provider_str = match provider_name.as_deref() {
        Some(p) if !p.starts_with('_') => p.to_string(),
        _ => Provider::from_url_and_model("", &model).name().to_string(),
    };

    let collector_name = find_str(obj, &["app_type", "collector", "source", "tool"]).unwrap_or_else(|| "cc_switch".to_string());
    let tool_name = find_str(obj, &["app_type", "tool", "source"]);

    let input = find_u64(obj, &["input_tokens", "prompt_tokens"]).unwrap_or(0);
    let output = find_u64(obj, &["output_tokens", "completion_tokens"]).unwrap_or(0);
    let cache_read = find_u64(obj, &["cache_read_tokens", "cache_reads"]).unwrap_or(0);
    let cache_creation = find_u64(obj, &["cache_creation_tokens", "cache_writes"]).unwrap_or(0);

    Some(UsageRecord {
        id: None,
        timestamp,
        collector: collector_name,
        tool: tool_name,
        provider: provider_str,
        model,
        input_tokens: input,
        output_tokens: output,
        reasoning_tokens: 0,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_creation,
        total_tokens: input + output + cache_read + cache_creation,
        cost_usd: find_f64(obj, &["cost_usd", "total_cost_usd", "cost", "total_cost"]).unwrap_or(0.0),
        cost_cny: find_f64(obj, &["cost_cny"]).unwrap_or(0.0),
        latency_ms: find_u64(obj, &["latency_ms", "duration_ms"]),
        is_stream: find_bool(obj, &["is_stream", "is_streaming", "streaming"]).unwrap_or(false),
        status_code: find_u64(obj, &["status_code"]).map(|v| v as u16),
        session_id: find_str(obj, &["session_id"]),
        request_id: find_str(obj, &["request_id"]),
        source_file: None,
        raw_json: serde_json::to_string(val).ok(),
        notes: None,
    })
}

fn get_columns(conn: &Connection, table: &str) -> Result<Vec<(String, String)>> {
    let sql = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&sql)?;
    let pairs = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(pairs)
}

fn find_column(columns: &[(String, String)], candidates: &[&str]) -> Option<(String, String)> {
    for c in candidates {
        if let Some((name, decl)) = columns.iter().find(|(name, _)| name == c) {
            return Some((name.clone(), decl.clone()));
        }
    }
    None
}

fn find_str(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<String> {
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

fn find_u64(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Some(v) = obj.get(*key) {
            if let Some(n) = v.as_u64() {
                return Some(n);
            }
        }
    }
    None
}

fn find_f64(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Some(v) = obj.get(*key) {
            if let Some(f) = v.as_f64() {
                return Some(f);
            }
            // cc-switch 把成本存成 TEXT（如 '0.0010507446'）
            if let Some(f) = v.as_str().and_then(|s| s.parse::<f64>().ok()) {
                return Some(f);
            }
        }
    }
    None
}

/// 解析时间戳：兼容 RFC3339/日期字符串与 epoch 数字（>1e12 视为毫秒，
/// 否则为秒——cc-switch `proxy_request_logs.created_at` 是 epoch 秒）。
fn find_timestamp(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<DateTime<Utc>> {
    for key in keys {
        if let Some(v) = obj.get(*key) {
            if let Some(s) = v.as_str() {
                if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
                    return Some(dt.with_timezone(&Utc));
                }
                if let Ok(dt) = DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f%:z") {
                    return Some(dt.with_timezone(&Utc));
                }
                if let Ok(dt) = DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
                    return Some(dt.with_timezone(&Utc));
                }
                if let Ok(epoch) = s.parse::<i64>() {
                    if let Some(dt) = epoch_to_utc(epoch) {
                        return Some(dt);
                    }
                }
            }
            if let Some(epoch) = v.as_i64() {
                if let Some(dt) = epoch_to_utc(epoch) {
                    return Some(dt);
                }
            }
        }
    }
    None
}

fn epoch_to_utc(epoch: i64) -> Option<DateTime<Utc>> {
    if epoch.abs() > 1_000_000_000_000 {
        DateTime::from_timestamp_millis(epoch)
    } else {
        DateTime::from_timestamp(epoch, 0)
    }
}

fn find_bool(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<bool> {
    for key in keys {
        if let Some(v) = obj.get(*key) {
            if let Some(b) = v.as_bool() {
                return Some(b);
            }
            // cc-switch 的 is_streaming 是 INTEGER 1/0
            if let Some(n) = v.as_i64() {
                return Some(n != 0);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 新版 cc-switch 的真实 schema：`proxy_request_logs`（epoch 秒时间戳 +
    /// TEXT 成本 + INTEGER is_streaming + provider_id '_session' 占位符）。
    #[tokio::test]
    async fn collects_from_proxy_request_logs_real_schema() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("cc-switch.db");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE proxy_request_logs (
                request_id TEXT, provider_id TEXT, app_type TEXT, model TEXT,
                input_tokens INTEGER, output_tokens INTEGER,
                cache_read_tokens INTEGER, cache_creation_tokens INTEGER,
                total_cost_usd TEXT, latency_ms INTEGER, status_code INTEGER,
                session_id TEXT, is_streaming INTEGER, created_at INTEGER
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO proxy_request_logs VALUES
             ('req-1','_session','claude','mimo-v2.5-pro-ultraspeed',1000,200,5000,0,'0.0010507446',800,200,'sess-1',1,1784225027),
             ('req-2','openai','codex','gpt-4o',2000,400,0,0,'0.02',1200,200,'sess-2',0,1784225043)",
            [],
        )
        .unwrap();
        drop(conn);

        let collector = CcSwitchCollector::with_db_path(db);
        let records = collector.collect(None).await.unwrap();
        assert_eq!(records.len(), 2);

        let first = &records[0];
        assert_eq!(first.request_id.as_deref(), Some("req-1"));
        // epoch 秒被正确转换（而不是回退为“现在”）
        assert_eq!(
            first.timestamp,
            DateTime::from_timestamp(1_784_225_027, 0).unwrap()
        );
        // TEXT 成本被解析
        assert!((first.cost_usd - 0.0010507446).abs() < 1e-9);
        assert_eq!(first.cache_read_tokens, 5000);
        assert_eq!(first.total_tokens, 1000 + 200 + 5000);
        assert!(first.is_stream);
        assert_eq!(first.collector, "claude");
        // '_session' 占位符不当作 provider 名
        assert!(!first.provider.starts_with('_'));

        let second = &records[1];
        assert_eq!(second.provider, "openai");
        assert!(!second.is_stream);

        // since 过滤按 epoch 秒比较：只命中更晚的 req-2（全量重复导入回归）
        let since = DateTime::from_timestamp(1_784_225_040, 0).unwrap();
        let recent = collector.collect(Some(since)).await.unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].request_id.as_deref(), Some("req-2"));
    }
}
