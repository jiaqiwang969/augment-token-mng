//! Codex archive SQLite storage.

use rusqlite::{Connection, OptionalExtension, params};
use std::path::{Path, PathBuf};

use super::archive::ArchiveSessionConfidence;

#[derive(Debug)]
pub struct CodexArchiveStorage {
    db_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveSessionRow {
    pub archive_session_id: String,
    pub gateway_profile_id: String,
    pub gateway_profile_name: Option<String>,
    pub member_code: Option<String>,
    pub display_label: Option<String>,
    pub prompt_cache_key: Option<String>,
    pub explicit_session_id: Option<String>,
    pub confidence: ArchiveSessionConfidence,
    pub source: String,
    pub originator: Option<String>,
    pub client_user_agent: Option<String>,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub turn_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveTurnRecord {
    pub archive_turn_id: String,
    pub archive_session_id: String,
    pub gateway_profile_id: String,
    pub gateway_profile_name: Option<String>,
    pub member_code: Option<String>,
    pub display_label: Option<String>,
    pub prompt_cache_key: Option<String>,
    pub explicit_session_id: Option<String>,
    pub confidence: ArchiveSessionConfidence,
    pub source: String,
    pub originator: Option<String>,
    pub client_user_agent: Option<String>,
    pub turn_id: Option<String>,
    pub request_path: String,
    pub request_method: String,
    pub model: String,
    pub format: String,
    pub status: String,
    pub completion_state: String,
    pub error_message: Option<String>,
    pub request_started_at: i64,
    pub request_finished_at: Option<i64>,
    pub request_duration_ms: Option<i64>,
    pub request_headers_json: Option<String>,
    pub request_body_json: Option<String>,
    pub response_headers_json: Option<String>,
    pub response_body_text: Option<String>,
    pub stream_was_interrupted: bool,
    pub sandbox: Option<String>,
    pub selected_account_id: Option<String>,
    pub selected_account_email: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
}

impl CodexArchiveStorage {
    pub fn new(data_dir: PathBuf) -> Result<Self, String> {
        let archive_dir = data_dir.join("data").join("archives");
        std::fs::create_dir_all(&archive_dir)
            .map_err(|e| format!("Failed to create archive directory: {}", e))?;

        let storage = Self {
            db_path: archive_dir.join("codex_archive.db"),
        };

        let mut conn = storage.get_connection()?;
        storage.init_schema(&mut conn)?;

        Ok(storage)
    }

    fn get_connection(&self) -> Result<Connection, String> {
        Connection::open(&self.db_path)
            .map_err(|e| format!("Failed to open archive database connection: {}", e))
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn db_size(&self) -> Result<u64, String> {
        std::fs::metadata(&self.db_path)
            .map(|metadata| metadata.len())
            .map_err(|e| format!("Failed to read archive database metadata: {}", e))
    }

    fn init_schema(&self, conn: &mut Connection) -> Result<(), String> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS codex_archive_sessions (
                archive_session_id TEXT PRIMARY KEY,
                gateway_profile_id TEXT NOT NULL,
                gateway_profile_name TEXT,
                member_code TEXT,
                display_label TEXT,
                prompt_cache_key TEXT,
                explicit_session_id TEXT,
                confidence TEXT NOT NULL,
                source TEXT NOT NULL,
                originator TEXT,
                client_user_agent TEXT,
                first_seen_at INTEGER NOT NULL,
                last_seen_at INTEGER NOT NULL,
                turn_count INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS codex_archive_turns (
                archive_turn_id TEXT PRIMARY KEY,
                archive_session_id TEXT NOT NULL,
                gateway_profile_id TEXT NOT NULL,
                turn_id TEXT,
                request_path TEXT NOT NULL,
                request_method TEXT NOT NULL,
                model TEXT NOT NULL,
                format TEXT NOT NULL,
                status TEXT NOT NULL,
                completion_state TEXT NOT NULL,
                error_message TEXT,
                request_started_at INTEGER NOT NULL,
                request_finished_at INTEGER,
                request_duration_ms INTEGER,
                request_headers_json TEXT,
                request_body_json TEXT,
                response_headers_json TEXT,
                response_body_text TEXT,
                stream_was_interrupted INTEGER NOT NULL DEFAULT 0,
                prompt_cache_key TEXT,
                sandbox TEXT,
                originator TEXT,
                selected_account_id TEXT,
                selected_account_email TEXT,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (archive_session_id) REFERENCES codex_archive_sessions(archive_session_id)
            );

            CREATE INDEX IF NOT EXISTS idx_codex_archive_turns_session
                ON codex_archive_turns(archive_session_id);
            CREATE INDEX IF NOT EXISTS idx_codex_archive_sessions_gateway_profile
                ON codex_archive_sessions(gateway_profile_id);
            CREATE INDEX IF NOT EXISTS idx_codex_archive_turns_prompt_cache_key
                ON codex_archive_turns(prompt_cache_key);
            CREATE INDEX IF NOT EXISTS idx_codex_archive_turns_turn_id
                ON codex_archive_turns(turn_id);
            CREATE INDEX IF NOT EXISTS idx_codex_archive_turns_request_started_at
                ON codex_archive_turns(request_started_at);
            ",
        )
        .map_err(|e| format!("Failed to initialize archive schema: {}", e))
    }

    pub fn record_turn(&self, turn: ArchiveTurnRecord) -> Result<(), String> {
        let mut conn = self.get_connection()?;
        let tx = conn
            .transaction()
            .map_err(|e| format!("Failed to begin archive transaction: {}", e))?;

        tx.execute(
            "INSERT INTO codex_archive_sessions (
                archive_session_id,
                gateway_profile_id,
                gateway_profile_name,
                member_code,
                display_label,
                prompt_cache_key,
                explicit_session_id,
                confidence,
                source,
                originator,
                client_user_agent,
                first_seen_at,
                last_seen_at,
                turn_count
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 0
            )
            ON CONFLICT(archive_session_id) DO UPDATE SET
                gateway_profile_id = excluded.gateway_profile_id,
                gateway_profile_name = COALESCE(excluded.gateway_profile_name, codex_archive_sessions.gateway_profile_name),
                member_code = COALESCE(excluded.member_code, codex_archive_sessions.member_code),
                display_label = COALESCE(excluded.display_label, codex_archive_sessions.display_label),
                prompt_cache_key = COALESCE(excluded.prompt_cache_key, codex_archive_sessions.prompt_cache_key),
                explicit_session_id = COALESCE(excluded.explicit_session_id, codex_archive_sessions.explicit_session_id),
                confidence = excluded.confidence,
                source = excluded.source,
                originator = COALESCE(excluded.originator, codex_archive_sessions.originator),
                client_user_agent = COALESCE(excluded.client_user_agent, codex_archive_sessions.client_user_agent),
                first_seen_at = MIN(codex_archive_sessions.first_seen_at, excluded.first_seen_at),
                last_seen_at = MAX(codex_archive_sessions.last_seen_at, excluded.last_seen_at)",
            params![
                turn.archive_session_id.as_str(),
                turn.gateway_profile_id.as_str(),
                turn.gateway_profile_name.as_deref(),
                turn.member_code.as_deref(),
                turn.display_label.as_deref(),
                turn.prompt_cache_key.as_deref(),
                turn.explicit_session_id.as_deref(),
                archive_confidence_to_db(turn.confidence),
                turn.source.as_str(),
                turn.originator.as_deref(),
                turn.client_user_agent.as_deref(),
                turn.request_started_at,
                turn.request_started_at,
            ],
        )
        .map_err(|e| format!("Failed to upsert archive session shell: {}", e))?;

        tx.execute(
            "INSERT INTO codex_archive_turns (
                archive_turn_id,
                archive_session_id,
                gateway_profile_id,
                turn_id,
                request_path,
                request_method,
                model,
                format,
                status,
                completion_state,
                error_message,
                request_started_at,
                request_finished_at,
                request_duration_ms,
                request_headers_json,
                request_body_json,
                response_headers_json,
                response_body_text,
                stream_was_interrupted,
                prompt_cache_key,
                sandbox,
                originator,
                selected_account_id,
                selected_account_email,
                input_tokens,
                output_tokens,
                total_tokens
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27
            )
            ON CONFLICT(archive_turn_id) DO UPDATE SET
                archive_session_id = excluded.archive_session_id,
                gateway_profile_id = excluded.gateway_profile_id,
                turn_id = excluded.turn_id,
                request_path = excluded.request_path,
                request_method = excluded.request_method,
                model = excluded.model,
                format = excluded.format,
                status = excluded.status,
                completion_state = excluded.completion_state,
                error_message = excluded.error_message,
                request_started_at = excluded.request_started_at,
                request_finished_at = excluded.request_finished_at,
                request_duration_ms = excluded.request_duration_ms,
                request_headers_json = excluded.request_headers_json,
                request_body_json = excluded.request_body_json,
                response_headers_json = excluded.response_headers_json,
                response_body_text = excluded.response_body_text,
                stream_was_interrupted = excluded.stream_was_interrupted,
                prompt_cache_key = excluded.prompt_cache_key,
                sandbox = excluded.sandbox,
                originator = excluded.originator,
                selected_account_id = excluded.selected_account_id,
                selected_account_email = excluded.selected_account_email,
                input_tokens = excluded.input_tokens,
                output_tokens = excluded.output_tokens,
                total_tokens = excluded.total_tokens",
            params![
                turn.archive_turn_id.as_str(),
                turn.archive_session_id.as_str(),
                turn.gateway_profile_id.as_str(),
                turn.turn_id.as_deref(),
                turn.request_path.as_str(),
                turn.request_method.as_str(),
                turn.model.as_str(),
                turn.format.as_str(),
                turn.status.as_str(),
                turn.completion_state.as_str(),
                turn.error_message.as_deref(),
                turn.request_started_at,
                turn.request_finished_at,
                turn.request_duration_ms,
                turn.request_headers_json.as_deref(),
                turn.request_body_json.as_deref(),
                turn.response_headers_json.as_deref(),
                turn.response_body_text.as_deref(),
                bool_to_sqlite(turn.stream_was_interrupted),
                turn.prompt_cache_key.as_deref(),
                turn.sandbox.as_deref(),
                turn.originator.as_deref(),
                turn.selected_account_id.as_deref(),
                turn.selected_account_email.as_deref(),
                turn.input_tokens,
                turn.output_tokens,
                turn.total_tokens,
            ],
        )
        .map_err(|e| format!("Failed to upsert archive turn: {}", e))?;

        let session_rollup = tx
            .query_row(
                "SELECT
                    MIN(request_started_at),
                    MAX(request_started_at),
                    COUNT(*)
                 FROM codex_archive_turns
                 WHERE archive_session_id = ?1",
                [turn.archive_session_id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, Option<i64>>(0)?
                            .unwrap_or(turn.request_started_at),
                        row.get::<_, Option<i64>>(1)?
                            .unwrap_or(turn.request_started_at),
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .map_err(|e| format!("Failed to compute archive session rollup: {}", e))?;

        tx.execute(
            "INSERT INTO codex_archive_sessions (
                archive_session_id,
                gateway_profile_id,
                gateway_profile_name,
                member_code,
                display_label,
                prompt_cache_key,
                explicit_session_id,
                confidence,
                source,
                originator,
                client_user_agent,
                first_seen_at,
                last_seen_at,
                turn_count
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14
            )
            ON CONFLICT(archive_session_id) DO UPDATE SET
                gateway_profile_id = excluded.gateway_profile_id,
                gateway_profile_name = COALESCE(excluded.gateway_profile_name, codex_archive_sessions.gateway_profile_name),
                member_code = COALESCE(excluded.member_code, codex_archive_sessions.member_code),
                display_label = COALESCE(excluded.display_label, codex_archive_sessions.display_label),
                prompt_cache_key = COALESCE(excluded.prompt_cache_key, codex_archive_sessions.prompt_cache_key),
                explicit_session_id = COALESCE(excluded.explicit_session_id, codex_archive_sessions.explicit_session_id),
                confidence = excluded.confidence,
                source = excluded.source,
                originator = COALESCE(excluded.originator, codex_archive_sessions.originator),
                client_user_agent = COALESCE(excluded.client_user_agent, codex_archive_sessions.client_user_agent),
                first_seen_at = excluded.first_seen_at,
                last_seen_at = excluded.last_seen_at,
                turn_count = excluded.turn_count",
            params![
                turn.archive_session_id.as_str(),
                turn.gateway_profile_id.as_str(),
                turn.gateway_profile_name.as_deref(),
                turn.member_code.as_deref(),
                turn.display_label.as_deref(),
                turn.prompt_cache_key.as_deref(),
                turn.explicit_session_id.as_deref(),
                archive_confidence_to_db(turn.confidence),
                turn.source.as_str(),
                turn.originator.as_deref(),
                turn.client_user_agent.as_deref(),
                session_rollup.0,
                session_rollup.1,
                session_rollup.2,
            ],
        )
        .map_err(|e| format!("Failed to upsert archive session: {}", e))?;

        tx.commit()
            .map_err(|e| format!("Failed to commit archive transaction: {}", e))
    }

    pub fn get_session(
        &self,
        archive_session_id: &str,
    ) -> Result<Option<ArchiveSessionRow>, String> {
        let conn = self.get_connection()?;

        conn.query_row(
            "SELECT
                archive_session_id,
                gateway_profile_id,
                gateway_profile_name,
                member_code,
                display_label,
                prompt_cache_key,
                explicit_session_id,
                confidence,
                source,
                originator,
                client_user_agent,
                first_seen_at,
                last_seen_at,
                turn_count
             FROM codex_archive_sessions
             WHERE archive_session_id = ?1",
            [archive_session_id],
            |row| {
                let confidence: String = row.get(7)?;
                Ok(ArchiveSessionRow {
                    archive_session_id: row.get(0)?,
                    gateway_profile_id: row.get(1)?,
                    gateway_profile_name: row.get(2)?,
                    member_code: row.get(3)?,
                    display_label: row.get(4)?,
                    prompt_cache_key: row.get(5)?,
                    explicit_session_id: row.get(6)?,
                    confidence: archive_confidence_from_db(&confidence),
                    source: row.get(8)?,
                    originator: row.get(9)?,
                    client_user_agent: row.get(10)?,
                    first_seen_at: row.get(11)?,
                    last_seen_at: row.get(12)?,
                    turn_count: row.get(13)?,
                })
            },
        )
        .optional()
        .map_err(|e| format!("Failed to query archive session: {}", e))
    }

    pub fn list_sessions(&self) -> Result<Vec<ArchiveSessionRow>, String> {
        let conn = self.get_connection()?;
        let mut stmt = conn
            .prepare(
                "SELECT
                    archive_session_id,
                    gateway_profile_id,
                    gateway_profile_name,
                    member_code,
                    display_label,
                    prompt_cache_key,
                    explicit_session_id,
                    confidence,
                    source,
                    originator,
                    client_user_agent,
                    first_seen_at,
                    last_seen_at,
                    turn_count
                 FROM codex_archive_sessions
                 ORDER BY last_seen_at DESC, first_seen_at DESC, archive_session_id ASC",
            )
            .map_err(|e| format!("Failed to prepare archive sessions query: {}", e))?;

        let rows = stmt
            .query_map([], |row| {
                let confidence: String = row.get(7)?;
                Ok(ArchiveSessionRow {
                    archive_session_id: row.get(0)?,
                    gateway_profile_id: row.get(1)?,
                    gateway_profile_name: row.get(2)?,
                    member_code: row.get(3)?,
                    display_label: row.get(4)?,
                    prompt_cache_key: row.get(5)?,
                    explicit_session_id: row.get(6)?,
                    confidence: archive_confidence_from_db(&confidence),
                    source: row.get(8)?,
                    originator: row.get(9)?,
                    client_user_agent: row.get(10)?,
                    first_seen_at: row.get(11)?,
                    last_seen_at: row.get(12)?,
                    turn_count: row.get(13)?,
                })
            })
            .map_err(|e| format!("Failed to query archive sessions: {}", e))?;

        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row.map_err(|e| format!("Failed to read archive session row: {}", e))?);
        }

