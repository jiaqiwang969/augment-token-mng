# Antigravity Env Bootstrap Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a safe local bootstrap and validation flow for Antigravity OAuth secrets so developers can recover credentials into `.env.antigravity` without recommitting secrets.

**Architecture:** Keep `.env.antigravity` as the canonical local secret file, then add two shell entry points: one to recover credentials from trusted local history and one to validate the active setup without leaking values. Wire both into `make` targets and regression tests.

**Tech Stack:** Make, Bash, Node test runner, Rust/Tauri existing env loader

---

### Task 1: Add failing regression coverage for bootstrap and check workflows

**Files:**
- Modify: `tests/antigravityEnv.test.js`

**Step 1: Write the failing test**

Add assertions that require:
- `Makefile` exposes `antigravity-env-bootstrap` and `antigravity-env-check`
- the bootstrap script exists
- the check script exists
- `AGENTS.md` documents the new workflow

**Step 2: Run test to verify it fails**

Run: `node --test tests/antigravityEnv.test.js`
Expected: FAIL because the new targets and scripts do not exist yet.

**Step 3: Write minimal implementation**

Create the missing files and Makefile entries only after observing the failing test.

**Step 4: Run test to verify it passes**

Run: `node --test tests/antigravityEnv.test.js`
Expected: PASS

### Task 2: Implement safe bootstrap and validation scripts

**Files:**
- Create: `scripts/bootstrap_antigravity_env.sh`
- Create: `scripts/check_antigravity_env.sh`
- Modify: `Makefile`

**Step 1: Write the failing test**

Extend the regression test to assert that:
- bootstrap prefers not to overwrite an existing `.env.antigravity` without confirmation/force
- check output is designed around presence/absence rather than printing raw secret values

**Step 2: Run test to verify it fails**

Run: `node --test tests/antigravityEnv.test.js`
Expected: FAIL because the scripts are still missing the required behavior.

**Step 3: Write minimal implementation**

Implement:
- bootstrap script that reads the last trusted historical values from git, writes `.env.antigravity`, sets mode `600`, and never echoes the secret contents
- check script that loads the env chain, verifies required keys, and prints masked metadata only
- Make targets that call both scripts

**Step 4: Run test to verify it passes**

Run: `node --test tests/antigravityEnv.test.js`
Expected: PASS

### Task 3: Document the safe operational workflow

**Files:**
- Modify: `AGENTS.md`
- Modify: `.env.antigravity.example`

**Step 1: Write the failing test**

Require `AGENTS.md` to mention:
- `make antigravity-env-bootstrap`
- `make antigravity-env-check`
- not printing or committing real values

**Step 2: Run test to verify it fails**

Run: `node --test tests/antigravityEnv.test.js`
Expected: FAIL because the wording is not present yet.

**Step 3: Write minimal implementation**

Update the example file comments and repository notes to match the new safe workflow.

**Step 4: Run test to verify it passes**

Run: `node --test tests/antigravityEnv.test.js`
Expected: PASS

### Task 4: Verify end to end

**Files:**
- No code changes expected

**Step 1: Run targeted regression tests**

Run: `node --test tests/antigravityEnv.test.js`
Expected: PASS

**Step 2: Run lightweight suite**

Run: `make test`
Expected: PASS

**Step 3: Run Rust verification**

Run: `cargo check --no-default-features`
Expected: PASS
