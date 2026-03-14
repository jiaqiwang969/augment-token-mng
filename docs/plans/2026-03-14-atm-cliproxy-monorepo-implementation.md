# ATM CLIProxy Monorepo Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Move the CLIProxy Go source tree into `augment-token-mng`, compile `cliproxy-server` from local source during builds, and keep the current sidecar runtime model unchanged.

**Architecture:** ATM remains the control plane and still launches `cliproxy-server` as a sidecar. The difference is that the sidecar binary will now be built from Go source stored inside this repository under `sidecars/cliproxy`, instead of being maintained as an external project and copied in by hand.

**Tech Stack:** Rust, Tauri 2, Cargo build scripts, Bash, Go 1.21+, existing CLIProxyAPI Go codebase

---

### Task 1: Vendor The Go Sidecar Source Tree

**Files:**
- Create: `sidecars/cliproxy/` (vendor the existing `CLIProxyAPI-wjq` source tree here)
- Modify: `.gitignore`

**Step 1: Write the failing test**

Run:

```bash
test -f sidecars/cliproxy/apps/server-go/go.mod
```

Expected: FAIL because the Go source tree is not in this repository yet.

**Step 2: Copy the source tree**

Copy the current working Go repo into the new in-repo location, preserving layout so existing imports and scripts keep working:

```bash
mkdir -p sidecars
rsync -a --delete /Users/jqwang/05-api-代理/temp/CLIProxyAPI-wjq/ sidecars/cliproxy/
```

Then add ignore rules for Go-side generated files that should not be tracked from the vendored tree, for example:

- `sidecars/cliproxy/apps/server-go/cli-proxy-api`
- `sidecars/cliproxy/apps/server-go/logs/`
- `sidecars/cliproxy/apps/server-go/data/`
- `sidecars/cliproxy/apps/server-go/config.yaml`

**Step 3: Run test to verify it passes**

Run:

```bash
test -f sidecars/cliproxy/apps/server-go/go.mod
go test ./internal/runtime/executor -run 'TestAuggieResponses_(UsesStoredConversationStateForPreviousResponseID|PreviousResponseContinuationRequiresStoredPriorResponse|PreviousResponseReplayStripsRequiredToolDirectiveFromStoredMessage)' -count=1
```

Working directory for the `go test` command:

```bash
sidecars/cliproxy/apps/server-go
```

Expected: PASS for the targeted continuation tests.

**Step 4: Verify the vendored tree is clean**

Run:

```bash
git status --short sidecars/cliproxy .gitignore
```

Expected: only source files and ignore-rule changes are staged or modified, no generated binaries/logs/data files.

**Step 5: Commit**

```bash
git add .gitignore sidecars/cliproxy
git commit -m "chore(cliproxy): vendor go sidecar source" -m "Co-authored-by: Codex <noreply@openai.com>"
```

### Task 2: Add A Deterministic Sidecar Build Script

**Files:**
- Create: `scripts/build-cliproxy.sh`
- Modify: `.gitignore`

**Step 1: Write the failing test**

Run:

```bash
test -x scripts/build-cliproxy.sh
```

Expected: FAIL because the build script does not exist yet.

**Step 2: Write minimal implementation**

Create `scripts/build-cliproxy.sh` with these behaviors:

- fail fast on shell errors
- locate repo root relative to the script
- build from `sidecars/cliproxy/apps/server-go`
- compile `./cmd/server/main.go`
- write the output to `src-tauri/resources/cliproxy-server`
- accept `GOOS` and `GOARCH` from the environment when provided
- infer a sane default for local macOS development when not provided
- print the effective source path, target, and output path

Also add ignore rules for any temporary build output the script creates, if needed.

**Step 3: Run test to verify it passes**

Run:

```bash
bash -n scripts/build-cliproxy.sh
scripts/build-cliproxy.sh
file src-tauri/resources/cliproxy-server
```

Expected:

- `bash -n` passes
- the script exits successfully
- `file` reports a native executable for the current machine

**Step 4: Verify the script uses vendored source**

Run:

```bash
rg -n "sidecars/cliproxy/apps/server-go|src-tauri/resources/cliproxy-server" scripts/build-cliproxy.sh
```

Expected: the script only references the in-repo source tree and the in-repo output path.

**Step 5: Commit**

```bash
git add scripts/build-cliproxy.sh .gitignore src-tauri/resources/cliproxy-server
git commit -m "build(cliproxy): add in-repo sidecar build script" -m "Co-authored-by: Codex <noreply@openai.com>"
```

### Task 3: Wire Tauri Build To Compile The Sidecar Automatically

