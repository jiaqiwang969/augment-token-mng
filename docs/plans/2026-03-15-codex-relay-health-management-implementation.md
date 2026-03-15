# Codex Relay Health Management Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add backend-managed health monitoring, bounded auto-repair, and always-visible UI status for the Codex local route and the public relay.

**Architecture:** Extend the existing Codex server config with relay settings and add a new relay health module on the Tauri side. The backend owns probing, cooldowns, repair sequencing, and event emission; the front end reads one shared snapshot for the global status chip and the detailed Codex dialog panel. Keep deployment shell tooling, but make it fail on unhealthy HTTP responses.

**Tech Stack:** Rust, Tauri 2, tokio, reqwest, Vue 3, vue-i18n, existing Codex command/event plumbing, shell relay scripts

---

### Task 1: Add Relay Config And Health Snapshot Models

**Files:**
- Modify: `src-tauri/src/platforms/openai/codex/pool.rs`
- Modify: `src-tauri/src/platforms/openai/codex/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/platforms/openai/codex/commands.rs`

**Step 1: Write the failing tests**

Add Rust tests for:

- default relay config values normalize to a 10-minute interval/cooldown
- missing relay fields deserialize cleanly from older persisted `CodexServerConfig`
- relay snapshot aggregate state maps to local-down/public-down/healthy/in-progress correctly

Use concrete assertions, for example:

```rust
assert_eq!(config.relay.health_check_interval_seconds, 600);
assert_eq!(config.relay.auto_repair_cooldown_seconds, 600);
assert_eq!(status.overall.state, "public_down");
```

**Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_relay_health_config -- --nocapture
```

Expected: FAIL because the relay config and health snapshot structs do not exist yet.

**Step 3: Add the new config/status types**

Implement:

- optional relay settings on `CodexServerConfig`
- per-layer health state struct
- aggregate relay snapshot struct

Keep serde defaults backward-compatible so existing `codex_config.json` still loads.

**Step 4: Register any new shared state placeholders**

Add the new relay health state/task handle fields to `AppState` in `src-tauri/src/lib.rs`.

**Step 5: Re-run the focused tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_relay_health_config -- --nocapture
```

Expected: PASS

### Task 2: Implement Local/Public Probes And Snapshot Refresh

**Files:**
- Create: `src-tauri/src/platforms/openai/codex/relay.rs`
- Modify: `src-tauri/src/platforms/openai/codex/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/platforms/openai/codex/relay.rs`

**Step 1: Write the failing tests**

Add unit tests in `relay.rs` for:

- `2xx` + non-empty model list => healthy
- `401`, `502`, timeout, invalid JSON, or empty model list => unhealthy
- local/public aggregate becomes yellow only when local is healthy and public is unhealthy

Example fixture:

```rust
let body = br#"{"data":[{"id":"gpt-5","object":"model"}],"object":"list"}"#;
assert!(models_payload_is_ready(body));
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_relay_probe -- --nocapture
```

Expected: FAIL because the relay probe helpers do not exist yet.

**Step 3: Implement probe helpers**

Implement in `relay.rs`:

- local probe using the current local Codex base URL and gateway API key
- public probe using the configured public URL and the same gateway API key
- short timeout
- readiness rule: `2xx` plus non-empty `data`

Factor shared HTTP handling so both layers use identical success criteria.

**Step 4: Implement snapshot refresh**

Add a helper that:

- reads current config and usable API key
- probes local first
- probes public only after local succeeds
- updates the in-memory snapshot
- emits a Tauri event when the snapshot changes

**Step 5: Re-run the focused tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_relay_probe -- --nocapture
```

Expected: PASS

### Task 3: Implement Bounded Repair Flow And Periodic Task

**Files:**
- Modify: `src-tauri/src/platforms/openai/codex/commands.rs`
- Modify: `src-tauri/src/platforms/openai/codex/relay.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/platforms/openai/codex/relay.rs`

**Step 1: Write the failing tests**

Add tests for:

- repair sequence always attempts local before public
- auto-repair respects the 10-minute cooldown
- manual repair bypasses cooldown but still prevents concurrent runs

Example assertion shape:

```rust
assert_eq!(attempts, vec!["repair_local", "probe_local", "repair_public", "probe_public"]);
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_relay_repair -- --nocapture
```

Expected: FAIL because repair orchestration and cooldown tracking do not exist yet.

**Step 3: Factor local repair into an internal helper**

Extract Codex server start/restart logic from command handlers so the background repair path can:

- start the server if stopped
- restart it if marked running but probe says unhealthy

Do not shell out for this path.

**Step 4: Implement public relay repair**

Implement a Rust helper mirroring `ssh -O check` and `ssh -fN -M -S ... -R ...` semantics from `scripts/start_remote_relay.sh`.

The helper should:

- derive or read control socket path
- short-circuit if tunnel is already alive
- remove stale control socket before reconnect
- return structured errors when SSH is missing or auth fails

**Step 5: Add the periodic task**

Use the same pattern as periodic quota refresh:

- start when Codex route/config is enabled
- run every configured interval
- abort/restart cleanly when settings change

**Step 6: Re-run the focused tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_relay_repair -- --nocapture
```

