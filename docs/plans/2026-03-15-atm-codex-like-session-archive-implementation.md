# ATM Codex-Like Session Archive Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a Codex-only transcript archive that groups requests by member key and synthetic rollout markers, persists raw request/response content, and exports codex-like JSONL session files under ATM app data.

**Architecture:** Reuse the existing `gateway profile -> Codex proxy -> request log` path, but add a separate archive store rather than overloading `codex_requests`. Extract `turn_id`, `prompt_cache_key`, and related markers at request ingress, write normalized session/turn rows during buffered and streaming responses, then materialize codex-like JSONL files from the archive store on demand.

**Tech Stack:** Rust, Tauri 2, warp, reqwest, tokio, rusqlite, serde/serde_json, bytes, chrono

---

### Task 1: Add Failing Tests For Codex Archive Marker Extraction

**Files:**
- Create: `src-tauri/src/platforms/openai/codex/archive.rs`
- Modify: `src-tauri/src/platforms/openai/codex/mod.rs`

**Step 1: Write the failing tests**

Add focused unit tests for:

- extracting `turn_id` and `sandbox` from `X-Codex-Turn-Metadata`
- extracting `prompt_cache_key` from a Responses API request body
- deriving a synthetic session identity from `gateway_profile_id + prompt_cache_key`
- falling back to a singleton session when only `turn_id` exists

Example target:

```rust
#[test]
fn derive_archive_session_prefers_prompt_cache_key() {
    let identity = derive_archive_session_identity(
        "codex-jdd",
        Some("019cec64-a5e3-7950-b0eb-81ff139811d4"),
        Some("019cec64-fd30-7b93-a518-bc6b90a6023b"),
    );

    assert_eq!(identity.confidence, ArchiveSessionConfidence::Medium);
    assert_eq!(identity.synthetic_session_key, "codex-jdd:prompt-cache:019cec64-a5e3-7950-b0eb-81ff139811d4");
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_archive_marker -- --nocapture
```

Expected: FAIL because the archive marker helpers do not exist yet.

**Step 3: Commit**

```bash
git add src-tauri/src/platforms/openai/codex/archive.rs src-tauri/src/platforms/openai/codex/mod.rs
git commit -m "test(codex-archive): cover marker extraction"
```

### Task 2: Implement Archive Marker Extraction And Session Identity Helpers

**Files:**
- Create: `src-tauri/src/platforms/openai/codex/archive.rs`
- Modify: `src-tauri/src/platforms/openai/codex/mod.rs`

**Step 1: Add archive marker types**

Implement types such as:

- `ArchiveSessionConfidence`
- `ArchiveRequestMarkers`
- `DerivedArchiveSession`

**Step 2: Implement extraction helpers**

Add helpers for:

- `extract_turn_metadata(headers: &HeaderMap) -> ArchiveRequestMarkers`
- `extract_prompt_cache_key(body: &Bytes) -> Option<String>`
- `derive_archive_session_identity(...) -> DerivedArchiveSession`

**Step 3: Keep the rules conservative**

Do not infer sessions from time windows. Only use:

- explicit future session marker
- `prompt_cache_key`
- `turn_id`

