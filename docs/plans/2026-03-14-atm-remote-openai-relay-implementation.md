# ATM Remote OpenAI Relay Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Let ATM share its local OpenAI-compatible `/v1` gateway through `https://<public-domain>/v1`, while supporting multiple external access keys managed from ATM.

**Architecture:** Keep ATM local-only on `127.0.0.1:8766`, extend the gateway access-profile layer to support multiple keys per target, add a local reverse-tunnel manager that forwards traffic to Ubuntu, and deploy nginx on Ubuntu as the public HTTPS ingress that proxies only supported `/v1/*` routes back through the tunnel.

**Tech Stack:** Rust, Tauri 2 commands, warp, tokio, reqwest, serde/serde_json, Vue 3 `<script setup>`, nginx, OpenSSH reverse tunnels

---

### Task 1: Add The Approved Design And Plan Documents

**Files:**
- Create: `docs/plans/2026-03-14-atm-remote-openai-relay-design.md`
- Create: `docs/plans/2026-03-14-atm-remote-openai-relay-implementation.md`

**Step 1: Write the design doc**

Capture the approved architecture, scope, non-goals, security model, UI direction, and rollout order.

**Step 2: Write the implementation plan**

Break the work into small, testable tasks that cover backend, frontend, tunnel runtime, and Ubuntu deployment.

**Step 3: Commit**

```bash
git add docs/plans/2026-03-14-atm-remote-openai-relay-design.md docs/plans/2026-03-14-atm-remote-openai-relay-implementation.md
git commit -m "docs: add remote openai relay design and plan"
```

### Task 2: Add Failing Tests For Multi-Key Gateway Profile Behavior

**Files:**
- Modify: `src-tauri/src/core/gateway_access.rs`
- Modify: `src-tauri/src/core/api_server.rs`
- Modify: `src-tauri/src/platforms/openai/codex/commands.rs`

**Step 1: Write the failing tests**

Add focused tests for:

- resolving any enabled profile when multiple profiles share `target = codex`
- preserving multiple Codex profiles on save
- rejecting disabled profiles even if the key matches
- validating a Codex request against any enabled matching key

Example target:

```rust
#[test]
fn gateway_access_resolves_any_enabled_matching_key() {
    let profiles = GatewayAccessProfiles {
        profiles: vec![
            GatewayAccessProfile { id: "a".into(), name: "Alice".into(), target: GatewayTarget::Codex, api_key: "sk-a".into(), enabled: true },
            GatewayAccessProfile { id: "b".into(), name: "Bob".into(), target: GatewayTarget::Codex, api_key: "sk-b".into(), enabled: true },
        ],
    };

    assert_eq!(profiles.find_by_key("sk-b").unwrap().id, "b");
}
```

**Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml gateway_access codex_access -- --nocapture
```

Expected: FAIL because the command/config layer still assumes one key per target.

**Step 3: Commit**

```bash
git add src-tauri/src/core/gateway_access.rs src-tauri/src/core/api_server.rs src-tauri/src/platforms/openai/codex/commands.rs
git commit -m "test(gateway): cover multi-key access profiles"
```

### Task 3: Implement Multi-Key Gateway Profile CRUD For ATM

**Files:**
- Modify: `src-tauri/src/core/gateway_access.rs`
- Modify: `src-tauri/src/platforms/openai/codex/commands.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Implement generic helpers**

Add shared helper functions for:

- listing profiles by target
- creating a new profile with a generated `sk-...` key
- updating profile metadata or key
- enabling/disabling a profile
- deleting a profile by id

Keep the persisted shape as `gateway_access_profiles.json`.

**Step 2: Add Tauri commands for Codex/OpenAI key management**

Expose commands for the OpenAI panel to:

- list Codex gateway profiles
- create a Codex gateway profile
- update a Codex gateway profile
- delete a Codex gateway profile

**Step 3: Keep backward compatibility**

Make `get_codex_access_config` continue to return a primary key for existing UI consumers, but back it with the first enabled Codex profile rather than a dedicated single-key field.

**Step 4: Run tests to verify they pass**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml gateway_access codex_access -- --nocapture
```

Expected: PASS for the new helpers and compatibility behavior.

**Step 5: Commit**

```bash
git add src-tauri/src/core/gateway_access.rs src-tauri/src/platforms/openai/codex/commands.rs src-tauri/src/lib.rs
git commit -m "feat(gateway): add multi-key codex access management"
```

### Task 4: Add Failing Tests For Public Endpoint And Relay Settings

**Files:**
- Create: `src-tauri/src/platforms/openai/codex/relay.rs`
- Modify: `src-tauri/src/platforms/openai/codex/commands.rs`

**Step 1: Write the failing tests**

Cover:

- default public base URL generation for `https://<public-domain>/v1`
- relay command payload shape
- tunnel command line generation
- status transitions for stopped vs running tunnel state

**Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml relay_ public_base_url -- --nocapture
```

Expected: FAIL because the relay runtime and settings model do not exist.

**Step 3: Commit**

```bash
git add src-tauri/src/platforms/openai/codex/commands.rs
git commit -m "test(relay): cover public endpoint settings"
```

### Task 5: Implement Local Reverse-Tunnel Management

**Files:**
- Create: `src-tauri/src/platforms/openai/codex/relay.rs`
- Modify: `src-tauri/src/platforms/openai/codex/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/platforms/openai/codex/mod.rs` or the nearest module export file

**Step 1: Implement the relay runtime**

Add a managed child-process wrapper around:

```bash
ssh -NT \
  -o ServerAliveInterval=30 \
  -o ServerAliveCountMax=3 \
  -o ExitOnForwardFailure=yes \
  -R 127.0.0.1:19090:127.0.0.1:8766 \
  "$ATM_RELAY_HOST"
```

Store:

- relay host
- relay remote port
- public base URL
- running status
- last error

**Step 2: Expose relay commands**

Add Tauri commands to:

- get relay status/settings
- save relay settings
- start relay
- stop relay

**Step 3: Run tests to verify they pass**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml relay_ public_base_url -- --nocapture
```

Expected: PASS for the relay settings and command-line tests.

**Step 4: Commit**

```bash
git add src-tauri/src/platforms/openai/codex/relay.rs src-tauri/src/platforms/openai/codex/commands.rs src-tauri/src/lib.rs
git commit -m "feat(relay): add reverse tunnel manager"
```

### Task 6: Add Failing UI Coverage For Multi-Key Sharing

**Files:**
- Modify: `src/components/openai/CodexServerDialog.vue`
- Modify: `src/locales/en-US.js`
- Modify: `src/locales/zh-CN.js`

**Step 1: Define the new UI surface**

Update the OpenAI/Codex dialog to expect:

- local base URL
- public base URL
- a list of external keys
- create / rotate / disable / delete actions
- relay start / stop controls

The first build should fail because the bindings and locale strings do not exist yet.

**Step 2: Run the build to verify it fails**

Run:

```bash
npm run build
```

Expected: FAIL because the new dialog state is not implemented.

**Step 3: Commit**

```bash
git add src/components/openai/CodexServerDialog.vue src/locales/en-US.js src/locales/zh-CN.js
git commit -m "test(ui): define multi-key relay management surface"
```

### Task 7: Implement The OpenAI/Codex Sharing UI

**Files:**
- Modify: `src/components/openai/CodexServerDialog.vue`
- Modify: `src/locales/en-US.js`
- Modify: `src/locales/zh-CN.js`

**Step 1: Add the gateway key list UI**

Show:

- key label
- masked key
- enabled/disabled status
- copy
- rotate
- delete

Add a create action for new external users.

**Step 2: Add relay controls**

Show:

- local URL
- public URL
- relay host and remote port
- relay running badge
- start / stop buttons

**Step 3: Keep the existing request-log and stats tabs intact**

Do not regress current Codex monitoring behavior.

**Step 4: Run the build to verify it passes**

Run:

```bash
npm run build
```

Expected: PASS for the updated dialog and locale strings.

**Step 5: Commit**

```bash
git add src/components/openai/CodexServerDialog.vue src/locales/en-US.js src/locales/zh-CN.js
git commit -m "feat(ui): add public sharing and relay controls"
```

### Task 8: Add Ubuntu Deployment Assets

**Files:**
- Create: `deploy/nginx/public-atm-relay.conf`
- Create: `scripts/deploy_remote_relay.sh`
- Create: `scripts/check_remote_relay.sh`
- Modify: `Makefile`

**Step 1: Add nginx config**

Create a server block snippet that:

- matches the public relay domain
- proxies only allowed `/v1/*` routes
- points to `127.0.0.1:19090`
- applies rate limiting and request-size limits

**Step 2: Add deployment/check scripts**

Provide scripts that:

- copy the nginx config to Ubuntu
- test nginx config
- reload nginx
- verify relay health

**Step 3: Add Make targets**

Add targets such as:

- `make relay-deploy`
- `make relay-check`

**Step 4: Verify scripts**

Run:

```bash
bash scripts/check_remote_relay.sh
```

Expected: script exits successfully when nginx and the tunnel target are reachable.

**Step 5: Commit**

```bash
git add deploy/nginx/public-atm-relay.conf scripts/deploy_remote_relay.sh scripts/check_remote_relay.sh Makefile
git commit -m "ops: add remote relay deployment assets"
```

### Task 9: Run End-To-End Verification

**Files:**
- No new files required

**Step 1: Run backend tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml gateway_access codex_access relay_ -- --nocapture
```

Expected: PASS.

**Step 2: Run frontend build**

```bash
npm run build
```

Expected: PASS.

**Step 3: Validate local and public routes**

Run local:

```bash
curl -sS http://127.0.0.1:8766/v1/models -H "Authorization: Bearer <local-key>"
```

Run public:

```bash
curl -sS https://<public-domain>/v1/models -H "Authorization: Bearer <external-key>"
```

Expected: both succeed; disabled keys fail with `401`.

**Step 4: Commit**

```bash
git add -A
git commit -m "feat(relay): expose atm openai gateway through remote ingress"
```
