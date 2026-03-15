# cliproxy Sidecar Workspace

This is the in-repo `cliproxy` sidecar workspace used by ATM. ATM owns the desktop UI and sidecar orchestration; this workspace keeps only the Go translation backend that ATM builds into `src-tauri/resources/cliproxy-server`.

## Layout

- `apps/server-go`: Go backend used by ATM
- `docs/plans`: protocol and migration notes that still matter to the backend
- `scripts`: lightweight workspace smoke checks

## Backend scope

- Supported login flows: `-auggie-login`, `-antigravity-login`
- Supported runtime providers: `auggie`, `antigravity`
- Removed from the active entry surface: standalone frontend apps, Darwin packaging, web token console, TUI mode, and non-target provider login commands

## Build from ATM root

```bash
npm run build:cliproxy
```

That command compiles:

- source: `sidecars/cliproxy/apps/server-go`
- output: `src-tauri/resources/cliproxy-server`

You can also build the backend directly:

```bash
cd apps/server-go
go build -trimpath -o cli-proxy-api ./cmd/server
```

## Local Auggie Responses Smoke Test

The quickest way to validate the OpenAI-compatible surface is to run the Go backend locally and hit `POST /v1/responses`.

From the repository root:

```bash
cd apps/server-go
cp config.example.yaml config.yaml
```

Then set at least these fields in `config.yaml`:

```yaml
host: "127.0.0.1"
port: 8317
auth-dir: "~/.cli-proxy-api"
api-keys:
  - "sk-local-auggie-test"
```

Complete Auggie upstream authentication in one of these ways:

- Official Auggie CLI session: `npm install -g @augmentcode/auggie` then `auggie login`
- Proxy-managed login: `go run ./cmd/server -config ./config.yaml -auggie-login`

Start the backend:

```bash
go run ./cmd/server -config ./config.yaml
```

Then verify models and `responses`:

```bash
curl -sS http://127.0.0.1:8317/v1/models \
  -H "Authorization: Bearer sk-local-auggie-test" | jq '.data[].id'

curl -sS http://127.0.0.1:8317/v1/responses \
  -H "Authorization: Bearer sk-local-auggie-test" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-5.4","input":"hi"}' | jq '.'
```

Important:

- The `Authorization: Bearer ...` value used by `curl` is the local proxy API key from `config.yaml`.
- It is not the Auggie upstream access token.
- Detailed Auggie auth, runtime file locations, and troubleshooting are documented in [apps/server-go/README.md](/Users/jqwang/05-api-代理/augment-token-mng/sidecars/cliproxy/apps/server-go/README.md).