**Step 4: Run tests to verify they pass**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_archive_marker -- --nocapture
```

Expected: PASS for marker extraction and session derivation.

**Step 5: Commit**

```bash
git add src-tauri/src/platforms/openai/codex/archive.rs src-tauri/src/platforms/openai/codex/mod.rs
git commit -m "feat(codex-archive): add request marker extraction"
```

### Task 3: Add Failing Tests For Archive SQLite Storage

**Files:**
- Create: `src-tauri/src/platforms/openai/codex/archive_storage.rs`
- Modify: `src-tauri/src/platforms/openai/codex/mod.rs`

**Step 1: Write the failing tests**

Cover:

- initializing archive tables
- upserting one session row and one turn row
- updating `last_seen_at` and `turn_count`
- querying turns by `archive_session_id`
- preserving `selected_account_id` as diagnostic metadata without using it in grouping

Example target:

```rust
#[test]
fn archive_storage_keeps_turns_under_same_prompt_cache_session() {
    let storage = create_test_archive_storage();

    storage.record_turn(seed_turn("codex-jdd", "prompt-a", "turn-1"));
    storage.record_turn(seed_turn("codex-jdd", "prompt-a", "turn-2"));

    let session = storage.get_session("codex-jdd:prompt-cache:prompt-a").unwrap().unwrap();
    assert_eq!(session.turn_count, 2);
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_archive_storage -- --nocapture
```

Expected: FAIL because the archive storage module and schema do not exist yet.

**Step 3: Commit**

```bash
git add src-tauri/src/platforms/openai/codex/archive_storage.rs src-tauri/src/platforms/openai/codex/mod.rs
git commit -m "test(codex-archive): cover archive storage"
```

### Task 4: Implement Archive Storage And Wire It Into AppState

**Files:**
- Create: `src-tauri/src/platforms/openai/codex/archive_storage.rs`
- Modify: `src-tauri/src/platforms/openai/codex/mod.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Add archive session and turn structs**

Implement storage-facing structs for:

- archive session rows
- archive turn rows
- archive query filters

**Step 2: Create SQLite tables**

Create `codex_archive.db` under app data with tables:

- `codex_archive_sessions`
- `codex_archive_turns`

Add indexes for:

- `archive_session_id`
- `gateway_profile_id`
- `prompt_cache_key`
- `turn_id`
- `request_started_at`

**Step 3: Initialize storage in `AppState`**

Add:

- `pub codex_archive_storage: Arc<Mutex<Option<Arc<CodexArchiveStorage>>>>`

Initialize it next to the existing `CodexLogStorage`.

**Step 4: Run tests to verify they pass**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_archive_storage -- --nocapture
```

Expected: PASS for schema creation and turn persistence.

**Step 5: Commit**

```bash
git add src-tauri/src/platforms/openai/codex/archive_storage.rs src-tauri/src/platforms/openai/codex/mod.rs src-tauri/src/lib.rs
git commit -m "feat(codex-archive): add archive sqlite storage"
```

### Task 5: Add Failing Tests For Transcript Capture In Codex Proxy Paths

**Files:**
- Modify: `src-tauri/src/platforms/openai/codex/server.rs`
- Modify: `src-tauri/src/platforms/openai/codex/archive.rs`
- Modify: `src-tauri/src/platforms/openai/codex/archive_storage.rs`

**Step 1: Write the failing tests**

Cover:

- buffered success responses record request and response bodies
- streaming success responses record full SSE transcript
- interrupted streams record partial transcript and `completion_state = interrupted`
- archive session grouping ignores `account_id`
- archived turn identity includes `gateway_profile_id`, `prompt_cache_key`, and `turn_id`

Example target:

```rust
#[tokio::test]
async fn interrupted_stream_records_partial_turn_archive() {
    let store = create_test_archive_storage();
    let capture = ArchiveTurnCapture::new(seed_markers("codex-jdd", Some("prompt-a"), Some("turn-1")));

    capture.append_response_chunk(Bytes::from("event: response.output_text.delta\n"));
    capture.finish_with_stream_error("upstream timeout".into(), &store).await.unwrap();

    let turn = store.get_turn("turn-1").unwrap().unwrap();
    assert_eq!(turn.completion_state, "interrupted");
    assert!(turn.response_body_text.contains("response.output_text.delta"));
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_archive_capture -- --nocapture
```

Expected: FAIL because the proxy path does not record transcript archive rows yet.

**Step 3: Commit**

```bash
git add src-tauri/src/platforms/openai/codex/server.rs src-tauri/src/platforms/openai/codex/archive.rs src-tauri/src/platforms/openai/codex/archive_storage.rs
git commit -m "test(codex-archive): cover proxy transcript capture"
```

### Task 6: Implement Transcript Capture For Buffered And Streaming Codex Responses

**Files:**
- Modify: `src-tauri/src/platforms/openai/codex/server.rs`
- Modify: `src-tauri/src/platforms/openai/codex/archive.rs`
- Modify: `src-tauri/src/platforms/openai/codex/archive_storage.rs`

**Step 1: Build an archive capture context at request ingress**

Inside `handle_passthrough_internal`, create a capture context from:

- `gateway_profile`
- request headers
- original request body
- request path and method
- `ForwardMeta`

Store the original request body before normalization mutates it.

**Step 2: Record buffered responses**

In the buffered response branches:

- persist request headers/body
- persist response headers/body
- copy usage metrics
- store `selected_account_id` and `selected_account_email` only as diagnostic fields

**Step 3: Record streaming responses**

In `build_streaming_response_with_metrics`:

- accumulate the full or partial SSE transcript in parallel with forwarding
- persist a completed archive turn when the stream finishes
- persist an interrupted archive turn when chunk reading fails

In `destream_responses_sse`:

- record the captured SSE text and extracted final JSON response

**Step 4: Keep request log behavior backward compatible**

Do not redesign the existing `RequestLog` schema in this task.

If stream error semantics need cleanup, store the truth first in the new archive fields and leave request-log cleanup for a follow-up.

**Step 5: Run tests to verify they pass**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_archive_capture -- --nocapture
```

Expected: PASS for buffered, streaming, and interrupted transcript capture.

**Step 6: Commit**

```bash
git add src-tauri/src/platforms/openai/codex/server.rs src-tauri/src/platforms/openai/codex/archive.rs src-tauri/src/platforms/openai/codex/archive_storage.rs
git commit -m "feat(codex-archive): capture codex request and response transcripts"
```

### Task 7: Add Failing Tests For Codex-Like JSONL Export

**Files:**
- Modify: `src-tauri/src/platforms/openai/codex/archive.rs`
- Modify: `src-tauri/src/platforms/openai/codex/archive_storage.rs`
- Modify: `src-tauri/src/platforms/openai/codex/commands.rs`

**Step 1: Write the failing tests**

Cover:

- exporting one archive session creates `rollout-*.jsonl`
- the first line is `session_meta`
- each turn emits `turn_context`
- request messages and response payloads become `response_item`
- interruption emits an `event_msg`

Example target:

```rust
#[test]
fn export_archive_session_writes_codex_like_jsonl() {
    let storage = seed_archive_session_with_one_turn();
    let out = export_archive_session_jsonl(&storage, "codex-jdd:prompt-cache:prompt-a").unwrap();

    let lines: Vec<Value> = out.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    assert_eq!(lines[0]["type"], "session_meta");
    assert!(lines.iter().any(|line| line["type"] == "turn_context"));
    assert!(lines.iter().any(|line| line["type"] == "response_item"));
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_archive_export -- --nocapture
```

Expected: FAIL because no export helper or command exists yet.

**Step 3: Commit**

```bash
git add src-tauri/src/platforms/openai/codex/archive.rs src-tauri/src/platforms/openai/codex/archive_storage.rs src-tauri/src/platforms/openai/codex/commands.rs
git commit -m "test(codex-archive): cover codex-like export"
```

### Task 8: Implement Export Commands And Local Archive Materialization

**Files:**
- Modify: `src-tauri/src/platforms/openai/codex/archive.rs`
- Modify: `src-tauri/src/platforms/openai/codex/archive_storage.rs`
- Modify: `src-tauri/src/platforms/openai/codex/commands.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Implement codex-like JSONL export helpers**

Add helpers to:

- map archive session rows to `session_meta`
- map turn rows to `turn_context`
- map stored request and response bodies to `response_item`
- emit `event_msg` for start, completion, and interruption

**Step 2: Add Tauri commands**

Add commands such as:

- `get_codex_archive_status`
- `list_codex_archive_sessions`
- `export_codex_archive_session`
- `materialize_codex_archive_session_files`

Wire them in `src-tauri/src/lib.rs`.

**Step 3: Write files under app data**

Materialize files under:

`<app_data_dir>/archives/codex-sessions/YYYY/MM/DD/rollout-<timestamp>-<archive_session_id>.jsonl`

Do not write into `~/.codex/sessions`.

**Step 4: Run tests to verify they pass**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_archive_export -- --nocapture
```

Expected: PASS for export and file materialization.

**Step 5: Commit**

```bash
git add src-tauri/src/platforms/openai/codex/archive.rs src-tauri/src/platforms/openai/codex/archive_storage.rs src-tauri/src/platforms/openai/codex/commands.rs src-tauri/src/lib.rs
git commit -m "feat(codex-archive): export codex-like session files"
```

### Task 9: Run End-To-End Verification Against The Local ATM Instance

**Files:**
- No code changes expected

**Step 1: Run focused Rust tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_archive -- --nocapture
```

Expected: PASS for marker extraction, storage, capture, and export.

**Step 2: Run a manual local request through ATM**

Use the running local gateway and one existing team key to send a small `/v1/responses` request.

Verify:

- a new archive session row exists
- a new archive turn row exists
- one `rollout-*.jsonl` file is materialized
- the file starts with `session_meta`

**Step 3: Verify grouping stability**

Send two requests from the same key with the same rollout markers when possible.

Expected:

- same `archive_session_id`
- different `turn_id`
- internal upstream account rotation does not change session grouping

**Step 4: Commit**

```bash
git add -u
git commit -m "test(codex-archive): verify local archive flow"
```
