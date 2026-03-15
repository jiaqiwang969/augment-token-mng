# Codex Archive Materialize Rebuild Design

**Goal:** When the app performs a full Codex archive materialization, remove legacy top-level JSONL exports and rebuild all session files under the current key/project/date layout.

**Context**

- The archive database is already correct and stores grouped sessions in `codex_archive.db`.
- The remaining issue is historical materialized JSONL files that were written under the old top-level date tree such as `archives/codex-sessions/2026/03/15/...`.
- Current code now writes new exports under `archives/codex-sessions/<gateway_profile_id>/<project-or-session>/<yyyy>/<mm>/<dd>/...`, but old files remain on disk.

**Requirements**

- Full materialization must remove only the legacy top-level export layout.
- Full materialization must then rewrite all session files from the database using the new layout.
- Single-session materialization must not trigger global cleanup.
- Existing new-layout exports must not be deleted by the legacy cleanup step.

**Recommended Approach**

- Add a small archive-side helper that identifies the materialized export root and removes only top-level year directories like `codex-sessions/2026`.
- Route full rebuilds through a shared helper that optionally performs the cleanup, lists sessions from the archive database, and writes each session file with the existing materialization code.
- Keep the cleanup logic close to `archive.rs`, because the path semantics already live there.

**Why This Approach**

- It is one-time and conservative: it only touches the known legacy shape.
- It avoids changing session grouping or database semantics.
- It gives us clear unit-test coverage without needing to spin up the Tauri command layer.

**Testing**

- Red/green test for full rebuild removing legacy top-level exports and rewriting the session under the new path.
- Red/green test for single-session materialization preserving legacy top-level exports.
