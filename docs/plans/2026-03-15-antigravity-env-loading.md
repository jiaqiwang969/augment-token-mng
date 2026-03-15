# Antigravity Env Loading Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make Antigravity OAuth credentials load safely from local env files while keeping real secrets out of git and preserving compatibility with older environment variable names.

**Architecture:** Add a small shell loader for `.env.antigravity` and `.env`, wire the Makefile dev commands through that loader, and update the Rust Antigravity OAuth module to resolve credentials from either the new `ATM_*` names or the legacy `CLIPROXY_*` names. Track only a template env file, ignore the real file, and document the workflow in a new repository-level `AGENTS.md`.

**Tech Stack:** Bash, Makefile, Rust, Node test runner, Rust unit tests

---

### Task 1: Capture the desired env-file behavior in tests

**Files:**
- Create: `tests/antigravityEnv.test.js`
- Modify: `src-tauri/src/platforms/antigravity/modules/oauth.rs`

**Step 1: Write the failing tests**

Add tests that expect:
- `.env.antigravity` parsing to accept comments and export both OAuth variables
- `.env` to be a fallback when `.env.antigravity` is absent
- Rust env resolution to mention both the new `ATM_*` and legacy `CLIPROXY_*` names in the compatibility path

**Step 2: Run test to verify it fails**

Run: `node --test tests/antigravityEnv.test.js`
Expected: FAIL because no antigravity env loader helper exists yet and the Rust file does not expose the fallback behavior.

**Step 3: Write minimal implementation**

Add the loader script and update Rust env resolution helpers.

**Step 4: Run test to verify it passes**

Run: `node --test tests/antigravityEnv.test.js`
Expected: PASS.

### Task 2: Wire local dev startup through env files

**Files:**
- Create: `scripts/load_antigravity_env.sh`
- Modify: `Makefile`
- Modify: `.gitignore`
- Create: `.env.antigravity.example`

**Step 1: Write the failing test**

Extend the Node test to assert:
- Makefile dev commands source `scripts/load_antigravity_env.sh`
- `.env.antigravity` is ignored
- `.env.antigravity.example` contains placeholder keys for the required OAuth values

**Step 2: Run test to verify it fails**

Run: `node --test tests/antigravityEnv.test.js`
Expected: FAIL because the files or references are missing.

**Step 3: Write minimal implementation**

Add the shell loader, make targets, gitignore entry, and example env file.

**Step 4: Run test to verify it passes**

Run: `node --test tests/antigravityEnv.test.js`
Expected: PASS.

### Task 3: Document the safety rules in the repository

**Files:**
- Create: `AGENTS.md`

**Step 1: Write the failing test**

Extend the Node test to assert that the repository `AGENTS.md` includes:
- the `.env.antigravity` workflow
- the rule that real OAuth credentials must never be committed
- the reminder to keep `.env.antigravity.example` updated when required env names change

**Step 2: Run test to verify it fails**

Run: `node --test tests/antigravityEnv.test.js`
Expected: FAIL because no repository-level `AGENTS.md` exists yet.

**Step 3: Write minimal implementation**

Create `AGENTS.md` with the Antigravity env-file guidance and commit hygiene notes.

**Step 4: Run test to verify it passes**

Run: `node --test tests/antigravityEnv.test.js`
Expected: PASS.

### Task 4: Verify the integrated behavior

**Files:**
- Test: `tests/antigravityEnv.test.js`
- Test: `src-tauri/src/platforms/antigravity/modules/oauth.rs`

**Step 1: Run the targeted JavaScript tests**

Run: `node --test tests/antigravityEnv.test.js`
Expected: PASS.

**Step 2: Run the targeted Rust tests**

Run: `cargo test antigravity::modules::oauth --no-default-features`
Expected: PASS.

**Step 3: Run a compile check**

Run: `cargo check --no-default-features`
Expected: PASS.

**Step 4: Commit**

```bash
git add AGENTS.md .gitignore .env.antigravity.example Makefile \
  scripts/load_antigravity_env.sh tests/antigravityEnv.test.js \
  docs/plans/2026-03-15-antigravity-env-loading.md \
  src-tauri/src/platforms/antigravity/modules/oauth.rs
git commit -m "fix: load antigravity oauth from env files"
```
