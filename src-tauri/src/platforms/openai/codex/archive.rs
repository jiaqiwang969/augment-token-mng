//! Codex archive marker helpers.

use bytes::Bytes;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use warp::http::HeaderMap;

use super::archive_storage::{ArchiveSessionRow, ArchiveTurnRecord, CodexArchiveStorage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveSessionConfidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArchiveRequestMarkers {
    pub turn_id: Option<String>,
    pub sandbox: Option<String>,
    pub prompt_cache_key: Option<String>,
    pub explicit_session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedArchiveSession {
    pub synthetic_session_key: String,
    pub confidence: ArchiveSessionConfidence,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ArchiveUsageStats {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
}

#[derive(Debug, Clone)]
pub struct ArchiveTurnContext {
    pub gateway_profile_id: String,
    pub gateway_profile_name: Option<String>,
    pub member_code: Option<String>,
    pub display_label: Option<String>,
    pub prompt_cache_key: Option<String>,
    pub explicit_session_id: Option<String>,
    pub markers: ArchiveRequestMarkers,
    pub session: DerivedArchiveSession,
    pub request_path: String,
    pub request_method: String,
    pub model: String,
    pub format: String,
    pub request_headers: HeaderMap,
    pub request_body: Bytes,
    pub request_started_at: i64,
}

#[derive(Debug, Clone)]
pub struct ArchiveTurnCapture {
    context: ArchiveTurnContext,
    request_headers_json: Option<String>,
    request_body_json: Option<String>,
    response_headers_json: Option<String>,
    response_body_text: String,
    originator: Option<String>,
    client_user_agent: Option<String>,
    selected_account_id: Option<String>,
    selected_account_email: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CodexTurnMetadataHeader {
    #[serde(default)]
    turn_id: Option<String>,
    #[serde(default)]
    sandbox: Option<String>,
}

pub fn extract_turn_metadata(headers: &HeaderMap) -> ArchiveRequestMarkers {
    let metadata = headers
        .get("X-Codex-Turn-Metadata")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| serde_json::from_str::<CodexTurnMetadataHeader>(value).ok());

    ArchiveRequestMarkers {
        turn_id: metadata
            .as_ref()
            .and_then(|value| normalize_marker(value.turn_id.as_deref())),
        sandbox: metadata
            .as_ref()
            .and_then(|value| normalize_marker(value.sandbox.as_deref())),
        prompt_cache_key: None,
        explicit_session_id: None,
    }
}

pub fn extract_prompt_cache_key(body: &Bytes) -> Option<String> {
    let root: serde_json::Value = serde_json::from_slice(body).ok()?;
    let object = root.as_object()?;
    normalize_marker(
        object
            .get("prompt_cache_key")
            .and_then(|value| value.as_str()),
    )
}

pub fn derive_archive_session_identity(
    gateway_profile_id: &str,
    prompt_cache_key: Option<&str>,
    turn_id: Option<&str>,
) -> DerivedArchiveSession {
    if let Some(prompt_cache_key) = normalize_marker(prompt_cache_key) {
        return DerivedArchiveSession {
            synthetic_session_key: format!(
                "{}:prompt-cache:{}",
                gateway_profile_id, prompt_cache_key
            ),
            confidence: ArchiveSessionConfidence::Medium,
        };
    }

    if let Some(turn_id) = normalize_marker(turn_id) {
        return DerivedArchiveSession {
            synthetic_session_key: format!("{}:turn:{}", gateway_profile_id, turn_id),
            confidence: ArchiveSessionConfidence::Low,
        };
    }

    DerivedArchiveSession {
        synthetic_session_key: format!("{}:singleton:unknown", gateway_profile_id),
        confidence: ArchiveSessionConfidence::Low,
    }
}

fn normalize_marker(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn export_archive_session_jsonl(
    storage: &CodexArchiveStorage,
    archive_session_id: &str,
) -> Result<String, String> {
    let session = storage
        .get_session(archive_session_id)?
        .ok_or_else(|| format!("Archive session not found: {}", archive_session_id))?;
    let turns = storage.get_turns_by_session(archive_session_id)?;
    if turns.is_empty() {
        return Err(format!(
            "Archive session has no recorded turns: {}",
            archive_session_id
        ));
    }

    let mut lines = Vec::new();
    lines.push(serialize_archive_event(build_session_meta_event(&session))?);

    for turn in turns {
        lines.push(serialize_archive_event(build_turn_start_event(&turn))?);
        lines.push(serialize_archive_event(build_turn_context_event(&turn))?);

        if let Some(request_body) = turn.request_body_json.as_deref() {
            lines.push(serialize_archive_event(build_payload_event(
                &turn,
                "archive_request",
                "user",
                request_body,
                turn.request_started_at,
            ))?);
        }

        if let Some(response_body) = turn.response_body_text.as_deref() {
            lines.push(serialize_archive_event(build_payload_event(
                &turn,
                "archive_response",
                "assistant",
                response_body,
                turn.request_finished_at.unwrap_or(turn.request_started_at),
            ))?);
        }

        lines.push(serialize_archive_event(build_turn_terminal_event(&turn))?);
    }

    Ok(lines.join("\n"))
}

pub fn materialize_archive_session_file(
    storage: &CodexArchiveStorage,
    app_data_dir: &Path,
    archive_session_id: &str,
) -> Result<PathBuf, String> {
    let session = storage
        .get_session(archive_session_id)?
        .ok_or_else(|| format!("Archive session not found: {}", archive_session_id))?;
    let output_path = archive_session_materialized_path(app_data_dir, &session);
    let jsonl = export_archive_session_jsonl(storage, archive_session_id)?;

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create archive export directory: {}", e))?;
    }
    std::fs::write(&output_path, jsonl)
        .map_err(|e| format!("Failed to write archive session export: {}", e))?;

    Ok(output_path)
}

fn serialize_archive_event(value: Value) -> Result<String, String> {
    serde_json::to_string(&value).map_err(|e| format!("Failed to serialize archive event: {}", e))
}

fn build_session_meta_event(session: &ArchiveSessionRow) -> Value {
    json!({
        "timestamp": format_event_timestamp(session.first_seen_at),
        "type": "session_meta",
        "payload": {
            "id": session.archive_session_id,
            "timestamp": format_event_timestamp(session.first_seen_at),
            "source": "atm_codex_archive",
            "model_provider": "codex",
            "gateway_profile_id": session.gateway_profile_id,
            "gateway_profile_name": session.gateway_profile_name,
            "member_code": session.member_code,
            "display_label": session.display_label,
            "prompt_cache_key": session.prompt_cache_key,
            "explicit_session_id": session.explicit_session_id,
            "confidence": archive_confidence_label(session.confidence),
            "originator": session.originator,
            "client_user_agent": session.client_user_agent,
            "turn_count": session.turn_count,
        }
    })
}

fn build_turn_start_event(turn: &ArchiveTurnRecord) -> Value {
    json!({
        "timestamp": format_event_timestamp(turn.request_started_at),
        "type": "event_msg",
        "payload": {
            "type": "archive_turn_started",
            "turn_id": archive_turn_identity(turn),
            "archive_turn_id": turn.archive_turn_id,
            "archive_session_id": turn.archive_session_id,
            "request_path": turn.request_path,
            "request_method": turn.request_method,
            "model": turn.model,
            "format": turn.format,
        }
    })
}

fn build_turn_context_event(turn: &ArchiveTurnRecord) -> Value {
    json!({
        "timestamp": format_event_timestamp(turn.request_started_at),
        "type": "turn_context",
        "payload": {
            "turn_id": archive_turn_identity(turn),
            "archive_turn_id": turn.archive_turn_id,
            "archive_session_id": turn.archive_session_id,
            "gateway_profile_id": turn.gateway_profile_id,
            "gateway_profile_name": turn.gateway_profile_name,
            "member_code": turn.member_code,
            "display_label": turn.display_label,
            "prompt_cache_key": turn.prompt_cache_key,
            "explicit_session_id": turn.explicit_session_id,
            "sandbox": turn.sandbox,
            "request_path": turn.request_path,
            "request_method": turn.request_method,
            "model": turn.model,
            "format": turn.format,
            "status": turn.status,
            "completion_state": turn.completion_state,
            "selected_account_id": turn.selected_account_id,
            "selected_account_email": turn.selected_account_email,
        }
    })
}

fn build_payload_event(
    turn: &ArchiveTurnRecord,
    source: &str,
    role: &str,
    raw_payload: &str,
    timestamp: i64,
) -> Value {
    json!({
        "timestamp": format_event_timestamp(timestamp),
        "type": "response_item",
        "payload": {
            "type": "message",
            "source": source,
            "role": role,
            "turn_id": archive_turn_identity(turn),
            "archive_turn_id": turn.archive_turn_id,
            "content": parse_archive_payload(raw_payload),
        }
    })
}

fn build_turn_terminal_event(turn: &ArchiveTurnRecord) -> Value {
    let terminal_type = if turn.stream_was_interrupted || turn.completion_state == "interrupted" {
        "archive_turn_interrupted"
    } else if turn.status == "error" {
        "archive_turn_error"
    } else {
        "archive_turn_completed"
    };
    let finished_at = turn.request_finished_at.unwrap_or(turn.request_started_at);

    json!({
        "timestamp": format_event_timestamp(finished_at),
        "type": "event_msg",
        "payload": {
            "type": terminal_type,
            "turn_id": archive_turn_identity(turn),
            "archive_turn_id": turn.archive_turn_id,
            "status": turn.status,
            "completion_state": turn.completion_state,
            "error_message": turn.error_message,
            "request_duration_ms": turn.request_duration_ms,
            "usage": {
                "input_tokens": turn.input_tokens,
                "output_tokens": turn.output_tokens,
                "total_tokens": turn.total_tokens,
            }
        }
    })
}

fn archive_session_materialized_path(app_data_dir: &Path, session: &ArchiveSessionRow) -> PathBuf {
    let dt = archive_datetime(session.first_seen_at);
    let owner_segment = archive_path_segment(Some(session.gateway_profile_id.as_str()), "legacy");
    let session_segment = archive_materialized_session_segment(session);
    let file_name = format!(
        "rollout-{}-{}.jsonl",
        dt.format("%Y-%m-%dT%H-%M-%SZ"),
        archive_path_segment(Some(session.archive_session_id.as_str()), "archive-session")
    );

    app_data_dir
        .join("archives")
        .join("codex-sessions")
        .join(owner_segment)
        .join(session_segment)
        .join(dt.format("%Y").to_string())
        .join(dt.format("%m").to_string())
        .join(dt.format("%d").to_string())
        .join(file_name)
}

fn archive_materialized_session_segment(session: &ArchiveSessionRow) -> String {
    archive_path_segment(
        session
            .explicit_session_id
            .as_deref()
            .or(session.prompt_cache_key.as_deref())
            .or(Some(session.archive_session_id.as_str())),
        "unknown-session",
    )
}

fn archive_path_segment(value: Option<&str>, fallback: &str) -> String {
    let normalized = normalize_marker(value);
    let sanitized = sanitize_archive_path_component(normalized.as_deref().unwrap_or(fallback));

    if sanitized.is_empty() {
        fallback.to_string()
    } else {
        sanitized
    }
}

fn sanitize_archive_path_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut last_was_underscore = false;

    for ch in value.chars() {
        let sanitized = match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => ch,
            _ => '_',
        };

        if sanitized == '_' {
            if last_was_underscore {
                continue;
            }
            last_was_underscore = true;
        } else {
            last_was_underscore = false;
        }

        out.push(sanitized);
    }

    out.trim_matches(|c| matches!(c, '_' | '.' | ' '))
        .to_string()
}

fn archive_turn_identity(turn: &ArchiveTurnRecord) -> String {
    turn.turn_id
        .clone()
        .unwrap_or_else(|| turn.archive_turn_id.clone())
}

fn parse_archive_payload(raw_payload: &str) -> Value {
    serde_json::from_str::<Value>(raw_payload)
        .unwrap_or_else(|_| Value::String(raw_payload.to_string()))
}

fn format_event_timestamp(timestamp: i64) -> String {
    archive_datetime(timestamp).to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn archive_datetime(timestamp: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(timestamp, 0).unwrap_or_else(chrono::Utc::now)
}

fn archive_confidence_label(confidence: ArchiveSessionConfidence) -> &'static str {
    match confidence {
        ArchiveSessionConfidence::High => "high",
        ArchiveSessionConfidence::Medium => "medium",
        ArchiveSessionConfidence::Low => "low",
    }
}

impl ArchiveTurnCapture {
    pub fn new(context: ArchiveTurnContext) -> Self {
        let originator = extract_header_value(&context.request_headers, "originator");
        let client_user_agent = extract_header_value(&context.request_headers, "user-agent");

        Self {
            request_headers_json: serialize_headers_for_archive(&context.request_headers),
            request_body_json: bytes_to_optional_text(&context.request_body),
            response_headers_json: None,
            response_body_text: String::new(),
            originator,
            client_user_agent,
            selected_account_id: None,
            selected_account_email: None,
            context,
        }
    }

    pub fn set_selected_account(
        &mut self,
        account_id: impl Into<String>,
        account_email: impl Into<String>,
    ) {
        self.selected_account_id = Some(account_id.into());
        self.selected_account_email = Some(account_email.into());
    }

    pub fn set_response_headers(&mut self, headers: &HeaderMap) {
        self.response_headers_json = serialize_headers_for_archive(headers);
    }

    pub fn append_response_chunk(&mut self, chunk: Bytes) {
        self.response_body_text
            .push_str(&String::from_utf8_lossy(&chunk));
    }

    pub fn finish_completed(
        self,
        status: &str,
        usage: ArchiveUsageStats,
        storage: &CodexArchiveStorage,
    ) -> Result<(), String> {
        self.finish_response(status, None, usage, storage)
    }

    pub fn finish_response(
        self,
        status: &str,
        error_message: Option<String>,
        usage: ArchiveUsageStats,
        storage: &CodexArchiveStorage,
    ) -> Result<(), String> {
        self.persist(status, "completed", error_message, false, usage, storage)
    }

    pub fn finish_with_stream_error(
        self,
        error_message: String,
        storage: &CodexArchiveStorage,
    ) -> Result<(), String> {
        self.persist(
            "error",
            "interrupted",
            Some(error_message),
            true,
            ArchiveUsageStats::default(),
            storage,
        )
    }

    fn persist(
        self,
        status: &str,
        completion_state: &str,
        error_message: Option<String>,
        stream_was_interrupted: bool,
        usage: ArchiveUsageStats,
        storage: &CodexArchiveStorage,
    ) -> Result<(), String> {
        let request_finished_at = chrono::Utc::now().timestamp();
        let request_duration_ms =
            Some((request_finished_at - self.context.request_started_at).max(0) * 1000);
        let turn_id = self.context.markers.turn_id.clone();
        let archive_turn_id = turn_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        storage.record_turn(ArchiveTurnRecord {
            archive_turn_id,
            archive_session_id: self.context.session.synthetic_session_key.clone(),
            gateway_profile_id: self.context.gateway_profile_id,
            gateway_profile_name: self.context.gateway_profile_name,
            member_code: self.context.member_code,
            display_label: self.context.display_label,
            prompt_cache_key: self.context.prompt_cache_key,
            explicit_session_id: self.context.explicit_session_id,
            confidence: self.context.session.confidence,
            source: "codex".to_string(),
            originator: self.originator,
            client_user_agent: self.client_user_agent,
            turn_id,
            request_path: self.context.request_path,
            request_method: self.context.request_method,
            model: self.context.model,
            format: self.context.format,
            status: status.to_string(),
            completion_state: completion_state.to_string(),
            error_message,
            request_started_at: self.context.request_started_at,
            request_finished_at: Some(request_finished_at),
            request_duration_ms,
            request_headers_json: self.request_headers_json,
            request_body_json: self.request_body_json,
            response_headers_json: self.response_headers_json,
            response_body_text: normalize_marker(Some(self.response_body_text.as_str())),
            stream_was_interrupted,
            sandbox: self.context.markers.sandbox,
            selected_account_id: self.selected_account_id,
            selected_account_email: self.selected_account_email,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage.total_tokens,
        })
    }
}

fn extract_header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| normalize_marker(Some(value)))
}

