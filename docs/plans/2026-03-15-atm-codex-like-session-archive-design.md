# ATM Codex-Like Session Archive Design

## Goal

Capture gateway traffic into a local archive that is as close as practical to `~/.codex/sessions`, while staying honest about what ATM really knows and what it has to infer.

The archive should let us answer three different questions without mixing them together:

- which team member key made the request
- which Codex rollout-like chain the request most likely belongs to
- what the raw request and response content looked like for later replay, audit, or dataset curation

## Confirmed Facts From The Current Runtime

From the live ATM instance and the real request sample currently being proxied:

- each external team member key is already resolved to one `GatewayAccessProfile`
- request logs already persist `gateway_profile_id`, `gateway_profile_name`, and member metadata
- real Codex requests include `X-Codex-Turn-Metadata` with a stable `turn_id`
- real Codex requests include `prompt_cache_key`
- the captured `prompt_cache_key` matched a real local Codex rollout filename once
- real Codex requests do not currently include `previous_response_id`
- real Codex requests do not currently include `metadata.session_id` or `metadata.project_id`
- internal upstream account selection is still round-robin and may switch on retries

These facts imply:

- member-level attribution is strong
- turn-level attribution is strong
- rollout/session attribution is only medium confidence unless the client starts sending stronger markers

## Problem Boundary

The hard problem is not file format. The hard problem is attribution.

We can reliably say:

- this request came from member key `A`
- this request was turn `B`
- this request likely belongs to rollout-like chain `C`

We cannot reliably say:

- this request belongs to project `X`
- these 40 requests are definitely one true business session

That remains impossible when one member key may drive many concurrent projects and the client does not send explicit session or project markers.

## Approved Decisions

- `1 member = 1 gateway profile = 1 first-class archive owner`
- never merge archive data across different `gateway_profile_id`
- internal upstream round-robin must never affect archive identity
- selected upstream account may be stored only as diagnostic metadata
- archive grouping is `synthetic rollout`, not guaranteed true project session
- Codex is the first implementation target because it already exposes `turn_id` and `prompt_cache_key`
- Augment can reuse the storage shape later, but it is out of scope for the first pass
- raw transcript capture should be explicit and local-only; do not silently sync or expose it remotely

## Identity Model

Archive identity should be layered and conservative.

### Layer 1: Member

Use `gateway_profile_id` as the stable owner key.

This is already resolved before the request reaches the Codex proxy path, so it is independent of whatever internal OpenAI account gets picked later.

### Layer 2: Synthetic Rollout

Use this priority order:

1. explicit future marker such as `X-Session-Id` or `metadata.session_id`
2. `prompt_cache_key`
3. fallback singleton derived from `turn_id`

This gives us:

- `high` confidence when the client sends an explicit session marker
- `medium` confidence when `prompt_cache_key` is present
- `low` confidence when only `turn_id` exists and the request must stand alone

### Layer 3: Turn

Use `X-Codex-Turn-Metadata.turn_id` as the stable request-level identity.

If the header is missing or malformed, generate a surrogate UUID and mark confidence accordingly.

## Why Internal Key Rotation Does Not Break This

Archive grouping happens at gateway ingress, before upstream account selection.

That means these fields define archive identity:

- `gateway_profile_id`
- explicit session marker if present
- `prompt_cache_key`
- `turn_id`

The following fields must not participate in session identity:

- internal OpenAI account id
- internal account email
- internal retry attempt count
- payment-required or forbidden failover account

This keeps archive stability intact even when ATM switches internal upstream accounts mid-run.

## Storage Architecture

Use a two-layer design:

1. normalized local SQLite storage for correctness and concurrency
2. codex-like JSONL materialization for replay/export compatibility

This is better than writing only JSONL files live because:

- concurrent turns from the same member are easier to store safely
- partial streams can be updated or marked interrupted
- exports can be regenerated if the JSONL format evolves

## Proposed Data Model

### `codex_archive_sessions`

- `archive_session_id`
- `gateway_profile_id`
- `gateway_profile_name`
- `member_code`
- `display_label`
- `prompt_cache_key`
- `explicit_session_id`
- `confidence`
- `source`
- `originator`
- `client_user_agent`
- `first_seen_at`
- `last_seen_at`
- `turn_count`

### `codex_archive_turns`

- `archive_turn_id`
- `archive_session_id`
- `turn_id`
- `request_path`
- `request_method`
- `model`
- `format`
- `status`
- `completion_state`
- `error_message`
- `request_started_at`
- `request_finished_at`
- `request_duration_ms`
- `request_headers_json`
- `request_body_json`
- `response_headers_json`
- `response_body_text`
- `stream_was_interrupted`
- `prompt_cache_key`
- `sandbox`
- `originator`
- `selected_account_id`
- `selected_account_email`
- `input_tokens`
- `output_tokens`
- `total_tokens`

`completion_state` should be more honest than the current request log status. Suggested values:

- `completed`
- `interrupted`
- `upstream_error`
- `client_cancelled`

## Codex-Like JSONL Materialization

Exported files should live under ATM app data, not directly inside `~/.codex`.

Recommended path:

`<app_data_dir>/archives/codex-sessions/YYYY/MM/DD/rollout-<timestamp>-<archive_session_id>.jsonl`

Each exported file should use a Codex-like event stream:

- first row: `session_meta`
- per turn: `turn_context`
- per turn start/finish/error: `event_msg`
- request and response payloads: `response_item`

This should be deliberately described as `codex-like`, not `native Codex`.

We can get close on structure, but not guarantee byte-for-byte parity with the real CLI session files.

## Request And Response Capture Strategy

### Request Side

Capture the gateway-facing request before any upstream normalization mutates it.

That means storing:

- original request headers with secrets redacted
- original request body
- extracted markers such as `turn_id`, `prompt_cache_key`, `originator`, and `sandbox`

Optionally, later versions may also store the normalized upstream request body for debugging. It is not required for the first pass.

### Response Side

For non-stream responses:

- capture the final body directly

For stream responses:

- continue forwarding chunks immediately to the client
- also accumulate the full SSE text in memory
- persist the captured SSE transcript when the stream ends
- if the stream read fails, persist the partial SSE text and mark the turn as `interrupted`

This gives the archive better truth than the current request log, which can still show `success` after a stream started but never completed.

## Scope For The First Pass

Included:

- Codex request marker extraction
- normalized archive SQLite store
- raw request and response transcript capture for Codex
- JSONL export in codex-like layout
- local commands to query and export archive sessions

Not included:

- UI panel work
- remote sync
- cross-key merge
- project naming inference
- Augment transcript archive
- encryption-at-rest beyond normal local app-data permissions

## Practical Outcome

With this design, one member key can drive many concurrent projects without corrupting member attribution.

The system will not promise true project sessions. Instead, it will produce:

- strong member ownership
- strong turn identity
- medium-confidence rollout grouping
- high-quality raw transcripts for later manual curation

That is the right level of honesty for the current gateway architecture.