        Ok(sessions)
    }

    pub fn total_sessions(&self) -> Result<i64, String> {
        let conn = self.get_connection()?;
        conn.query_row("SELECT COUNT(*) FROM codex_archive_sessions", [], |row| {
            row.get(0)
        })
        .map_err(|e| format!("Failed to count archive sessions: {}", e))
    }

    pub fn total_turns(&self) -> Result<i64, String> {
        let conn = self.get_connection()?;
        conn.query_row("SELECT COUNT(*) FROM codex_archive_turns", [], |row| {
            row.get(0)
        })
        .map_err(|e| format!("Failed to count archive turns: {}", e))
    }

    pub fn get_turns_by_session(
        &self,
        archive_session_id: &str,
    ) -> Result<Vec<ArchiveTurnRecord>, String> {
        let conn = self.get_connection()?;
        let mut stmt = conn
            .prepare(
                "SELECT
                    t.archive_turn_id,
                    t.archive_session_id,
                    s.gateway_profile_id,
                    s.gateway_profile_name,
                    s.member_code,
                    s.display_label,
                    s.prompt_cache_key,
                    s.explicit_session_id,
                    s.confidence,
                    s.source,
                    COALESCE(t.originator, s.originator),
                    s.client_user_agent,
                    t.turn_id,
                    t.request_path,
                    t.request_method,
                    t.model,
                    t.format,
                    t.status,
                    t.completion_state,
                    t.error_message,
                    t.request_started_at,
                    t.request_finished_at,
                    t.request_duration_ms,
                    t.request_headers_json,
                    t.request_body_json,
                    t.response_headers_json,
                    t.response_body_text,
                    t.stream_was_interrupted,
                    t.sandbox,
                    t.selected_account_id,
                    t.selected_account_email,
                    t.input_tokens,
                    t.output_tokens,
                    t.total_tokens
                 FROM codex_archive_turns t
                 INNER JOIN codex_archive_sessions s
                    ON s.archive_session_id = t.archive_session_id
                 WHERE t.archive_session_id = ?1
                 ORDER BY t.request_started_at ASC, t.archive_turn_id ASC",
            )
            .map_err(|e| format!("Failed to prepare archive turns query: {}", e))?;

        let rows = stmt
            .query_map([archive_session_id], |row| {
                let confidence: String = row.get(8)?;
                Ok(ArchiveTurnRecord {
                    archive_turn_id: row.get(0)?,
                    archive_session_id: row.get(1)?,
                    gateway_profile_id: row.get(2)?,
                    gateway_profile_name: row.get(3)?,
                    member_code: row.get(4)?,
                    display_label: row.get(5)?,
                    prompt_cache_key: row.get(6)?,
                    explicit_session_id: row.get(7)?,
                    confidence: archive_confidence_from_db(&confidence),
                    source: row.get(9)?,
                    originator: row.get(10)?,
                    client_user_agent: row.get(11)?,
                    turn_id: row.get(12)?,
                    request_path: row.get(13)?,
                    request_method: row.get(14)?,
                    model: row.get(15)?,
                    format: row.get(16)?,
                    status: row.get(17)?,
                    completion_state: row.get(18)?,
                    error_message: row.get(19)?,
                    request_started_at: row.get(20)?,
                    request_finished_at: row.get(21)?,
                    request_duration_ms: row.get(22)?,
                    request_headers_json: row.get(23)?,
                    request_body_json: row.get(24)?,
                    response_headers_json: row.get(25)?,
                    response_body_text: row.get(26)?,
                    stream_was_interrupted: row.get::<_, i64>(27)? != 0,
                    sandbox: row.get(28)?,
                    selected_account_id: row.get(29)?,
                    selected_account_email: row.get(30)?,
                    input_tokens: row.get(31)?,
                    output_tokens: row.get(32)?,
                    total_tokens: row.get(33)?,
                })
            })
            .map_err(|e| format!("Failed to query archive turns: {}", e))?;

        let mut turns = Vec::new();
        for row in rows {
            turns.push(row.map_err(|e| format!("Failed to read archive turn row: {}", e))?);
        }

        Ok(turns)
    }

    pub fn get_turn(&self, archive_turn_id: &str) -> Result<Option<ArchiveTurnRecord>, String> {
        let conn = self.get_connection()?;

        conn.query_row(
            "SELECT
                t.archive_turn_id,
                t.archive_session_id,
                s.gateway_profile_id,
                s.gateway_profile_name,
                s.member_code,
                s.display_label,
                s.prompt_cache_key,
                s.explicit_session_id,
                s.confidence,
                s.source,
                COALESCE(t.originator, s.originator),
                s.client_user_agent,
                t.turn_id,
                t.request_path,
                t.request_method,
                t.model,
                t.format,
                t.status,
                t.completion_state,
                t.error_message,
                t.request_started_at,
                t.request_finished_at,
                t.request_duration_ms,
                t.request_headers_json,
                t.request_body_json,
                t.response_headers_json,
                t.response_body_text,
                t.stream_was_interrupted,
                t.sandbox,
                t.selected_account_id,
                t.selected_account_email,
                t.input_tokens,
                t.output_tokens,
                t.total_tokens
             FROM codex_archive_turns t
             INNER JOIN codex_archive_sessions s
                ON s.archive_session_id = t.archive_session_id
             WHERE t.archive_turn_id = ?1",
            [archive_turn_id],
            |row| {
                let confidence: String = row.get(8)?;
                Ok(ArchiveTurnRecord {
                    archive_turn_id: row.get(0)?,
                    archive_session_id: row.get(1)?,
                    gateway_profile_id: row.get(2)?,
                    gateway_profile_name: row.get(3)?,
                    member_code: row.get(4)?,
                    display_label: row.get(5)?,
                    prompt_cache_key: row.get(6)?,
                    explicit_session_id: row.get(7)?,
                    confidence: archive_confidence_from_db(&confidence),
                    source: row.get(9)?,
                    originator: row.get(10)?,
                    client_user_agent: row.get(11)?,
                    turn_id: row.get(12)?,
                    request_path: row.get(13)?,
                    request_method: row.get(14)?,
                    model: row.get(15)?,
                    format: row.get(16)?,
                    status: row.get(17)?,
                    completion_state: row.get(18)?,
                    error_message: row.get(19)?,
                    request_started_at: row.get(20)?,
                    request_finished_at: row.get(21)?,
                    request_duration_ms: row.get(22)?,
                    request_headers_json: row.get(23)?,
                    request_body_json: row.get(24)?,
                    response_headers_json: row.get(25)?,
                    response_body_text: row.get(26)?,
                    stream_was_interrupted: row.get::<_, i64>(27)? != 0,
                    sandbox: row.get(28)?,
                    selected_account_id: row.get(29)?,
                    selected_account_email: row.get(30)?,
                    input_tokens: row.get(31)?,
                    output_tokens: row.get(32)?,
                    total_tokens: row.get(33)?,
                })
            },
        )
        .optional()
        .map_err(|e| format!("Failed to query archive turn: {}", e))
    }
}

