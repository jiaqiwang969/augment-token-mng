# Repository Notes

## Antigravity OAuth Secrets
- Keep Antigravity OAuth credentials in `.env.antigravity` for local development.
- `make dev` and `make tauri-dev-full` auto-load `${repo_root}/.env` first and `${repo_root}/.env.antigravity` second, so the dedicated file overrides generic local env settings.
- Use `make antigravity-env-bootstrap` to materialize `.env.antigravity` from already-exported local env values or trusted local git history.
- `make antigravity-env-bootstrap` intentionally refuses to overwrite an existing `.env.antigravity`; if the file already exists, treat that refusal as a safety check and run `make antigravity-env-check` instead of forcing a rewrite.
- Use `make antigravity-env-check` before local runs to verify the active Antigravity OAuth setup without printing secret values.
- Preferred variable names are `ATM_ANTIGRAVITY_OAUTH_CLIENT_ID` and `ATM_ANTIGRAVITY_OAUTH_CLIENT_SECRET`.
- Legacy `CLIPROXY_ANTIGRAVITY_OAUTH_CLIENT_ID` and `CLIPROXY_ANTIGRAVITY_OAUTH_CLIENT_SECRET` remain supported only as a compatibility fallback. New docs and new setup instructions should use the `ATM_*` names.

## Dev Startup
- `make dev` may legitimately reuse an already-running ATM Vite dev server on port `1420`; seeing a log that the current dev server was detected is normal and is not, by itself, a startup failure.
- If `make dev` fails with `resource path 'resources/cliproxy-server' doesn't exist`, treat it as a missing packaged sidecar resource under `src-tauri/resources/cliproxy-server`, not as a Vite port problem.
- When checking local startup issues, separate the frontend/Vite state from the Rust/Tauri build state before deciding what failed.

## Commit Hygiene
- Never commit real Antigravity OAuth client IDs, client secrets, or a populated `.env.antigravity` file.
- Never commit a populated `.env.relay` file or real relay API keys.
- Never print real Antigravity OAuth client IDs or client secrets to terminal output, logs, screenshots, or copied documentation.
- Keep `.env.antigravity` ignored by git and update `.env.antigravity.example` whenever the required env names or setup flow changes.
- Keep `.env.relay` ignored by git. Treat it as the source of truth for `make deploy`, and make sure it contains a currently valid gateway key before claiming relay deploy is broken.
- Before commit or push, check staged diffs for `ATM_ANTIGRAVITY_OAUTH_` and `CLIPROXY_ANTIGRAVITY_OAUTH_` values to make sure only placeholders or compatibility references are present.

## Relay Deployment
- `make deploy` now validates both protocol families end to end:
  - OpenAI-compatible relay on `/v1/*`
  - Gemini native relay on `/v1beta/*`
- The public relay host may already have a generic `/v1beta/` gateway block; avoid adding a second broad `/v1beta/` nginx location in the managed ATM relay block. Only proxy the narrower Gemini-native ATM paths:
  - `= /v1beta/models`
  - `^~ /v1beta/models/`
- The local relay health script may use Node, but remote loopback checks must not assume Node is installed on the Ubuntu relay host. Prefer `python3` for remote JSON validation.

## Regression Checks
- If you change Antigravity env loading, update `tests/antigravityEnv.test.js` in the same change.
- If you change relay deployment behavior, update `tests/relayConfig.test.js` in the same change.
- If you change the local startup workflow, keep the `make dev` and `make tauri-dev-full` targets aligned with `scripts/load_antigravity_env.sh`.
