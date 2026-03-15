use rusqlite::{Connection, params};
use std::collections::BTreeMap;
use std::path::PathBuf;

use super::models::{
    DailyStats, DailyStatsResponse, GatewayDailyStatsResponse, GatewayDailyStatsSeries, LogPage,
    LogQuery, LogSummary, ModelTokenStats, PeriodTokenStats, RequestLog,
};

#[derive(Debug)]
pub struct AntigravityLogStorage {
    db_path: PathBuf,
}

fn escape_sql_like_value(value: &str) -> String {
    value.trim().replace('\'', "''")
}

fn build_log_where_clause(query: &LogQuery) -> String {
    let mut clause = String::from(" WHERE 1=1");

    if let Some(start_ts) = query.start_ts {
        clause.push_str(&format!(" AND timestamp >= {}", start_ts));
    }
    if let Some(end_ts) = query.end_ts {
        clause.push_str(&format!(" AND timestamp <= {}", end_ts));
    }
    if let Some(model) = &query.model {
        if !model.trim().is_empty() {
            clause.push_str(&format!(
                " AND model LIKE '%{}%'",
                escape_sql_like_value(model)
            ));
        }
    }
    if let Some(format) = &query.format {
        if !format.trim().is_empty() {
            clause.push_str(&format!(
                " AND format LIKE '%{}%'",
                escape_sql_like_value(format)
            ));
        }
    }
    if let Some(status) = &query.status {
        if !status.trim().is_empty() {
            clause.push_str(&format!(
                " AND status = '{}'",
                escape_sql_like_value(status)
            ));
        }
    }
    if let Some(account_id) = &query.account_id {
        if !account_id.trim().is_empty() {
            clause.push_str(&format!(
                " AND account_id = '{}'",
                escape_sql_like_value(account_id)
            ));
        }
    }
    if let Some(member_code) = &query.member_code {
        if !member_code.trim().is_empty() {
            clause.push_str(&format!(
                " AND member_code = '{}'",
                escape_sql_like_value(member_code)
            ));
        }
    }

    clause
}

impl AntigravityLogStorage {
    pub fn new(data_dir: PathBuf) -> Result<Self, String> {
        let logs_dir = data_dir.join("logs");
        std::fs::create_dir_all(&logs_dir)
            .map_err(|e| format!("Failed to create logs directory: {}", e))?;

        let db_path = logs_dir.join("antigravity_logs.db");
        let storage = Self { db_path };

        let mut conn = storage
            .get_connection()
            .map_err(|e| format!("Failed to open database: {}", e))?;
        storage
            .init_schema(&mut conn)
            .map_err(|e| format!("Failed to initialize schema: {}", e))?;

        Ok(storage)
    }

    fn get_connection(&self) -> Result<Connection, String> {
        Connection::open(&self.db_path)
            .map_err(|e| format!("Failed to open database connection: {}", e))
    }