fn bool_to_sqlite(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

fn archive_confidence_to_db(confidence: ArchiveSessionConfidence) -> &'static str {
    match confidence {
        ArchiveSessionConfidence::High => "high",
        ArchiveSessionConfidence::Medium => "medium",
        ArchiveSessionConfidence::Low => "low",
    }
}

fn archive_confidence_from_db(value: &str) -> ArchiveSessionConfidence {
    match value {
        "high" => ArchiveSessionConfidence::High,
        "medium" => ArchiveSessionConfidence::Medium,
        _ => ArchiveSessionConfidence::Low,
    }
}

#[cfg(test)]
mod tests {
    use super::{ArchiveSessionRow, ArchiveTurnRecord, CodexArchiveStorage};
    use crate::platforms::openai::codex::archive::ArchiveSessionConfidence;
    use tempfile::tempdir;

    fn create_test_archive_storage() -> CodexArchiveStorage {
        let temp_dir = tempdir().unwrap();
        let data_dir = temp_dir.path().to_path_buf();
        std::mem::forget(temp_dir);
        CodexArchiveStorage::new(data_dir).unwrap()
    }

    fn seed_turn(
        gateway_profile_id: &str,
        prompt_cache_key: &str,
        turn_id: &str,
        request_started_at: i64,
        selected_account_id: Option<&str>,
    ) -> ArchiveTurnRecord {
        ArchiveTurnRecord {
            archive_turn_id: format!("archive-turn-{}", turn_id),
            archive_session_id: format!("{}:prompt-cache:{}", gateway_profile_id, prompt_cache_key),
            gateway_profile_id: gateway_profile_id.to_string(),
            gateway_profile_name: Some("姜大大".to_string()),
            member_code: Some("jdd".to_string()),
            display_label: Some("姜大大 · jdd · 产品与方法论".to_string()),
            prompt_cache_key: Some(prompt_cache_key.to_string()),
            explicit_session_id: None,
            confidence: ArchiveSessionConfidence::Medium,
            source: "codex".to_string(),
            originator: Some("codex_cli_rs".to_string()),
            client_user_agent: Some("codex_cli_rs/0.0.0".to_string()),
            turn_id: Some(turn_id.to_string()),
            request_path: "/v1/responses".to_string(),
            request_method: "POST".to_string(),
            model: "gpt-5.4".to_string(),
            format: "openai-responses".to_string(),
            status: "success".to_string(),
            completion_state: "completed".to_string(),
            error_message: None,
            request_started_at,
            request_finished_at: Some(request_started_at + 1),
            request_duration_ms: Some(1),
            request_headers_json: Some("{\"accept\":\"text/event-stream\"}".to_string()),
            request_body_json: Some("{\"input\":\"hello\"}".to_string()),
            response_headers_json: Some("{\"content-type\":\"text/event-stream\"}".to_string()),
            response_body_text: Some("data: hello".to_string()),
            stream_was_interrupted: false,
            sandbox: Some("seatbelt".to_string()),
            selected_account_id: selected_account_id.map(str::to_string),
            selected_account_email: selected_account_id.map(|value| format!("{value}@example.com")),
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
        }
    }

