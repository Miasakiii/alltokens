/// SQLite schema DDL
pub const SCHEMA: &str = r#"
-- 请求级记录
CREATE TABLE IF NOT EXISTS api_requests (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp         TEXT NOT NULL,
    collector         TEXT NOT NULL,
    tool              TEXT,
    provider          TEXT NOT NULL,
    model             TEXT NOT NULL,

    input_tokens      INTEGER NOT NULL DEFAULT 0,
    output_tokens     INTEGER NOT NULL DEFAULT 0,
    reasoning_tokens  INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens INTEGER NOT NULL DEFAULT 0,
    cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens      INTEGER NOT NULL DEFAULT 0,

    cost_usd          REAL NOT NULL DEFAULT 0.0,
    cost_cny          REAL NOT NULL DEFAULT 0.0,

    latency_ms        INTEGER,
    is_stream         INTEGER NOT NULL DEFAULT 0,
    status_code       INTEGER,
    session_id        TEXT,
    request_id        TEXT,

    source_file       TEXT,
    raw_json          TEXT,
    notes             TEXT
);

CREATE INDEX IF NOT EXISTS idx_requests_timestamp ON api_requests(timestamp);
CREATE INDEX IF NOT EXISTS idx_requests_provider ON api_requests(provider);
CREATE INDEX IF NOT EXISTS idx_requests_model ON api_requests(model);
CREATE INDEX IF NOT EXISTS idx_requests_collector ON api_requests(collector);
CREATE INDEX IF NOT EXISTS idx_requests_tool ON api_requests(tool);

-- 日汇总 (自动维护)
CREATE TABLE IF NOT EXISTS daily_summary (
    date              TEXT NOT NULL,
    provider          TEXT NOT NULL,
    model             TEXT NOT NULL,
    collector         TEXT NOT NULL,
    request_count     INTEGER NOT NULL DEFAULT 0,
    total_input       INTEGER NOT NULL DEFAULT 0,
    total_output      INTEGER NOT NULL DEFAULT 0,
    total_cache_read  INTEGER NOT NULL DEFAULT 0,
    total_cache_creation INTEGER NOT NULL DEFAULT 0,
    total_tokens      INTEGER NOT NULL DEFAULT 0,
    total_cost_usd    REAL NOT NULL DEFAULT 0.0,
    total_cost_cny    REAL NOT NULL DEFAULT 0.0,
    avg_latency_ms    INTEGER,
    cache_hit_rate    REAL NOT NULL DEFAULT 0.0,
    PRIMARY KEY (date, provider, model, collector)
);

-- 定价表
CREATE TABLE IF NOT EXISTS pricing (
    provider          TEXT NOT NULL,
    model             TEXT NOT NULL,
    input_per_mtok    REAL NOT NULL,
    output_per_mtok   REAL NOT NULL,
    cache_read_per_mtok REAL DEFAULT 0.0,
    cache_create_per_mtok REAL DEFAULT 0.0,
    effective_from    TEXT,
    source            TEXT DEFAULT 'builtin',
    PRIMARY KEY (provider, model, effective_from)
);

-- 采集器状态 (记录上次采集位置)
CREATE TABLE IF NOT EXISTS collector_state (
    collector_id      TEXT PRIMARY KEY,
    last_scan_at      TEXT,
    last_file_offset  TEXT,         -- JSON: { "path": offset }
    metadata          TEXT          -- JSON: 采集器自定义状态
);

-- 应用配置
CREATE TABLE IF NOT EXISTS app_config (
    key               TEXT PRIMARY KEY,
    value             TEXT NOT NULL
);
"#;

use rusqlite::Connection;

/// 当前 schema 版本 (PRAGMA user_version 目标值)
pub const SCHEMA_VERSION: i64 = 1;

/// 基于 PRAGMA user_version 的递增迁移。
///
/// 从当前版本逐步升级到 [`SCHEMA_VERSION`]，每一步在独立事务中执行并更新
/// user_version。每一步迁移必须幂等：对已升级的库重复执行不产生副作用。
pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let mut version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    while version < SCHEMA_VERSION {
        let next = version + 1;
        let tx = conn.unchecked_transaction()?;
        match next {
            1 => migrate_to_v1(&tx)?,
            _ => unreachable!("no migration defined for schema version {next}"),
        }
        tx.execute_batch(&format!("PRAGMA user_version = {next}"))?;
        tx.commit()?;
        version = next;
    }
    Ok(())
}

/// v1: 为旧库补齐 reasoning_tokens 列 (新库 SCHEMA 已自带)
fn migrate_to_v1(conn: &Connection) -> rusqlite::Result<()> {
    if !column_exists(conn, "api_requests", "reasoning_tokens")? {
        conn.execute(
            "ALTER TABLE api_requests ADD COLUMN reasoning_tokens INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    Ok(())
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}
