# ATM Augment Proxy Panel Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a dedicated Augment proxy panel inside the Augment page so users can see readiness, copy the unified Codex CLI base URL, and control the shared ATM API server without disturbing the existing Augment token-management flow.

**Architecture:** Reuse the shipped Augment sidecar integration as-is. Add one backend Tauri command that summarizes proxy readiness, then render a compact frontend panel that consumes that summary and uses the existing API server start/stop commands.

**Tech Stack:** Rust, Tauri 2 commands, Vue 3 `<script setup>`, Pinia, warp-backed API server state, existing Augment sidecar manager

---

### Task 1: Add Backend Augment Proxy Status Summary

**Files:**
- Modify: `src-tauri/src/platforms/augment/commands.rs`
- Modify: `src-tauri/src/platforms/augment/proxy_server.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Write the failing test**

Add backend tests that lock the status summary behavior:

- when API server is not running, `proxy_base_url` still resolves to `http://127.0.0.1:8766/v1`
- when sidecar manager is absent, `sidecar_configured` is `false`
- when tokens include banned / expired / zero-credit entries, only truly usable accounts are counted

Example target:

```rust
#[test]
fn augment_proxy_status_counts_only_usable_accounts() {
    let tokens = vec![usable_token(), banned_token(), expired_token()];
    let status = summarize_augment_proxy_status(false, None, false, false, &tokens);
    assert_eq!(status.total_accounts, 3);
    assert_eq!(status.available_accounts, 1);
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml augment_proxy_status_counts_only_usable_accounts -- --nocapture
```

Expected: FAIL because the summary helper / command does not exist yet.

**Step 3: Write minimal implementation**

Implement:

- a serializable `AugmentProxyStatus` response struct
- a pure summary helper that computes counts and user-facing base URLs
- `get_augment_proxy_status` command
- a small `pub(crate)` export for the existing token usability helper so summary logic and runtime logic stay aligned

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml augment_proxy_status_ -- --nocapture
```

Expected: PASS for the new backend proxy-status tests.

**Step 5: Commit**

```bash
git add src-tauri/src/platforms/augment/commands.rs src-tauri/src/platforms/augment/proxy_server.rs src-tauri/src/lib.rs
git commit -m "feat(augment): add proxy status command"
```

### Task 2: Build The Augment Proxy Panel Component

**Files:**
- Create: `src/components/token/AugmentProxyPanel.vue`
- Modify: `src/components/token/TokenList.vue`

**Step 1: Define the failing surface**

Create the new component shell and wire it into `TokenList.vue` below the existing header row, with placeholder bindings for:

- loading state
- error state
- status badges
- base URL
- copy actions
- API server start / stop actions

The first build should fail because the component logic still references missing data or translations.

**Step 2: Run build to verify it fails**

Run:

```bash
pnpm build
```

Expected: FAIL because the new component is not fully wired yet.

**Step 3: Write minimal implementation**

Implement the panel so it:

- calls `invoke('get_augment_proxy_status')` on mount
- listens for `api-server-status-changed`
- listens for `tokens-updated`
- uses existing `start_api_server_cmd` / `stop_api_server`
- copies the proxy base URL
- copies a shell snippet like:

```bash
export OPENAI_BASE_URL=http://127.0.0.1:8766/v1
```

Mount the panel in `TokenList.vue` without changing the rest of the token list behavior.

**Step 4: Run build to verify it passes**

Run:

```bash
pnpm build
```

Expected: PASS.

**Step 5: Commit**

```bash
git add src/components/token/AugmentProxyPanel.vue src/components/token/TokenList.vue
git commit -m "feat(augment): add proxy panel"
```

### Task 3: Add Locale Copy And Final Verification

**Files:**
- Modify: `src/locales/zh-CN.js`
- Modify: `src/locales/en-US.js`

**Step 1: Write the failing surface**

Add the new translation keys the component expects:

- panel title
- status labels
- inline descriptions
- button labels
- copy success / failure toasts

If keys are missing, the build or UI output will expose untranslated strings.

**Step 2: Run verification to expose gaps**

Run:

```bash
pnpm build
```

Expected: FAIL or reveal missing bindings if any translation keys are still missing.

**Step 3: Write minimal implementation**

Add matching `zh-CN` and `en-US` strings for the new panel. Keep the copy direct and operational.

**Step 4: Run full verification**

Run:

```bash
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
cargo build --manifest-path src-tauri/Cargo.toml
```

Expected:

- frontend build passes
- Rust tests pass
- Tauri Rust build passes

**Step 5: Commit**

```bash
git add src/locales/zh-CN.js src/locales/en-US.js
git commit -m "feat(augment): localize proxy panel"
```