    #[test]
    fn codex_archive_storage_initializes_expected_tables() {
        let storage = create_test_archive_storage();
        let conn = storage.get_connection().unwrap();

        let sessions_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'codex_archive_sessions'",
                [],
                |row: &rusqlite::Row<'_>| row.get(0),
            )
            .unwrap();
        let turns_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'codex_archive_turns'",
                [],
                |row: &rusqlite::Row<'_>| row.get(0),
            )
            .unwrap();

        assert_eq!(sessions_exists, 1);
        assert_eq!(turns_exists, 1);
    }

    #[test]
    fn codex_archive_storage_places_db_under_data_archives_folder() {
        let temp_dir = tempdir().unwrap();
        let root_dir = temp_dir.path().to_path_buf();

        let storage = CodexArchiveStorage::new(root_dir.clone()).unwrap();
        let expected_db_path = root_dir
            .join("data")
            .join("archives")
            .join("codex_archive.db");

        assert_eq!(storage.db_path, expected_db_path);
        assert!(storage.db_path.exists());
    }

    #[test]
    fn codex_archive_storage_keeps_turns_under_same_prompt_cache_session() {
        let storage = create_test_archive_storage();

        storage
            .record_turn(seed_turn(
                "codex-jdd",
                "prompt-a",
                "turn-1",
                1_000,
                Some("acct-1"),
            ))
            .unwrap();
        storage
            .record_turn(seed_turn(
                "codex-jdd",
                "prompt-a",
                "turn-2",
                2_000,
                Some("acct-2"),
            ))
            .unwrap();

        let session = storage
            .get_session("codex-jdd:prompt-cache:prompt-a")
            .unwrap()
            .unwrap();

        assert_eq!(
            session,
            ArchiveSessionRow {
                archive_session_id: "codex-jdd:prompt-cache:prompt-a".to_string(),
                gateway_profile_id: "codex-jdd".to_string(),
                gateway_profile_name: Some("姜大大".to_string()),
                member_code: Some("jdd".to_string()),
                display_label: Some("姜大大 · jdd · 产品与方法论".to_string()),
                prompt_cache_key: Some("prompt-a".to_string()),
                explicit_session_id: None,
                confidence: ArchiveSessionConfidence::Medium,
                source: "codex".to_string(),
                originator: Some("codex_cli_rs".to_string()),
                client_user_agent: Some("codex_cli_rs/0.0.0".to_string()),
                first_seen_at: 1_000,
                last_seen_at: 2_000,
                turn_count: 2,
            }
        );
    }

    #[test]
    fn codex_archive_storage_lists_turns_by_session_and_keeps_selected_account_diagnostic_only() {
        let storage = create_test_archive_storage();

        storage
            .record_turn(seed_turn(
                "codex-jdd",
                "prompt-a",
                "turn-1",
                1_000,
                Some("acct-1"),
            ))
            .unwrap();
        storage
            .record_turn(seed_turn(
                "codex-jdd",
                "prompt-a",
                "turn-2",
                2_000,
                Some("acct-2"),
            ))
            .unwrap();

        let turns = storage
            .get_turns_by_session("codex-jdd:prompt-cache:prompt-a")
            .unwrap();

        assert_eq!(turns.len(), 2);
        assert_eq!(
            turns[0].archive_session_id,
            "codex-jdd:prompt-cache:prompt-a"
        );
        assert_eq!(turns[0].selected_account_id.as_deref(), Some("acct-1"));
        assert_eq!(
            turns[1].archive_session_id,
            "codex-jdd:prompt-cache:prompt-a"
        );
        assert_eq!(turns[1].selected_account_id.as_deref(), Some("acct-2"));
    }

    #[test]
    fn codex_archive_export_lists_sessions_with_latest_activity_first() {
        let storage = create_test_archive_storage();

        storage
            .record_turn(seed_turn(
                "codex-jdd",
                "prompt-a",
                "turn-1",
                1_000,
                Some("acct-1"),
            ))
            .unwrap();
        storage
            .record_turn(seed_turn(
                "codex-jdd",
                "prompt-b",
                "turn-2",
                2_000,
                Some("acct-2"),
            ))
            .unwrap();

        let sessions = storage.list_sessions().unwrap();

        assert_eq!(sessions.len(), 2);
        assert_eq!(
            sessions[0].archive_session_id,
            "codex-jdd:prompt-cache:prompt-b"
        );
        assert_eq!(
            sessions[1].archive_session_id,
            "codex-jdd:prompt-cache:prompt-a"
        );
    }

    #[test]
    fn codex_archive_export_counts_recorded_sessions() {
        let storage = create_test_archive_storage();

        storage
            .record_turn(seed_turn(
                "codex-jdd",
                "prompt-a",
                "turn-1",
                1_000,
                Some("acct-1"),
            ))
            .unwrap();
        storage
            .record_turn(seed_turn(
                "codex-jdd",
                "prompt-b",
                "turn-2",
                2_000,
                Some("acct-2"),
            ))
            .unwrap();

        assert_eq!(storage.total_sessions().unwrap(), 2);
    }
}