**Files:**
- Modify: `src-tauri/build.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `package.json`

**Step 1: Write the failing test**

Run:

```bash
rg -n "build-cliproxy|ATM_SKIP_CLIPROXY_BUILD|cargo:rerun-if-changed=../sidecars/cliproxy" src-tauri/build.rs package.json src-tauri/Cargo.toml
```

Expected: FAIL or return no matches because automatic sidecar build hooks are not wired yet.

**Step 2: Write minimal implementation**

Update `src-tauri/build.rs` so it:

- emits `cargo:rerun-if-changed` for:
  - `../sidecars/cliproxy`
  - `../scripts/build-cliproxy.sh`
- reads Cargo target env vars such as:
  - `CARGO_CFG_TARGET_OS`
  - `CARGO_CFG_TARGET_ARCH`
- maps them to `GOOS` / `GOARCH`
- invokes `../scripts/build-cliproxy.sh`
- supports an escape hatch such as `ATM_SKIP_CLIPROXY_BUILD=1`
- fails the build with a clear error if the script or Go compile step fails

Update `package.json` to add an explicit helper script such as:

```json
"build:cliproxy": "./scripts/build-cliproxy.sh"
```

If `src-tauri/Cargo.toml` needs extra build-time dependencies for path handling or command execution, add them there.

**Step 3: Run test to verify it passes**

Run:

```bash
rm -f src-tauri/resources/cliproxy-server
cargo build --manifest-path src-tauri/Cargo.toml
test -x src-tauri/resources/cliproxy-server
```

Expected:

- Cargo build succeeds
- the sidecar binary is recreated automatically
- the recreated binary is executable

**Step 4: Verify opt-out still works**

Run:

```bash
ATM_SKIP_CLIPROXY_BUILD=1 cargo build --manifest-path src-tauri/Cargo.toml
```

Expected: Cargo build succeeds without trying to rebuild the sidecar.

**Step 5: Commit**

```bash
git add src-tauri/build.rs src-tauri/Cargo.toml package.json src-tauri/resources/cliproxy-server
git commit -m "build(cliproxy): wire tauri build to compile sidecar" -m "Co-authored-by: Codex <noreply@openai.com>"
```

### Task 4: Stop Treating The Binary As Hand-Maintained Source

**Files:**
- Modify: `.gitignore`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `docs/plans/2026-03-14-atm-cliproxy-monorepo-design.md`
- Modify: `docs/plans/2026-03-14-atm-augment-sidecar-integration.md`

**Step 1: Write the failing test**

Run:

```bash
rg -n "手工|手动|manual|precompiled|预编译" docs/plans/2026-03-14-atm-cliproxy-monorepo-design.md docs/plans/2026-03-13-atm-augment-sidecar-integration.md
```

Expected: matches still describe the binary as precompiled or manually copied.

**Step 2: Write minimal implementation**

Update docs so they consistently describe the new source-of-truth model:

- Go source lives in `sidecars/cliproxy`
- `src-tauri/resources/cliproxy-server` is a generated build artifact
- Tauri still bundles the binary from the same resource path

Add or adjust ignore rules as needed if the binary should no longer be hand-edited.

Do not remove `src-tauri/tauri.conf.json` resource registration unless Tauri bundling is migrated to a different supported mechanism.

**Step 3: Run test to verify it passes**

Run:

```bash
rg -n "sidecars/cliproxy|generated build artifact|build-cliproxy" docs/plans/2026-03-14-atm-cliproxy-monorepo-design.md docs/plans/2026-03-13-atm-augment-sidecar-integration.md
rg -n "resources/cliproxy-server" src-tauri/tauri.conf.json
```

Expected:

- docs describe the new flow accurately
- Tauri still bundles `resources/cliproxy-server`

**Step 4: Smoke-check repository cleanliness**

Run:

```bash
git status --short
```

Expected: no unexpected generated junk outside the intentionally tracked changes.

**Step 5: Commit**

```bash
git add .gitignore src-tauri/tauri.conf.json docs/plans/2026-03-14-atm-cliproxy-monorepo-design.md docs/plans/2026-03-13-atm-augment-sidecar-integration.md
git commit -m "docs(cliproxy): update monorepo build and bundle flow" -m "Co-authored-by: Codex <noreply@openai.com>"
```

### Task 5: Validate The Integrated Build And Runtime Contract

**Files:**
- Verify: `src-tauri/src/platforms/augment/sidecar.rs`
- Verify: `src-tauri/src/platforms/augment/proxy_server.rs`
- Verify: `sidecars/cliproxy/apps/server-go/internal/runtime/executor/auggie_executor.go`

**Step 1: Write the failing test**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml sidecar_ -- --nocapture
```

Expected: if the migration broke binary lookup or sidecar assumptions, these tests fail and show where the contract drifted.

**Step 2: Run Go translation smoke tests**

Run:

```bash
go test ./internal/runtime/executor -run 'TestAuggieResponses_(UsesStoredConversationStateForPreviousResponseID|PreviousResponseContinuationRequiresStoredPriorResponse)' -count=1
go test ./internal/runtime/executor -run 'TestAuggieCountTokens_ReturnsTranslatedOpenAIUsage' -count=1
```

Working directory:

```bash
sidecars/cliproxy/apps/server-go
```

Expected: PASS

**Step 3: Run Rust gateway smoke tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml unified_ -- --nocapture
```

Expected: PASS for unified `/v1` gateway routing and sidecar proxy tests.

**Step 4: Manual binary sanity check**

Run:

```bash
src-tauri/resources/cliproxy-server -help >/tmp/cliproxy-help.txt 2>&1 || true
test -s /tmp/cliproxy-help.txt
```

Expected: the built binary launches and prints help or usage text.

**Step 5: Commit**

```bash
git add sidecars/cliproxy scripts/build-cliproxy.sh src-tauri/build.rs package.json .gitignore docs/plans/2026-03-13-atm-augment-sidecar-integration.md docs/plans/2026-03-14-atm-cliproxy-monorepo-design.md
git commit -m "feat(cliproxy): migrate sidecar source into atm monorepo" -m "Co-authored-by: Codex <noreply@openai.com>"
```
