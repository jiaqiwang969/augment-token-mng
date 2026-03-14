# ATM Unified V1 Gateway Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the public `/augment` entry with a unified `http://127.0.0.1:8766/v1` gateway where external API keys select whether a request is routed to Codex or the managed Augment sidecar.

**Architecture:** Add a shared gateway access-profile store in ATM app data, migrate the legacy Codex API key into that store, and put a thin `/v1/*` router in front of the existing Codex and Augment backend handlers. Keep Augment as a black-box sidecar integration and expose only generated external keys plus Codex-ready config snippets in the Augment panel.

**Tech Stack:** Rust, Tauri 2 commands, warp, reqwest, tokio, serde/serde_json, Vue 3 `<script setup>`, existing Codex server and Augment sidecar modules

---

### Task 1: Add Shared Gateway Access-Profile Storage And Migration

**Files:**
- Create: `src-tauri/src/core/gateway_access.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/platforms/openai/codex/commands.rs`
- Test: `src-tauri/src/core/gateway_access.rs`

**Step 1: Write the failing test**

Add focused unit tests for:

- loading an empty profile set from app data
- migrating a legacy Codex `api_key` into a shared `target = codex` profile
- resolving a profile by bearer token
- refusing disabled profiles

Example target:

```rust
#[test]
fn migrate_legacy_codex_key_creates_shared_codex_profile() {
    let profiles = GatewayAccessProfiles::migrate_from_legacy_codex_key(Some("sk-legacy".into()));
    let profile = profiles.find_by_key("sk-legacy").expect("profile should exist");
    assert_eq!(profile.target, GatewayTarget::Codex);
    assert!(profile.enabled);
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml gateway_access -- --nocapture
```

Expected: FAIL because the shared gateway-access module and migration logic do not exist yet.

**Step 3: Write minimal implementation**

Implement:

- `GatewayTarget`
- `GatewayAccessProfile`
- `GatewayAccessProfiles`
- `load_gateway_access_profiles`
- `write_gateway_access_profiles`
- `migrate_from_legacy_codex_key`
- `find_by_key`

Wire the shared store into `AppState` so runtime routes and Tauri commands can reuse the same source of truth.

