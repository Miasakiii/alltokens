use crate::heatmap::{fill_heatmap_days, heatmap_date_range, DEFAULT_HEATMAP_DAYS};
use crate::invocation::{
    extract_skill_names_from_json, extract_tool_names_from_json, NOTE_INVOCATION_SKILL,
    NOTE_INVOCATION_TOOL,
};
use crate::model::*;
use crate::pricing::PricingEngine;
use crate::project::extract_project_name;
use crate::schema::SCHEMA;
use chrono::{NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("source database has no api_requests table")]
    InvalidSource,
}

/// `Storage::merge_from` 的结果统计
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MergeResult {
    /// 源库 api_requests 总行数
    pub scanned: u64,
    /// 实际插入的行数
    pub inserted: u64,
    /// 因重复而跳过的行数
    pub skipped: u64,
}

/// Thread-safe storage wrapper
#[derive(Clone)]
pub struct Storage {
    conn: Arc<Mutex<Connection>>,
}

impl Storage {
    /// 打开或创建数据库
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(SCHEMA)?;
        // Migration: add reasoning_tokens column if missing (for existing DBs)
        let _ = conn.execute(
            "ALTER TABLE api_requests ADD COLUMN reasoning_tokens INTEGER NOT NULL DEFAULT 0",
            [],
        );
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 内存数据库 (测试用)
    pub fn memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 插入一条 usage 记录
    pub fn insert_record(&self, record: &UsageRecord) -> Result<i64, StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO api_requests (
                timestamp, collector, tool, provider, model,
                input_tokens, output_tokens, reasoning_tokens, cache_read_tokens, cache_creation_tokens, total_tokens,
                cost_usd, cost_cny,
                latency_ms, is_stream, status_code, session_id, request_id,
                source_file, raw_json, notes
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",
            params![
                record.timestamp.to_rfc3339(),
                record.collector,
                record.tool,
                record.provider,
                record.model,
                record.input_tokens as i64,
                record.output_tokens as i64,
                record.reasoning_tokens as i64,
                record.cache_read_tokens as i64,
                record.cache_creation_tokens as i64,
                record.total_tokens as i64,
                record.cost_usd,
                record.cost_cny,
                record.latency_ms.map(|v| v as i64),
                record.is_stream as i32,
                record.status_code.map(|v| v as i32),
                record.session_id,
                record.request_id,
                record.source_file,
                record.raw_json,
                record.notes,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// 批量插入 (事务)
    pub fn insert_records(&self, records: &[UsageRecord]) -> Result<usize, StorageError> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO api_requests (
                    timestamp, collector, tool, provider, model,
                    input_tokens, output_tokens, reasoning_tokens, cache_read_tokens, cache_creation_tokens, total_tokens,
                    cost_usd, cost_cny,
                    latency_ms, is_stream, status_code, session_id, request_id,
                    source_file, raw_json, notes
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",
            )?;
            for r in records {
                stmt.execute(params![
                    r.timestamp.to_rfc3339(),
                    r.collector,
                    r.tool,
                    r.provider,
                    r.model,
                    r.input_tokens as i64,
                    r.output_tokens as i64,
                    r.reasoning_tokens as i64,
                    r.cache_read_tokens as i64,
                    r.cache_creation_tokens as i64,
                    r.total_tokens as i64,
                    r.cost_usd,
                    r.cost_cny,
                    r.latency_ms.map(|v| v as i64),
                    r.is_stream as i32,
                    r.status_code.map(|v| v as i32),
                    r.session_id,
                    r.request_id,
                    r.source_file,
                    r.raw_json,
                    r.notes,
                ])?;
            }
        }
        tx.commit()?;
        Ok(records.len())
    }

