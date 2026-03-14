//! Codex 日志持久化存储
//!
//! 使用 SQLite 存储请求日志，支持高效的查询和筛选

use rusqlite::{Connection, params};
use std::collections::BTreeMap;
use std::path::PathBuf;

use super::models::{
    DailyStats, DailyStatsResponse, GatewayDailyStatsResponse, GatewayDailyStatsSeries, LogPage,
    LogQuery, ModelTokenStats, PeriodTokenStats, RequestLog,
};

/// 日志存储管理器
#[derive(Debug)]
pub struct CodexLogStorage {
    db_path: PathBuf,
}

impl CodexLogStorage {
    /// 创建新的日志存储
    pub fn new(data_dir: PathBuf) -> Result<Self, String> {
        let logs_dir = data_dir.join("logs");
        std::fs::create_dir_all(&logs_dir)
            .map_err(|e| format!("Failed to create logs directory: {}", e))?;

        let db_path = logs_dir.join("codex_logs.db");
        let storage = Self { db_path };

        // 初始化数据库表
        let mut conn = storage
            .get_connection()
            .map_err(|e| format!("Failed to open database: {}", e))?;
        storage
            .init_schema(&mut conn)
            .map_err(|e| format!("Failed to initialize schema: {}", e))?;

        Ok(storage)
    }

    /// 获取数据库连接
    fn get_connection(&self) -> Result<Connection, String> {
        Connection::open(&self.db_path)
            .map_err(|e| format!("Failed to open database connection: {}", e))
    }

    /// 初始化数据库表结构
    fn init_schema(&self, conn: &mut Connection) -> Result<(), String> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS codex_requests (
                id TEXT PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                account_id TEXT NOT NULL,
                account_email TEXT NOT NULL,
                model TEXT NOT NULL,
                format TEXT NOT NULL,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL,
                error_message TEXT,
                request_duration_ms INTEGER,
                date_key INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|e| format!("Failed to create table: {}", e))?;

        ensure_column_exists(conn, "codex_requests", "gateway_profile_id", "TEXT")?;
        ensure_column_exists(conn, "codex_requests", "gateway_profile_name", "TEXT")?;

