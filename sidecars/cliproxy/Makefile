.PHONY: build test smoke

build:
	cd apps/server-go && go build ./cmd/server

test:
	cd apps/server-go && go test ./internal/translator ./internal/cmd ./sdk/auth ./sdk/cliproxy

smoke:
	bash ./scripts/smoke.sh
