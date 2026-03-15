# Antigravity API Service Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add an Antigravity API-service dialog under the existing Antigravity account-management page, route Antigravity API keys through ATM's unified `/v1` gateway, and proxy requests to the bundled `cliproxy-server` sidecar with member-specific logs and charts.

**Architecture:** Extend the shared gateway to recognize `GatewayTarget::Antigravity`, add a dedicated `platforms::antigravity::api_service` module plus `AntigravitySidecar`, and reuse the Codex service UI/data shape where practical. Keep the sidecar runtime model unchanged, keep Antigravity account storage in the existing platform module, and isolate Antigravity keys, logs, and stats from Codex.

**Tech Stack:** Rust, Tauri 2, tokio, warp, reqwest, serde/serde_json, rusqlite, Vue 3 `<script setup>`, Chart.js, in-repo Go sidecar build (`sidecars/cliproxy`)

---

### Task 1: Add Failing Tests For Shared Gateway Antigravity Target Support

**Files:**
- Modify: `src-tauri/src/core/gateway_access.rs`
- Modify: `src-tauri/src/core/api_server.rs`

**Step 1: Write the failing test**

Add tests that prove:

- `GatewayTarget` can serialize and deserialize an `antigravity` variant
- `GatewayAccessProfiles::list_by_target(GatewayTarget::Antigravity)` returns only Antigravity profiles
- unified gateway dispatch recognizes an Antigravity profile as a valid `/v1` target

Example target:

```rust
#[test]
fn gateway_access_lists_antigravity_profiles_by_target() {
    let profiles = GatewayAccessProfiles {
        profiles: vec![
            GatewayAccessProfile {
                id: "ag-jdd".into(),
                name: "JDD Antigravity".into(),
                target: GatewayTarget::Antigravity,
                api_key: "sk-ant-jdd-12345678".into(),
                enabled: true,
                member_code: Some("jdd".into()),
                role_title: None,
                persona_summary: None,
                color: None,
                notes: None,
            },
        ],
    };

    assert_eq!(profiles.list_by_target(GatewayTarget::Antigravity).len(), 1);
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml gateway_access antigravity -- --nocapture
```

Expected: FAIL because `GatewayTarget::Antigravity` does not exist yet.

**Step 3: Write minimal implementation**

