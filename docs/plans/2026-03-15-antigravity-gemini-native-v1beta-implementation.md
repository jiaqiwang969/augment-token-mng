# Antigravity Gemini Native V1Beta Gateway Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a Gemini-native `/v1beta/*` gateway in ATM so Antigravity keys can call Gemini native endpoints through the same domain and key system while keeping Claude on the OpenAI-compatible `/v1/*` surface.

**Architecture:** Extend the shared ATM gateway with a second protocol family for `/v1beta/*`, restrict it to `GatewayTarget::Antigravity`, and reuse the existing Antigravity sidecar proxy flow to relay Gemini native requests to the in-repo `cliproxy-server`. Keep logging shared with the Antigravity API service, but tag Gemini-native traffic separately.

**Tech Stack:** Rust, Tauri 2, tokio, warp, reqwest, serde_json, bytes, hyper, in-repo Go sidecar

---

### Task 1: Add Failing Unified Gateway Tests For `/v1beta/*`

**Files:**
- Modify: `src-tauri/src/core/api_server.rs`

**Step 1: Write the failing test**

Add tests that prove:

- `GET /v1beta/models` resolves gateway auth like `/v1/*`
- an Antigravity gateway key is accepted on `/v1beta/*`
- a Codex or Augment key is rejected on `/v1beta/*`

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml v1beta -- --nocapture
```

Expected: FAIL because ATM does not yet expose unified `/v1beta/*` routes.

**Step 3: Write minimal implementation**

Add the route filters and dispatch scaffolding for `/v1beta/*` in `src-tauri/src/core/api_server.rs`.

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml v1beta -- --nocapture
```

Expected: PASS for the new dispatch tests.

**Step 5: Commit**

```bash
git add src-tauri/src/core/api_server.rs
git commit -m "feat(antigravity): add unified v1beta gateway dispatch"
```

### Task 2: Add Failing Antigravity Proxy Tests For Gemini Native Models

**Files:**
- Modify: `src-tauri/src/platforms/antigravity/api_service/server.rs`

**Step 1: Write the failing test**

Add tests that prove:

- `GET /v1beta/models` is forwarded to the sidecar with the internal API key
- `GET /v1beta/models/gemini-3.1-pro-preview` is forwarded unchanged
- `POST /v1beta/models/gemini-3.1-pro-preview:generateContent` forwards the JSON body unchanged
- native SSE responses are preserved for stream Gemini actions

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml antigravity_unified_v1beta -- --nocapture
```

Expected: FAIL because the Antigravity API service only knows `/v1/*`.

**Step 3: Write minimal implementation**

Extend the Antigravity proxy handler so it can classify and forward Gemini-native `/v1beta/*` requests.

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml antigravity_unified_v1beta -- --nocapture
```

Expected: PASS for models, action forwarding, and stream passthrough.

**Step 5: Commit**

```bash
git add src-tauri/src/platforms/antigravity/api_service/server.rs
git commit -m "feat(antigravity): proxy gemini native v1beta requests"
```

### Task 3: Update Request Logging To Distinguish Gemini Native Traffic

**Files:**
- Modify: `src-tauri/src/platforms/antigravity/api_service/server.rs`

**Step 1: Write the failing test**

Add tests that prove Gemini-native requests are logged with a distinct format tag such as `gemini-native`.

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml gemini_native_log -- --nocapture
```

Expected: FAIL because Gemini-native routes currently fall through the OpenAI format inference.

**Step 3: Write minimal implementation**

Update request-format inference and log record construction to tag `/v1beta/*` requests as `gemini-native`.

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml gemini_native_log -- --nocapture
```

Expected: PASS for the new log-format test.

**Step 5: Commit**

```bash
git add src-tauri/src/platforms/antigravity/api_service/server.rs
git commit -m "feat(antigravity): tag gemini native request logs"
```

### Task 4: Verify End-To-End Build And Targeted Tests

**Files:**
- Modify: `src-tauri/src/core/api_server.rs`
- Modify: `src-tauri/src/platforms/antigravity/api_service/server.rs`

**Step 1: Run focused Rust tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml v1beta -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml antigravity_unified_v1beta -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml gemini_native_log -- --nocapture
```

Expected: PASS.

**Step 2: Run compile verification**

Run:

```bash
cargo check --manifest-path src-tauri/Cargo.toml --no-default-features
```

Expected: PASS.

**Step 3: Manual smoke test**

Run:

```bash
curl https://lingkong.xyz/v1beta/models \
  -H "Authorization: Bearer <sk-ant-...>"
```

And:

```bash
curl https://lingkong.xyz/v1beta/models/gemini-3.1-pro-preview:generateContent \
  -H "Authorization: Bearer <sk-ant-...>" \
  -H "Content-Type: application/json" \
  -d '{"contents":[{"role":"user","parts":[{"text":"Say ok"}]}]}'
```

Expected: native Gemini model list and a normal Gemini-native response payload.

**Step 4: Commit**

```bash
git add src-tauri/src/core/api_server.rs src-tauri/src/platforms/antigravity/api_service/server.rs
git commit -m "feat(antigravity): add gemini native v1beta gateway"
```
