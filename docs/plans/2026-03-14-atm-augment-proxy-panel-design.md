# ATM Augment Proxy Panel Design

## Goal

Expose the already-implemented Augment sidecar capability inside the Augment page, without changing the existing Augment account-management workflow.

The user should be able to open the Augment page and immediately answer four questions:

1. Is the ATM API server running?
2. Is the bundled Augment proxy capability available?
3. Are there usable Augment accounts right now?
4. What exact URL should Codex CLI point at?

## Scope

This iteration adds a compact proxy panel to the Augment page.

The panel is an operational surface, not a new account-management flow.

It will:

- show Augment proxy readiness
- show the external base URL: `http://127.0.0.1:8766/v1`
- show whether the shared ATM API server is running
- show whether the bundled `cliproxy-server` sidecar is installed and currently running
- show total Augment accounts vs usable Augment accounts
- let the user copy the base URL
- let the user copy a Codex CLI environment example
- let the user start or stop the shared ATM API server from the Augment page
- explain that the sidecar is lazily started on first request

## Non-Goals

- no rewrite of the existing Augment token list
- no new token-selection strategy UI
- no real persisted “enable/disable Augment proxy” switch
- no eager sidecar boot button
- no quota-aware or smart-score routing controls
- no crash-monitoring UI

## Why No Real Toggle In This Slice

The backend proxy surface is already routed through unified `/v1/*`, while the current architecture intentionally treats the Go sidecar as a black-box translator.

Adding a true toggle now would force a second layer of routing rules, persisted config, and extra rejection states before the user has even seen the feature in the UI.

That is unnecessary complexity for the first UI slice.

The panel should therefore be informational and operational:

- API server start/stop is real
- Augment proxy visibility is real
- Augment proxy availability is derived from actual runtime state
- Augment proxy enable/disable is deferred

## Placement

The panel should live inside `src/components/token/TokenList.vue`, directly below the existing page header row.

Reasons:

- it keeps the Augment page self-contained
- it stays close to the token pool that powers the proxy
- it does not require adding another modal or global settings entry
- it preserves the existing Augment management flow instead of splitting it across screens

Implementation-wise, the panel itself should be a separate component so `TokenList.vue` does not absorb another large block of logic.

## Backend Contract

Add a new Tauri command in `src-tauri/src/platforms/augment/commands.rs`:

- `get_augment_proxy_status`

It should return a single summary object for the panel, not raw internal structures.

Recommended response shape:

```json
{
  "api_server_running": true,
  "api_server_address": "http://127.0.0.1:8766",
  "proxy_base_url": "http://127.0.0.1:8766/v1",
  "sidecar_configured": true,
  "sidecar_running": false,
  "sidecar_healthy": false,
  "total_accounts": 8,
  "available_accounts": 5
}
```

Notes:

- `proxy_base_url` is the user-facing URL, not the sidecar localhost URL.
- `sidecar_configured` means ATM has a managed sidecar instance, which in practice means the bundled binary was resolved successfully.
- `sidecar_running` and `sidecar_healthy` reflect the lazy runtime state.
- `available_accounts` must reuse the same usability rules as the actual proxy handler, so the panel and runtime do not disagree.

## Frontend Behavior

Create a dedicated component:

- `src/components/token/AugmentProxyPanel.vue`

The component should:

- load proxy status on mount
- refresh proxy status on demand
- react to `api-server-status-changed`
- react to `tokens-updated`
- show clear status copy for these states:
  - API server stopped
  - sidecar binary missing
  - no usable Augment accounts
  - ready for first request
  - sidecar already running

The user-facing copy should stay plain:

- “API server not running”
- “No usable Augment accounts”
- “Proxy ready, sidecar will start on first request”
- “Proxy active”

The panel should also include:

- primary URL display
- copy base URL button
- copy `OPENAI_BASE_URL` example button
- start server button when stopped
- stop server button when running

## Data Flow

1. User opens Augment page.
2. `TokenList.vue` renders the new `AugmentProxyPanel`.
3. The panel invokes `get_augment_proxy_status`.
4. Backend reads:
   - API server state from `AppState.api_server`
   - sidecar runtime state from `AppState.augment_sidecar`
   - Augment accounts from `AppState.storage_manager`
5. Backend counts usable accounts using the same filter as the live Augment path behind unified `/v1/*`.
6. Frontend renders status badges, counts, and copyable commands.
7. If the user starts or stops the API server from the panel, the existing server commands are reused.

## Error Handling

The panel should degrade cleanly:

- command failure -> show a lightweight inline error state plus retry button
- clipboard failure -> toast error
- start/stop API server failure -> toast error

The panel must not block token-list rendering if proxy status fails to load.

## Testing

Backend:

- add unit tests for the proxy status summary helper / command-facing logic
- verify usable-account counts match the same banned / expired / exhausted rules used by `proxy_server.rs`

Frontend:

- no new frontend unit-test harness is required in this slice
- rely on `pnpm build` to catch integration errors

## Success Criteria

This feature is successful when:

1. The Augment page shows a dedicated proxy panel.
2. The panel shows `http://127.0.0.1:8766/v1` as the Codex CLI base URL.
3. The panel accurately reflects whether the ATM API server is running.
4. The panel accurately reflects whether usable Augment accounts exist.
5. The panel can copy the base URL and the shell export example.
6. The panel can start and stop the shared ATM API server.
7. The implementation does not alter the existing Augment token-management behavior.
