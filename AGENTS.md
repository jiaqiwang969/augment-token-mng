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
- Never print real Antigravity OAuth client IDs or client secrets to terminal output, logs, screenshots, or copied documentation.
- Keep `.env.antigravity` ignored by git and update `.env.antigravity.example` whenever the required env names or setup flow changes.
- Before commit or push, check staged diffs for `ATM_ANTIGRAVITY_OAUTH_` and `CLIPROXY_ANTIGRAVITY_OAUTH_` values to make sure only placeholders or compatibility references are present.

## Regression Checks
- If you change Antigravity env loading, update `tests/antigravityEnv.test.js` in the same change.
- If you change the local startup workflow, keep the `make dev` and `make tauri-dev-full` targets aligned with `scripts/load_antigravity_env.sh`.