Use `gateway_access_profiles.json` under app data.

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml gateway_access -- --nocapture
```

Expected: PASS for the new storage and migration tests.

**Step 5: Commit**

```bash
git add src-tauri/src/core/gateway_access.rs src-tauri/src/lib.rs src-tauri/src/platforms/openai/codex/commands.rs
git commit -m "feat(gateway): add shared access profile store"
```

### Task 2: Refactor Codex And Augment Backends Behind A Unified `/v1` Router

**Files:**
- Modify: `src-tauri/src/core/api_server.rs`
- Modify: `src-tauri/src/platforms/openai/codex/server.rs`
- Modify: `src-tauri/src/platforms/augment/proxy_server.rs`
- Test: `src-tauri/src/core/api_server.rs`
- Test: `src-tauri/src/platforms/augment/proxy_server.rs`

**Step 1: Write the failing test**

Add route-level tests that lock the new dispatch behavior:

- a valid Codex gateway key hits the Codex backend
- a valid Augment gateway key hits the Augment backend
- an unknown key returns `401`
- an Augment key with no usable accounts returns `503`

Example target:

```rust
#[tokio::test]
async fn unified_gateway_rejects_unknown_api_key() {
    let route = build_unified_gateway_test_route(test_state_with_profiles(vec![]));
    let response = warp::test::request()
        .method("GET")
        .path("/v1/models")
        .header("authorization", "Bearer sk-missing")
        .reply(&route)
        .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml unified_gateway_ -- --nocapture
```

Expected: FAIL because `/v1/*` is still owned by the old Codex-only routing path.

**Step 3: Write minimal implementation**

Implement a thin gateway router that:

- extracts API keys from `Authorization` and `x-api-key`
- resolves an enabled access profile
- dispatches by `GatewayTarget`

Refactor the existing backend code into reusable internal handlers:

- Codex: keep health and pool routes, but expose a prevalidated `/v1/*` backend handler
- Augment: keep sidecar orchestration, but stop depending on the public `/augment` prefix

Update `api_server.rs` to register unified `/v1/*` routes and remove the public Augment route registration.

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml unified_gateway_ augment_ -- --nocapture
```

Expected: PASS for the new unified gateway tests and the updated Augment proxy routing tests.

**Step 5: Commit**

```bash
git add src-tauri/src/core/api_server.rs src-tauri/src/platforms/openai/codex/server.rs src-tauri/src/platforms/augment/proxy_server.rs
git commit -m "feat(gateway): route unified v1 requests by key"
```

### Task 3: Add Augment Gateway Access Commands And Normalize Public URLs

**Files:**
- Modify: `src-tauri/src/platforms/augment/commands.rs`
- Modify: `src-tauri/src/platforms/openai/codex/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/platforms/augment/commands.rs`

**Step 1: Write the failing test**

Add command-level tests for:

- Augment proxy status now reporting `http://127.0.0.1:8766/v1`
- Augment gateway-access payload returning the saved key
- generated Codex snippets using `/v1`, not `/augment`
- generated curl example using the external gateway key

Example target:

```rust
#[test]
fn augment_gateway_snippets_use_unified_v1_base_url() {
    let config = build_augment_gateway_access_config("sk-auggie");
    assert!(config.curl_example.contains("http://127.0.0.1:8766/v1/chat/completions"));
    assert!(config.config_pool_snippet.contains("base_url = \"http://127.0.0.1:8766/v1\""));
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml augment_gateway_ -- --nocapture
```

Expected: FAIL because the commands still return `/augment` URLs and do not expose access-key/snippet helpers.

**Step 3: Write minimal implementation**

Implement:

- `get_augment_gateway_access_config`
- `set_augment_gateway_access_config`
- snippet builders for `auth-pool.json` and `config-pool.toml`
- a generated curl example
- status normalization so Augment public URLs point at `/v1`

Also update `get_codex_access_config` so the public server URL it returns is the unified `/v1` base URL.

Register any new Tauri commands in `src-tauri/src/lib.rs`.

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml augment_gateway_ augment_proxy_status_ -- --nocapture
```

Expected: PASS for the new Augment gateway command tests and the updated status tests.

**Step 5: Commit**

```bash
git add src-tauri/src/platforms/augment/commands.rs src-tauri/src/platforms/openai/codex/commands.rs src-tauri/src/lib.rs
git commit -m "feat(augment): expose unified gateway access config"
```

### Task 4: Update The Augment Proxy Panel For Key Management And Codex Export

**Files:**
- Modify: `src/components/token/AugmentProxyPanel.vue`
- Modify: `src/components/token/TokenList.vue`
- Modify: `src/locales/zh-CN.js`
- Modify: `src/locales/en-US.js`

**Step 1: Define the failing surface**

Update the component bindings so the panel expects:

- unified base URL `http://127.0.0.1:8766/v1`
- access key state
- show/hide key toggle
- save / rotate actions
- curl example copy action
- `auth-pool.json` snippet copy action
- `config-pool.toml` snippet copy action

The first build should fail because the new bindings and locale strings do not exist yet.

**Step 2: Run build to verify it fails**

Run:

```bash
npm run build
```

Expected: FAIL because the panel is not fully wired to the new command payloads and locale keys.

**Step 3: Write minimal implementation**

Implement the new panel flow:

- load proxy status and access config on mount
- generate keys client-side with the same `sk-...` shape already used in the Codex dialog
- save through the new Augment gateway access command
- copy the raw key, curl example, `auth-pool.json` snippet, and `config-pool.toml` snippet
- keep the existing status cards and API server controls

Do not add a separate modal.

Keep the feature inside the current Augment page panel.

**Step 4: Run build to verify it passes**

Run:

```bash
npm run build
```

Expected: PASS.

**Step 5: Commit**

```bash
git add src/components/token/AugmentProxyPanel.vue src/components/token/TokenList.vue src/locales/zh-CN.js src/locales/en-US.js
git commit -m "feat(augment): add unified gateway key controls"
```

### Task 5: Run Full Verification And Smoke-Test The Unified Gateway

**Files:**
- Verify only

**Step 1: Run targeted Rust tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml gateway_access unified_gateway_ augment_gateway_ -- --nocapture
```

Expected: PASS.

**Step 2: Run full project verification**

Run:

```bash
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo build --manifest-path src-tauri/Cargo.toml
```

Expected:

- frontend build passes
- Rust tests pass
- Tauri Rust build passes

**Step 3: Run a local manual smoke test**

With the local ATM server running on `8766`, verify both paths:

```bash
curl http://127.0.0.1:8766/v1/models \
  -H "Authorization: Bearer <codex-key>"

curl http://127.0.0.1:8766/v1/chat/completions \
  -H "Authorization: Bearer <augment-key>" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-5.4","messages":[{"role":"user","content":"Hello"}]}'
```

Expected:

- Codex key returns the Codex-visible model list
- Augment key routes through the sidecar and returns an OpenAI-compatible response

**Step 4: Commit the verified integration**

```bash
git add .
git commit -m "feat(gateway): unify public v1 routing for codex and augment"
```