    /// 强制 WAL checkpoint 并截断 -wal 文件（导出快照前调用，
    /// 保证主 db 文件自包含、可安全复制）。
    pub fn checkpoint(&self) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }

    /// 从另一个 AllTokens 数据库合并用量记录（多设备文件同步）。
    ///
    /// 纯 SQL 集合操作：ATTACH 源库后 `INSERT … SELECT … WHERE NOT EXISTS`。
    /// 去重键 `(collector, timestamp, model, total_tokens, request_id)`——
    /// 同一数据源在不同设备上采集出的记录这些字段完全一致，而两台设备
    /// 各自产生完全相同元组的概率可忽略。幂等：重复合并不会重复插入。
    pub fn merge_from(&self, source_db: &Path) -> Result<MergeResult, StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "ATTACH DATABASE ?1 AS src",
            params![source_db.to_string_lossy().as_ref()],
        )?;

        let result = (|| -> Result<MergeResult, StorageError> {
            let has_table: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM src.sqlite_master WHERE type='table' AND name='api_requests'",
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .map(|c| c > 0)?;
            if !has_table {
                return Err(StorageError::InvalidSource);
            }

            let scanned: u64 = conn
                .query_row("SELECT COUNT(*) FROM src.api_requests", [], |r| {
                    r.get::<_, i64>(0)
                })? as u64;

            let inserted = conn.execute(
                "INSERT INTO main.api_requests (
                    timestamp, collector, tool, provider, model,
                    input_tokens, output_tokens, reasoning_tokens, cache_read_tokens, cache_creation_tokens, total_tokens,
                    cost_usd, cost_cny,
                    latency_ms, is_stream, status_code, session_id, request_id,
                    source_file, raw_json, notes
                )
                SELECT s.timestamp, s.collector, s.tool, s.provider, s.model,
                       s.input_tokens, s.output_tokens, s.reasoning_tokens, s.cache_read_tokens, s.cache_creation_tokens, s.total_tokens,
                       s.cost_usd, s.cost_cny,
                       s.latency_ms, s.is_stream, s.status_code, s.session_id, s.request_id,
                       s.source_file, s.raw_json, s.notes
                FROM src.api_requests s
                WHERE NOT EXISTS (
                    SELECT 1 FROM main.api_requests d
                    WHERE d.collector = s.collector
                      AND d.timestamp = s.timestamp
                      AND d.model = s.model
                      AND d.total_tokens = s.total_tokens
                      AND IFNULL(d.request_id, '') = IFNULL(s.request_id, '')
                )",
                [],
            )? as u64;

            Ok(MergeResult {
                scanned,
                inserted,
                skipped: scanned - inserted,
            })
        })();

        // 无论成功与否都尝试 DETACH
        let detach = conn.execute("DETACH DATABASE src", []);
        match (result, detach) {
            (Ok(r), Ok(_)) => Ok(r),
            (Err(e), _) => Err(e),
            (Ok(_), Err(e)) => Err(e.into()),
        }
    }

    /// 查询请求列表 (带过滤 + 分页)
    pub fn get_requests(
        &self,
        filter: &RequestFilter,
        pagination: &Pagination,
    ) -> Result<PaginatedResult<UsageRecord>, StorageError> {
        let conn = self.conn.lock().unwrap();

        let (where_clause, params_vec) = build_where_clause(filter);

        // Count
        let count_sql = format!("SELECT COUNT(*) FROM api_requests {where_clause}");
        let total: u64 = conn
            .query_row(&count_sql, rusqlite::params_from_iter(&params_vec), |row| {
                row.get::<_, i64>(0)
            })? as u64;

        // Data
        let offset = (pagination.page * pagination.page_size) as i64;
        let limit = pagination.page_size as i64;
        let data_sql = format!(
            "SELECT id, timestamp, collector, tool, provider, model,
                    input_tokens, output_tokens, reasoning_tokens, cache_read_tokens, cache_creation_tokens, total_tokens,
                    cost_usd, cost_cny, latency_ms, is_stream, status_code, session_id, request_id,
                    source_file, raw_json, notes
             FROM api_requests {where_clause} ORDER BY timestamp DESC LIMIT {limit} OFFSET {offset}"
        );

        let mut stmt = conn.prepare(&data_sql)?;
        let items = stmt
            .query_map(rusqlite::params_from_iter(&params_vec), map_usage_record_row)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PaginatedResult {
            items,
            total,
            page: pagination.page,
            page_size: pagination.page_size,
        })
    }

    /// Export all matching requests (no pagination).
    pub fn export_requests(&self, filter: &RequestFilter) -> Result<Vec<UsageRecord>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let (where_clause, params_vec) = build_where_clause(filter);
        let sql = format!(
            "SELECT id, timestamp, collector, tool, provider, model,
                    input_tokens, output_tokens, reasoning_tokens, cache_read_tokens, cache_creation_tokens, total_tokens,
                    cost_usd, cost_cny, latency_ms, is_stream, status_code, session_id, request_id,
                    source_file, raw_json, notes
             FROM api_requests {where_clause} ORDER BY timestamp DESC"
        );

        let mut stmt = conn.prepare(&sql)?;
        let items = stmt
            .query_map(rusqlite::params_from_iter(&params_vec), map_usage_record_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(items)
    }

    /// 获取总体统计
    pub fn get_overview(&self, filter: &RequestFilter) -> Result<OverviewStats, StorageError> {
        let conn = self.conn.lock().unwrap();
        let (where_clause, params_vec) = build_where_clause(filter);

        let sql = format!(
            "SELECT
                COUNT(*),
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(reasoning_tokens), 0),
                COALESCE(SUM(cache_read_tokens), 0),
                COALESCE(SUM(cache_creation_tokens), 0),
                COALESCE(SUM(total_tokens), 0),
                COALESCE(SUM(cost_usd), 0.0),
                COALESCE(SUM(cost_cny), 0.0),
                COALESCE(SUM(CASE WHEN status_code IS NOT NULL AND status_code >= 200 AND status_code < 300 THEN 1 ELSE 0 END), 0)
             FROM api_requests {where_clause}"
        );

        let result = conn.query_row(&sql, rusqlite::params_from_iter(&params_vec), |row| {
            let count: i64 = row.get(0)?;
            let input: i64 = row.get(1)?;
            let output: i64 = row.get(2)?;
            let reasoning: i64 = row.get(3)?;
            let cache_read: i64 = row.get(4)?;
            let cache_creation: i64 = row.get(5)?;
            let total: i64 = row.get(6)?;
            let cost_usd: f64 = row.get(7)?;
            let cost_cny: f64 = row.get(8)?;
            let success: i64 = row.get(9)?;

            let cacheable = input + cache_creation + cache_read;
            let cache_hit_rate = if cacheable > 0 {
                cache_read as f64 / cacheable as f64
            } else {
                0.0
            };
            let success_rate = if count > 0 {
                success as f64 / count as f64
            } else {
                0.0
            };

            Ok(OverviewStats {
                total_requests: count as u64,
                total_input_tokens: input as u64,
                total_output_tokens: output as u64,
                total_reasoning_tokens: reasoning as u64,
                total_cache_read_tokens: cache_read as u64,
                total_cache_creation_tokens: cache_creation as u64,
                total_tokens: total as u64,
                total_cost_usd: cost_usd,
                total_cost_cny: cost_cny,
                cache_hit_rate,
                success_rate,
            })
        })?;

        Ok(result)
    }

    /// 按 provider 分组统计
    pub fn get_provider_stats(&self, filter: &RequestFilter) -> Result<Vec<ProviderStats>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let (where_clause, params_vec) = build_where_clause(filter);

        let sql = format!(
            "SELECT provider,
                    COUNT(*),
                    COALESCE(SUM(total_tokens), 0),
                    COALESCE(SUM(cost_usd), 0.0),
                    COALESCE(SUM(cost_cny), 0.0),
                    COALESCE(SUM(cache_read_tokens), 0),
                    COALESCE(SUM(input_tokens + cache_creation_tokens + cache_read_tokens), 0)
             FROM api_requests {where_clause} GROUP BY provider ORDER BY SUM(total_tokens) DESC"
        );

        let mut stmt = conn.prepare(&sql)?;
        let results = stmt
            .query_map(rusqlite::params_from_iter(&params_vec), |row| {
                let cache_read: i64 = row.get(5)?;
                let cacheable: i64 = row.get(6)?;
                let cache_hit_rate = if cacheable > 0 {
                    cache_read as f64 / cacheable as f64
                } else {
                    0.0
                };
                Ok(ProviderStats {
                    provider: row.get(0)?,
                    request_count: row.get::<_, i64>(1)? as u64,
                    total_tokens: row.get::<_, i64>(2)? as u64,
                    total_cost_usd: row.get(3)?,
                    total_cost_cny: row.get(4)?,
                    cache_hit_rate,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(results)
    }

    /// 按 model 分组统计
    pub fn get_model_stats(&self, filter: &RequestFilter) -> Result<Vec<ModelStats>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let (where_clause, params_vec) = build_where_clause(filter);

        let sql = format!(
            "SELECT provider, model,
                    COUNT(*),
                    COALESCE(SUM(input_tokens), 0),
                    COALESCE(SUM(output_tokens), 0),
                    COALESCE(SUM(cache_read_tokens), 0),
                    COALESCE(SUM(cache_creation_tokens), 0),
                    COALESCE(SUM(total_tokens), 0),
                    COALESCE(SUM(cost_usd), 0.0),
                    COALESCE(SUM(cost_cny), 0.0)
             FROM api_requests {where_clause} GROUP BY provider, model ORDER BY SUM(total_tokens) DESC"
        );

        let mut stmt = conn.prepare(&sql)?;
        let results = stmt
            .query_map(rusqlite::params_from_iter(&params_vec), |row| {
                let cache_read: i64 = row.get(5)?;
                let cache_creation: i64 = row.get(6)?;
                let input: i64 = row.get(3)?;
                let cacheable = input + cache_creation + cache_read;
                let cache_hit_rate = if cacheable > 0 {
                    cache_read as f64 / cacheable as f64
                } else {
                    0.0
                };
                Ok(ModelStats {
                    provider: row.get(0)?,
                    model: row.get(1)?,
                    request_count: row.get::<_, i64>(2)? as u64,
                    total_input: input as u64,
                    total_output: row.get::<_, i64>(4)? as u64,
                    total_cache_read: cache_read as u64,
                    total_cache_creation: cache_creation as u64,
                    total_tokens: row.get::<_, i64>(7)? as u64,
                    total_cost_usd: row.get(8)?,
                    total_cost_cny: row.get(9)?,
                    cache_hit_rate,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(results)
    }

    /// 按 tool 分组统计
    pub fn get_tool_stats(&self, filter: &RequestFilter) -> Result<Vec<ToolStats>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let (where_clause, params_vec) = build_where_clause(filter);

        let sql = format!(
            "SELECT collector, tool,
                    COUNT(*),
                    COALESCE(SUM(total_tokens), 0),
                    COALESCE(SUM(cost_usd), 0.0),
                    COALESCE(SUM(cost_cny), 0.0)
             FROM api_requests {where_clause} GROUP BY collector, tool ORDER BY SUM(total_tokens) DESC"
        );

        let mut stmt = conn.prepare(&sql)?;
        let results = stmt
            .query_map(rusqlite::params_from_iter(&params_vec), |row| {
                Ok(ToolStats {
                    collector: row.get(0)?,
                    tool: row.get(1)?,
                    request_count: row.get::<_, i64>(2)? as u64,
                    total_tokens: row.get::<_, i64>(3)? as u64,
                    total_cost_usd: row.get(4)?,
                    total_cost_cny: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(results)
    }

    /// 按项目分组统计（从 source_file / session_id 路径推断）
    pub fn get_project_stats(&self, filter: &RequestFilter) -> Result<Vec<ProjectStats>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let (where_clause, params_vec) = build_where_clause(filter);

        let sql = format!(
            "SELECT source_file, session_id,
                    COUNT(*),
                    COALESCE(SUM(total_tokens), 0),
                    COALESCE(SUM(cost_usd), 0.0),
                    COALESCE(SUM(cost_cny), 0.0)
             FROM api_requests {where_clause}
             GROUP BY source_file, session_id"
        );

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(&params_vec), |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)? as u64,
                    row.get::<_, i64>(3)? as u64,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut merged: std::collections::BTreeMap<String, ProjectStats> = std::collections::BTreeMap::new();
        for (source_file, session_id, request_count, total_tokens, total_cost_usd, total_cost_cny) in rows {
            let project = match extract_project_name(source_file.as_deref(), session_id.as_deref()) {
                Some(name) => name,
                None => continue,
            };
            merged
                .entry(project.clone())
                .and_modify(|entry| {
                    entry.request_count += request_count;
                    entry.total_tokens += total_tokens;
                    entry.total_cost_usd += total_cost_usd;
                    entry.total_cost_cny += total_cost_cny;
                })
                .or_insert(ProjectStats {
                    project,
                    request_count,
                    total_tokens,
                    total_cost_usd,
                    total_cost_cny,
                });
        }

        let mut results: Vec<ProjectStats> = merged.into_values().collect();
        results.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens));
        Ok(results)
    }

    /// 按 session_id + provider + model 分组的会话级统计（借鉴 codex-token-hud session grouping）。
    /// 仅统计带 session_id 的记录；按最近活跃倒序，最多返回 200 个会话。
    pub fn get_session_stats(&self, filter: &RequestFilter) -> Result<Vec<SessionStats>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let (where_clause, params_vec) = build_where_clause(filter);
        let scoped = if where_clause.is_empty() {
            "WHERE session_id IS NOT NULL AND session_id != ''".to_string()
        } else {
            format!("{where_clause} AND session_id IS NOT NULL AND session_id != ''")
        };

        let sql = format!(
            "SELECT session_id, provider, model, collector,
                    COUNT(*),
                    COALESCE(SUM(input_tokens), 0),
                    COALESCE(SUM(output_tokens), 0),
                    COALESCE(SUM(total_tokens), 0),
                    COALESCE(SUM(cost_usd), 0.0),
                    COALESCE(SUM(cost_cny), 0.0),
                    MIN(timestamp), MAX(timestamp)
             FROM api_requests {scoped}
             GROUP BY session_id, provider, model, collector
             ORDER BY MAX(timestamp) DESC
             LIMIT 200"
        );

        let mut stmt = conn.prepare(&sql)?;
        let results = stmt
            .query_map(rusqlite::params_from_iter(&params_vec), |row| {
                let first_seen: String = row.get(10)?;
                let last_seen: String = row.get(11)?;
                let duration_secs = duration_between(&first_seen, &last_seen);
                Ok(SessionStats {
                    session_id: row.get(0)?,
                    provider: row.get(1)?,
                    model: row.get(2)?,
                    collector: row.get(3)?,
                    request_count: row.get::<_, i64>(4)? as u64,
                    total_input: row.get::<_, i64>(5)? as u64,
                    total_output: row.get::<_, i64>(6)? as u64,
                    total_tokens: row.get::<_, i64>(7)? as u64,
                    total_cost_usd: row.get(8)?,
                    total_cost_cny: row.get(9)?,
                    first_seen,
                    last_seen,
                    duration_secs,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(results)
    }

    /// Agent tool invocation TOP (parsed from raw_json / transcript lines)
    pub fn get_tool_invocation_ranking(
        &self,
        filter: &RequestFilter,
    ) -> Result<Vec<ToolInvocationStats>, StorageError> {
        self.aggregate_invocation_ranking(filter, NOTE_INVOCATION_TOOL, extract_tool_names_from_json)
    }

    /// Skill usage TOP (parsed from raw_json / transcript lines)
    pub fn get_skill_invocation_ranking(
        &self,
        filter: &RequestFilter,
    ) -> Result<Vec<SkillInvocationStats>, StorageError> {
        let rows = self.aggregate_invocation_ranking(filter, NOTE_INVOCATION_SKILL, extract_skill_names_from_json)?;
        Ok(rows
            .into_iter()
            .map(|row| SkillInvocationStats {
                name: row.name,
                invocation_count: row.invocation_count,
            })
            .collect())
    }

    fn aggregate_invocation_ranking<F>(
        &self,
        filter: &RequestFilter,
        invocation_note: &str,
        extract: F,
    ) -> Result<Vec<ToolInvocationStats>, StorageError>
    where
        F: Fn(&str) -> Vec<String>,
    {
        let conn = self.conn.lock().unwrap();
        let (where_clause, params_vec) = build_where_clause(filter);
        let extra = if where_clause.is_empty() {
            "WHERE raw_json IS NOT NULL".to_string()
        } else {
            format!("{where_clause} AND raw_json IS NOT NULL")
        };

        let sql = format!(
            "SELECT raw_json, notes, tool FROM api_requests {extra}"
        );

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(&params_vec), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut counts: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
        for (raw_json, notes, tool) in rows {
            if notes.as_deref() == Some(invocation_note) {
                if let Some(name) = tool.filter(|n| !n.is_empty()) {
                    *counts.entry(name).or_insert(0) += 1;
                }
                continue;
            }

            for name in extract(&raw_json) {
                *counts.entry(name).or_insert(0) += 1;
            }
        }

        let mut results: Vec<ToolInvocationStats> = counts
            .into_iter()
            .map(|(name, invocation_count)| ToolInvocationStats {
                name,
                invocation_count,
            })
            .collect();
        results.sort_by(|a, b| {
            b.invocation_count
                .cmp(&a.invocation_count)
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(results)
    }

    /// 日趋势
    pub fn get_daily_trends(&self, filter: &RequestFilter) -> Result<Vec<DailySummary>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let (where_clause, params_vec) = build_where_clause(filter);

        let sql = format!(
            "SELECT DATE(timestamp) as date, provider, model, collector,
                    COUNT(*),
                    COALESCE(SUM(input_tokens), 0),
                    COALESCE(SUM(output_tokens), 0),
                    COALESCE(SUM(cache_read_tokens), 0),
                    COALESCE(SUM(cache_creation_tokens), 0),
                    COALESCE(SUM(total_tokens), 0),
                    COALESCE(SUM(cost_usd), 0.0),
                    COALESCE(SUM(cost_cny), 0.0),
                    COALESCE(AVG(latency_ms), 0)
             FROM api_requests {where_clause}
             GROUP BY date, provider, model, collector
             ORDER BY date DESC"
        );

        let mut stmt = conn.prepare(&sql)?;
        let results = stmt
            .query_map(rusqlite::params_from_iter(&params_vec), |row| {
                let cache_read: i64 = row.get(7)?;
                let cache_creation: i64 = row.get(8)?;
                let input: i64 = row.get(5)?;
                let cacheable = input + cache_creation + cache_read;
                let cache_hit_rate = if cacheable > 0 {
                    cache_read as f64 / cacheable as f64
                } else {
                    0.0
                };
                Ok(DailySummary {
                    date: row.get(0)?,
                    provider: row.get(1)?,
                    model: row.get(2)?,
                    collector: row.get(3)?,
                    request_count: row.get::<_, i64>(4)? as u64,
                    total_input: input as u64,
                    total_output: row.get::<_, i64>(6)? as u64,
                    total_cache_read: cache_read as u64,
                    total_cache_creation: cache_creation as u64,
                    total_tokens: row.get::<_, i64>(9)? as u64,
                    total_cost_usd: row.get(10)?,
                    total_cost_cny: row.get(11)?,
                    avg_latency_ms: Some(row.get::<_, f64>(12)? as u64),
                    cache_hit_rate,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(results)
    }

    /// Daily token totals for calendar heatmap (dense series with zero-fill).
    pub fn get_token_heatmap(
        &self,
        filter: &RequestFilter,
        period_days: u32,
    ) -> Result<TokenHeatmap, StorageError> {
        let period_days = if period_days == 0 {
            DEFAULT_HEATMAP_DAYS
        } else {
            period_days
        };
        let end = Utc::now().date_naive();
        let (start, end) = heatmap_date_range(period_days, end);

        let mut heatmap_filter = filter.clone();
        heatmap_filter.start_date = Some(match &heatmap_filter.start_date {
            Some(existing) => {
                let existing_date = NaiveDate::parse_from_str(&existing[..10.min(existing.len())], "%Y-%m-%d")
                    .unwrap_or(start);
                if existing_date > start {
                    format!("{}T00:00:00", existing_date.format("%Y-%m-%d"))
                } else {
                    format!("{}T00:00:00", start.format("%Y-%m-%d"))
                }
            }
            None => format!("{}T00:00:00", start.format("%Y-%m-%d")),
        });
        heatmap_filter.end_date = Some(format!(
            "{}T23:59:59",
            end.format("%Y-%m-%d")
        ));

        let conn = self.conn.lock().unwrap();
        let (where_clause, params_vec) = build_where_clause(&heatmap_filter);

        let sql = format!(
            "SELECT DATE(timestamp) as date,
                    COALESCE(SUM(total_tokens), 0),
                    COALESCE(SUM(cost_usd), 0.0),
                    COALESCE(SUM(cost_cny), 0.0),
                    COUNT(*)
             FROM api_requests {where_clause}
             GROUP BY date
             ORDER BY date ASC"
        );

        let mut stmt = conn.prepare(&sql)?;
        let rows: Vec<(String, u64, f64, f64, u64)> = stmt
            .query_map(rusqlite::params_from_iter(&params_vec), |row| {
                Ok((
                    row.get(0)?,
                    row.get::<_, i64>(1)? as u64,
                    row.get(2)?,
                    row.get(3)?,
                    row.get::<_, i64>(4)? as u64,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let days = fill_heatmap_days(&rows, start, end);

        Ok(TokenHeatmap {
            period_days,
            start_date: start.format("%Y-%m-%d").to_string(),
            end_date: end.format("%Y-%m-%d").to_string(),
            days,
        })
    }

    /// Hour-of-week activity aggregation (weekday x hour grid, server-local time).
    ///
    /// Deliberately grouped with SQLite's `'localtime'` modifier rather than
    /// the UTC grouping used by trends/heatmap: the activity-rhythm view is
    /// only meaningful in the user's own timezone, and server + dashboard run
    /// on the same machine in the local-first deployment.
    pub fn get_hour_of_week(
        &self,
        filter: &RequestFilter,
    ) -> Result<Vec<HourOfWeekCell>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let (where_clause, params_vec) = build_where_clause(filter);

        let sql = format!(
            "SELECT CAST(strftime('%w', timestamp, 'localtime') AS INTEGER) as weekday,
                    CAST(strftime('%H', timestamp, 'localtime') AS INTEGER) as hour,
                    COALESCE(SUM(total_tokens), 0),
                    COUNT(*)
             FROM api_requests {where_clause}
             GROUP BY weekday, hour"
        );

        let mut stmt = conn.prepare(&sql)?;
        let results = stmt
            .query_map(rusqlite::params_from_iter(&params_vec), |row| {
                Ok(HourOfWeekCell {
                    weekday: row.get::<_, i64>(0)? as u8,
                    hour: row.get::<_, i64>(1)? as u8,
                    total_tokens: row.get::<_, i64>(2)? as u64,
                    request_count: row.get::<_, i64>(3)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(results)
    }

    /// 采集器状态管理
    pub fn get_collector_state(&self, collector_id: &str) -> Result<Option<String>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let result: Option<String> = conn
            .query_row(
                "SELECT metadata FROM collector_state WHERE collector_id = ?1",
                params![collector_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(result)
    }

    pub fn set_collector_state(
        &self,
        collector_id: &str,
        last_scan_at: &str,
        metadata: &str,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO collector_state (collector_id, last_scan_at, metadata) VALUES (?1, ?2, ?3)",
            params![collector_id, last_scan_at, metadata],
        )?;
        Ok(())
    }

    /// 获取上次采集时间
    pub fn get_last_scan(&self, collector_id: &str) -> Result<Option<chrono::DateTime<Utc>>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let result: Option<String> = conn
            .query_row(
                "SELECT last_scan_at FROM collector_state WHERE collector_id = ?1",
                params![collector_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(result.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))))
    }

    /// 记录数
    pub fn count(&self) -> Result<u64, StorageError> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM api_requests", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    /// Read a config value by key.
    pub fn get_config(&self, key: &str) -> Result<Option<String>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let val: Option<String> = conn
            .query_row(
                "SELECT value FROM app_config WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()?;
        Ok(val)
    }

    /// Upsert a config value.
    pub fn set_config(&self, key: &str, value: &str) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO app_config (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    /// Load budget config (defaults when unset).
    pub fn get_budget_config(&self) -> Result<BudgetConfig, StorageError> {
        match self.get_config("budget")? {
            Some(raw) => serde_json::from_str(&raw).map_err(StorageError::Serde),
            None => Ok(BudgetConfig::default()),
        }
    }

    /// Persist budget config.
    pub fn set_budget_config(&self, config: &BudgetConfig) -> Result<(), StorageError> {
        let raw = serde_json::to_string(config)?;
        self.set_config("budget", &raw)
    }

    /// Load desktop widget config (defaults when unset).
    pub fn get_widget_config(&self) -> Result<WidgetConfig, StorageError> {
        match self.get_config("widget")? {
            Some(raw) => serde_json::from_str(&raw).map_err(StorageError::Serde),
            None => Ok(WidgetConfig::default()),
        }
    }

    /// Persist desktop widget config.
    pub fn set_widget_config(&self, config: &WidgetConfig) -> Result<(), StorageError> {
        let raw = serde_json::to_string(config)?;
        self.set_config("widget", &raw)
    }

    /// Load subscription config (defaults when unset).
    pub fn get_subscription_config(&self) -> Result<SubscriptionConfig, StorageError> {
        match self.get_config("subscription")? {
            Some(raw) => serde_json::from_str(&raw).map_err(StorageError::Serde),
            None => Ok(SubscriptionConfig::default()),
        }
    }

    /// Persist subscription config.
    pub fn set_subscription_config(&self, config: &SubscriptionConfig) -> Result<(), StorageError> {
        let raw = serde_json::to_string(config)?;
        self.set_config("subscription", &raw)
    }

    /// Load user pricing config (defaults when unset).
    pub fn get_pricing_config(&self) -> Result<PricingConfig, StorageError> {
        match self.get_config("pricing")? {
            Some(raw) => serde_json::from_str(&raw).map_err(StorageError::Serde),
            None => Ok(PricingConfig::default()),
        }
    }

    /// Persist user pricing config.
    pub fn set_pricing_config(&self, config: &PricingConfig) -> Result<(), StorageError> {
        let raw = serde_json::to_string(config)?;
        self.set_config("pricing", &raw)
    }

    /// Builtin pricing merged with user overrides and optional exchange rate.
    pub fn load_pricing_engine(&self) -> Result<PricingEngine, StorageError> {
        let mut engine = PricingEngine::from_builtin();
        let config = self.get_pricing_config()?;
        engine.merge_user_pricing(config.overrides);
        if let Some(rate) = config.usd_to_cny {
            engine.set_usd_to_cny(rate);
        }
        Ok(engine)
    }

    /// Load per-collector enable flags.
    pub fn get_collectors_config(&self) -> Result<CollectorsConfig, StorageError> {
        match self.get_config("collectors")? {
            Some(raw) => serde_json::from_str(&raw).map_err(StorageError::Serde),
            None => Ok(CollectorsConfig::default()),
        }
    }

    /// Persist per-collector enable flags.
    pub fn set_collectors_config(&self, config: &CollectorsConfig) -> Result<(), StorageError> {
        let raw = serde_json::to_string(config)?;
        self.set_config("collectors", &raw)
    }

    /// Whether a collector is enabled (defaults to true).
    pub fn is_collector_enabled(&self, collector_id: &str) -> Result<bool, StorageError> {
        let config = self.get_collectors_config()?;
        Ok(config.enabled.get(collector_id).copied().unwrap_or(true))
    }

    /// Load general app preferences.
    pub fn get_general_config(&self) -> Result<GeneralConfig, StorageError> {
        match self.get_config("general")? {
            Some(raw) => serde_json::from_str(&raw).map_err(StorageError::Serde),
            None => Ok(GeneralConfig::default()),
        }
    }

    /// Persist general app preferences.
    pub fn set_general_config(&self, config: &GeneralConfig) -> Result<(), StorageError> {
        let raw = serde_json::to_string(config)?;
        self.set_config("general", &raw)
    }

    /// Load data retention preferences.
    pub fn get_data_config(&self) -> Result<DataConfig, StorageError> {
        match self.get_config("data")? {
            Some(raw) => serde_json::from_str(&raw).map_err(StorageError::Serde),
            None => Ok(DataConfig::default()),
        }
    }

    /// Persist data retention preferences.
    pub fn set_data_config(&self, config: &DataConfig) -> Result<(), StorageError> {
        let raw = serde_json::to_string(config)?;
        self.set_config("data", &raw)
    }

    /// Load cached Codex quota snapshot (from app-server JSON-RPC).
    pub fn get_codex_quota_snapshot(
        &self,
    ) -> Result<Option<crate::model::CodexQuotaSnapshot>, StorageError> {
        match self.get_config("codex_quota")? {
            Some(raw) => {
                serde_json::from_str(&raw).map_err(StorageError::Serde).map(Some)
            }
            None => Ok(None),
        }
    }

    /// Persist Codex quota snapshot.
    pub fn set_codex_quota_snapshot(
        &self,
        snapshot: &crate::model::CodexQuotaSnapshot,
    ) -> Result<(), StorageError> {
        let raw = serde_json::to_string(snapshot)?;
        self.set_config("codex_quota", &raw)
    }

    /// Load cached Claude quota snapshot (from statusLine snapshot file).
    pub fn get_claude_quota_snapshot(
        &self,
    ) -> Result<Option<crate::model::ClaudeQuotaSnapshot>, StorageError> {
        match self.get_config("claude_quota")? {
            Some(raw) => {
                serde_json::from_str(&raw).map_err(StorageError::Serde).map(Some)
            }
            None => Ok(None),
        }
    }

    /// Persist Claude quota snapshot.
    pub fn set_claude_quota_snapshot(
        &self,
        snapshot: &crate::model::ClaudeQuotaSnapshot,
    ) -> Result<(), StorageError> {
        let raw = serde_json::to_string(snapshot)?;
        self.set_config("claude_quota", &raw)
    }

    /// Delete usage records older than the given number of days. Returns rows removed.
    pub fn purge_records_older_than_days(&self, days: u32) -> Result<usize, StorageError> {
        if days == 0 {
            return Ok(0);
        }
        let cutoff = Utc::now() - chrono::Duration::days(days as i64);
        let cutoff_iso = cutoff.to_rfc3339();
        let conn = self.conn.lock().unwrap();
        let deleted = conn.execute(
            "DELETE FROM api_requests WHERE timestamp < ?1",
            params![cutoff_iso],
        )?;
        Ok(deleted)
    }
}

/// 计算两个 RFC3339 时间戳之间的秒数差（用于会话时长）。解析失败或负值时返回 0。
fn duration_between(first: &str, last: &str) -> u64 {
    let parse = |s: &str| chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp());
    match (parse(first), parse(last)) {
        (Some(a), Some(b)) if b >= a => (b - a) as u64,
        _ => 0,
    }
}

fn build_where_clause(filter: &RequestFilter) -> (String, Vec<Box<dyn rusqlite::types::ToSql>>) {
    let mut conditions = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    if let Some(ref provider) = filter.provider {
        conditions.push(format!("provider = ?{idx}"));
        params.push(Box::new(provider.clone()));
        idx += 1;
    }
    if let Some(ref model) = filter.model {
        conditions.push(format!("model = ?{idx}"));
        params.push(Box::new(model.clone()));
        idx += 1;
    }
    if let Some(ref collector) = filter.collector {
        conditions.push(format!("collector = ?{idx}"));
        params.push(Box::new(collector.clone()));
        idx += 1;
    }
    if let Some(ref tool) = filter.tool {
        conditions.push(format!("tool = ?{idx}"));
        params.push(Box::new(tool.clone()));
        idx += 1;
    }
    if let Some(ref start) = filter.start_date {
        conditions.push(format!("timestamp >= ?{idx}"));
        params.push(Box::new(start.clone()));
        idx += 1;
    }
    if let Some(ref end) = filter.end_date {
        conditions.push(format!("timestamp <= ?{idx}"));
        params.push(Box::new(end.clone()));
        idx += 1;
    }
    if let Some(min) = filter.min_tokens {
        conditions.push(format!("total_tokens >= ?{idx}"));
        params.push(Box::new(min as i64));
        idx += 1;
    }
    if let Some(max) = filter.max_tokens {
        conditions.push(format!("total_tokens <= ?{idx}"));
        params.push(Box::new(max as i64));
        #[allow(unused_assignments)]
        {
            idx += 1;
        }
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    (where_clause, params)
}

fn map_usage_record_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<UsageRecord> {
    Ok(UsageRecord {
        id: Some(row.get(0)?),
        timestamp: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(1)?)
            .unwrap_or_default()
            .with_timezone(&Utc),
        collector: row.get(2)?,
        tool: row.get(3)?,
        provider: row.get(4)?,
        model: row.get(5)?,
        input_tokens: row.get::<_, i64>(6)? as u64,
        output_tokens: row.get::<_, i64>(7)? as u64,
        reasoning_tokens: row.get::<_, i64>(8)? as u64,
        cache_read_tokens: row.get::<_, i64>(9)? as u64,
        cache_creation_tokens: row.get::<_, i64>(10)? as u64,
        total_tokens: row.get::<_, i64>(11)? as u64,
        cost_usd: row.get(12)?,
        cost_cny: row.get(13)?,
        latency_ms: row.get::<_, Option<i64>>(14)?.map(|v| v as u64),
        is_stream: row.get::<_, i32>(15)? != 0,
        status_code: row.get::<_, Option<i32>>(16)?.map(|v| v as u16),
        session_id: row.get(17)?,
        request_id: row.get(18)?,
        source_file: row.get(19)?,
        raw_json: row.get(20)?,
        notes: row.get(21)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeZone, Utc};

    fn sample_record(
        timestamp: &str,
        collector: &str,
        provider: &str,
        model: &str,
        input: u64,
        output: u64,
        cache_read: u64,
        cost_usd: f64,
        status: Option<u16>,
    ) -> UsageRecord {
        UsageRecord {
            id: None,
            timestamp: DateTime::parse_from_rfc3339(timestamp)
                .unwrap()
                .with_timezone(&Utc),
            collector: collector.to_string(),
            tool: Some(collector.to_string()),
            provider: provider.to_string(),
            model: model.to_string(),
            input_tokens: input,
            output_tokens: output,
            reasoning_tokens: 0,
            cache_read_tokens: cache_read,
            cache_creation_tokens: 0,
            total_tokens: input + output + cache_read,
            cost_usd,
            cost_cny: cost_usd * 7.25,
            latency_ms: Some(100),
            is_stream: false,
            status_code: status,
            session_id: None,
            request_id: None,
            source_file: None,
            raw_json: None,
            notes: None,
        }
    }

    #[test]
    fn insert_and_count_records() {
        let storage = Storage::memory().unwrap();
        assert_eq!(storage.count().unwrap(), 0);

        storage
            .insert_record(&sample_record(
                "2026-07-10T10:00:00Z",
                "claude_code",
                "anthropic",
                "claude-sonnet-4-20250514",
                1000,
                500,
                200,
                0.01,
                Some(200),
            ))
            .unwrap();

        assert_eq!(storage.count().unwrap(), 1);
    }

    #[test]
    fn batch_insert_in_transaction() {
        let storage = Storage::memory().unwrap();
        let records = vec![
            sample_record(
                "2026-07-10T10:00:00Z",
                "cursor",
                "openai",
                "gpt-4o",
                100,
                50,
                0,
                0.001,
                Some(200),
            ),
            sample_record(
                "2026-07-10T11:00:00Z",
                "cursor",
                "openai",
                "gpt-4o",
                200,
                100,
                0,
                0.002,
                Some(500),
            ),
        ];

        assert_eq!(storage.insert_records(&records).unwrap(), 2);
        assert_eq!(storage.count().unwrap(), 2);
    }

    #[test]
    fn get_requests_filter_and_pagination() {
        let storage = Storage::memory().unwrap();
        storage
            .insert_records(&[
                sample_record(
                    "2026-07-10T10:00:00Z",
                    "claude_code",
                    "anthropic",
                    "claude-sonnet-4",
                    1000,
                    500,
                    0,
                    0.01,
                    Some(200),
                ),
                sample_record(
                    "2026-07-10T11:00:00Z",
                    "cursor",
                    "openai",
                    "gpt-4o",
                    2000,
                    1000,
                    0,
                    0.02,
                    Some(200),
                ),
                sample_record(
                    "2026-07-10T12:00:00Z",
                    "cursor",
                    "openai",
                    "gpt-4o-mini",
                    500,
                    250,
                    0,
                    0.005,
                    Some(429),
                ),
            ])
            .unwrap();

        let filter = RequestFilter {
            provider: Some("openai".to_string()),
            ..Default::default()
        };
        let page = Pagination {
            page: 0,
            page_size: 1,
        };

        let result = storage.get_requests(&filter, &page).unwrap();
        assert_eq!(result.total, 2);
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].model, "gpt-4o-mini");
    }

    #[test]
    fn get_overview_aggregates_cache_and_success_rates() {
        let storage = Storage::memory().unwrap();
        storage
            .insert_records(&[
                sample_record(
                    "2026-07-10T10:00:00Z",
                    "claude_code",
                    "anthropic",
                    "claude-sonnet-4",
                    1000,
                    500,
                    500,
                    0.01,
                    Some(200),
                ),
                sample_record(
                    "2026-07-10T11:00:00Z",
                    "cursor",
                    "openai",
                    "gpt-4o",
                    1000,
                    500,
                    0,
                    0.02,
                    Some(500),
                ),
            ])
            .unwrap();

        let overview = storage.get_overview(&RequestFilter::default()).unwrap();
        assert_eq!(overview.total_requests, 2);
        assert_eq!(overview.total_input_tokens, 2000);
        assert_eq!(overview.total_output_tokens, 1000);
        assert_eq!(overview.total_cache_read_tokens, 500);
        assert!((overview.cache_hit_rate - 0.2).abs() < 0.001);
        assert!((overview.success_rate - 0.5).abs() < 0.001);
        assert!((overview.total_cost_usd - 0.03).abs() < 0.001);
    }

    #[test]
    fn group_by_provider_model_and_tool() {
        let storage = Storage::memory().unwrap();
        storage
            .insert_records(&[
                sample_record(
                    "2026-07-10T10:00:00Z",
                    "cursor",
                    "openai",
                    "gpt-4o",
                    1000,
                    500,
                    0,
                    0.01,
                    Some(200),
                ),
                sample_record(
                    "2026-07-10T11:00:00Z",
                    "cursor",
                    "openai",
                    "gpt-4o",
                    2000,
                    1000,
                    0,
                    0.02,
                    Some(200),
                ),
            ])
            .unwrap();

        let providers = storage.get_provider_stats(&RequestFilter::default()).unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].provider, "openai");
        assert_eq!(providers[0].request_count, 2);
        assert_eq!(providers[0].total_tokens, 4500);

        let models = storage.get_model_stats(&RequestFilter::default()).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].model, "gpt-4o");
        assert_eq!(models[0].total_input, 3000);

        let tools = storage.get_tool_stats(&RequestFilter::default()).unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].collector, "cursor");
    }

    #[test]
    fn daily_trends_group_by_date() {
        let storage = Storage::memory().unwrap();
        storage
            .insert_records(&[
                sample_record(
                    "2026-07-10T10:00:00Z",
                    "cursor",
                    "openai",
                    "gpt-4o",
                    1000,
                    500,
                    0,
                    0.01,
                    Some(200),
                ),
                sample_record(
                    "2026-07-11T10:00:00Z",
                    "cursor",
                    "openai",
                    "gpt-4o",
                    2000,
                    1000,
                    0,
                    0.02,
                    Some(200),
                ),
            ])
            .unwrap();

        let trends = storage.get_daily_trends(&RequestFilter::default()).unwrap();
        assert_eq!(trends.len(), 2);
        assert!(trends.iter().any(|t| t.date == "2026-07-10"));
        assert!(trends.iter().any(|t| t.date == "2026-07-11"));
    }

    #[test]
    fn token_heatmap_aggregates_same_day_and_zero_fills() {
        use chrono::{Duration, Utc};

        let storage = Storage::memory().unwrap();
        let today = Utc::now().date_naive();
        let day_a = (today - Duration::days(2)).format("%Y-%m-%d").to_string();
        let day_b = today.format("%Y-%m-%d").to_string();
        let gap = (today - Duration::days(1)).format("%Y-%m-%d").to_string();

        storage
            .insert_records(&[
                sample_record(
                    &format!("{day_a}T10:00:00Z"),
                    "cursor",
                    "openai",
                    "gpt-4o",
                    1000,
                    500,
                    0,
                    0.01,
                    Some(200),
                ),
                sample_record(
                    &format!("{day_a}T15:00:00Z"),
                    "cursor",
                    "openai",
                    "gpt-4o",
                    2000,
                    1000,
                    0,
                    0.02,
                    Some(200),
                ),
                sample_record(
                    &format!("{day_b}T10:00:00Z"),
                    "claude_code",
                    "anthropic",
                    "claude-sonnet-4",
                    500,
                    250,
                    0,
                    0.005,
                    Some(200),
                ),
            ])
            .unwrap();

        let heatmap = storage
            .get_token_heatmap(&RequestFilter::default(), 7)
            .unwrap();

        assert_eq!(heatmap.period_days, 7);
        assert_eq!(heatmap.days.len(), 7);

        let active = heatmap
            .days
            .iter()
            .find(|d| d.date == day_a)
            .expect("active day");
        assert_eq!(active.total_tokens, 4500);
        assert_eq!(active.request_count, 2);
        assert!((active.total_cost_usd - 0.03).abs() < 0.001);

        let empty = heatmap
            .days
            .iter()
            .find(|d| d.date == gap)
            .expect("gap day");
        assert_eq!(empty.total_tokens, 0);
        assert_eq!(empty.request_count, 0);
    }

    #[test]
    fn token_heatmap_respects_provider_filter() {
        use chrono::Utc;

        let storage = Storage::memory().unwrap();
        let today = Utc::now().date_naive().format("%Y-%m-%d").to_string();

        storage
            .insert_records(&[
                sample_record(
                    &format!("{today}T10:00:00Z"),
                    "cursor",
                    "openai",
                    "gpt-4o",
                    1000,
                    500,
                    0,
                    0.01,
                    Some(200),
                ),
                sample_record(
                    &format!("{today}T11:00:00Z"),
                    "claude_code",
                    "anthropic",
                    "claude-sonnet-4",
                    800,
                    400,
                    0,
                    0.008,
                    Some(200),
                ),
            ])
            .unwrap();

        let filter = RequestFilter {
            provider: Some("openai".to_string()),
            ..Default::default()
        };
        let heatmap = storage.get_token_heatmap(&filter, 3).unwrap();
        let openai_day = heatmap
            .days
            .iter()
            .find(|d| d.date == today)
            .expect("today");
        assert_eq!(openai_day.total_tokens, 1500);
        assert_eq!(openai_day.request_count, 1);
    }

    #[test]
    fn group_by_project_from_source_paths() {
        let storage = Storage::memory().unwrap();
        let mut claude_a = sample_record(
            "2026-07-10T10:00:00Z",
            "claude_code",
            "anthropic",
            "claude-sonnet-4",
            1000,
            500,
            0,
            0.01,
            Some(200),
        );
        claude_a.source_file = Some(
            "/home/user/.claude/projects/-home-user-dev-alltokens/usage/2026-07-10.jsonl"
                .to_string(),
        );

        let mut claude_b = sample_record(
            "2026-07-10T11:00:00Z",
            "claude_code",
            "anthropic",
            "claude-sonnet-4",
            2000,
            1000,
            0,
            0.02,
            Some(200),
        );
        claude_b.source_file = Some(
            "/home/user/.claude/projects/-home-user-dev-other/usage/2026-07-10.jsonl".to_string(),
        );

        let mut codex = sample_record(
            "2026-07-10T12:00:00Z",
            "codex",
            "openai",
            "gpt-4o",
            500,
            250,
            0,
            0.005,
            Some(200),
        );
        codex.source_file =
            Some("/home/user/.codex/sessions/2026/07/10/rollout-sess-abc.jsonl".to_string());

        storage
            .insert_records(&[claude_a, claude_b, codex])
            .unwrap();

        let stats = storage.get_project_stats(&RequestFilter::default()).unwrap();
        assert_eq!(stats.len(), 3);
        assert_eq!(stats[0].project, "other");
        assert_eq!(stats[0].total_tokens, 3000);
        assert_eq!(stats[1].project, "alltokens");
        assert_eq!(stats[1].total_tokens, 1500);
        assert_eq!(stats[2].project, "sess-abc");
        assert_eq!(stats[2].total_tokens, 750);
    }

    #[test]
    fn session_stats_group_and_duration() {
        let storage = Storage::memory().unwrap();
        let mut a1 = sample_record(
            "2026-07-10T10:00:00Z",
            "codex",
            "openai",
            "gpt-4o",
            100,
            50,
            0,
            0.001,
            Some(200),
        );
        a1.session_id = Some("sess-a".to_string());
        let mut a2 = sample_record(
            "2026-07-10T10:05:00Z",
            "codex",
            "openai",
            "gpt-4o",
            200,
            100,
            0,
            0.002,
            Some(200),
        );
        a2.session_id = Some("sess-a".to_string());
        let mut b1 = sample_record(
            "2026-07-10T11:00:00Z",
            "claude_code",
            "anthropic",
            "claude-sonnet-4",
            300,
            150,
            0,
            0.01,
            Some(200),
        );
        b1.session_id = Some("sess-b".to_string());
        // 无 session_id 的记录应被排除
        let orphan = sample_record(
            "2026-07-10T12:00:00Z",
            "cursor",
            "openai",
            "gpt-4o",
            999,
            999,
            0,
            0.5,
            Some(200),
        );

        storage.insert_records(&[a1, a2, b1, orphan]).unwrap();

        let stats = storage.get_session_stats(&RequestFilter::default()).unwrap();
        assert_eq!(stats.len(), 2, "only sessions with a session_id are grouped");
        // ORDER BY MAX(timestamp) DESC -> sess-b (11:00) first
        assert_eq!(stats[0].session_id, "sess-b");
        assert_eq!(stats[0].request_count, 1);
        assert_eq!(stats[0].duration_secs, 0);

        assert_eq!(stats[1].session_id, "sess-a");
        assert_eq!(stats[1].request_count, 2);
        assert_eq!(stats[1].total_tokens, 450);
        assert_eq!(stats[1].duration_secs, 300, "10:00 -> 10:05 = 300s");
    }

    #[test]
    fn tool_invocation_ranking_from_raw_json_and_notes() {
        let storage = Storage::memory().unwrap();
        let mut bash = sample_record(
            "2026-07-10T10:00:00Z",
            "claude_code",
            "anthropic",
            "claude-sonnet-4",
            0,
            0,
            0,
            0.0,
            None,
        );
        bash.raw_json = Some(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{}}]}}"#
                .to_string(),
        );

        let mut read = sample_record(
            "2026-07-10T10:01:00Z",
            "claude_code",
            "anthropic",
            "claude-sonnet-4",
            0,
            0,
            0,
            0.0,
            None,
        );
        read.tool = Some("Read".to_string());
        read.notes = Some(crate::invocation::NOTE_INVOCATION_TOOL.to_string());
        read.raw_json = Some(r#"{"type":"tool_use","name":"Read"}"#.to_string());

        let mut skill = sample_record(
            "2026-07-10T10:02:00Z",
            "claude_code",
            "anthropic",
            "claude-sonnet-4",
            0,
            0,
            0,
            0.0,
            None,
        );
        skill.tool = Some("canvas".to_string());
        skill.notes = Some(crate::invocation::NOTE_INVOCATION_SKILL.to_string());
        skill.raw_json = Some(
            r#"{"type":"tool_use","name":"Skill","input":{"skill":"canvas"}}"#.to_string(),
        );

        storage.insert_records(&[bash, read, skill]).unwrap();

        let tools = storage
            .get_tool_invocation_ranking(&RequestFilter::default())
            .unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "Bash");
        assert_eq!(tools[0].invocation_count, 1);
        assert_eq!(tools[1].name, "Read");
        assert_eq!(tools[1].invocation_count, 1);

        let skills = storage
            .get_skill_invocation_ranking(&RequestFilter::default())
            .unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "canvas");
        assert_eq!(skills[0].invocation_count, 1);
    }

    #[test]
    fn collector_state_roundtrip() {
        let storage = Storage::memory().unwrap();
        assert!(storage.get_collector_state("claude_code").unwrap().is_none());

        storage
            .set_collector_state("claude_code", "2026-07-10T12:00:00Z", r#"{"offset":42}"#)
            .unwrap();

        assert_eq!(
            storage.get_collector_state("claude_code").unwrap(),
            Some(r#"{"offset":42}"#.to_string())
        );

        let last_scan = storage.get_last_scan("claude_code").unwrap().unwrap();
        assert_eq!(last_scan, Utc.with_ymd_and_hms(2026, 7, 10, 12, 0, 0).unwrap());
    }

    #[test]
    fn date_range_filter() {
        let storage = Storage::memory().unwrap();
        storage
            .insert_records(&[
                sample_record(
                    "2026-07-09T10:00:00Z",
                    "cursor",
                    "openai",
                    "gpt-4o",
                    100,
                    50,
                    0,
                    0.001,
                    Some(200),
                ),
                sample_record(
                    "2026-07-10T10:00:00Z",
                    "cursor",
                    "openai",
                    "gpt-4o",
                    200,
                    100,
                    0,
                    0.002,
                    Some(200),
                ),
            ])
            .unwrap();

        let filter = RequestFilter {
            start_date: Some("2026-07-10T00:00:00Z".to_string()),
            ..Default::default()
        };
        let result = storage.get_requests(&filter, &Pagination::default()).unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.items[0].input_tokens, 200);
    }

    #[test]
    fn export_requests_returns_all_matches() {
        let storage = Storage::memory().unwrap();
        storage
            .insert_records(&[
                sample_record(
                    "2026-07-10T10:00:00Z",
                    "claude_code",
                    "anthropic",
                    "claude-sonnet-4",
                    1000,
                    500,
                    0,
                    0.01,
                    Some(200),
                ),
                sample_record(
                    "2026-07-10T11:00:00Z",
                    "cursor",
                    "openai",
                    "gpt-4o",
                    2000,
                    1000,
                    0,
                    0.02,
                    Some(200),
                ),
            ])
            .unwrap();

        let exported = storage
            .export_requests(&RequestFilter {
                provider: Some("openai".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(exported.len(), 1);
        assert_eq!(exported[0].provider, "openai");
    }

    #[test]
    fn budget_config_round_trip() {
        let storage = Storage::memory().unwrap();
        let config = BudgetConfig {
            monthly_usd: Some(100.0),
            enabled: true,
        };
        storage.set_budget_config(&config).unwrap();
        let loaded = storage.get_budget_config().unwrap();
        assert_eq!(loaded.monthly_usd, Some(100.0));
        assert!(loaded.enabled);
    }

    #[test]
    fn widget_config_round_trip() {
        let storage = Storage::memory().unwrap();
        // 缺省：隐藏 + 无位置
        let default = storage.get_widget_config().unwrap();
        assert!(!default.visible);
        assert_eq!(default.x, None);

        let config = WidgetConfig {
            visible: true,
            x: Some(120),
            y: Some(80),
        };
        storage.set_widget_config(&config).unwrap();
        let loaded = storage.get_widget_config().unwrap();
        assert!(loaded.visible);
        assert_eq!(loaded.x, Some(120));
        assert_eq!(loaded.y, Some(80));

        // 仅位置更新（窗口拖动后持久化），visible 保持不变
        let moved = WidgetConfig {
            visible: loaded.visible,
            x: Some(400),
            y: Some(300),
        };
        storage.set_widget_config(&moved).unwrap();
        let loaded = storage.get_widget_config().unwrap();
        assert!(loaded.visible);
        assert_eq!(loaded.x, Some(400));
    }

    #[test]
    fn subscription_config_round_trip() {
        let storage = Storage::memory().unwrap();
        let config = SubscriptionConfig {
            tiers: vec![
                SubscriptionTier { label: "Claude Max".to_string(), monthly_usd: 100.0 },
                SubscriptionTier { label: "Codex Plus".to_string(), monthly_usd: 20.0 },
            ],
            enabled: true,
        };
        storage.set_subscription_config(&config).unwrap();
        let loaded = storage.get_subscription_config().unwrap();
        assert_eq!(loaded.tiers.len(), 2);
        assert_eq!(loaded.tiers[0].label, "Claude Max");
        assert_eq!(loaded.tiers[0].monthly_usd, 100.0);
        assert_eq!(loaded.tiers[1].monthly_usd, 20.0);
        assert!(loaded.enabled);
    }

    #[test]
    fn pricing_config_and_engine_load() {
        use crate::pricing::PricingEntry;

        let storage = Storage::memory().unwrap();
        let config = PricingConfig {
            usd_to_cny: Some(8.0),
            overrides: vec![PricingEntry {
                provider: "openai".to_string(),
                model: "gpt-4o".to_string(),
                input_per_mtok: 1.5,
                output_per_mtok: 3.0,
                cache_read_per_mtok: 0.0,
                cache_create_per_mtok: 0.0,
                context_window: 0,
            }],
        };
        storage.set_pricing_config(&config).unwrap();
        let engine = storage.load_pricing_engine().unwrap();
        assert!((engine.usd_to_cny() - 8.0).abs() < f64::EPSILON);
        assert!((engine.find("openai", "gpt-4o").unwrap().input_per_mtok - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn collector_enable_defaults_true() {
        let storage = Storage::memory().unwrap();
        assert!(storage.is_collector_enabled("cursor").unwrap());

        let mut enabled = std::collections::HashMap::new();
        enabled.insert("cursor".to_string(), false);
        storage
            .set_collectors_config(&CollectorsConfig { enabled })
            .unwrap();
        assert!(!storage.is_collector_enabled("cursor").unwrap());
        assert!(storage.is_collector_enabled("claude_code").unwrap());
    }

    #[test]
    fn general_config_round_trip() {
        let storage = Storage::memory().unwrap();
        let config = GeneralConfig {
            auto_scan_interval_minutes: 15,
            launch_at_startup: true,
        };
        storage.set_general_config(&config).unwrap();
        let loaded = storage.get_general_config().unwrap();
        assert_eq!(loaded.auto_scan_interval_minutes, 15);
        assert!(loaded.launch_at_startup);
    }

    #[test]
    fn data_config_round_trip_and_purge() {
        let storage = Storage::memory().unwrap();
        let old_ts = (Utc::now() - chrono::Duration::days(120)).to_rfc3339();
        let recent_ts = (Utc::now() - chrono::Duration::days(1)).to_rfc3339();
        let old = sample_record(&old_ts, "test", "openai", "gpt-4", 10, 5, 0, 0.01, None);
        let recent = sample_record(&recent_ts, "test", "openai", "gpt-4", 10, 5, 0, 0.01, None);
        storage.insert_record(&old).unwrap();
        storage.insert_record(&recent).unwrap();

        storage
            .set_data_config(&DataConfig { retention_days: 90 })
            .unwrap();
        let loaded = storage.get_data_config().unwrap();
        assert_eq!(loaded.retention_days, 90);

        let deleted = storage.purge_records_older_than_days(90).unwrap();
        assert_eq!(deleted, 1);

        let remaining = storage
            .export_requests(&RequestFilter::default())
            .unwrap();
        assert_eq!(remaining.len(), 1);
    }

    #[test]
    fn hour_of_week_groups_by_local_weekday_and_hour() {
        use chrono::{Datelike, Timelike};

        let storage = Storage::memory().unwrap();
        // Two records at nearby UTC instants share the same local cell;
        // a third one lands in a different hour.
        let t1 = "2026-07-14T10:00:00+00:00";
        let t2 = "2026-07-14T10:30:00+00:00";
        let t3 = "2026-07-15T22:00:00+00:00";
        storage
            .insert_record(&sample_record(t1, "test", "openai", "gpt-4", 100, 50, 0, 0.01, None))
            .unwrap();
        storage
            .insert_record(&sample_record(t2, "test", "openai", "gpt-4", 200, 50, 0, 0.01, None))
            .unwrap();
        storage
            .insert_record(&sample_record(t3, "test", "openai", "gpt-4", 10, 5, 0, 0.01, None))
            .unwrap();

        let cells = storage.get_hour_of_week(&RequestFilter::default()).unwrap();
        assert_eq!(cells.len(), 2);

        // Expected cell derived with the same system timezone SQLite uses.
        let local1 = DateTime::parse_from_rfc3339(t1)
            .unwrap()
            .with_timezone(&chrono::Local);
        let cell = cells
            .iter()
            .find(|c| {
                c.weekday == local1.weekday().num_days_from_sunday() as u8
                    && c.hour == local1.hour() as u8
            })
            .expect("pair cell must exist");
        assert_eq!(cell.request_count, 2);
        assert_eq!(cell.total_tokens, 400); // (100+50) + (200+50)

        let local3 = DateTime::parse_from_rfc3339(t3)
            .unwrap()
            .with_timezone(&chrono::Local);
        let cell3 = cells
            .iter()
            .find(|c| {
                c.weekday == local3.weekday().num_days_from_sunday() as u8
                    && c.hour == local3.hour() as u8
            })
            .expect("third cell must exist");
        assert_eq!(cell3.request_count, 1);
        assert_eq!(cell3.total_tokens, 15);
        assert!(cells.iter().all(|c| c.weekday <= 6 && c.hour <= 23));
    }

    #[test]
    fn merge_from_inserts_only_new_records_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let src_path = dir.path().join("src.db");
        let src = Storage::open(&src_path).unwrap();
        src.insert_records(&[
            sample_record("2026-07-10T10:00:00Z", "codex", "openai", "gpt-4o", 100, 50, 0, 0.01, None),
            sample_record("2026-07-11T10:00:00Z", "codex", "openai", "gpt-4o", 200, 60, 0, 0.02, None),
        ])
        .unwrap();
        drop(src);

        let dst = Storage::memory().unwrap();
        // 与源库完全相同的一条记录 → 合并时应跳过
        dst.insert_record(&sample_record(
            "2026-07-10T10:00:00Z", "codex", "openai", "gpt-4o", 100, 50, 0, 0.01, None,
        ))
        .unwrap();

        let result = dst.merge_from(&src_path).unwrap();
        assert_eq!(result.scanned, 2);
        assert_eq!(result.inserted, 1);
        assert_eq!(result.skipped, 1);
        assert_eq!(dst.count().unwrap(), 2);

        // 幂等：再次合并不再插入
        let again = dst.merge_from(&src_path).unwrap();
        assert_eq!(again.inserted, 0);
        assert_eq!(again.skipped, 2);
        assert_eq!(dst.count().unwrap(), 2);
    }

    #[test]
    fn merge_from_rejects_db_without_api_requests() {
        let dir = tempfile::tempdir().unwrap();
        let alien = dir.path().join("alien.db");
        let conn = rusqlite::Connection::open(&alien).unwrap();
        conn.execute_batch("CREATE TABLE something_else (id INTEGER);")
            .unwrap();
        drop(conn);

        let dst = Storage::memory().unwrap();
        let err = dst.merge_from(&alien).unwrap_err();
        assert!(matches!(err, StorageError::InvalidSource));
    }

    #[test]
    fn checkpoint_then_file_copy_yields_consistent_snapshot() {
        // 模拟 CLI `sync export`：checkpoint 截断 WAL 后复制主文件。
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("data.db");
        let snapshot = dir.path().join("snapshot.db");

        let storage = Storage::open(&db_path).unwrap();
        storage
            .insert_record(&sample_record(
                "2026-07-10T10:00:00Z", "codex", "openai", "gpt-4o", 100, 50, 0, 0.01, None,
            ))
            .unwrap();
        storage.checkpoint().unwrap();
        std::fs::copy(&db_path, &snapshot).unwrap();
        drop(storage);

        let restored = Storage::open(&snapshot).unwrap();
        assert_eq!(restored.count().unwrap(), 1);
    }
}
