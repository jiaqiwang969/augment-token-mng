# ATM Augment Sidecar Integration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Bundle CLIProxyAPI into ATM as an internal sidecar so Codex CLI can call `http://127.0.0.1:8766/augment` without manually starting a second service.

**Architecture:** ATM remains the control plane that selects Augment accounts, syncs auth files, and proxies `/augment/v1/*` requests. CLIProxyAPI remains the black-box translator that exposes the OpenAI-compatible API surface and talks to Augment upstream.

**Tech Stack:** Rust, Tauri 2, tokio, warp, reqwest, serde_json, build-generated Go sidecar (`cliproxy-server`)

---

### Task 1: Lock The CLIProxyAPI Contract In Tests

**Files:**
- Modify: `src-tauri/src/platforms/augment/sidecar.rs`
- Test: `src-tauri/src/platforms/augment/sidecar.rs`
- Reference: `docs/plans/2026-03-13-atm-augment-sidecar-design.md`

**Step 1: Write the failing test**

Add unit tests that assert:

- the generated config uses `routing.strategy`, not `routing-strategy`
- the generated config includes `host`, `port`, `auth-dir`, `api-keys`, `debug`, and `request-log`
- the generated auth JSON includes `type`, `label`, `access_token`, `tenant_url`, `client_id`, `login_mode`, and `last_refresh`
- the generated filename follows `auggie-<tenant>.json`

Example skeleton:

```rust
#[test]
fn sidecar_config_yaml_matches_cliproxy_contract() {
    let yaml = build_config_yaml(43123, "/tmp/auth", "sk-test");
    assert!(yaml.contains("routing:"));
    assert!(yaml.contains("strategy: round-robin"));
    assert!(!yaml.contains("routing-strategy:"));
}

#[test]
fn sidecar_auth_json_matches_auggie_metadata_contract() {
    let token = sample_token("https://tenant.augmentcode.com/", "token-1");
    let (name, raw) = build_auth_file(&token);
    assert_eq!(name, "auggie-tenant-augmentcode-com.json");
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(json["type"], "auggie");
    assert_eq!(json["tenant_url"], "https://tenant.augmentcode.com/");
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml sidecar_config_yaml_matches_cliproxy_contract -- --nocapture
```

Expected: FAIL because the current implementation still uses the wrong routing key shape or lacks extracted helpers for stable testing.

**Step 3: Write minimal implementation**

Refactor `sidecar.rs` to extract pure helpers such as:

- `build_config_yaml(...) -> String`
- `build_auth_file(token: &TokenData) -> Result<(String, String), String>`
- `auth_filename_for_tenant(...) -> String`

Then make `write_config()` and `sync_accounts()` call those helpers.

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml sidecar_ -- --nocapture
```

Expected: PASS for the new sidecar contract tests.

**Step 5: Commit**

```bash
git add src-tauri/src/platforms/augment/sidecar.rs
git commit -m "test(augment): lock sidecar contract"
```

### Task 2: Make Sidecar Startup Async-Safe

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/platforms/augment/proxy_server.rs`
- Modify: `src-tauri/src/platforms/augment/sidecar.rs`
- Test: `src-tauri/src/platforms/augment/proxy_server.rs`

**Step 1: Write the failing test**

Add pure helper tests around the proxy startup path so the request handler behavior is testable without launching the whole Tauri app.

At minimum, cover:

- `/augment/v1/responses` becomes `/v1/responses`
- raw query strings are preserved
- inbound `Authorization` is dropped and replaced later

Example skeleton:

```rust
#[test]
fn inner_path_strips_augment_prefix() {
    assert_eq!(inner_path("/augment/v1/responses"), "/v1/responses");
}

#[test]
fn upstream_url_preserves_raw_query() {
    let url = build_upstream_url("http://127.0.0.1:9000", "/v1/models", Some("a=1&b=2"));
    assert_eq!(url, "http://127.0.0.1:9000/v1/models?a=1&b=2");
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml inner_path_strips_augment_prefix -- --nocapture
```

Expected: FAIL because the current helper functions do not exist yet.

**Step 3: Write minimal implementation**

Refactor the proxy handler to:

- extract pure path and URL helpers
- replace the blocking start path with an async-safe pattern
- call `ensure_running()` directly from the request path
- use an async-aware mutex for `augment_sidecar` if needed to avoid holding `std::sync::Mutex` across `.await`

Do not keep the final implementation on `spawn_blocking + block_on`.

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml inner_path_ -- --nocapture
```

Expected: PASS for the new proxy helper tests.

**Step 5: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/platforms/augment/proxy_server.rs src-tauri/src/platforms/augment/sidecar.rs
git commit -m "refactor(augment): make sidecar startup async-safe"
```

### Task 3: Define Minimal First-Release Account Filtering

**Files:**
- Modify: `src-tauri/src/platforms/augment/proxy_server.rs`
- Test: `src-tauri/src/platforms/augment/proxy_server.rs`
- Reference: `src-tauri/src/data/storage/augment/traits.rs`

**Step 1: Write the failing test**

Add tests for a pure `is_token_usable(...)` helper that encode the first-release rules:

- reject missing `access_token`
- reject missing `tenant_url`
- reject known banned states like `SUSPENDED` and `INVALID_TOKEN`
- keep the implementation intentionally minimal unless the repo already has stable expiry/quota fields

Example skeleton:

```rust
#[test]
fn token_without_access_token_is_not_usable() {
    let token = sample_token("", "https://tenant.augmentcode.com/");
    assert!(!is_token_usable(&token));
}

#[test]
fn suspended_token_is_not_usable() {
    let token = sample_banned_token("SUSPENDED");
    assert!(!is_token_usable(&token));
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml token_without_access_token_is_not_usable -- --nocapture
```