Expected: PASS

### Task 4: Expose Commands And Tighten Scripted Relay Checks

**Files:**
- Modify: `src-tauri/src/platforms/openai/codex/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `scripts/check_remote_relay.sh`
- Test: `tests/relayConfig.test.js`

**Step 1: Write the failing tests**

Add coverage for:

- new invoke commands being registered and serializing relay status cleanly
- shell relay health check failing on non-2xx instead of printing and succeeding

If shell test coverage is awkward, add a small JS test around helper extraction or document the shell behavior with a focused smoke command.

**Step 2: Run test to verify it fails**

Run:

```bash
npm test -- relayConfig
```

Expected: FAIL because the relay health-check behavior still accepts unhealthy HTTP responses.

**Step 3: Add new commands**

Expose Tauri commands for:

- `get_codex_relay_health_status`
- `refresh_codex_relay_health_status`
- `repair_codex_relay_health`

Keep the payload shape stable and reuse the same snapshot struct as the background task.

**Step 4: Make `check_remote_relay.sh` strict**

Update the script so that:

- `401` and `502` fail the script
- authenticated success requires `2xx`
- output still shows enough diagnostics for an operator

**Step 5: Re-run focused verification**

Run:

```bash
npm test -- relayConfig
cargo test --manifest-path src-tauri/Cargo.toml codex_relay -- --nocapture
```

Expected: PASS

### Task 5: Add Global Status Chip And Detailed Codex Dialog Panel

**Files:**
- Create: `src/components/openai/CodexRelayStatusChip.vue`
- Modify: `src/App.vue`
- Modify: `src/components/openai/CodexServerDialog.vue`
- Modify: `src/locales/en-US.js`
- Modify: `src/locales/zh-CN.js`
- Test: `tests/codexServerDialog.test.js`

**Step 1: Write the failing tests**

Add front-end tests covering:

- global chip renders green/yellow/red/in-progress states
- Codex dialog shows local/public rows, timestamps, errors, and repair state
- “Refresh now” and “Repair now” invoke the new backend commands and disable correctly while busy

**Step 2: Run test to verify it fails**

Run:

```bash
npm test -- codexServerDialog
```

Expected: FAIL because the global chip and relay detail panel do not exist yet.

**Step 3: Build the reusable chip**

Create a compact component that accepts the backend snapshot and renders:

- badge color
- short label
- last-known error tooltip or short hint

**Step 4: Mount the chip globally**

Place the chip in `src/App.vue` sidebar/bottom controls so it remains visible outside the Codex modal.

**Step 5: Extend `CodexServerDialog.vue`**

Add a new relay health section near the local/public URL area with:

- local/public status badges
- last checked/success timestamps
- last error text
- auto-repair cooldown text
- manual refresh button
- manual repair button

Use backend events for live updates instead of adding another fast UI polling loop.

**Step 6: Add locale strings**

Add the new labels, short status text, and repair/result messages in both locale files.

**Step 7: Re-run focused UI tests**

Run:

```bash
npm test -- codexServerDialog
```

Expected: PASS

### Task 6: Run End-To-End Verification

**Files:**
- No code changes

**Step 1: Run Rust verification**

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_relay -- --nocapture
```

Expected: PASS

**Step 2: Run targeted front-end verification**

```bash
npm test -- codexServerDialog relayConfig
```

Expected: PASS

**Step 3: Run manual local/public smoke checks**

Verify:

- healthy steady state shows green
- stopping local Codex route drives red then recovers after repair
- dropping the SSH tunnel drives yellow then recovers after relay repair
- repair cooldown blocks repeated automatic attempts for 10 minutes

**Step 4: Commit**

```bash
git add src-tauri/src/platforms/openai/codex/pool.rs \
  src-tauri/src/platforms/openai/codex/commands.rs \
  src-tauri/src/platforms/openai/codex/relay.rs \
  src-tauri/src/lib.rs \
  scripts/check_remote_relay.sh \
  src/components/openai/CodexRelayStatusChip.vue \
  src/components/openai/CodexServerDialog.vue \
  src/App.vue \
  src/locales/en-US.js \
  src/locales/zh-CN.js \
  tests/codexServerDialog.test.js \
  tests/relayConfig.test.js
git commit -m "feat: add codex relay health management"
```
