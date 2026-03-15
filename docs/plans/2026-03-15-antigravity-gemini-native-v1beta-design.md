# ATM Antigravity Gemini Native V1Beta Gateway Design

## Goal

Expose Antigravity-backed Gemini native endpoints through ATM without mixing them into the OpenAI-compatible `/v1/*` surface.

The external contract becomes:

- OpenAI-compatible traffic:
  - `http://127.0.0.1:8766/v1/*`
  - `https://lingkong.xyz/v1/*`
- Gemini native traffic:
  - `http://127.0.0.1:8766/v1beta/*`
  - `https://lingkong.xyz/v1beta/*`

Both surfaces keep using the same Antigravity gateway API key format such as `sk-ant-*`, but protocol boundaries stay explicit.

## Approved Decisions

- Claude remains on the OpenAI-compatible surface:
  - `/v1/models`
  - `/v1/chat/completions`
  - `/v1/responses`
- Gemini moves to the native Gemini surface:
  - `/v1beta/models`
  - `/v1beta/models/*action`
- The caller should not see an `/antigravity/*` prefix.
- Authentication continues to use the shared ATM gateway profile store.
- Only `GatewayTarget::Antigravity` keys are allowed to use `/v1beta/*`.
- ATM remains a control plane and relay; the bundled `cliproxy-server` sidecar remains the protocol engine.

## Why This Change Is Needed

The current Antigravity integration only exposes the OpenAI-compatible `/v1/*` paths. That works for Claude-style models, but it filters out Gemini models from the public OpenAI surface because the sidecar intentionally treats Gemini native support as a separate protocol family.

This is why:

- the upstream Antigravity model pool can show many Gemini models
- but `https://lingkong.xyz/v1/models` only returns the OpenAI-compatible subset
- and direct `/v1/chat/completions` requests with Gemini model IDs fail with `model is not available on this endpoint`

The correct fix is not to force Gemini into `/v1/*`. The correct fix is to expose the sidecar's native Gemini surface through ATM.

## Current Codebase Fit

The repo already has the pieces needed:

- shared key-based gateway dispatch in `src-tauri/src/core/api_server.rs`
- Antigravity-side sidecar lifecycle management in `src-tauri/src/platforms/antigravity/sidecar.rs`
- Antigravity request proxying in `src-tauri/src/platforms/antigravity/api_service/server.rs`
- native Gemini routes already implemented inside the in-repo Go sidecar:
  - `sidecars/cliproxy/apps/server-go/internal/api/server.go`
  - `sidecars/cliproxy/apps/server-go/sdk/api/handlers/gemini/gemini_handlers.go`

So this is a relay-surface expansion, not a new protocol implementation.

## Architecture

### External Surface

ATM will expose two protocol families:

- OpenAI-compatible:
  - `GET /v1/models`
  - `POST /v1/chat/completions`
  - `POST /v1/responses`
- Gemini native:
  - `GET /v1beta/models`
  - `GET /v1beta/models/*action`
  - `POST /v1beta/models/*action`

The routing key remains the gateway API key.

### Routing Model

ATM resolves the bearer key to a `GatewayAccessProfile`.

- `Codex` keys are valid only for `/v1/*`
- `Augment` keys are valid only for `/v1/*`
- `Antigravity` keys are valid for:
  - `/v1/*` for Claude/OpenAI-compatible traffic
  - `/v1beta/*` for Gemini native traffic

If a non-Antigravity key calls `/v1beta/*`, ATM returns an authorization-style rejection instead of attempting a fallback route.

### Runtime Model

ATM still does the same control-plane work:

- load Antigravity accounts from ATM storage
- filter usable accounts
- ensure the Antigravity sidecar is running
- inject the internal sidecar API key
- forward request method, path, headers, query, and body
- stream or buffer the upstream response back to the client
- record request logs

The sidecar still does:

- Gemini native request handling
- model lookup
- upstream execution
- native Gemini response formatting

## Code Changes

### Shared Gateway

Modify `src-tauri/src/core/api_server.rs` to add a second unified gateway surface for `/v1beta/*`.

That surface should:

- resolve the gateway profile from the same authorization headers
- allow only `GatewayTarget::Antigravity`
- forward the request to a new Antigravity Gemini-native handler

### Antigravity API Service

Modify `src-tauri/src/platforms/antigravity/api_service/server.rs`.

Add a Gemini-native relay path that can proxy:

- `GET /v1beta/models`
- `GET /v1beta/models/*action`
- `POST /v1beta/models/*action`

The implementation should reuse the existing helper flow where possible:

- account lookup
- sidecar startup and health checks
- request header filtering
- authorization replacement
- request logging

The main difference is that the relay path must preserve Gemini-native URLs and content types instead of assuming OpenAI request formats.

## Logging

Existing Antigravity logs should continue to work, but the request format field should distinguish Gemini native traffic from OpenAI-compatible traffic.

Recommended values:

- `openai-chat`
- `openai-responses`
- `gemini-native`

This lets the UI keep one Antigravity service view while showing which protocol family each request used.

## Error Handling

ATM should return clear errors for:

- invalid or missing gateway key
- non-Antigravity key hitting `/v1beta/*`
- no available Antigravity accounts
- sidecar not initialized or unhealthy
- upstream relay errors

ATM should not translate Gemini-native error payloads into OpenAI-style errors on `/v1beta/*`. That surface should preserve native semantics as much as possible.

## Testing Strategy

Add tests in Rust for:

- unified `/v1beta/*` route dispatch by key target
- rejection of non-Antigravity keys on `/v1beta/*`
- Antigravity Gemini-native proxying for `GET /v1beta/models`
- Antigravity Gemini-native proxying for `POST /v1beta/models/...:generateContent`
- SSE passthrough for streaming Gemini-native responses

The sidecar itself already has native Gemini handler tests in Go. ATM only needs to test that it forwards correctly and preserves the contract.

## Out Of Scope

- adding Gemini native UI widgets or separate Gemini dashboards
- changing the Antigravity key model
- making Codex or Augment keys work on `/v1beta/*`
- reworking the sidecar's native Gemini catalog logic
- forcing Gemini support into `/v1/*`
