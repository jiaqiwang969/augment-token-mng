.PHONY: help dev tauri-dev-full frontend-dev cliproxy test antigravity-env-bootstrap antigravity-env-check relay-start relay-stop relay-check deploy deploy-check

help:
	@printf '%s\n' \
		'make dev             Start ATM in normal desktop dev mode (auto-loads .env.antigravity if present)' \
		'make tauri-dev-full  Start Tauri dev without skipping cliproxy rebuilds (auto-loads .env.antigravity)' \
		'make antigravity-env-bootstrap  Materialize .env.antigravity from local env or trusted git history' \
		'make antigravity-env-check      Validate Antigravity OAuth env setup without printing secrets' \
		'make frontend-dev    Start Vite only' \
		'make cliproxy        Rebuild the Go cliproxy sidecar' \
		'make test            Run the lightweight Node regression tests' \
		'make deploy          Apply remote nginx relay config, start the tunnel, and verify health' \
		'make deploy-check    Verify the relay end to end using .env.relay' \
		'make relay-start     Start the reverse SSH relay' \
		'make relay-stop      Stop the reverse SSH relay' \
		'make relay-check     Check local, remote-loopback, and public relay health'

dev:
	bash -lc 'source "./scripts/load_antigravity_env.sh" && load_antigravity_env && ATM_SKIP_CLIPROXY_BUILD=1 npx tauri dev'

tauri-dev-full:
	bash -lc 'source "./scripts/load_antigravity_env.sh" && load_antigravity_env && npx tauri dev'

antigravity-env-bootstrap:
	bash ./scripts/bootstrap_antigravity_env.sh

antigravity-env-check:
	bash ./scripts/check_antigravity_env.sh

frontend-dev:
	npm run dev

cliproxy:
	npm run build:cliproxy

test:
	node --test \
		tests/tauriBridge.test.js \
		tests/ensureViteDev.test.js \
		tests/relayConfig.test.js \
		tests/antigravityEnv.test.js \
		tests/antigravityServerDialog.test.js
	node tests/antigravity-api-service-ui.test.mjs

deploy:
	node scripts/deploy_remote_relay.mjs

deploy-check:
	./scripts/check_remote_relay.sh

relay-start:
	./scripts/start_remote_relay.sh

relay-stop:
	./scripts/stop_remote_relay.sh

relay-check:
	./scripts/check_remote_relay.sh