fn bytes_to_optional_text(bytes: &Bytes) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    normalize_marker(Some(text.as_ref()))
}

fn serialize_headers_for_archive(headers: &HeaderMap) -> Option<String> {
    let mut map = Map::new();

    for (name, value) in headers.iter() {
        let header_name = name.as_str().to_ascii_lowercase();
        let raw_value = value.to_str().unwrap_or_default();
        let stored_value = if should_redact_archive_header(&header_name) {
            "[REDACTED]".to_string()
        } else {
            raw_value.to_string()
        };
        map.insert(header_name, Value::String(stored_value));
    }

    if map.is_empty() {
        None
    } else {
        serde_json::to_string(&Value::Object(map)).ok()
    }
}

fn should_redact_archive_header(header_name: &str) -> bool {
    matches!(
        header_name,
        "authorization" | "x-api-key" | "cookie" | "set-cookie"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        ArchiveSessionConfidence, ArchiveTurnCapture, ArchiveTurnContext, ArchiveUsageStats,
        derive_archive_session_identity, export_archive_session_jsonl, extract_prompt_cache_key,
        extract_turn_metadata, materialize_archive_session_file,
    };
    use crate::platforms::openai::codex::archive_storage::CodexArchiveStorage;
    use bytes::Bytes;
    use serde_json::Value;
    use tempfile::tempdir;
    use warp::http::{HeaderMap, HeaderValue};

    fn create_test_archive_storage() -> CodexArchiveStorage {
        let temp_dir = tempdir().unwrap();
        let data_dir = temp_dir.path().to_path_buf();
        std::mem::forget(temp_dir);
        CodexArchiveStorage::new(data_dir).unwrap()
    }

    fn seed_capture(prompt_cache_key: Option<&str>, turn_id: Option<&str>) -> ArchiveTurnCapture {
        let mut request_headers = HeaderMap::new();
        request_headers.insert("originator", HeaderValue::from_static("codex_cli_rs"));
        request_headers.insert("user-agent", HeaderValue::from_static("codex_cli_rs/0.0.0"));
        if let Some(turn_id) = turn_id {
            request_headers.insert(
                "X-Codex-Turn-Metadata",
                HeaderValue::from_str(&format!(
                    r#"{{"turn_id":"{turn_id}","sandbox":"seatbelt"}}"#
                ))
                .unwrap(),
            );
        }

        let markers = extract_turn_metadata(&request_headers);
        let session = derive_archive_session_identity("codex-jdd", prompt_cache_key, turn_id);

        ArchiveTurnCapture::new(ArchiveTurnContext {
            gateway_profile_id: "codex-jdd".to_string(),
            gateway_profile_name: Some("姜大大".to_string()),
            member_code: Some("jdd".to_string()),
            display_label: Some("姜大大 · jdd · 产品与方法论".to_string()),
            prompt_cache_key: prompt_cache_key.map(str::to_string),
            explicit_session_id: None,
            markers,
            session,
            request_path: "/v1/responses".to_string(),
            request_method: "POST".to_string(),
            model: "gpt-5.4".to_string(),
            format: "openai-responses".to_string(),
            request_headers,
            request_body: Bytes::from_static(br#"{"input":"hello"}"#),
            request_started_at: 1_000,
        })
    }

    #[test]
    fn codex_archive_marker_extracts_turn_metadata_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Codex-Turn-Metadata",
            HeaderValue::from_static(
                r#"{"turn_id":"019cec64-fd30-7b93-a518-bc6b90a6023b","sandbox":"seatbelt"}"#,
            ),
        );

        let markers = extract_turn_metadata(&headers);

        assert_eq!(
            markers.turn_id.as_deref(),
            Some("019cec64-fd30-7b93-a518-bc6b90a6023b")
        );
        assert_eq!(markers.sandbox.as_deref(), Some("seatbelt"));
    }

    #[test]
    fn codex_archive_marker_extracts_prompt_cache_key_from_responses_body() {
        let body = Bytes::from_static(
            br#"{
                "model":"gpt-5.4",
                "prompt_cache_key":"019cec64-a5e3-7950-b0eb-81ff139811d4",
                "input":"hello"
            }"#,
        );

        let prompt_cache_key = extract_prompt_cache_key(&body);

        assert_eq!(
            prompt_cache_key.as_deref(),
            Some("019cec64-a5e3-7950-b0eb-81ff139811d4")
        );
    }

    #[test]
    fn codex_archive_marker_derive_session_prefers_prompt_cache_key() {
        let identity = derive_archive_session_identity(
            "codex-jdd",
            Some("019cec64-a5e3-7950-b0eb-81ff139811d4"),
            Some("019cec64-fd30-7b93-a518-bc6b90a6023b"),
        );

        assert_eq!(identity.confidence, ArchiveSessionConfidence::Medium);
        assert_eq!(
            identity.synthetic_session_key,
            "codex-jdd:prompt-cache:019cec64-a5e3-7950-b0eb-81ff139811d4"
        );
    }

    #[test]
    fn codex_archive_marker_derive_session_falls_back_to_turn_id() {
        let identity = derive_archive_session_identity(
            "codex-jdd",
            None,
            Some("019cec64-fd30-7b93-a518-bc6b90a6023b"),
        );

        assert_eq!(identity.confidence, ArchiveSessionConfidence::Low);
        assert_eq!(
            identity.synthetic_session_key,
            "codex-jdd:turn:019cec64-fd30-7b93-a518-bc6b90a6023b"
        );
    }

    #[test]
    fn codex_archive_capture_buffered_response_persists_request_and_response_body() {
        let storage = create_test_archive_storage();
        let mut capture = seed_capture(Some("prompt-a"), Some("turn-1"));
        let mut response_headers = HeaderMap::new();
        response_headers.insert("content-type", HeaderValue::from_static("application/json"));

        capture.set_selected_account("acct-1", "acct-1@example.com");
        capture.set_response_headers(&response_headers);
        capture.append_response_chunk(Bytes::from_static(br#"{"id":"resp_1"}"#));
        capture
            .finish_completed(
                "success",
                ArchiveUsageStats {
                    input_tokens: 10,
                    output_tokens: 20,
                    total_tokens: 30,
                },
                &storage,
            )
            .unwrap();

        let turn = storage.get_turn("turn-1").unwrap().unwrap();

        assert_eq!(turn.completion_state, "completed");
        assert_eq!(turn.status, "success");
        assert_eq!(
            turn.request_body_json.as_deref(),
            Some("{\"input\":\"hello\"}")
        );
        assert_eq!(
            turn.response_body_text.as_deref(),
            Some("{\"id\":\"resp_1\"}")
        );
        assert_eq!(turn.selected_account_id.as_deref(), Some("acct-1"));
    }

    #[test]
    fn codex_archive_capture_stream_completion_persists_full_sse_transcript() {
        let storage = create_test_archive_storage();
        let mut capture = seed_capture(Some("prompt-a"), Some("turn-1"));

        capture.append_response_chunk(Bytes::from_static(
            b"event: response.output_text.delta\ndata: {\"delta\":\"hel\"}\n\n",
        ));
        capture.append_response_chunk(Bytes::from_static(
            b"event: response.completed\ndata: {\"response\":{\"id\":\"resp_1\"}}\n\n",
        ));
        capture
            .finish_completed(
                "success",
                ArchiveUsageStats {
                    input_tokens: 10,
                    output_tokens: 20,
                    total_tokens: 30,
                },
                &storage,
            )
            .unwrap();

        let turn = storage.get_turn("turn-1").unwrap().unwrap();

        assert_eq!(turn.completion_state, "completed");
        assert!(
            turn.response_body_text
                .as_deref()
                .unwrap_or_default()
                .contains("response.completed")
        );
    }

    #[test]
    fn codex_archive_capture_interrupted_stream_records_partial_transcript() {
        let storage = create_test_archive_storage();
        let mut capture = seed_capture(Some("prompt-a"), Some("turn-1"));

        capture.append_response_chunk(Bytes::from_static(
            b"event: response.output_text.delta\ndata: {\"delta\":\"hel\"}\n\n",
        ));
        capture
            .finish_with_stream_error("upstream timeout".to_string(), &storage)
            .unwrap();

        let turn = storage.get_turn("turn-1").unwrap().unwrap();

        assert_eq!(turn.completion_state, "interrupted");
        assert_eq!(turn.status, "error");
        assert!(turn.stream_was_interrupted);
        assert!(
            turn.response_body_text
                .as_deref()
                .unwrap_or_default()
                .contains("response.output_text.delta")
        );
    }

    #[test]
    fn codex_archive_export_writes_codex_like_jsonl() {
        let storage = create_test_archive_storage();

        let mut completed_turn = seed_capture(Some("prompt-a"), Some("turn-1"));
        completed_turn.append_response_chunk(Bytes::from_static(br#"{"id":"resp_1"}"#));
        completed_turn
            .finish_completed(
                "success",
                ArchiveUsageStats {
                    input_tokens: 10,
                    output_tokens: 20,
                    total_tokens: 30,
                },
                &storage,
            )
            .unwrap();

        let mut interrupted_turn = seed_capture(Some("prompt-a"), Some("turn-2"));
        interrupted_turn.append_response_chunk(Bytes::from_static(
            b"event: response.output_text.delta\ndata: {\"delta\":\"partial\"}\n\n",
        ));
        interrupted_turn
            .finish_with_stream_error("upstream timeout".to_string(), &storage)
            .unwrap();

        let out =
            export_archive_session_jsonl(&storage, "codex-jdd:prompt-cache:prompt-a").unwrap();
        let lines: Vec<Value> = out
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();

        assert_eq!(lines[0]["type"], "session_meta");
        assert!(
            lines.iter().any(|line| line["type"] == "turn_context"),
            "expected turn_context rows"
        );
        assert!(
            lines.iter().any(|line| {
                line["type"] == "response_item" && line["payload"]["source"] == "archive_request"
            }),
            "expected request payload rows"
        );
        assert!(
            lines.iter().any(|line| {
                line["type"] == "response_item" && line["payload"]["source"] == "archive_response"
            }),
            "expected response payload rows"
        );
        assert!(
            lines.iter().any(|line| {
                line["type"] == "event_msg" && line["payload"]["type"] == "archive_turn_interrupted"
            }),
            "expected interruption rows"
        );
    }

    #[test]
    fn codex_archive_export_materializes_jsonl_under_key_and_rollout_tree() {
        let temp_dir = tempdir().unwrap();
        let storage = CodexArchiveStorage::new(temp_dir.path().join("storage")).unwrap();

        let mut capture = ArchiveTurnCapture::new(ArchiveTurnContext {
            gateway_profile_id: "codex:jdd".to_string(),
            gateway_profile_name: Some("姜大大".to_string()),
            member_code: Some("jdd".to_string()),
            display_label: Some("姜大大 · jdd · 产品与方法论".to_string()),
            prompt_cache_key: Some("project:alpha/beta".to_string()),
            explicit_session_id: None,
            markers: super::ArchiveRequestMarkers {
                turn_id: Some("turn-1".to_string()),
                sandbox: Some("seatbelt".to_string()),
                prompt_cache_key: Some("project:alpha/beta".to_string()),
                explicit_session_id: None,
            },
            session: derive_archive_session_identity(
                "codex:jdd",
                Some("project:alpha/beta"),
                Some("turn-1"),
            ),
            request_path: "/v1/responses".to_string(),
            request_method: "POST".to_string(),
            model: "gpt-5.4".to_string(),
            format: "openai-responses".to_string(),
            request_headers: HeaderMap::new(),
            request_body: Bytes::from_static(br#"{"input":"hello"}"#),
            request_started_at: 1_000,
        });
        capture.append_response_chunk(Bytes::from_static(br#"{"id":"resp_1"}"#));
        capture
            .finish_completed("success", ArchiveUsageStats::default(), &storage)
            .unwrap();

        let output_path = materialize_archive_session_file(
            &storage,
            temp_dir.path(),
            "codex:jdd:prompt-cache:project:alpha/beta",
        )
        .unwrap();

        assert_eq!(
            output_path,
            temp_dir.path().join(
                "archives/codex-sessions/codex_jdd/project_alpha_beta/1970/01/01/rollout-1970-01-01T00-16-40Z-codex_jdd_prompt-cache_project_alpha_beta.jsonl"
            )
        );
        assert!(output_path.exists());

        let contents = std::fs::read_to_string(&output_path).unwrap();
        let first_line: Value = serde_json::from_str(contents.lines().next().unwrap()).unwrap();
        assert_eq!(first_line["type"], "session_meta");
    }

    #[test]
    fn codex_archive_export_materialization_prefers_explicit_session_segment() {
        let temp_dir = tempdir().unwrap();
        let storage = CodexArchiveStorage::new(temp_dir.path().join("storage")).unwrap();

        let mut capture = ArchiveTurnCapture::new(ArchiveTurnContext {
            gateway_profile_id: "codex-jdd".to_string(),
            gateway_profile_name: Some("姜大大".to_string()),
            member_code: Some("jdd".to_string()),
            display_label: Some("姜大大 · jdd · 产品与方法论".to_string()),
            prompt_cache_key: Some("prompt-a".to_string()),
            explicit_session_id: Some("repo:atm/archive".to_string()),
            markers: super::ArchiveRequestMarkers {
                turn_id: Some("turn-1".to_string()),
                sandbox: Some("seatbelt".to_string()),
                prompt_cache_key: Some("prompt-a".to_string()),
                explicit_session_id: Some("repo:atm/archive".to_string()),
            },
            session: derive_archive_session_identity("codex-jdd", Some("prompt-a"), Some("turn-1")),
            request_path: "/v1/responses".to_string(),
            request_method: "POST".to_string(),
            model: "gpt-5.4".to_string(),
            format: "openai-responses".to_string(),
            request_headers: HeaderMap::new(),
            request_body: Bytes::from_static(br#"{"input":"hello"}"#),
            request_started_at: 1_000,
        });
        capture.append_response_chunk(Bytes::from_static(br#"{"id":"resp_1"}"#));
        capture
            .finish_completed("success", ArchiveUsageStats::default(), &storage)
            .unwrap();

        let output_path = materialize_archive_session_file(
            &storage,
            temp_dir.path(),
            "codex-jdd:prompt-cache:prompt-a",
        )
        .unwrap();

        assert_eq!(
            output_path,
            temp_dir.path().join(
                "archives/codex-sessions/codex-jdd/repo_atm_archive/1970/01/01/rollout-1970-01-01T00-16-40Z-codex-jdd_prompt-cache_prompt-a.jsonl"
            )
        );
    }
}
