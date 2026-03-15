# ATM Antigravity API Service Design

## Goal

Add an `Antigravity API 服务` entry under the existing Antigravity account-management page, so ATM can expose Antigravity accounts through the same unified OpenAI-compatible gateway already used by Codex:

- local: `http://127.0.0.1:8766/v1`
- public: `https://lingkong.xyz/v1`

The new service must:

- keep `Codex` and `Antigravity` API keys independent
- reuse the existing ATM gateway, team-member, chart, and export patterns
- keep Antigravity runtime execution behind the bundled `cliproxy-server` sidecar
- keep the product boundary inside `augment-token-mng`, with no runtime dependency on an external repository path

## Approved Decisions

- `Antigravity API 服务` is not a new top-level platform page
- it is a jump page under `Antigravity 账号管理`
- the dialog layout should closely match `OpenAI -> Codex API 服务`
- external traffic keeps using unified `/v1`, not `/antigravity/v1`
- request routing is determined by gateway API key, not by URL path prefix
- `Codex` and `Antigravity` keys are separate even for the same person
- member identity is shared by `member_code`, but logs and stats remain isolated by target
- runtime remains a sidecar, but source/build/packaging stay inside this repository
- Phase 1 focuses on the API-service surface and does not redesign the existing Antigravity account-management page

## Why This Fits The Current Codebase

The current ATM codebase already has the correct backbone for this feature:

- shared gateway profiles live in `src-tauri/src/core/gateway_access.rs`
- the unified `/v1` gateway dispatch already exists in `src-tauri/src/core/api_server.rs`
- Codex already provides a complete reference implementation for:
  - service dialog UI
  - member-specific gateway keys
  - request logs
  - daily trend charts
  - token-share charts
  - access export
- Augment already proves the runtime sidecar pattern inside ATM:
  - lazy start
  - local config generation
  - auth file sync
  - streaming proxy
- the bundled `cliproxy-server` source already lives under:
  - `sidecars/cliproxy/apps/server-go`

The Antigravity work is therefore an integration problem, not a protocol-research problem.

## Existing Antigravity Capability In The Sidecar

The in-repo Go sidecar already contains Antigravity support:

- auth:
  - `sidecars/cliproxy/apps/server-go/internal/auth/antigravity/*`
- runtime executor:
  - `sidecars/cliproxy/apps/server-go/internal/runtime/executor/antigravity_executor.go`
- OpenAI Responses translation:
  - `sidecars/cliproxy/apps/server-go/internal/translator/antigravity/openai/responses/*`
- OpenAI Chat Completions translation:
  - `sidecars/cliproxy/apps/server-go/internal/translator/antigravity/openai/chat-completions/*`
- Gemini and Claude request/response translation:
  - `sidecars/cliproxy/apps/server-go/internal/translator/antigravity/gemini/*`
  - `sidecars/cliproxy/apps/server-go/internal/translator/antigravity/claude/*`

Phase 1 should assume this capability is the translation engine of record. ATM should not reimplement the protocol conversion in Rust.

## Architecture

### External Surface

Clients call one of:

- `http://127.0.0.1:8766/v1`
- `https://lingkong.xyz/v1`

Supported OpenAI-style endpoints for Antigravity in Phase 1:

- `GET /v1/models`
- `POST /v1/responses`
- `POST /v1/chat/completions`

The caller does not see an Antigravity-specific path.

### Routing Model

ATM resolves the incoming API key into a `GatewayAccessProfile`. The profile determines the target:

- `GatewayTarget::Codex`
- `GatewayTarget::Augment`
- `GatewayTarget::Antigravity`

If the matched profile target is `Antigravity`, ATM forwards the request to the Antigravity API-service handler.

### Runtime Model

ATM remains the control plane:

- reads Antigravity accounts from ATM storage
- filters usable accounts
- writes sidecar auth files
- starts and health-checks the sidecar
- proxies request and response bodies
- records logs and aggregated usage

`cliproxy-server` remains the translation plane:

- OpenAI-style request parsing
- model routing inside Antigravity provider space
- upstream Antigravity protocol execution
- OpenAI-style response conversion

## Code Layout

### Shared Core Changes

Modify:

- `src-tauri/src/core/gateway_access.rs`
- `src-tauri/src/core/api_server.rs`
- `src-tauri/src/lib.rs`

Add:

- `GatewayTarget::Antigravity`
- Antigravity runtime state in `AppState`
- unified `/v1` dispatch for Antigravity
- Tauri command registration for the Antigravity API service

### Antigravity Account Management Stays Separate

Keep the existing account-management module as the source of truth for Antigravity credentials:

- `src-tauri/src/platforms/antigravity/commands.rs`
- `src-tauri/src/platforms/antigravity/models/*`
- `src-tauri/src/platforms/antigravity/modules/*`

This module continues to own:

- account CRUD
- OAuth exchange
- refresh-token handling
- quota refresh
- local app switching

### New Antigravity API-Service Module

Create:

- `src-tauri/src/platforms/antigravity/api_service/mod.rs`
- `src-tauri/src/platforms/antigravity/api_service/models.rs`
- `src-tauri/src/platforms/antigravity/api_service/commands.rs`
- `src-tauri/src/platforms/antigravity/api_service/server.rs`
- `src-tauri/src/platforms/antigravity/api_service/logger.rs`
- `src-tauri/src/platforms/antigravity/api_service/team_profiles.rs`
- `src-tauri/src/platforms/antigravity/sidecar.rs`

