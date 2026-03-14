# ATM Remote OpenAI Relay Design

## Goal

Expose ATM's local OpenAI-compatible gateway to a small external user group without moving the actual upstream execution off the local Mac.

The local ATM process must continue to:

- run on `127.0.0.1:8766`
- use the local VPN/network environment
- select and execute requests against the existing Codex/OpenAI and Augment backends

The relay server should become a public HTTPS entrypoint that forwards traffic back to the local ATM instance.

## Approved Direction

Use a reverse-tunnel relay architecture:

`external client -> https://<public-domain>/v1 -> nginx on the relay host -> reverse tunnel -> local ATM http://127.0.0.1:8766/v1`

The Ubuntu server is the public ingress layer.

The local ATM instance remains the actual API gateway and keeps ownership of:

- external API-key validation
- backend routing
- account pool selection
- upstream execution

## Scope

This slice includes:

- multi-key external access support for the ATM OpenAI-compatible gateway
- a public relay endpoint design for `https://<public-domain>/v1`
- deployment assets for Ubuntu nginx
- a local reverse-tunnel runner that connects the Mac to Ubuntu
- UI updates so ATM can show local/public base URLs and manage multiple keys

## Non-Goals

- no direct public binding of ATM on `0.0.0.0`
- no migration of upstream execution to Ubuntu
- no custom external auth service on Ubuntu in the first slice
- no end-to-end encryption past Ubuntu TLS termination in the first slice
- no per-user billing or quota tracking in the first slice

## Why This Architecture Fits The Constraints

The user's key constraint is that the local Mac has the working VPN path, while the Ubuntu server does not.

That rules out moving the real OpenAI/Augment execution to Ubuntu.

At the same time, external users want a simple public endpoint and do not want to install a private-network client.

The relay model satisfies both constraints:

- public users talk to `<public-domain>`
- Ubuntu handles HTTPS and request ingress
- the local Mac still makes the real upstream calls

## Security Model

### ATM Stays Local-Only

ATM continues to bind only to `127.0.0.1:8766`.

This prevents direct Internet access to the Tauri-hosted API server even if public relay configuration is wrong elsewhere.

### Public HTTPS Terminates On Ubuntu

Clients connect to:

- `https://<public-domain>/v1/models`
- `https://<public-domain>/v1/responses`
- `https://<public-domain>/v1/chat/completions`

nginx on Ubuntu terminates TLS and proxies only those OpenAI-compatible paths to the reverse tunnel target.

### External API Keys Are Validated In ATM

ATM already has a gateway access-profile store.

The first version should extend it so multiple enabled keys can point at the same target, especially `codex`.

Each external user gets a dedicated key.

This enables:

- per-user key rotation
- per-user revocation
- simple attribution without introducing a second auth system

### Public Path Allowlist

Ubuntu should expose only:

- `GET /v1/models`
- `POST /v1/responses`
- `POST /v1/chat/completions`

All `/api/*`, pool management routes, and internal helper routes stay private.

### Public Ingress Hardening

Ubuntu nginx should enforce:

- request body size limit
- rate limiting by IP
- low idle/read timeout limits
- basic request logging
- no proxying of unsupported paths

### Tunnel Hardening

The reverse tunnel should:

- use SSH public-key auth only
- bind the remote forwarded port to `127.0.0.1`
- not expose the forwarded port directly on Ubuntu
- auto-reconnect when the network drops

## ATM Data Model Changes

The current `gateway_access_profiles.json` model already stores a list of profiles, but the Codex/Augment command layers currently collapse each target to one key.

The approved change is:

- keep the shared `profiles` array
- allow multiple enabled profiles with the same `target`
- stop deleting all existing `codex` profiles when saving a new Codex key
- add CRUD-style commands for listing, creating, updating, and deleting gateway access profiles

Recommended profile shape remains:

```json
{
  "id": "codex-user-a",
  "name": "Alice",
  "target": "codex",
  "api_key": "sk-...",
  "enabled": true
}
```

This is intentionally minimal.

The first slice does not require quotas, tags, or advanced role policy.

## UI Changes

The Codex/OpenAI service panel should become the main control surface for public sharing.

It should show:

- local base URL: `http://127.0.0.1:8766/v1`
- public base URL: `https://<public-domain>/v1`
- list of external access keys
- create / rotate / disable / delete actions
- copy-ready snippets for downstream clients

The Augment panel can remain focused on the Augment sidecar and account health.

## Tunnel Runtime

The local Mac should manage a reverse tunnel process similar to the Augment sidecar lifecycle pattern.

Recommended tunnel direction:

- Ubuntu loopback port `19090`
- forwarded to local `127.0.0.1:8766`

Example shape:

```bash
ssh -NT \
  -o ServerAliveInterval=30 \
  -o ServerAliveCountMax=3 \
  -o ExitOnForwardFailure=yes \
  -R 127.0.0.1:19090:127.0.0.1:8766 \
  "$ATM_RELAY_HOST"
```

nginx then proxies `https://<public-domain>/v1/*` to `http://127.0.0.1:19090/v1/*`.

## Ubuntu Ingress Behavior

Ubuntu nginx should:

- terminate TLS for `<public-domain>`
- proxy only supported `/v1/*` paths to `127.0.0.1:19090`
- return `404` for everything else under the relay server block
- keep existing unrelated sites untouched

## Reliability Expectations

The first slice should be reliable enough for a small private group:

- tunnel auto-reconnect support
- nginx config test before reload
- local status surfaced in ATM
- clear distinction between local endpoint and public endpoint

## Testing Strategy

The implementation should add automated coverage for:

- multi-key gateway profile resolution
- CRUD operations for gateway profiles
- Codex server acceptance of any enabled matching key
- public URL generation for local/public base URLs

Manual validation should cover:

- local `/v1/models` still works
- public `https://<public-domain>/v1/models` works through the tunnel
- disabled key is rejected
- unsupported public path is rejected by nginx

## Rollout Order

1. Generalize ATM gateway profiles to multi-key management.
2. Update the OpenAI/Codex panel to manage and display the keys.
3. Add local reverse-tunnel management.
4. Deploy nginx relay config to Ubuntu.
5. Validate public traffic end-to-end with one external key.