Expected: FAIL because the helper is not extracted yet.

**Step 3: Write minimal implementation**

Extract `is_token_usable()` and make `get_available_tokens()` delegate to it.

If an expiry or quota field is already clearly present and trustworthy in the Rust model, add it now. If not, stop at the minimal reliable rules and document richer filtering as follow-up work.

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml suspended_token_is_not_usable -- --nocapture
```

Expected: PASS for the new filtering tests.

**Step 5: Commit**

```bash
git add src-tauri/src/platforms/augment/proxy_server.rs
git commit -m "test(augment): define minimal token usability rules"
```

### Task 4: Finish Wiring The ATM Server Surface

**Files:**
- Modify: `src-tauri/src/platforms/augment.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/core/api_server.rs`
- Modify: `src-tauri/src/platforms/augment/proxy_server.rs`
- Test: `src-tauri/src/platforms/augment/proxy_server.rs`

**Step 1: Write the failing test**

Add a narrow route test or helper-level test that proves:

- `/augment/v1/models`
- `/augment/v1/responses`
- `/augment/v1/chat/completions`

are the supported ATM entrypoints for the sidecar integration.

Example skeleton:

```rust
#[test]
fn supported_augment_paths_are_recognized() {
    assert!(is_supported_augment_path("/augment/v1/models"));
    assert!(is_supported_augment_path("/augment/v1/responses"));
    assert!(is_supported_augment_path("/augment/v1/chat/completions"));
    assert!(!is_supported_augment_path("/augment/v0/management"));
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml supported_augment_paths_are_recognized -- --nocapture
```

Expected: FAIL because the helper does not exist yet.

**Step 3: Write minimal implementation**

Complete the server wiring so:

- `AppState` exposes the sidecar handle cleanly
- `augment.rs` exports the new modules
- `api_server.rs` mounts Augment routes and maps Augment-specific rejections
- the proxy handler rejects unsupported `/augment/*` paths with normal local API errors instead of silently forwarding arbitrary paths

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml supported_augment_paths_are_recognized -- --nocapture
```

Expected: PASS.

**Step 5: Commit**

```bash
git add src-tauri/src/platforms/augment.rs src-tauri/src/lib.rs src-tauri/src/core/api_server.rs src-tauri/src/platforms/augment/proxy_server.rs
git commit -m "feat(augment): wire ATM proxy routes to sidecar"
```

### Task 5: Bundle The Sidecar And Verify End-To-End Smoke Paths

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Verify: `src-tauri/resources/cliproxy-server` (generated artifact)
- Verify: `src-tauri/src/platforms/augment/sidecar.rs`
- Verify: `docs/plans/2026-03-13-atm-augment-sidecar-design.md`

**Step 1: Write the failing test**

Add a lightweight config assertion test or manual verification checklist item that proves the bundle resource is declared.

Because `tauri.conf.json` is configuration, treat this as a characterization check rather than an elaborate unit test.

Example check:

```bash
rg -n '"resources"' src-tauri/tauri.conf.json
rg -n 'cliproxy-server' src-tauri/tauri.conf.json
```

**Step 2: Run test to verify it fails**

Run:

```bash
rg -n 'cliproxy-server' src-tauri/tauri.conf.json
```

Expected: no match before the bundle resource entry is added.

**Step 3: Write minimal implementation**

Update `src-tauri/tauri.conf.json` so the bundle includes:

```json
"resources": ["resources/cliproxy-server"]
```

Then confirm the runtime binary lookup path in `sidecar.rs` still works in both dev mode and bundled mode.

**Step 4: Run test to verify it passes**

Run:

```bash
rg -n 'cliproxy-server' src-tauri/tauri.conf.json
cargo test --manifest-path src-tauri/Cargo.toml sidecar_ -- --nocapture
cargo build --manifest-path src-tauri/Cargo.toml
```

Expected:

- `cliproxy-server` appears in `tauri.conf.json`
- sidecar tests pass
- Rust build succeeds

Then run the manual smoke checks:

```bash
curl -sS http://127.0.0.1:8766/augment/v1/models
curl -sS http://127.0.0.1:8766/augment/v1/responses -H 'Content-Type: application/json' -d '{"model":"gpt-5.4","input":"hi"}'
curl -sS http://127.0.0.1:8766/augment/v1/chat/completions -H 'Content-Type: application/json' -d '{"model":"gpt-5.4","messages":[{"role":"user","content":"hi"}]}'
```

Expected:

- ATM lazily starts the sidecar
- requests succeed without manually starting port `8317`
- SSE and non-streaming responses are forwarded correctly

**Step 5: Commit**

```bash
git add src-tauri/tauri.conf.json
git commit -m "build(augment): bundle cliproxy sidecar"
```

### Task 6: Final Verification And Cleanup

**Files:**
- Verify: `src-tauri/src/platforms/augment/sidecar.rs`
- Verify: `src-tauri/src/platforms/augment/proxy_server.rs`
- Verify: `src-tauri/src/core/api_server.rs`
- Verify: `src-tauri/src/lib.rs`
- Verify: `src-tauri/tauri.conf.json`

**Step 1: Write the failing test**

Create a final verification checklist before claiming the feature is done:

- unit tests for sidecar helpers
- unit tests for proxy helpers
- `cargo build`
- manual `/augment/v1/*` smoke checks
- sidecar restart-on-demand smoke check

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: at least one failure until the remaining implementation work is complete.

**Step 3: Write minimal implementation**

Finish any remaining mismatches found by tests or smoke checks, then rerun the full verification set.

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml
cargo build --manifest-path src-tauri/Cargo.toml
```

Expected: PASS for tests and a successful build.

**Step 5: Commit**

```bash
git add -u src-tauri
git commit -m "feat(augment): integrate cliproxy sidecar into ATM"
```