Add the `Antigravity` target variant and the minimum routing plumbing needed for the new tests to compile.

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml gateway_access antigravity -- --nocapture
```

Expected: PASS for the new target support tests.

**Step 5: Commit**

```bash
git add src-tauri/src/core/gateway_access.rs src-tauri/src/core/api_server.rs
git commit -m "feat(antigravity): add shared gateway target support"
```

### Task 2: Add Failing Tests For Antigravity Gateway Profile Commands And Readable Keys

**Files:**
- Create: `src-tauri/src/platforms/antigravity/api_service/models.rs`
- Create: `src-tauri/src/platforms/antigravity/api_service/team_profiles.rs`
- Create: `src-tauri/src/platforms/antigravity/api_service/commands.rs`
- Modify: `src-tauri/src/platforms/antigravity/mod.rs`

**Step 1: Write the failing test**

Add tests that prove:

- Antigravity gateway keys use `sk-ant-<member_code>-<random8>`
- Antigravity profile CRUD is isolated to `GatewayTarget::Antigravity`
- duplicate `member_code` is rejected inside Antigravity profiles
- Antigravity team defaults can be imported or initialized without touching Codex profiles

Example target:

```rust
#[test]
fn generate_antigravity_gateway_api_key_uses_member_code_prefix() {
    let key = generate_antigravity_gateway_api_key("jdd");

    assert!(key.starts_with("sk-ant-jdd-"));
    assert_eq!(key.len(), "sk-ant-jdd-".len() + 8);
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml antigravity_gateway_profile antigravity_api_key -- --nocapture
```

Expected: FAIL because the Antigravity API-service command module and key generator do not exist yet.

**Step 3: Write minimal implementation**

Create the Antigravity API-service profile models, key generator, and in-memory helper functions needed to satisfy the tests.

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml antigravity_gateway_profile antigravity_api_key -- --nocapture
```

Expected: PASS for Antigravity profile creation and readable key generation.

**Step 5: Commit**

```bash
git add src-tauri/src/platforms/antigravity/api_service/models.rs src-tauri/src/platforms/antigravity/api_service/team_profiles.rs src-tauri/src/platforms/antigravity/api_service/commands.rs src-tauri/src/platforms/antigravity/mod.rs
git commit -m "feat(antigravity): add gateway profile commands and readable keys"
```

### Task 3: Add Failing Tests For Antigravity Sidecar Runtime Artifacts

**Files:**
- Create: `src-tauri/src/platforms/antigravity/sidecar.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Write the failing test**

Add tests that prove:

- `AntigravitySidecar::new` uses Antigravity-specific runtime paths
- config generation writes the expected localhost host, dynamic port, auth dir, and internal key
- `sync_accounts` writes Antigravity auth files without reusing Augment runtime paths
- stale runtime metadata cleanup preserves the manager instance

Example target:

```rust
#[test]
fn antigravity_sidecar_uses_dedicated_runtime_paths() {
    let dir = tempdir().unwrap();
    let sidecar = AntigravitySidecar::new(dir.path(), PathBuf::from("/tmp/cliproxy-server"));

    assert!(sidecar.config_path().ends_with("cliproxy_antigravity_config.yaml"));
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml antigravity_sidecar -- --nocapture
```

Expected: FAIL because the Antigravity sidecar manager does not exist.

**Step 3: Write minimal implementation**

Create `sidecar.rs` by reusing the Augment sidecar pattern with Antigravity-specific runtime directories, config, auth serialization, and lifecycle helpers.

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml antigravity_sidecar -- --nocapture
```

Expected: PASS for runtime-path, config, and auth-sync behavior.

**Step 5: Commit**

```bash
git add src-tauri/src/platforms/antigravity/sidecar.rs src-tauri/src/lib.rs
git commit -m "feat(antigravity): add dedicated sidecar manager"
```

### Task 4: Add Failing Tests For Unified `/v1` Antigravity Proxying

**Files:**
- Create: `src-tauri/src/platforms/antigravity/api_service/server.rs`
- Modify: `src-tauri/src/core/api_server.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Write the failing test**

Add integration-style tests that prove:

- `GET /v1/models` routes to the Antigravity handler when the API key target is Antigravity
- `POST /v1/responses` forwards request bodies to the sidecar
- `POST /v1/chat/completions` forwards request bodies to the sidecar
- missing or empty Antigravity account pools return a clear error

Example target:

```rust
#[tokio::test]
async fn unified_gateway_routes_antigravity_models_request() {
    // build test app state with one Antigravity gateway profile
    // assert that /v1/models dispatches into the Antigravity handler
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml unified_gateway_routes_antigravity antigravity_proxy -- --nocapture
```

Expected: FAIL because there is no Antigravity unified gateway handler yet.

**Step 3: Write minimal implementation**

Add the Antigravity unified gateway handler, wire it into `api_server.rs`, and store the sidecar manager in `AppState`.

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml unified_gateway_routes_antigravity antigravity_proxy -- --nocapture
```

Expected: PASS for unified dispatch and no-account error handling.

**Step 5: Commit**

```bash
git add src-tauri/src/platforms/antigravity/api_service/server.rs src-tauri/src/core/api_server.rs src-tauri/src/lib.rs
git commit -m "feat(antigravity): route unified gateway traffic to sidecar"
```

### Task 5: Add Failing Tests For Conservative Usage Logging And Daily Stats

**Files:**
- Create: `src-tauri/src/platforms/antigravity/api_service/logger.rs`
- Modify: `src-tauri/src/platforms/antigravity/api_service/models.rs`
- Modify: `src-tauri/src/platforms/antigravity/api_service/server.rs`

**Step 1: Write the failing test**

Add tests that prove:

- Antigravity request logs persist `member_code`, `role_title`, `color`, and `api_key_suffix`
- daily stats are grouped by Antigravity gateway profile
- when no reliable `usage` object exists, token counters are recorded as `0`
- streaming extraction only finalizes usage from the completed event

Example target:

```rust
#[test]
fn antigravity_usage_defaults_to_zero_when_usage_missing() {
    let usage = extract_usage_from_json_bytes(&Bytes::from_static(br#"{"id":"resp_1"}"#));

    assert_eq!(usage.total_tokens, 0);
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml antigravity_usage antigravity_daily_stats -- --nocapture
```

Expected: FAIL because Antigravity logging and usage aggregation do not exist yet.

**Step 3: Write minimal implementation**

Implement the Antigravity logger, `usage` extractors, and daily-series aggregation with the same data shape used by the Codex dialog.

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml antigravity_usage antigravity_daily_stats -- --nocapture
```

Expected: PASS for conservative usage accounting and member-aware aggregation.

**Step 5: Commit**

```bash
git add src-tauri/src/platforms/antigravity/api_service/logger.rs src-tauri/src/platforms/antigravity/api_service/models.rs src-tauri/src/platforms/antigravity/api_service/server.rs
git commit -m "feat(antigravity): add member-aware logs and stats"
```

### Task 6: Add Tauri Commands For Antigravity Service Status, Access Config, And Member Management

**Files:**
- Modify: `src-tauri/src/platforms/antigravity/api_service/commands.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Write the failing test**

Add tests that prove:

- the service can return local and public base URLs
- the service can list, create, update, regenerate, and delete Antigravity gateway profiles
- bulk export returns the correct `OPENAI_BASE_URL` and member-specific key entries
- existing Antigravity account-management commands remain untouched

Example target:

```rust
#[test]
fn antigravity_access_config_uses_shared_v1_base_url() {
    let cfg = build_antigravity_access_config("http://127.0.0.1:8766/v1".into(), "sk-ant-jdd-12345678".into());

    assert_eq!(cfg.base_url, "http://127.0.0.1:8766/v1");
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml antigravity_access_config antigravity_gateway_commands -- --nocapture
```

Expected: FAIL because the Antigravity service commands are not exposed through Tauri yet.

**Step 3: Write minimal implementation**

Finish the Tauri commands, register them in `generate_handler!`, and keep the command names clearly scoped under Antigravity API service.

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml antigravity_access_config antigravity_gateway_commands -- --nocapture
```

Expected: PASS for access config, profile CRUD, and export helpers.

**Step 5: Commit**

```bash
git add src-tauri/src/platforms/antigravity/api_service/commands.rs src-tauri/src/lib.rs
git commit -m "feat(antigravity): expose API service commands"
```

### Task 7: Add Failing Frontend Tests Or Focused UI Regression Notes For The Antigravity Service Entry

**Files:**
- Modify: `src/components/platform/AntigravityAccountManager.vue`
- Create: `src/components/antigravity/AntigravityServerDialog.vue`
- Modify: `src/locales/zh-CN.js`
- Modify: `src/locales/en-US.js`

**Step 1: Write the failing test**

If the repository has suitable frontend test coverage, add focused tests that prove:

- Antigravity account manager shows a service-entry button
- clicking the button opens `AntigravityServerDialog`
- the dialog defaults to the overview tab

If adding automated frontend tests is too heavy for the current setup, write a precise manual regression checklist in the commit and implement against that checklist.

**Step 2: Run test to verify it fails**

Run the smallest available UI test command or record the manual regression gap before implementation.

Expected: the Antigravity service entry does not exist yet.

**Step 3: Write minimal implementation**

Add the service-entry button to `AntigravityAccountManager.vue`, create the new dialog component, and wire its open and close state.

**Step 4: Run test to verify it passes**

Run the chosen frontend verification command or execute the manual regression checklist:

- open Antigravity page
- click the new service button
- confirm the dialog opens and closes correctly

Expected: PASS for dialog entry behavior.

**Step 5: Commit**

```bash
git add src/components/platform/AntigravityAccountManager.vue src/components/antigravity/AntigravityServerDialog.vue src/locales/zh-CN.js src/locales/en-US.js
git commit -m "feat(antigravity): add API service dialog entry"
```

### Task 8: Build The Antigravity Service Dialog By Reusing The Codex Service Shape

**Files:**
- Create: `src/components/antigravity/AntigravityServerDialog.vue`
- Modify: `src/utils/codexTeamUi.js`
- Optionally create: `src/utils/teamGatewayUi.js`

**Step 1: Write the failing test**

Add the smallest possible test or verifiable harness for:

- member table rows
- default all-member selection
- label simplification to `member_code`
- token-share pie chart data
- export-all-access bundle

If no automated harness exists, document the manual acceptance list before implementation.

**Step 2: Run test to verify it fails**

Run the chosen verification path.

Expected: the Antigravity dialog either does not exist or does not render the required team-management sections.

**Step 3: Write minimal implementation**

Build the dialog by reusing the Codex layout and shared chart helpers where practical:

- overview tab
- logs tab
- member table
- monthly trend chart
- token-share pie chart
- export-all-members access block

Keep labels concise by preferring `member_code` such as `jdd`, `jqw`, and `cr`.

**Step 4: Run test to verify it passes**

Verify:

- all members are selected by default
- zero-usage dates render as `0`, not empty
- removing a member removes that series from the chart
- pie chart updates with the visible data

Expected: PASS for the required dialog behavior.

**Step 5: Commit**

```bash
git add src/components/antigravity/AntigravityServerDialog.vue src/utils/codexTeamUi.js src/utils/teamGatewayUi.js
git commit -m "feat(antigravity): add codex-like service dashboard"
```

### Task 9: Run End-To-End Verification For Local Gateway And Public Relay

**Files:**
- Verify: `src-tauri/src/core/api_server.rs`
- Verify: `src-tauri/src/platforms/antigravity/api_service/*`
- Verify: `src/components/antigravity/AntigravityServerDialog.vue`

**Step 1: Build the sidecar**

Run:

```bash
make cliproxy
```

Expected: `src-tauri/resources/cliproxy-server` is rebuilt successfully from `sidecars/cliproxy/apps/server-go`.

**Step 2: Run backend tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml antigravity -- --nocapture
```

Expected: PASS for the Antigravity gateway, sidecar, and logging test coverage added above.

**Step 3: Start the app**

Run:

```bash
make dev
```

Expected: ATM starts normally, reusing an existing Vite dev server if present.

**Step 4: Smoke test the local gateway**

Run a local request with an Antigravity key:

```bash
curl http://127.0.0.1:8766/v1/models \
  -H "Authorization: Bearer <antigravity-member-key>"
```

Expected: a valid OpenAI-style model list is returned from the Antigravity target.

**Step 5: Smoke test a generation path**

Run:

```bash
curl http://127.0.0.1:8766/v1/chat/completions \
  -H "Authorization: Bearer <antigravity-member-key>" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4-5",
    "messages": [{"role": "user", "content": "hello"}]
  }'
```

Expected: a valid OpenAI-style response is returned and the Antigravity service dialog shows the request in logs and charts.

**Step 6: Smoke test the public relay if available**

Run:

```bash
curl https://lingkong.xyz/v1/models \
  -H "Authorization: Bearer <antigravity-member-key>"
```

Expected: the public relay reaches the same Antigravity target without exposing a new external path.

**Step 7: Commit**

```bash
git add src-tauri/src/core/api_server.rs src-tauri/src/platforms/antigravity src/components/antigravity src/components/platform/AntigravityAccountManager.vue src/utils/codexTeamUi.js src/locales/zh-CN.js src/locales/en-US.js
git commit -m "feat(antigravity): ship unified API service integration"
```
