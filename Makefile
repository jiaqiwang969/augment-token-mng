.PHONY: help dev tauri-dev-full frontend-dev cliproxy test relay-start relay-stop relay-check

help:
	@printf '%s\n' \
		'make dev             Start ATM in normal desktop dev mode (reuses existing Vite on :1420)' \
		'make tauri-dev-full  Start Tauri dev without skipping cliproxy rebuilds' \
		'make frontend-dev    Start Vite only' \
		'make cliproxy        Rebuild the Go cliproxy sidecar' \
		'make test            Run the lightweight Node regression tests' \
		'make relay-start     Start the reverse SSH relay' \
		'make relay-stop      Stop the reverse SSH relay' \
		'make relay-check     Check local, remote-loopback, and public relay health'

dev:
	ATM_SKIP_CLIPROXY_BUILD=1 npx tauri dev

tauri-dev-full:
	npx tauri dev

frontend-dev:
	npm run dev

cliproxy:
	npm run build:cliproxy

test:
	node --test tests/tauriBridge.test.js tests/ensureViteDev.test.js

relay-start:
	./scripts/start_remote_relay.sh

relay-stop:
	./scripts/stop_remote_relay.sh

relay-check:
	./scripts/check_remote_relay.sh
