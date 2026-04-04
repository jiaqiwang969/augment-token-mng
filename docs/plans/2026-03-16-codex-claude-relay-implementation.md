# Codex Claude Relay Compatibility Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the public ATM relay proxy Claude-native `/v1/messages` routes so Codex/Claude clients can reach ATM through nginx.

**Architecture:** Keep the relay allowlist explicit. Add two Anthropic-native nginx locations, then extend the relay health script with stable POST probes that expect ATM-generated Claude error JSON. Protect the behavior with focused repository tests.

**Tech Stack:** nginx template, bash health-check script, Node test runner

---

### Task 1: Add failing regression coverage

**Files:**
- Modify: `tests/relayConfig.test.js`

**Step 1: Write the failing test**

- Add one test asserting `deploy/nginx/public-atm-relay.conf.template` contains `location = /v1/messages` and `location = /v1/messages/count_tokens`
- Add one test asserting `scripts/check_remote_relay.sh` contains Claude-native probe logic, Anthropic headers, and `/v1/messages` paths

**Step 2: Run test to verify it fails**

Run: `node --test tests/relayConfig.test.js`
Expected: FAIL because the template and script do not yet contain Claude-native relay coverage

### Task 2: Patch relay allowlist

**Files:**
- Modify: `deploy/nginx/public-atm-relay.conf.template`

**Step 1: Write minimal implementation**

- Insert exact-match nginx locations for:
  - `/v1/messages`
  - `/v1/messages/count_tokens`
- Keep them before the catch-all `location ^~ /v1/ { return 404; }`

**Step 2: Keep behavior narrow**

- Do not replace the catch-all block
- Do not broaden to a blanket `/v1/*` proxy

### Task 3: Patch relay health checks

**Files:**
- Modify: `scripts/check_remote_relay.sh`

**Step 1: Add Claude-native probe helpers**

- Add helper logic for POST probes that expect HTTP `400`
- Validate Anthropic-style JSON error shape:
  - `"type":"error"`
  - `"invalid_request_error"`

**Step 2: Use stable probe payloads**

- Send a minimal Claude-native payload to `/v1/messages`
- Send a minimal Claude-native payload to `/v1/messages/count_tokens`
- Use an intentionally invalid model so success does not depend on upstream account availability

**Step 3: Cover local, remote loopback, and public HTTPS**

- Probe local relay
- Probe remote loopback
- Probe public HTTPS relay

### Task 4: Verify

**Files:**
- Test: `tests/relayConfig.test.js`

**Step 1: Run focused verification**

Run: `node --test tests/relayConfig.test.js`
Expected: PASS

**Step 2: Summarize follow-up**

- Mention that remote deploy still needs `make deploy` or the existing relay repair flow to publish the nginx template change

