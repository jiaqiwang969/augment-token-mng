# ATM One-Command Relay Deploy Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a `make deploy` workflow that reads relay settings from a local `.env.relay`, patches the managed relay block inside the remote nginx site file, ensures the reverse tunnel is running, and verifies the public `/v1` path end to end.

**Architecture:** Keep secrets in an untracked `.env.relay`, move relay deploy logic into small reusable Node helpers for parsing, template rendering, and managed-block patching, and keep the operational shell layer focused on `ssh` and `scp` execution.

**Tech Stack:** GNU make, Node.js ESM scripts, bash, nginx, OpenSSH, node:test

---

### Task 1: Add The Design And Plan Documents

**Files:**
- Create: `docs/plans/2026-03-14-atm-one-command-relay-deploy-design.md`
- Create: `docs/plans/2026-03-14-atm-one-command-relay-deploy.md`

**Step 1: Write the design doc**

Document the approved `.env.relay` model, `make deploy` responsibilities, and non-goals.

**Step 2: Write the implementation plan**

Break the work into small testable tasks for config loading, templating, scripts, and Make targets.

### Task 2: Add Failing Tests For Relay Config Helpers

**Files:**
- Create: `tests/relayConfig.test.js`
- Create: `scripts/relayConfig.mjs`

**Step 1: Write failing tests**

Cover:

- parsing dotenv-style content
- overriding file values with process env values
- validating required relay settings
- rendering the nginx template with `server_name` and `proxy_pass`
- replacing or inserting the managed relay block in an existing nginx site file

**Step 2: Run the tests and confirm failure**

Run:

```bash
node --test tests/relayConfig.test.js
```

Expected: FAIL because the helper module does not exist yet.

### Task 3: Implement Relay Config Helpers

**Files:**
- Create: `scripts/relayConfig.mjs`
- Modify: `tests/relayConfig.test.js`

**Step 1: Implement the minimal helper functions**

Add pure functions for:

- reading dotenv text into key/value pairs
- loading `.env.relay` plus environment overrides
- validating required relay settings
- rendering nginx config from a template

**Step 2: Re-run the tests**

Run:

```bash
node --test tests/relayConfig.test.js
```

Expected: PASS

### Task 4: Add Relay Template And Example Env

**Files:**
- Create: `.env.relay.example`
- Create: `deploy/nginx/public-atm-relay.conf.template`
- Modify: `.gitignore`

**Step 1: Add the example env file**

Document the required and optional relay deployment settings with placeholders.

**Step 2: Add the nginx template**

Replace hard-coded values with placeholders for:

- `ATM_RELAY_REMOTE_PORT`

**Step 3: Ignore the real env file**

Ensure `.env.relay` is not tracked.

### Task 5: Implement One-Command Deploy Script

**Files:**
- Create: `scripts/deploy_remote_relay.mjs`
- Modify: `scripts/start_remote_relay.sh`
- Modify: `scripts/check_remote_relay.sh`
- Modify: `scripts/stop_remote_relay.sh`

**Step 1: Add deploy script behavior**

The script should:

- load relay settings from `.env.relay`
- fetch the current remote nginx site file
- render the relay block template
- upsert the managed relay block into the site file
- upload the updated site file to the configured remote nginx path
- run remote nginx validation and reload
- call the existing relay-start and relay-check flows

**Step 2: Make relay shell scripts env-aware**

Each script should auto-load `.env.relay` if present so manual exports are not required.

**Step 3: Re-run targeted verification**

Run:

```bash
bash -n scripts/start_remote_relay.sh scripts/check_remote_relay.sh scripts/stop_remote_relay.sh
node --test tests/relayConfig.test.js
```

Expected: PASS

### Task 6: Wire Make Targets

**Files:**
- Modify: `Makefile`

**Step 1: Add one-command targets**

Add:

- `make deploy`
- `make deploy-check`

Keep the existing relay targets working.

**Step 2: Verify help text**

Run:

```bash
make help
```

Expected: the new deploy commands are listed.

### Task 7: Run Full Verification

**Files:**
- No code changes required

**Step 1: Run local verification**

Run:

```bash
node --test tests/tauriBridge.test.js tests/ensureViteDev.test.js tests/relayConfig.test.js
npm run build
```

Expected: PASS

**Step 2: Run live deploy verification**

Run:

```bash
make deploy
```

Expected:

- remote nginx config passes `nginx -t`
- nginx reload succeeds
- reverse tunnel is running
- local, remote-loopback, and public `/v1/models` checks all succeed