Responsibilities:

- service dialog configuration and status commands
- dedicated Antigravity gateway-profile CRUD
- readable Antigravity key generation
- request logging and aggregation
- sidecar lifecycle
- unified-gateway request handling

### Frontend

Modify:

- `src/components/platform/AntigravityAccountManager.vue`
- `src/locales/zh-CN.js`
- `src/locales/en-US.js`

Create:

- `src/components/antigravity/AntigravityServerDialog.vue`

Optional shared extraction if useful during implementation:

- move reusable helper logic from `src/utils/codexTeamUi.js` into a shared utility file

## UI Design

The Antigravity account page gets one new header action that opens `AntigravityServerDialog`.

The dialog should mirror the Codex service dialog in structure:

- overview tab
- request logs tab
- local URL and public URL
- member-centric key table
- monthly trend chart
- token-share pie chart
- member selection and filtering
- access export

The page should feel like the Antigravity twin of `CodexServerDialog`, not like a separate product.

## Identity And Key Model

### Shared Member Identity

The same person can exist across multiple service targets by shared `member_code`.

Examples:

- `jdd` can own one Codex key and one Antigravity key
- `jqw` can own one Codex key and one Antigravity key

Shared identity fields remain:

- `name`
- `member_code`
- `role_title`
- `persona_summary`
- `color`
- `notes`

### Independent Service Keys

Antigravity keys must be independent from Codex keys.

Recommended Antigravity format:

- `sk-ant-<member_code>-<random8>`

Examples:

- `sk-ant-jdd-a4f29c7e`
- `sk-ant-jqw-3f8d10ab`

This keeps:

- visual readability
- per-service isolation
- easier export and debugging

## Data Flow

For a normal Antigravity request:

1. client calls `POST /v1/responses`
2. ATM resolves the incoming API key to an enabled `GatewayAccessProfile`
3. the profile target is `Antigravity`
4. ATM loads accounts from `antigravity_storage_manager`
5. ATM filters usable accounts
6. ATM ensures `AntigravitySidecar` is running
7. ATM syncs account auth files into the sidecar auth directory
8. ATM forwards the request to the sidecar on a private localhost port using an internal bearer key
9. the sidecar translates and executes against Antigravity upstream
10. ATM streams or returns the sidecar response to the client
11. ATM records request logs and usage tagged with:
   - `target = Antigravity`
   - `profile_id`
   - `member_code`
   - `model`
   - `format`
   - `status`

## Sidecar Design

The Antigravity API service should use its own sidecar manager instance and runtime artifacts, even though it launches the same `cliproxy-server` binary as Augment.

Recommended state:

- `port`
- `child`
- `auth_dir`
- `home_dir`
- `config_path`
- `runtime_path`
- `api_key`
- `binary_path`

The sidecar should be:

- created with `AppState`, but not eagerly started
- lazily started on the first Antigravity API request
- health-checked through localhost
- stopped when the shared ATM API server stops

Antigravity and Augment should not share runtime files or ports.

## Logging And Stats

Antigravity logs should follow the Codex shape closely so the frontend can reuse the same rendering model.

Each persisted request log should carry:

- `gateway_profile_id`
- `gateway_profile_name`
- `member_code`
- `role_title`
- `color`
- `api_key_suffix`
- `model`
- `format`
- `status`
- `duration_ms`
- `input_tokens`
- `output_tokens`
- `total_tokens`
- `timestamp`

Daily series should be grouped by Antigravity gateway profile and enriched with:

- `profile_id`
- `profile_name`
- `member_code`
- `role_title`
- `color`

Codex and Antigravity analytics remain separate. Phase 1 should not attempt a cross-service merged chart.

## Usage Accounting Rule

To avoid false token spikes, usage accounting should be conservative:

- if the sidecar returns OpenAI-style `usage`, persist it
- if the final response does not include reliable `usage`, record request count but set token counters to `0`
- do not estimate token counts locally

This rule is mandatory for both streaming and non-streaming paths.

## Error Handling

Phase 1 should include these behaviors:

- if no Antigravity accounts are available, return a clear gateway error
- if the sidecar is missing or cannot start, return a clear sidecar error
- if the sidecar health check fails, restart once before failing the request
- if a single account cannot be materialized into an auth file, skip that account rather than crash the whole pool
- if all accounts fail materialization, fail the request with a clear no-account error
- if upstream usage is absent, do not fail the request; log token counts as `0`

Phase 1 should not include advanced circuit breaking or multi-hop retries.

## Build And Packaging

The sidecar binary continues to be built from in-repo source:

- source: `sidecars/cliproxy/apps/server-go`
- build script: `scripts/build-cliproxy.sh`
- output: `src-tauri/resources/cliproxy-server`

No external repository path should be required for local builds or bundled app packaging.

## Testing Strategy

Phase 1 should cover:

- shared-profile target support for `GatewayTarget::Antigravity`
- Antigravity key generation and profile CRUD
- sidecar config generation and auth sync
- unified `/v1` dispatch to Antigravity
- streaming and non-streaming proxy paths
- conservative `usage` handling
- Antigravity service dialog rendering and member-selection behavior

## Non-Goals

Phase 1 does not include:

- replacing the existing Antigravity account-management page
- merging Codex and Antigravity charts into one dashboard
- rewriting Antigravity protocol translation in Rust
- building a full generic multi-provider framework before shipping this feature
- advanced sidecar self-healing beyond one restart attempt
