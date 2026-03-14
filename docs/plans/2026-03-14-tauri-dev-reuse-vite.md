# Tauri Dev Reuse Existing Vite Server Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make `make dev` and `npx tauri dev` reuse the existing ATM Vite dev server on port 1420 instead of failing with `Port 1420 is already in use`.

**Architecture:** Add a small Node preflight script that probes `http://localhost:1420`, distinguishes between the current ATM Vite server, no server, and a conflicting process, and then either exits cleanly, starts `npm run dev`, or fails with a precise error. Point Tauri `beforeDevCommand` at that script so the behavior is centralized instead of living only in `Makefile`.

**Tech Stack:** Node ESM scripts, Tauri v2 config, Vite dev server, Node built-in test runner

---

### Task 1: Add a failing regression test for dev-server reuse classification

**Files:**
- Create: `tests/ensureViteDev.test.js`
- Test: `tests/ensureViteDev.test.js`

**Step 1: Write the failing test**

Write a Node test that imports `scripts/ensure-vite-dev.mjs` and asserts three outcomes:
- reuse when the probe looks like the current ATM Vite server
- start when nothing reusable is detected
- conflict when port 1420 is occupied by a non-ATM response

**Step 2: Run test to verify it fails**

Run: `node --test tests/ensureViteDev.test.js`
Expected: FAIL because `scripts/ensure-vite-dev.mjs` does not exist yet

**Step 3: Write minimal implementation**

Create `scripts/ensure-vite-dev.mjs` and export a pure `classifyDevServerProbe(...)` helper that makes the test pass.

**Step 4: Run test to verify it passes**

Run: `node --test tests/ensureViteDev.test.js`
Expected: PASS with `3` passing tests

### Task 2: Implement CLI behavior for reusing or starting the Vite dev server

**Files:**
- Create: `scripts/ensure-vite-dev.mjs`
- Modify: `src-tauri/tauri.conf.json`

**Step 1: Write the failing test**

Extend the script logic so it can:
- probe `http://localhost:1420/@vite/client`
- probe `http://localhost:1420/`
- classify the port as `reuse`, `start`, or `conflict`

**Step 2: Run targeted test**

Run: `node --test tests/ensureViteDev.test.js`
Expected: existing classification tests still govern the behavior

**Step 3: Write minimal implementation**

Implement CLI mode:
- `reuse` -> print a short reuse message and exit `0`
- `start` -> spawn `npm run dev` with inherited stdio and wait
- `conflict` -> print a clear error and exit non-zero

Update `src-tauri/tauri.conf.json`:
- set `build.beforeDevCommand` to `node ./scripts/ensure-vite-dev.mjs`

**Step 4: Run test to verify it passes**

Run: `node --test tests/ensureViteDev.test.js`
Expected: PASS

### Task 3: Fold the new flow into the local developer entrypoints

**Files:**
- Modify: `Makefile`

**Step 1: Update the command surface**

Keep `make dev` as:
- `ATM_SKIP_CLIPROXY_BUILD=1 npx tauri dev`

Document that the Tauri preflight now reuses an existing frontend automatically, so `make dev` works whether port `1420` is free or already serving the ATM Vite app.

**Step 2: Run lightweight verification**

Run: `make help`
Expected: the target list still renders and `make dev` remains the primary entrypoint

### Task 4: Verify the full flow end to end

**Files:**
- Test: `tests/ensureViteDev.test.js`
- Test: `src-tauri/tauri.conf.json`
- Test: `Makefile`

**Step 1: Run regression tests**

Run: `node --test tests/tauriBridge.test.js tests/ensureViteDev.test.js`
Expected: all tests pass

**Step 2: Run production build**

Run: `npm run build`
Expected: Vite build succeeds

**Step 3: Verify startup behavior**

Run: `make dev`
Expected:
- if ATM Vite is already on `1420`, Tauri reuses it and continues starting
- if nothing is on `1420`, the script starts `npm run dev`
- if another app holds `1420`, startup fails with a precise conflict error