        // 创建索引
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_timestamp ON codex_requests(timestamp DESC)",
            [],
        )
        .map_err(|e| format!("Failed to create timestamp index: {}", e))?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_date_key ON codex_requests(date_key)",
            [],
        )
        .map_err(|e| format!("Failed to create date_key index: {}", e))?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_status ON codex_requests(status)",
            [],
        )
        .map_err(|e| format!("Failed to create status index: {}", e))?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_model ON codex_requests(model)",
            [],
        )
        .map_err(|e| format!("Failed to create model index: {}", e))?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_gateway_profile_id ON codex_requests(gateway_profile_id)",
            [],
        )
        .map_err(|e| format!("Failed to create gateway profile index: {}", e))?;

        Ok(())
    }

    /// 批量写入日志（内部使用，由 flush 方法调用）
    fn write_logs_internal(&self, logs: &[RequestLog]) -> Result<(), String> {
        if logs.is_empty() {
            return Ok(());
        }

        let mut conn = self.get_connection()?;
        let tx = conn
            .transaction()
            .map_err(|e| format!("Failed to begin transaction: {}", e))?;

        for log in logs {
            let date_key = calculate_date_key(log.timestamp);

            tx.execute(
                "INSERT OR REPLACE INTO codex_requests
                 (id, timestamp, account_id, account_email, model, format,
                  input_tokens, output_tokens, total_tokens, status,
                  error_message, request_duration_ms, date_key,
                  gateway_profile_id, gateway_profile_name)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    log.id.clone(),
                    log.timestamp,
                    log.account_id.clone(),
                    log.account_email.clone(),
                    log.model.clone(),
                    log.format.clone(),
                    log.input_tokens,
                    log.output_tokens,
                    log.total_tokens,
                    log.status.clone(),
                    log.error_message.clone(),
                    log.request_duration_ms,
                    date_key,
                    log.gateway_profile_id.clone(),
                    log.gateway_profile_name.clone(),
                ],
            )
            .map_err(|e| format!("Failed to insert log: {}", e))?;
        }

        tx.commit()
            .map_err(|e| format!("Failed to commit transaction: {}", e))?;
        Ok(())
    }

    /// 添加日志（立即写入数据库）
    pub async fn add_log(&self, log: RequestLog) {
        // 直接写入数据库，不使用缓冲区
        if let Err(e) = self.write_logs_internal(&[log]) {
            eprintln!("[CodexLog] Failed to write log to database: {}", e);
        }
    }

    /// 刷新缓冲区到数据库（已改为立即写入，此方法保留为空操作以保持兼容性）
    pub async fn flush(&self) {
        // 由于已改为立即写入模式，缓冲区始终为空，无需操作
    }

    /// 查询日志
    pub fn query_logs(&self, query: &LogQuery) -> Result<LogPage, String> {
        let conn = self.get_connection()?;

        let mut sql = String::from("SELECT * FROM codex_requests WHERE 1=1");

        if let Some(start_ts) = query.start_ts {
            sql.push_str(&format!(" AND timestamp >= {}", start_ts));
        }
        if let Some(end_ts) = query.end_ts {
            sql.push_str(&format!(" AND timestamp <= {}", end_ts));
        }
        if let Some(model) = &query.model {
            if !model.trim().is_empty() {
                sql.push_str(&format!(
                    " AND model LIKE '%{}%'",
                    model.trim().replace('\'', "''")
                ));
            }
        }
        if let Some(format) = &query.format {
            if !format.trim().is_empty() {
                sql.push_str(&format!(
                    " AND format LIKE '%{}%'",
                    format.trim().replace('\'', "''")
                ));
            }
        }
        if let Some(status) = &query.status {
            if !status.trim().is_empty() {
                sql.push_str(&format!(" AND status = '{}'", status.trim()));
            }
        }
        if let Some(account_id) = &query.account_id {
            if !account_id.trim().is_empty() {
                sql.push_str(&format!(
                    " AND account_id = '{}'",
                    account_id.trim().replace('\'', "''")
                ));
            }
        }

        // 先获取总数
        let count_sql = sql.replace("SELECT *", "SELECT COUNT(*)");
        let total: i64 = conn
            .query_row(&count_sql, [], |row| row.get(0))
            .map_err(|e| format!("Failed to count logs: {}", e))?;
        let total = total as usize;

        // 排序和分页
        sql.push_str(&format!(
            " ORDER BY timestamp DESC LIMIT {} OFFSET {}",
            query.limit.unwrap_or(100).max(1),
            query.offset.unwrap_or(0)
        ));

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let mut items = Vec::new();
        let log_rows = stmt
            .query_map([], |row| {
                Ok(RequestLog {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    account_id: row.get(2)?,
                    account_email: row.get(3)?,
                    model: row.get(4)?,
                    format: row.get(5)?,
                    input_tokens: row.get(6)?,
                    output_tokens: row.get(7)?,
                    total_tokens: row.get(8)?,
                    status: row.get(9)?,
                    error_message: row.get(10)?,
                    request_duration_ms: row.get(11)?,
                    gateway_profile_id: row.get(13)?,
                    gateway_profile_name: row.get(14)?,
                })
            })
            .map_err(|e| format!("Failed to execute query: {}", e))?;

        for log in log_rows {
            items.push(log.map_err(|e| format!("Failed to read log row: {}", e))?);
        }

        Ok(LogPage { total, items })
    }

    /// 获取模型统计
    pub fn get_model_stats(
        &self,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<ModelTokenStats>, String> {
        let conn = self.get_connection()?;

        let mut stmt = conn
            .prepare(
                "SELECT model,
                    COUNT(*) as requests,
                    SUM(input_tokens) as input_tokens,
                    SUM(output_tokens) as output_tokens,
                    SUM(total_tokens) as total_tokens
             FROM codex_requests
             WHERE timestamp >= ?1 AND timestamp <= ?2
             GROUP BY model
             ORDER BY total_tokens DESC",
            )
            .map_err(|e| format!("Failed to prepare stats query: {}", e))?;

        let mut stats = Vec::new();
        let rows = stmt
            .query_map([start_ts, end_ts], |row| {
                Ok(ModelTokenStats {
                    model: row.get(0)?,
                    requests: row.get(1)?,
                    input_tokens: row.get(2)?,
                    output_tokens: row.get(3)?,
                    total_tokens: row.get(4)?,
                })
            })
            .map_err(|e| format!("Failed to execute stats query: {}", e))?;

        for stat in rows {
            stats.push(stat.map_err(|e| format!("Failed to read stat row: {}", e))?);
        }

        Ok(stats)
    }

    /// 获取周期统计
    pub fn get_period_stats(&self, now_ts: i64) -> Result<PeriodTokenStats, String> {
        let conn = self.get_connection()?;

        let calculate_stats = |period_start: i64| -> Result<(u64, u64), String> {
            let mut stmt = conn
                .prepare(
                    "SELECT COUNT(*) as requests, SUM(total_tokens) as tokens
                 FROM codex_requests
                 WHERE timestamp >= ?1 AND timestamp <= ?2",
                )
                .map_err(|e| format!("Failed to prepare period query: {}", e))?;

            let mut result = (0u64, 0u64);
            let rows = stmt
                .query_map([period_start, now_ts], |row| {
                    Ok((row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)? as u64))
                })
                .map_err(|e| format!("Failed to execute period query: {}", e))?;

            for row in rows {
                result = row.map_err(|e| format!("Failed to read period result: {}", e))?;
            }
            Ok(result)
        };

        let now = chrono::DateTime::from_timestamp(now_ts, 0)
            .unwrap_or_else(chrono::Utc::now)
            .with_timezone(&chrono::Utc);

        let today_start = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map(|d| d.and_utc().timestamp())
            .unwrap_or(0);
        let week_start = (now - chrono::Duration::days(6))
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map(|d| d.and_utc().timestamp())
            .unwrap_or(0);
        let month_start = (now - chrono::Duration::days(29))
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map(|d| d.and_utc().timestamp())
            .unwrap_or(0);

        let (today_requests, today_tokens) = calculate_stats(today_start)?;
        let (week_requests, week_tokens) = calculate_stats(week_start)?;
        let (month_requests, month_tokens) = calculate_stats(month_start)?;

        Ok(PeriodTokenStats {
            today_requests,
            today_tokens,
            week_requests,
            week_tokens,
            month_requests,
            month_tokens,
        })
    }

    /// 获取每日统计数据（过去30天）
    pub fn get_daily_stats(&self, days: u32) -> Result<DailyStatsResponse, String> {
        let conn = self.get_connection()?;

        let mut stats = Vec::new();
        let now = chrono::Utc::now();

        for i in 0..days {
            let date = (now - chrono::Duration::days(i as i64)).date_naive();
            let date_str = date.format("%Y-%m-%d").to_string();

            let day_start = date
                .and_hms_opt(0, 0, 0)
                .map(|d| d.and_utc().timestamp())
                .unwrap_or(0);
            let day_end = date
                .and_hms_opt(23, 59, 59)
                .map(|d| d.and_utc().timestamp())
                .unwrap_or(0);

            let mut stmt = conn
                .prepare(
                    "SELECT COUNT(*) as requests, SUM(total_tokens) as tokens
                 FROM codex_requests
                 WHERE timestamp >= ?1 AND timestamp <= ?2",
                )
                .map_err(|e| format!("Failed to prepare daily stats query: {}", e))?;

            let mut result = (0u64, 0u64);
            let rows = stmt
                .query_map([day_start, day_end], |row| {
                    let requests: i64 = row.get(0)?;
                    let tokens: Option<i64> = row.get(1)?;
                    Ok((requests as u64, tokens.unwrap_or(0) as u64))
                })
                .map_err(|e| format!("Failed to execute daily stats query: {}", e))?;

            for row in rows {
                result = row.map_err(|e| format!("Failed to read daily stats result: {}", e))?;
            }

            stats.push(DailyStats {
                date: date_str,
                requests: result.0,
                tokens: result.1,
            });
        }

        stats.reverse();
        Ok(DailyStatsResponse { stats })
    }

    pub fn get_daily_stats_by_gateway_profile(
        &self,
        days: u32,
    ) -> Result<GatewayDailyStatsResponse, String> {
        let conn = self.get_connection()?;
        let now = chrono::Utc::now();
        let day_count = days.max(1);

        let dates: Vec<_> = (0..day_count)
            .rev()
            .map(|offset| now - chrono::Duration::days(offset as i64))
            .map(|dt| dt.date_naive())
            .collect();

        let start_ts = dates
            .first()
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .map(|dt| dt.and_utc().timestamp())
            .unwrap_or(0);
        let end_ts = dates
            .last()
            .and_then(|date| date.and_hms_opt(23, 59, 59))
            .map(|dt| dt.and_utc().timestamp())
            .unwrap_or(0);

        let mut stmt = conn
            .prepare(
                "SELECT date_key,
                        COALESCE(gateway_profile_id, 'legacy') AS profile_id,
                        COALESCE(gateway_profile_name, 'Legacy') AS profile_name,
                        COUNT(*) AS requests,
                        COALESCE(SUM(total_tokens), 0) AS tokens
                 FROM codex_requests
                 WHERE timestamp >= ?1 AND timestamp <= ?2
                 GROUP BY date_key, profile_id, profile_name
                 ORDER BY profile_name ASC, date_key ASC",
            )
            .map_err(|e| format!("Failed to prepare gateway daily stats query: {}", e))?;

        let mut grouped: BTreeMap<(String, String), BTreeMap<i64, (u64, u64)>> = BTreeMap::new();
        let rows = stmt
            .query_map([start_ts, end_ts], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)? as u64,
                    row.get::<_, i64>(4)?.max(0) as u64,
                ))
            })
            .map_err(|e| format!("Failed to execute gateway daily stats query: {}", e))?;

        for row in rows {
            let (date_key, profile_id, profile_name, requests, tokens) =
                row.map_err(|e| format!("Failed to read gateway daily stats row: {}", e))?;
            grouped
                .entry((profile_id, profile_name))
                .or_default()
                .insert(date_key, (requests, tokens));
        }

        let mut series = Vec::new();
        for ((profile_id, profile_name), per_day) in grouped {
            let mut stats = Vec::with_capacity(dates.len());
            for date in &dates {
                let date_key = date.format("%Y%m%d").to_string().parse().unwrap_or(0);
                let (requests, tokens) = per_day.get(&date_key).copied().unwrap_or((0, 0));
                stats.push(DailyStats {
                    date: date.format("%Y-%m-%d").to_string(),
                    requests,
                    tokens,
                });
            }
            series.push(GatewayDailyStatsSeries {
                profile_id,
                profile_name,
                stats,
            });
        }

        Ok(GatewayDailyStatsResponse { series })
    }

    /// 清空所有日志
    pub fn clear_all(&self) -> Result<usize, String> {
        let conn = self.get_connection()?;
        let count = conn
            .execute("DELETE FROM codex_requests", [])
            .map_err(|e| format!("Failed to clear logs: {}", e))?;
        Ok(count)
    }

    /// 删除指定日期之前的日志
    pub fn delete_before(&self, date_key: i64) -> Result<usize, String> {
        let conn = self.get_connection()?;
        let count = conn
            .execute("DELETE FROM codex_requests WHERE date_key < ?1", [date_key])
            .map_err(|e| format!("Failed to delete old logs: {}", e))?;
        Ok(count)
    }

    /// 获取数据库大小（字节）
    pub fn db_size(&self) -> Result<u64, String> {
        let metadata = std::fs::metadata(&self.db_path)
            .map_err(|e| format!("Failed to get db metadata: {}", e))?;
        Ok(metadata.len())
    }

    /// 获取日志总数
    pub fn total_logs(&self) -> Result<i64, String> {
        let conn = self.get_connection()?;
        conn.query_row("SELECT COUNT(*) FROM codex_requests", [], |row| row.get(0))
            .map_err(|e| format!("Failed to count logs: {}", e))
    }

    /// 获取全时间累计统计
    pub fn get_all_time_stats(&self) -> Result<(u64, u64), String> {
        let conn = self.get_connection()?;
        conn.query_row(
            "SELECT COUNT(*) as requests, SUM(total_tokens) as tokens FROM codex_requests",
            [],
            |row| {
                let requests: i64 = row.get(0)?;
                let tokens: i64 = row.get(1)?;
                Ok((requests.max(0) as u64, tokens.max(0) as u64))
            },
        )
        .map_err(|e| format!("Failed to get all time stats: {}", e))
    }
}