    fn init_schema(&self, conn: &mut Connection) -> Result<(), String> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS antigravity_requests (
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

        ensure_column_exists(conn, "antigravity_requests", "gateway_profile_id", "TEXT")?;
        ensure_column_exists(conn, "antigravity_requests", "gateway_profile_name", "TEXT")?;
        ensure_column_exists(conn, "antigravity_requests", "member_code", "TEXT")?;
        ensure_column_exists(conn, "antigravity_requests", "role_title", "TEXT")?;
        ensure_column_exists(conn, "antigravity_requests", "display_label", "TEXT")?;
        ensure_column_exists(conn, "antigravity_requests", "api_key_suffix", "TEXT")?;
        ensure_column_exists(conn, "antigravity_requests", "color", "TEXT")?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_antigravity_timestamp ON antigravity_requests(timestamp DESC)",
            [],
        )
        .map_err(|e| format!("Failed to create timestamp index: {}", e))?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_antigravity_date_key ON antigravity_requests(date_key)",
            [],
        )
        .map_err(|e| format!("Failed to create date_key index: {}", e))?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_antigravity_status ON antigravity_requests(status)",
            [],
        )
        .map_err(|e| format!("Failed to create status index: {}", e))?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_antigravity_model ON antigravity_requests(model)",
            [],
        )
        .map_err(|e| format!("Failed to create model index: {}", e))?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_antigravity_gateway_profile_id ON antigravity_requests(gateway_profile_id)",
            [],
        )
        .map_err(|e| format!("Failed to create gateway profile index: {}", e))?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_antigravity_member_code ON antigravity_requests(member_code)",
            [],
        )
        .map_err(|e| format!("Failed to create member_code index: {}", e))?;

        Ok(())
    }

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
                "INSERT OR REPLACE INTO antigravity_requests
                 (id, timestamp, account_id, account_email, model, format,
                  input_tokens, output_tokens, total_tokens, status,
                  error_message, request_duration_ms, date_key,
                  gateway_profile_id, gateway_profile_name, member_code, role_title,
                  display_label, api_key_suffix, color)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
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
                    log.member_code.clone(),
                    log.role_title.clone(),
                    log.display_label.clone(),
                    log.api_key_suffix.clone(),
                    log.color.clone(),
                ],
            )
            .map_err(|e| format!("Failed to insert log: {}", e))?;
        }

        tx.commit()
            .map_err(|e| format!("Failed to commit transaction: {}", e))?;
        Ok(())
    }

    pub async fn add_log(&self, log: RequestLog) {
        if let Err(err) = self.write_logs_internal(&[log]) {
            eprintln!("[AntigravityLog] Failed to write log to database: {}", err);
        }
    }

    pub fn query_logs(&self, query: &LogQuery) -> Result<LogPage, String> {
        let conn = self.get_connection()?;
        let where_clause = build_log_where_clause(query);
        let mut sql = format!("SELECT * FROM antigravity_requests{}", where_clause);

        let count_sql = sql.replace("SELECT *", "SELECT COUNT(*)");
        let total: i64 = conn
            .query_row(&count_sql, [], |row| row.get(0))
            .map_err(|e| format!("Failed to count logs: {}", e))?;
        let total = total as usize;

        sql.push_str(&format!(
            " ORDER BY timestamp DESC LIMIT {} OFFSET {}",
            query.limit.unwrap_or(100).max(1),
            query.offset.unwrap_or(0)
        ));

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let mut items = Vec::new();
        let rows = stmt
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
                    member_code: row.get(15)?,
                    role_title: row.get(16)?,
                    display_label: row.get(17)?,
                    api_key_suffix: row.get(18)?,
                    color: row.get(19)?,
                })
            })
            .map_err(|e| format!("Failed to execute query: {}", e))?;

        for row in rows {
            items.push(row.map_err(|e| format!("Failed to read log row: {}", e))?);
        }

        Ok(LogPage { total, items })
    }

    pub fn get_log_summary(&self, query: &LogQuery) -> Result<LogSummary, String> {
        let conn = self.get_connection()?;
        let sql = format!(
            "SELECT COUNT(*) as total_requests,
                    COALESCE(SUM(CASE WHEN LOWER(status) = 'success' THEN 1 ELSE 0 END), 0) as success_requests,
                    COALESCE(SUM(total_tokens), 0) as total_tokens
             FROM antigravity_requests{}",
            build_log_where_clause(query)
        );

        let (total_requests, success_requests, total_tokens): (i64, i64, i64) = conn
            .query_row(&sql, [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(|e| format!("Failed to query log summary: {}", e))?;

        let total_requests = total_requests.max(0) as u64;
        let success_requests = success_requests.max(0) as u64;
        let error_requests = total_requests.saturating_sub(success_requests);
        let total_tokens = total_tokens.max(0) as u64;
        let success_rate = if total_requests > 0 {
            (success_requests as f64 / total_requests as f64) * 100.0
        } else {
            0.0
        };

        Ok(LogSummary {
            total_requests,
            success_requests,
            error_requests,
            total_tokens,
            success_rate,
        })
    }

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
                 FROM antigravity_requests
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

        for row in rows {
            stats.push(row.map_err(|e| format!("Failed to read stat row: {}", e))?);
        }

        Ok(stats)
    }

    pub fn get_period_stats(&self, now_ts: i64) -> Result<PeriodTokenStats, String> {
        let conn = self.get_connection()?;

        let calculate_stats = |period_start: i64| -> Result<(u64, u64), String> {
            let mut stmt = conn
                .prepare(
                    "SELECT COUNT(*) as requests, SUM(total_tokens) as tokens
                     FROM antigravity_requests
                     WHERE timestamp >= ?1 AND timestamp <= ?2",
                )
                .map_err(|e| format!("Failed to prepare period query: {}", e))?;

            let mut result = (0u64, 0u64);
            let rows = stmt
                .query_map([period_start, now_ts], |row| {
                    let requests: i64 = row.get(0)?;
                    let tokens: Option<i64> = row.get(1)?;
                    Ok((requests.max(0) as u64, tokens.unwrap_or(0).max(0) as u64))
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
            .map(|dt| dt.and_utc().timestamp())
            .unwrap_or(0);
        let week_start = (now - chrono::Duration::days(6))
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map(|dt| dt.and_utc().timestamp())
            .unwrap_or(0);
        let month_start = (now - chrono::Duration::days(29))
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map(|dt| dt.and_utc().timestamp())
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

    pub fn get_daily_stats(&self, days: u32) -> Result<DailyStatsResponse, String> {
        let conn = self.get_connection()?;
        let mut stats = Vec::new();
        let now = chrono::Utc::now();

        for i in 0..days {
            let date = (now - chrono::Duration::days(i as i64)).date_naive();
            let date_str = date.format("%Y-%m-%d").to_string();
            let day_start = date
                .and_hms_opt(0, 0, 0)
                .map(|dt| dt.and_utc().timestamp())
                .unwrap_or(0);
            let day_end = date
                .and_hms_opt(23, 59, 59)
                .map(|dt| dt.and_utc().timestamp())
                .unwrap_or(0);

            let mut stmt = conn
                .prepare(
                    "SELECT COUNT(*) as requests, SUM(total_tokens) as tokens
                     FROM antigravity_requests
                     WHERE timestamp >= ?1 AND timestamp <= ?2",
                )
                .map_err(|e| format!("Failed to prepare daily stats query: {}", e))?;

            let mut result = (0u64, 0u64);
            let rows = stmt
                .query_map([day_start, day_end], |row| {
                    let requests: i64 = row.get(0)?;
                    let tokens: Option<i64> = row.get(1)?;
                    Ok((requests.max(0) as u64, tokens.unwrap_or(0).max(0) as u64))
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
                        COALESCE(gateway_profile_name, display_label, 'Legacy') AS profile_name,
                        member_code,
                        role_title,
                        color,
                        COUNT(*) AS requests,
                        COALESCE(SUM(total_tokens), 0) AS tokens
                 FROM antigravity_requests
                 WHERE timestamp >= ?1 AND timestamp <= ?2
                 GROUP BY date_key, profile_id, profile_name, member_code, role_title, color
                 ORDER BY profile_name ASC, date_key ASC",
            )
            .map_err(|e| format!("Failed to prepare gateway daily stats query: {}", e))?;

        let mut grouped: BTreeMap<
            (
                String,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
            ),
            BTreeMap<i64, (u64, u64)>,
        > = BTreeMap::new();

        let rows = stmt
            .query_map([start_ts, end_ts], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, i64>(6)?.max(0) as u64,
                    row.get::<_, i64>(7)?.max(0) as u64,
                ))
            })
            .map_err(|e| format!("Failed to execute gateway daily stats query: {}", e))?;

        for row in rows {
            let (
                date_key,
                profile_id,
                profile_name,
                member_code,
                role_title,
                color,
                requests,
                tokens,
            ) = row.map_err(|e| format!("Failed to read gateway daily stats row: {}", e))?;
            grouped
                .entry((profile_id, profile_name, member_code, role_title, color))
                .or_default()
                .insert(date_key, (requests, tokens));
        }

        let mut series = Vec::new();
        for ((profile_id, profile_name, member_code, role_title, color), per_day) in grouped {
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
                member_code,
                role_title,
                color,
                stats,
            });
        }

        Ok(GatewayDailyStatsResponse { series })
    }

    pub fn clear_all(&self) -> Result<usize, String> {
        let conn = self.get_connection()?;
        conn.execute("DELETE FROM antigravity_requests", [])
            .map_err(|e| format!("Failed to clear logs: {}", e))
    }

    pub fn delete_before(&self, date_key: i64) -> Result<usize, String> {
        let conn = self.get_connection()?;
        conn.execute(
            "DELETE FROM antigravity_requests WHERE date_key < ?1",
            [date_key],
        )
        .map_err(|e| format!("Failed to delete old logs: {}", e))
    }

    pub fn db_size(&self) -> Result<u64, String> {
        let metadata = std::fs::metadata(&self.db_path)
            .map_err(|e| format!("Failed to get db metadata: {}", e))?;
        Ok(metadata.len())
    }

    pub fn db_path(&self) -> &std::path::Path {
        &self.db_path
    }

    pub fn total_logs(&self) -> Result<i64, String> {
        let conn = self.get_connection()?;
        conn.query_row("SELECT COUNT(*) FROM antigravity_requests", [], |row| {
            row.get(0)
        })
        .map_err(|e| format!("Failed to count logs: {}", e))
    }

    pub fn get_all_time_stats(&self) -> Result<(u64, u64), String> {
        let conn = self.get_connection()?;
        conn.query_row(
            "SELECT COUNT(*) as requests, COALESCE(SUM(total_tokens), 0) as tokens FROM antigravity_requests",
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
        member_code: Option<&str>,
        role_title: Option<&str>,
        api_key: Option<&str>,
        color: Option<&str>,
    ) -> RequestLog {
        let api_key_suffix = api_key
            .and_then(|value| value.rsplit('-').next())
            .map(str::to_string);

        serde_json::from_value(serde_json::json!({
            "id": id,
            "timestamp": timestamp,
            "account_id": "antigravity-sidecar",
            "account_email": "",
            "model": "claude-sonnet-4-5",
            "format": "openai-responses",
            "input_tokens": total_tokens / 2,
            "output_tokens": total_tokens / 2,
            "total_tokens": total_tokens,
            "status": "success",
            "error_message": null,
            "request_duration_ms": 120,
            "gateway_profile_id": gateway_profile_id,
            "gateway_profile_name": gateway_profile_name,
            "member_code": member_code,
            "role_title": role_title,
            "display_label": null,
            "api_key_suffix": api_key_suffix,
            "color": color,
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn antigravity_daily_stats_are_grouped_by_gateway_profile() {
        let temp_dir = tempdir().unwrap();
        let storage = AntigravityLogStorage::new(temp_dir.path().to_path_buf()).unwrap();
        let now = chrono::Utc::now().timestamp();
        let yesterday = now - 24 * 60 * 60;

        storage
            .add_log(build_log(
                "a",
                yesterday,
                100,
                Some("ant-jdd"),
                Some("姜大大"),
                Some("jdd"),
                Some("产品与方法论"),
                Some("sk-ant-jdd-a4f29c7e"),
                Some("#4c6ef5"),
            ))
            .await;
        storage
            .add_log(build_log(
                "b",
                now,
                200,
                Some("ant-jdd"),
                Some("姜大大"),
                Some("jdd"),
                Some("产品与方法论"),
                Some("sk-ant-jdd-a4f29c7e"),
                Some("#4c6ef5"),
            ))
            .await;
        storage
            .add_log(build_log(
                "c",
                now,
                300,
                Some("ant-cr"),
                Some("CR"),
                Some("cr"),
                Some("执行导向"),
                Some("sk-ant-cr-19de8831"),
                Some("#12b886"),
            ))
            .await;

        let response = storage.get_daily_stats_by_gateway_profile(2).unwrap();

        assert_eq!(response.series.len(), 2);
        assert_eq!(response.series[0].profile_id, "ant-cr");
        assert_eq!(response.series[1].profile_id, "ant-jdd");
        assert_eq!(response.series[1].stats[0].tokens, 100);
        assert_eq!(response.series[1].stats[1].tokens, 200);
    }

    #[tokio::test]
    async fn antigravity_log_storage_persists_member_metadata() {
        let temp_dir = tempdir().unwrap();
        let storage = AntigravityLogStorage::new(temp_dir.path().to_path_buf()).unwrap();
        let now = chrono::Utc::now().timestamp();

        storage
            .add_log(build_log(
                "log-1",
                now,
                42,
                Some("ant-jdd"),
                Some("姜大大"),
                Some("jdd"),
                Some("产品与方法论"),
                Some("sk-ant-jdd-a4f29c7e"),
                Some("#4c6ef5"),
            ))
            .await;

        let query: LogQuery =
            serde_json::from_value(serde_json::json!({ "memberCode": "jdd" })).unwrap();
        let page = storage.query_logs(&query).unwrap();
        let row = serde_json::to_value(&page.items[0]).unwrap();

        assert_eq!(page.total, 1);
        assert_eq!(row["member_code"], "jdd");
        assert_eq!(row["role_title"], "产品与方法论");
        assert_eq!(row["api_key_suffix"], "a4f29c7e");
        assert_eq!(row["color"], "#4c6ef5");
    }
}
