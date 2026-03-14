# ATM Unified V1 Gateway Design

## Goal

Turn ATM port `8766` into a single OpenAI-compatible gateway entrypoint:

- external clients use `http://127.0.0.1:8766/v1`
- ATM does not expose `/augment`
- API keys select the backend target
- Augment continues to run behind the managed CLIProxyAPI sidecar

This keeps the current "ATM = control plane, CLIProxyAPI = translation engine" split intact, while making the public interface cleaner and more neutral.

## Scope

This slice changes the external access model for the local gateway.

It introduces:

- a unified public `/v1/*` gateway entry
- external access profiles that map API keys to backend targets
- Augment access-key generation and management in the Augment proxy panel
- request routing from `/v1/*` to either Codex or Augment
- Codex configuration snippets for `~/.codex/auth-pool.json` and `~/.codex/config-pool.toml`

## Non-Goals

- no direct exposure of `/augment`
- no rewrite of the current Augment token-management flow
- no rewrite of the current Codex upstream execution logic
- no automatic mutation of `~/.codex/auth-pool.json`
- no automatic mutation of `~/.codex/config-pool.toml`
- no multi-key pool UI for a single backend target
- no per-client or per-machine access-control model

## Why `/v1` Must Be Unified

ATM already serves OpenAI-compatible routes on port `8766`.

If Augment remains under `/augment/v1/*`, the public interface leaks the backend identity and forces downstream tools to know ATM internals.

The approved direction is therefore:

- public surface: `http://127.0.0.1:8766/v1`
- backend identity hidden behind the gateway
- backend selection driven by API key lookup

This is a better fit for Codex and any other OpenAI-compatible client.

## Public Contract

Public routes stay limited to OpenAI-compatible endpoints:

- `GET /v1/models`
- `POST /v1/responses`
- `POST /v1/chat/completions`

The gateway accepts either:

- `Authorization: Bearer <key>`
- `x-api-key: <key>`

The key is resolved to an access profile.

If the profile target is:

- `codex` -> dispatch to the current Codex backend flow
- `augment` -> dispatch to the Augment sidecar flow

Clients should never need to know whether the request ultimately lands on ChatGPT Codex or Augment.

## Shared Access-Profile Model

Instead of a platform-specific Augment access config, ATM should store shared gateway access profiles in app data.

Recommended persisted shape:

```json
{
  "profiles": [
    {
      "id": "codex-default",
      "name": "Codex Default",
      "target": "codex",
      "api_key": "sk-...",
      "enabled": true
    },
    {
      "id": "augment-default",
      "name": "Augment Default",
      "target": "augment",
      "api_key": "sk-...",
      "enabled": true
    }
  ]
}
```

Recommended storage file:

- `gateway_access_profiles.json`

Recommended fields per profile:

- `id`
- `name`
- `target`
- `api_key`
- `enabled`

This model is intentionally small.

The current requirement is not "many Augment keys with complex policy".

The current requirement is "clean external gateway keys that can route to different backends".

## Migration Strategy

ATM already has a persisted Codex API key in the Codex config.

To avoid breaking existing users:

1. On startup, load shared gateway profiles.
2. If no shared `codex` profile exists but legacy Codex `api_key` exists, create one migrated profile in memory.
3. Persist the migrated profile set back to app data.
4. Keep the old Codex config field as compatibility input for this migration window.

Augment gets its own new profile and does not overwrite the existing Codex key.

## Request Routing Architecture

Introduce a thin gateway layer in front of the current backend-specific handlers.

Recommended split:

1. `gateway_router`
   - extract API key from headers
   - resolve profile
   - reject unauthorized requests
   - dispatch by `target`

2. `codex_backend_handler`
   - reuse the current Codex forwarding logic
   - stop treating Codex config as the only public auth source

3. `augment_backend_handler`
   - reuse the current sidecar lifecycle and forwarding logic
   - stop depending on public `/augment` routes

This avoids creating a single oversized handler with intertwined Codex and Augment code.

## Augment Runtime Behavior

The Augment path remains operationally identical behind the gateway:

1. Load Augment accounts from storage.
2. Filter usable accounts with the same rules as the runtime proxy.
3. Ensure the managed sidecar is running.
4. Sync the auth files into the sidecar runtime directory.
5. Forward the OpenAI-compatible request to the sidecar.
6. Stream SSE responses back without reformatting them in ATM.

The sidecar internal API key remains private and must never be shown in the UI.

## UI Changes

Keep the existing Augment proxy panel as the control surface.

Add a new `Access Key` block:

- public base URL: `http://127.0.0.1:8766/v1`
- generated Augment gateway key
- show/hide key
- copy key
- generate key
- rotate key
- save key
- copy a ready-to-run `curl` example

Add a new `Codex Integration` block:

- copy `auth-pool.json` snippet
- copy `config-pool.toml` snippet
- clearly state that the exported base URL is `http://127.0.0.1:8766/v1`

The panel remains informational and operational.

It is not a general-purpose gateway admin console.

## Codex Export Shape

The exported snippets should be copy-paste friendly.

Example `auth-pool.json` snippet:

```json
{
  "OPENAI_API_KEY_POOL_AUGMENT_1": "sk-..."
}
```

Example `config-pool.toml` snippet:

```toml
[model_providers.atm-augment]
base_url = "http://127.0.0.1:8766/v1"
env_key = "OPENAI_API_KEY_POOL_AUGMENT_1"

[[model_providers.atm-augment.account_pool]]
base_url = "http://127.0.0.1:8766/v1"
env_key = "OPENAI_API_KEY_POOL_AUGMENT_1"
```

This gives the user a real key and a real base URL without forcing ATM to rewrite global Codex files in the first iteration.

## Error Handling

Gateway responses should be explicit but not leak internal details.

Recommended behavior:

- `401 Unauthorized`
  - missing API key
  - unknown API key
  - disabled profile
- `503 Service Unavailable`
  - Augment sidecar binary missing
  - no usable Augment accounts
  - sidecar startup failed
- `502 Bad Gateway`
  - backend forwarding failure after routing
- `404 Not Found`
  - unsupported route under the public gateway

Recommended error copy:

- `Unauthorized: invalid gateway API key`
- `Augment backend unavailable: no usable accounts`
- `Augment backend unavailable: sidecar not ready`

## Testing

Backend tests should cover:

- API-key extraction from `Authorization` and `x-api-key`
- profile lookup and disabled-profile rejection
- migration from legacy Codex key into shared gateway profiles
- unified `/v1/models` dispatch to Codex vs Augment based on key
- unified `/v1/chat/completions` unauthorized rejection
- Augment `503` cases when there are no usable accounts or no sidecar

Frontend verification should cover:

- panel renders unified `/v1` base URL
- generated curl example uses `/v1`, not `/augment`
- Codex snippet generation uses the exported key and unified base URL

Build verification remains:

- `npm run build`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `cargo build --manifest-path src-tauri/Cargo.toml`

## Success Criteria

This feature is successful when:

1. ATM no longer requires exposing `/augment` to external clients.
2. External clients can call `http://127.0.0.1:8766/v1`.
3. API keys determine whether `/v1/*` routes to Codex or Augment.
4. Augment still runs through the managed CLIProxyAPI sidecar.
5. The Augment panel can generate and display a real external access key.
6. The panel exports usable Codex configuration snippets based on `base_url + key`.
7. Existing Codex users do not lose access because of the shared-gateway migration.