/// 计算日期键 (YYYYMMDD)
fn calculate_date_key(timestamp: i64) -> i64 {
    let dt = chrono::DateTime::from_timestamp(timestamp, 0).unwrap_or_else(chrono::Utc::now);
    let date = dt.format("%Y%m%d").to_string();
    date.parse().unwrap_or(0)
}

fn ensure_column_exists(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    let pragma = format!("PRAGMA table_info({})", table);
    let mut stmt = conn
        .prepare(&pragma)
        .map_err(|e| format!("Failed to inspect {} schema: {}", table, e))?;
    let mut rows = stmt
        .query([])
        .map_err(|e| format!("Failed to query {} schema: {}", table, e))?;

    while let Some(row) = rows
        .next()
        .map_err(|e| format!("Failed to iterate {} schema rows: {}", table, e))?
    {
        let existing: String = row
            .get(1)
            .map_err(|e| format!("Failed to read {} schema column name: {}", table, e))?;
        if existing == column {
            return Ok(());
        }
    }

    let sql = format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, definition);
    conn.execute(&sql, [])
        .map_err(|e| format!("Failed to add {}.{}: {}", table, column, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn build_log(
        id: &str,
        timestamp: i64,
        total_tokens: i64,
        gateway_profile_id: Option<&str>,
        gateway_profile_name: Option<&str>,
    ) -> RequestLog {
        RequestLog {
            id: id.to_string(),
            timestamp,
            account_id: "account-1".to_string(),
            account_email: "user@example.com".to_string(),
            model: "gpt-5".to_string(),
            format: "openai-responses".to_string(),
            input_tokens: total_tokens / 2,
            output_tokens: total_tokens / 2,
            total_tokens,
            status: "success".to_string(),
            error_message: None,
            request_duration_ms: Some(120),
            gateway_profile_id: gateway_profile_id.map(str::to_string),
            gateway_profile_name: gateway_profile_name.map(str::to_string),
        }
    }

    #[test]
    fn codex_storage_groups_daily_stats_by_gateway_profile() {
        let temp_dir = tempdir().unwrap();
        let storage = CodexLogStorage::new(temp_dir.path().to_path_buf()).unwrap();
        let now = chrono::Utc::now().timestamp();
        let yesterday = now - 24 * 60 * 60;

        storage
            .write_logs_internal(&[
                build_log("a", yesterday, 100, Some("codex-user-a"), Some("Alice")),
                build_log("b", now, 200, Some("codex-user-a"), Some("Alice")),
                build_log("c", now, 300, Some("codex-user-b"), Some("Bob")),
            ])
            .unwrap();

        let response = storage.get_daily_stats_by_gateway_profile(2).unwrap();

        assert_eq!(response.series.len(), 2);
        assert_eq!(response.series[0].profile_id, "codex-user-a");
        assert_eq!(response.series[0].profile_name, "Alice");
        assert_eq!(response.series[0].stats.len(), 2);
        assert_eq!(response.series[0].stats[0].tokens, 100);
        assert_eq!(response.series[0].stats[1].tokens, 200);
        assert_eq!(response.series[1].profile_id, "codex-user-b");
        assert_eq!(response.series[1].stats[1].tokens, 300);
    }

    #[test]
    fn codex_storage_falls_back_to_legacy_series_for_historical_logs_without_profile() {
        let temp_dir = tempdir().unwrap();
        let storage = CodexLogStorage::new(temp_dir.path().to_path_buf()).unwrap();
        let now = chrono::Utc::now().timestamp();

        storage
            .write_logs_internal(&[build_log("legacy", now, 42, None, None)])
            .unwrap();

        let response = storage.get_daily_stats_by_gateway_profile(1).unwrap();

        assert_eq!(response.series.len(), 1);
        assert_eq!(response.series[0].profile_id, "legacy");
        assert_eq!(response.series[0].profile_name, "Legacy");
        assert_eq!(response.series[0].stats[0].tokens, 42);
    }
}
