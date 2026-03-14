# ATM Augment Sidecar Integration Design

## Goal

Let Codex CLI use Augment through ATM only.

The user should configure a single base URL:

`http://127.0.0.1:8766/augment`

After that, ATM should:

1. Choose usable Augment accounts from its own token pool.
2. Materialize those accounts as CLIProxyAPI auth JSON files.
3. Lazily start and manage the bundled `cliproxy-server` sidecar.
4. Forward `/augment/v1/*` traffic to the sidecar and stream the response back.

## Non-Goals

- Rewriting Augment-to-OpenAI protocol translation in Rust.
- Replacing CLIProxyAPI model mapping or Responses API alignment logic.
- Building the UI toggle or status panel in this iteration.
- Implementing smart scoring or quota-based auto-switching beyond the current minimum viable filtering rules.
- Adding automatic crash restart logic beyond request-time lazy restart.

## Approved Architecture

This integration uses a strict split of responsibilities:

- ATM is the control plane.
- CLIProxyAPI is the data-plane protocol translator.

In plain terms:

- ATM manages Augment accounts.
- CLIProxyAPI translates Augment upstream behavior into the OpenAI-compatible surface Codex CLI expects.

ATM must not reimplement the translation logic that already exists in CLIProxyAPI.

## Why This Split Is Correct

ATM already knows how to manage Augment accounts, bans, refresh state, and availability.

CLIProxyAPI already knows how to expose OpenAI-compatible endpoints like:

- `GET /v1/models`
- `POST /v1/responses`
- `POST /v1/chat/completions`

Trying to merge those responsibilities into one codebase would duplicate working translation code and create a second protocol stack to maintain.

The correct integration is to let ATM host CLIProxyAPI as an internal sidecar.

## Runtime Flow

1. Codex CLI sends a request to ATM at `http://127.0.0.1:8766/augment/...`.
2. ATM loads Augment tokens from its storage manager.
3. ATM filters the token set down to accounts that are usable enough for the first release.
4. ATM writes those accounts into a temporary auth directory as CLIProxyAPI-compatible `auggie-*.json` files.
5. ATM ensures the bundled `cliproxy-server` process is running on a private localhost port.
6. ATM forwards the request to `http://127.0.0.1:{sidecar-port}/v1/...` with the internal sidecar API key.
7. CLIProxyAPI translates the OpenAI-compatible request into Augment upstream calls.
8. CLIProxyAPI returns normal JSON or SSE.
9. ATM passes that response back to Codex CLI without changing the translation payload.

## Component Boundaries

### ATM Sidecar Manager

Location:

- `src-tauri/src/platforms/augment/sidecar.rs`

Responsibilities:

- Find the bundled `cliproxy-server` binary.
- Allocate an internal localhost port.
- Write temporary `config.yaml`.
- Write and refresh auth JSON files.
- Start and stop the child process.
- Run a health check against `/v1/models`.
- Expose `base_url()` and the internal API key to the proxy layer.

### ATM Augment Proxy

Location:

- `src-tauri/src/platforms/augment/proxy_server.rs`

Responsibilities:

- Own `/augment/v1/models`
- Own `/augment/v1/responses`
- Own `/augment/v1/chat/completions`
- Load usable Augment tokens before each request.
- Sync auth files before each request.
- Ensure the sidecar is available before forwarding.
- Replace inbound client authorization with the internal sidecar API key.
- Preserve JSON and SSE responses.

### CLIProxyAPI Sidecar

Runtime bundle target:

- `src-tauri/resources/cliproxy-server` (generated from in-repo Go source during build)

Responsibilities:

- OpenAI-compatible request parsing.
- Augment upstream execution.
- Responses API alignment.
- Chat Completions alignment.
- Model exposure and model alias behavior.
- All Augment-to-OpenAI protocol translation.

## Contract With CLIProxyAPI

ATM should treat CLIProxyAPI as a black box with three integration surfaces only:

1. `config.yaml`
2. `auth-dir/*.json`
3. `GET/POST /v1/*`

### Config Contract

The generated config should follow the Go server's actual config schema, not an approximate version.

Important detail:

- The routing key must be nested as:

```yaml
routing:
  strategy: round-robin
```

not:

```yaml
routing-strategy: round-robin
```

The sidecar config also needs the internal API key in `api-keys`, and should keep `client-api-keys` only if we intentionally want structured scoping for the internal key.

### Auth File Contract

CLIProxyAPI reads auth files by treating the JSON body as provider metadata.

For Auggie/Augment, the working metadata shape is:

```json
{
  "type": "auggie",
  "label": "tenant.augmentcode.com",
  "access_token": "token",
  "tenant_url": "https://tenant.augmentcode.com/",
  "scopes": ["email"],
  "client_id": "auggie-cli",
  "login_mode": "localhost",
  "last_refresh": "2026-03-13T00:00:00Z"
}
```

That means ATM does not need to serialize the full Go `Auth` object. It only needs to emit metadata JSON that the file store can load.

## First-Release Availability Rules

For this iteration, "usable token" should mean:

- `access_token` is present
- `tenant_url` is present
- token is not in a known banned or invalid state

If ATM already has trustworthy local state for expiry or quota exhaustion, the filter may include those checks.

If those fields are not consistently available in the current Rust model, the release should ship with the smallest reliable filter set and leave smarter selection for follow-up work.

## Error Handling

ATM should convert integration failures into clear proxy-facing errors:

- no usable Augment account -> `503`
- sidecar binary missing -> `503`
- sidecar failed to start or failed health check -> `503`
- sidecar upstream request failure -> `502`

The response body should follow the existing local proxy error shape used by the Codex server so CLI clients get consistent failures.

## Concurrency and Lifecycle Notes

The sidecar lifecycle should avoid holding a blocking mutex across `.await`.

The cleanest fix is to use an async-aware lock for the sidecar state, or otherwise refactor the start path so the proxy handler can:

1. lock
2. inspect state
3. drop lock
4. await startup
5. lock again

The current `spawn_blocking + block_on` pattern is a temporary workaround and should not be the final form.

## Packaging Notes

Bundling is required for the feature to be real.

That means:

- keep `src-tauri/resources/cliproxy-server` as the bundle resource path
- generate `src-tauri/resources/cliproxy-server` during build instead of hand-maintaining it in git
- register it in `src-tauri/tauri.conf.json`
- keep the runtime lookup logic compatible with both dev mode and bundled mode

## Verification

The feature is successful when all of the following are true:

1. ATM starts on `127.0.0.1:8766`.
2. A user does not need to start CLIProxyAPI manually.
3. `GET http://127.0.0.1:8766/augment/v1/models` succeeds when a usable Augment account exists.
4. `POST http://127.0.0.1:8766/augment/v1/responses` streams or returns a valid OpenAI-style response through the sidecar.
5. `POST http://127.0.0.1:8766/augment/v1/chat/completions` forwards correctly.
6. Killing the sidecar and sending another request causes ATM to start it again on demand.

## Out Of Scope Follow-Ups

- UI switch and live sidecar status
- Smart account scoring
- Quota-aware account switching
- Automatic crash monitoring and restart loop
- Antigravity sidecar parity
