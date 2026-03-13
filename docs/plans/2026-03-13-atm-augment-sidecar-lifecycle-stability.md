# ATM Augment Sidecar Lifecycle Stability Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Close the remaining sidecar lifecycle gaps so ATM cleans up `cliproxy-server` on shutdown and reclaims stale managed sidecars left behind by previous crashes.

**Architecture:** Keep CLIProxyAPI as a black-box translator. Extend the Rust sidecar manager with runtime metadata and cleanup helpers, then wire those helpers into both the API server stop path and the Tauri app exit path. Reclaim only ATM-managed sidecars by verifying persisted runtime metadata against the live process before killing anything.

**Tech Stack:** Rust, Tauri 2, tokio, sysinfo, serde_json, bundled Go binary (`cliproxy-server`)

---

### Task 1: Add A Blocking Sidecar Shutdown Path For App Exit

**Files:**
- Modify: `src-tauri/src/platforms/augment/sidecar.rs`
- Modify: `src-tauri/src/core/api_server.rs`
- Test: `src-tauri/src/platforms/augment/sidecar.rs`
- Test: `src-tauri/src/core/api_server.rs`

**Step 1: Write the failing tests**

Add tests that assert:

- a synchronous sidecar shutdown helper clears runtime artifacts and child state without async waiting
- a blocking manager shutdown helper keeps the sidecar manager registered but stops its runtime state

Example skeleton:

```rust
#[test]
fn sidecar_force_stop_clears_runtime_files() {
    let mut sidecar = sample_running_sidecar();
    sidecar.force_stop();
    assert!(!sidecar.is_running());
}

#[test]
fn stop_managed_augment_sidecar_blocking_keeps_manager_registered() {
    let managed = tokio::sync::Mutex::new(Some(sample_sidecar()));
    stop_managed_augment_sidecar_blocking(&managed);
    assert!(managed.blocking_lock().as_ref().is_some());
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml stop_managed_augment_sidecar_blocking_keeps_manager_registered -- --nocapture
```

Expected: FAIL because the blocking cleanup path does not exist yet.

**Step 3: Write minimal implementation**

- extract a synchronous `force_stop()` helper on `AugmentSidecar`
- make `Drop` reuse the same cleanup path
- add `stop_managed_augment_sidecar_blocking(...)`
- keep the existing async stop path for command handlers and tests

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml stop_managed_augment_sidecar_ -- --nocapture
```

Expected: PASS for the new blocking cleanup tests.

**Step 5: Commit**

```bash
git add src-tauri/src/platforms/augment/sidecar.rs src-tauri/src/core/api_server.rs
git commit -m "fix(augment): add blocking sidecar shutdown path"
```

### Task 2: Persist Managed Sidecar Runtime Metadata And Reap Stale Orphans

**Files:**
- Modify: `src-tauri/src/platforms/augment/sidecar.rs`
- Test: `src-tauri/src/platforms/augment/sidecar.rs`
- Reference: `src-tauri/Cargo.toml`

**Step 1: Write the failing tests**

Add tests for pure helpers that assert:

- runtime metadata can be serialized with pid/config/binary identity
- a live process only matches stale-sidecar cleanup criteria when both the binary identity and `-config` path match
- unrelated processes are never treated as ATM-managed sidecars

Example skeleton:

```rust
#[test]
fn stale_sidecar_process_requires_matching_config_path() {
    assert!(!process_matches_sidecar_metadata(
        "cliproxy-server",
        Some("/tmp/cliproxy-server"),
        &["cliproxy-server", "-config", "/tmp/other.yaml"],
        Path::new("/tmp/cliproxy-server"),
        Path::new("/tmp/cliproxy_config.yaml"),
    ));
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml stale_sidecar_process_requires_matching_config_path -- --nocapture
```

Expected: FAIL because the metadata helpers and match logic do not exist yet.

**Step 3: Write minimal implementation**

- add a runtime metadata file under the sidecar app-data area
- persist metadata after successful spawn
- remove metadata on stop
- before starting a new sidecar, inspect persisted metadata and kill only a verified stale managed process
- use `sysinfo` for cross-platform process lookup and kill

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml stale_sidecar_ -- --nocapture
```

Expected: PASS for the new stale-process identification tests.

**Step 5: Commit**

```bash
git add src-tauri/src/platforms/augment/sidecar.rs
git commit -m "fix(augment): reap stale managed sidecars"
```

### Task 3: Wire Cleanup Into The Tauri App Exit Path

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/core/api_server.rs`
- Test: `src-tauri/src/core/api_server.rs`
- Reference: `/Users/jqwang/14-新闻-爬虫/worldmonitor/src-tauri/src/main.rs`

**Step 1: Write the failing test**

Add a focused helper-level test that proves the new blocking cleanup entrypoint is callable from non-async shutdown code and leaves the manager reusable for the next start.

Example skeleton:

```rust
#[test]
fn blocking_shutdown_leaves_sidecar_manager_reusable() {
    let managed = tokio::sync::Mutex::new(Some(sample_sidecar()));
    stop_managed_augment_sidecar_blocking(&managed);
    assert!(managed.blocking_lock().as_ref().unwrap().api_key().starts_with("sk-atm-internal-"));
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml blocking_shutdown_leaves_sidecar_manager_reusable -- --nocapture
```

Expected: FAIL because the blocking cleanup entrypoint is not wired for app-exit reuse yet.

**Step 3: Write minimal implementation**

- switch the Tauri bootstrap from bare `.run(generate_context!())` to `.build(...).run(...)`
- on `RunEvent::ExitRequested` and `RunEvent::Exit`, call the blocking sidecar cleanup helper
- keep existing tray close behavior unchanged

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml blocking_shutdown_leaves_sidecar_manager_reusable -- --nocapture
```

Expected: PASS for the new blocking shutdown test.

**Step 5: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/core/api_server.rs
git commit -m "fix(augment): stop sidecar on app exit"
```
