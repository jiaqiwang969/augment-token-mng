# Codex Archive Materialize Rebuild Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Remove legacy top-level Codex archive JSONL exports during full materialization and rebuild all session files under the current layout.

**Architecture:** Keep the cleanup and rebuild orchestration in `archive.rs`, where export path rules already live. The Tauri command continues to be the entrypoint, but delegates to a shared helper that optionally removes legacy top-level year directories before rewriting session JSONL files from SQLite.

**Tech Stack:** Rust, Tauri 2, rusqlite, std::fs, tempfile

---

### Task 1: Add Failing Tests For Full-Rebuild Cleanup Semantics

**Files:**
- Modify: `src-tauri/src/platforms/openai/codex/archive.rs`

**Step 1: Write the failing tests**

Add tests covering:

- full rebuild removes legacy `archives/codex-sessions/<yyyy>/...` exports and rewrites the session under the new layout
- single-session materialization keeps legacy top-level exports untouched

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_archive_export_rebuild -- --nocapture
```

Expected: FAIL because the rebuild helper and legacy cleanup behavior do not exist yet.

### Task 2: Implement Legacy Cleanup And Shared Rebuild Helper

**Files:**
- Modify: `src-tauri/src/platforms/openai/codex/archive.rs`
- Modify: `src-tauri/src/platforms/openai/codex/commands.rs`

**Step 1: Add archive export root and legacy cleanup helpers**

Implement helpers that:

- resolve `archives/codex-sessions`
- remove only top-level year directories such as `2026`

**Step 2: Add a shared rebuild helper**

Implement a helper that:

- accepts `Option<&str>` for `archive_session_id`
- performs legacy cleanup only for `None`
- materializes the requested/all sessions with the existing per-session writer

**Step 3: Wire the Tauri command to the helper**

Update `materialize_codex_archive_session_files` to delegate to the shared rebuild helper.

**Step 4: Run tests to verify they pass**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_archive_export_rebuild -- --nocapture
```

Expected: PASS

### Task 3: Run Full Verification

**Files:**
- No code changes

**Step 1: Run archive-focused verification**

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_archive -- --nocapture
```

Expected: PASS

**Step 2: Run full Rust verification**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: PASS
