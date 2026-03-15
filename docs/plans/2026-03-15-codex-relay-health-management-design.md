# Codex Relay Health Management Design

**Goal:** Add always-visible health monitoring and bounded automatic repair for the Codex local route (`http://127.0.0.1:8766/v1`) and the public relay (`https://lingkong.xyz/v1`) without adding latency to user traffic.

**Context**

- The Codex local route is served by the ATM Tauri app on port `8766`.
- The public relay is currently `nginx -> 127.0.0.1:19090 -> ssh -R -> 127.0.0.1:8766`.
- The production incident on 2026-03-15 was not an OpenAI outage or nginx misconfiguration. The root cause was that the reverse SSH tunnel had dropped, so the server-side loopback port `19090` was no longer listening and nginx returned `502`.
- The current relay deploy/check tooling is not strict enough: [scripts/check_remote_relay.sh](/Users/jqwang/05-api-代理/augment-token-mng/scripts/check_remote_relay.sh) prints responses but does not fail on `401` or `502`, so a broken relay can look superficially “checked”.
- The current Codex UI already shows local/public URLs in [CodexServerDialog.vue](/Users/jqwang/05-api-代理/augment-token-mng/src/components/openai/CodexServerDialog.vue), but it does not surface relay health, repair state, or repair actions.

**Requirements**

- Show a clear Codex relay status globally in ATM, not only inside the Codex dialog.
- Keep detailed status and repair actions in the Codex dialog.
- Run health checks out of band, never on the live request forwarding path.
- Check local route first, then public relay.
- Support both bounded automatic repair and manual one-click repair.
- Limit automatic repair attempts to once per 10 minutes per layer.
- Re-verify health after every repair attempt; command success alone is not sufficient.
- Reuse existing authenticated `/v1/models` readiness semantics so a `401`, empty model list, or `502` all count as unhealthy.
- Continue working for current operator setups that already rely on `.env.relay`, while moving toward ATM-owned persisted config.

**Non-Goals**

- Do not move relay health logic into the per-request proxy path.
- Do not add high-frequency polling; 10-minute cadence is sufficient for the first version.
- Do not redesign the full Codex team dashboard.
- Do not require a separate daemon or external observability stack for the first version.

**Recommended Approach**

### 1. Introduce a backend relay health manager

Add a dedicated Codex relay health module on the Tauri side that owns:

- relay config resolution
- local/public probe helpers
- aggregate status snapshot
- repair orchestration
- a periodic background task

This should follow the same model as the existing periodic Codex quota refresh task in [commands.rs](/Users/jqwang/05-api-代理/augment-token-mng/src-tauri/src/platforms/openai/codex/commands.rs), but with a separate task handle and state snapshot.

### 2. Persist relay configuration in ATM, with `.env.relay` fallback

Extend `CodexServerConfig` with an optional relay block. The first version should support:

- `public_base_url`
- `host`
- `remote_port`
- `local_port`
- `control_socket`
- `health_check_interval_seconds`
- `auto_repair_enabled`
- `auto_repair_cooldown_seconds`

The runtime should load this from persisted app-data JSON first. If fields are absent, it may import compatible values from `.env.relay` as a fallback for existing developer/operator setups.

This gives three benefits:

- the app no longer depends only on repo-local shell state
- repair logic can run from Tauri directly
- the UI can show exactly which relay target it is checking

### 3. Define explicit two-layer health state

The backend should publish a snapshot with:

- `local`
- `public`
- `overall`

Each layer should include:

- `healthy`
- `last_checked_at`
- `last_success_at`
- `last_error`
- `repair_in_progress`
- `last_repair_attempt_at`
- `last_repair_result`
- `cooldown_until`

The aggregate state should map to four simple UI states:

- green: local and public healthy
- yellow: local healthy, public unhealthy
- red: local unhealthy
- blue: repair in progress

### 4. Reuse authenticated `/v1/models` probes

Probe logic should stay lightweight:

- local probe: `GET {local_base_url}/models`
- public probe: `GET {public_base_url}/models`
- both with a valid Codex gateway API key
- 2-second or similarly short timeout
- require `2xx` plus a non-empty `data` array

This matches the existing readiness logic in [sidecar.rs](/Users/jqwang/05-api-代理/augment-token-mng/src-tauri/src/platforms/augment/sidecar.rs#L527) and avoids inventing a second health definition.

### 5. Repair local first, public second

Repair order must be deterministic:

1. Probe local route
2. If local is unhealthy, repair local route
3. Re-probe local route
4. Only after local is healthy, probe public relay
5. If public is unhealthy, repair relay tunnel
6. Re-probe public relay

Local repair should reuse/factor the existing Codex server start logic rather than shelling out. If the server is stopped, start it. If it is running but unhealthy, restart it through internal helpers.

Public repair should use the same SSH control-socket pattern as [scripts/start_remote_relay.sh](/Users/jqwang/05-api-代理/augment-token-mng/scripts/start_remote_relay.sh), but implemented from Tauri so the app can perform one-click repair without depending on repo scripts. The backend can still keep the shell script for operator workflows and deployment.

### 6. Emit health updates to the UI

The backend should expose:

- a “get current snapshot” command
- a “manual refresh now” command
- a “manual repair now” command

It should also emit an event when the snapshot changes so the UI can stay updated without aggressive front-end polling.

### 7. Add a global status light plus dialog details

UI should have two surfaces:

- global compact chip in the app shell/sidebar so the user can always see the current state
- detailed Codex relay panel in `CodexServerDialog`

The global chip should show:

- color/state
- short label
- tooltip or compact text

The dialog should show:

- local/public status rows
- last checked time
- last success time
- current/public URLs
- last error
- last repair attempt/result
- “Refresh now” button
- “Repair now” button

### 8. Tighten deploy-time checks

Keep the shell relay tooling, but fix it so health checks fail fast when the relay is not actually healthy:

- non-2xx must fail
- unauthenticated `401` must fail
- successful response must still satisfy readiness expectations

This prevents future false positives during deployment.

**Why This Approach**

- It keeps all probing off the hot request path, so user traffic is not slowed down.
- It mirrors the real dependency order: local route first, public relay second.
- It gives the operator one authoritative status model instead of scattered scripts and manual curl checks.
- It is incremental: first version can keep existing relay infrastructure while making it observable and recoverable.
- It handles the exact incident that already occurred, rather than solving a hypothetical problem.

**Risks And Mitigations**

- SSH is not available or key auth fails.
  - Surface this as a visible repair failure, not a silent retry loop.
- Relay config is missing.
  - Show public relay status as “not configured” rather than “healthy”.
- Automatic repair loops.
  - Enforce cooldown per layer and prevent concurrent repair runs.
- UI drift between global chip and dialog.
  - Use the same backend snapshot/event source for both surfaces.

**Testing**

- Rust unit tests for config fallback, health classification, cooldown handling, and aggregate state transitions.
- Rust tests for repair command construction and “repair local before public” sequencing.
- Front-end tests for the new status chip and dialog rendering/action states.
- Manual smoke test:
  - local healthy + public healthy -> green
  - kill local route -> red, local repair path runs
  - drop relay tunnel -> yellow, public repair path runs
  - repair exhausted / SSH unavailable -> visible error state
