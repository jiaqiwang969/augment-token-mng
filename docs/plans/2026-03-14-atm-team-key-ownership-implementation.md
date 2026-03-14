# ATM Team Key Ownership Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Turn Codex gateway key management into a team-centric system where the built-in 10-person squad each owns one readable dedicated key and all logs, filters, and charts are grouped by person.

**Architecture:** Reuse the existing `GatewayAccessProfile -> gateway auth -> request logging -> daily stats` chain. Extend the shared profile schema with person metadata, add a built-in team preset catalog plus readable key generation, persist member identity into request logs, and upgrade the existing Codex panel to manage people instead of anonymous keys.

**Tech Stack:** Rust, Tauri 2 commands, serde/serde_json, rusqlite, warp, Vue 3 `<script setup>`, Chart.js, vite

---

### Task 1: Add Failing Tests For Team Metadata And Readable Key Generation

**Files:**
- Modify: `src-tauri/src/core/gateway_access.rs`
- Modify: `src-tauri/src/platforms/openai/codex/commands.rs`
- Create: `src-tauri/src/platforms/openai/codex/team_profiles.rs`
- Modify: `src-tauri/src/platforms/openai/codex/mod.rs`

**Step 1: Write the failing tests**

Add focused tests for:

- a Codex profile can carry `member_code`, `role_title`, `persona_summary`, `color`, and `notes`
- team key generation produces `sk-team-jdd-xxxxxxxx`
- team template import is idempotent by `member_code`

Example target:

```rust
#[test]
fn generate_team_gateway_api_key_uses_member_code_prefix() {
    let key = generate_team_gateway_api_key("jdd");

    assert!(key.starts_with("sk-team-jdd-"));
    assert_eq!(key.len(), "sk-team-jdd-".len() + 8);
}
```

**Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml team_gateway gateway_access -- --nocapture
```

Expected: FAIL because the shared profile schema and helper module do not yet support team metadata or readable key generation.

**Step 3: Commit**

```bash
git add src-tauri/src/core/gateway_access.rs src-tauri/src/platforms/openai/codex/commands.rs src-tauri/src/platforms/openai/codex/team_profiles.rs src-tauri/src/platforms/openai/codex/mod.rs
git commit -m "test(team): cover member metadata and readable keys"
```

### Task 2: Implement Team Presets And Enriched Gateway Profiles

**Files:**
- Modify: `src-tauri/src/core/gateway_access.rs`
- Create: `src-tauri/src/platforms/openai/codex/team_profiles.rs`
- Modify: `src-tauri/src/platforms/openai/codex/mod.rs`
- Modify: `src-tauri/src/platforms/openai/codex/commands.rs`

**Step 1: Extend the shared profile schema**

Add optional fields to `GatewayAccessProfile`:

- `member_code`
- `role_title`
- `persona_summary`
- `color`
- `notes`

Keep them optional for backward compatibility.

**Step 2: Add the built-in 10-member preset catalog**

Create `team_profiles.rs` with a stable preset list that includes:

- name
- member code
- role title
- persona summary
- default color

**Step 3: Add a readable key generator**

Implement `generate_team_gateway_api_key(member_code: &str) -> String` and keep the existing generic generator for non-team cases.

**Step 4: Run tests to verify they pass**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml team_gateway gateway_access -- --nocapture
```

Expected: PASS for the new schema and helper behavior.

**Step 5: Commit**

```bash
git add src-tauri/src/core/gateway_access.rs src-tauri/src/platforms/openai/codex/team_profiles.rs src-tauri/src/platforms/openai/codex/mod.rs src-tauri/src/platforms/openai/codex/commands.rs
git commit -m "feat(team): add member metadata and built-in presets"
```

### Task 3: Add Failing Tests For Team Import And Member Management Commands

**Files:**
- Modify: `src-tauri/src/platforms/openai/codex/commands.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Write the failing tests**

Cover:

- importing the built-in 10-member squad creates exactly 10 Codex profiles
- importing twice does not duplicate members
- regenerating one member key preserves `member_code` and `name`
- deleting a built-in member is rejected or avoided by design

Example target:

```rust
#[test]
fn import_codex_team_template_is_idempotent() {
    let mut profiles = GatewayAccessProfiles::default();

    profiles = import_team_template_into_profiles(profiles);
    profiles = import_team_template_into_profiles(profiles);

    assert_eq!(profiles.list_by_target(GatewayTarget::Codex).len(), 10);
}
```

**Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_team import_codex_team -- --nocapture
```

Expected: FAIL because the team import and member-specific management commands do not exist yet.

**Step 3: Commit**

```bash
git add src-tauri/src/platforms/openai/codex/commands.rs src-tauri/src/lib.rs
git commit -m "test(team): cover team import and member commands"
```

### Task 4: Implement Member-Centric Commands And Tauri Wiring

**Files:**
- Modify: `src-tauri/src/platforms/openai/codex/commands.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Reuse the current profile CRUD path**

Extend the current Codex gateway profile commands so their payloads can carry the new metadata.

**Step 2: Add dedicated team commands**

Add commands such as:

- import the built-in 10-member template
- regenerate a member key
- reset one member to template defaults
- update notes, role title, and color

**Step 3: Keep backward compatibility**

Existing custom profiles should continue to work even without `member_code`.

**Step 4: Run tests to verify they pass**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_team import_codex_team -- --nocapture
```

Expected: PASS for import, idempotency, and member-key regeneration.

**Step 5: Commit**

```bash
git add src-tauri/src/platforms/openai/codex/commands.rs src-tauri/src/lib.rs
git commit -m "feat(team): add member-centric gateway management commands"
```

### Task 5: Add Failing Tests For Member Metadata In Logs And Stats

**Files:**
- Modify: `src-tauri/src/platforms/openai/codex/models.rs`
- Modify: `src-tauri/src/platforms/openai/codex/storage.rs`
- Modify: `src-tauri/src/platforms/openai/codex/server.rs`

**Step 1: Write the failing tests**

Cover:

- `RequestLog` can persist `member_code`, `role_title`, `display_label`, and `api_key_suffix`
- storage query can filter logs by one member
- profile-grouped daily stats include enough member identity for the chart

Example target:

```rust
#[test]
fn codex_storage_groups_daily_stats_by_member_identity() {
    let storage = create_test_storage();
    seed_log(&storage, Some("jdd"), Some("姜大大"), Some("#4c6ef5"));

    let response = storage.get_daily_stats_by_gateway_profile(30).unwrap();

    assert_eq!(response.series[0].profile_name, "姜大大");
    assert_eq!(response.series[0].member_code.as_deref(), Some("jdd"));
}
```

**Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_storage member_daily_stats query_logs -- --nocapture
```

Expected: FAIL because the log model and storage schema do not yet persist or aggregate member metadata.

**Step 3: Commit**

```bash
git add src-tauri/src/platforms/openai/codex/models.rs src-tauri/src/platforms/openai/codex/storage.rs src-tauri/src/platforms/openai/codex/server.rs
git commit -m "test(team): cover member metadata in logs and stats"
```

### Task 6: Implement Member-Aware Logging, Filtering, And Aggregation

**Files:**
- Modify: `src-tauri/src/platforms/openai/codex/models.rs`
- Modify: `src-tauri/src/platforms/openai/codex/storage.rs`
- Modify: `src-tauri/src/platforms/openai/codex/server.rs`
- Modify: `src-tauri/src/platforms/openai/codex/commands.rs`

**Step 1: Extend the log model**

Add optional fields to `RequestLog` and the daily-series response for:

- `member_code`
- `role_title`
- `display_label`
- `api_key_suffix`
- `color`

**Step 2: Extend the SQLite schema**

Add matching columns and indexes in `codex_requests`, plus query support in `LogQuery` for member filtering.

**Step 3: Populate member metadata at request write time**

When the gateway resolves the matching profile, write the person's metadata directly into the log row.

**Step 4: Enrich stats responses**

Return member name, member code, and color alongside each per-member time series so the UI can render stable labels and colors without re-deriving them.

**Step 5: Run tests to verify they pass**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml codex_storage member_daily_stats query_logs -- --nocapture
```

Expected: PASS for persistence, filtering, and member-grouped aggregation.

**Step 6: Commit**

```bash
git add src-tauri/src/platforms/openai/codex/models.rs src-tauri/src/platforms/openai/codex/storage.rs src-tauri/src/platforms/openai/codex/server.rs src-tauri/src/platforms/openai/codex/commands.rs
git commit -m "feat(team): persist and aggregate member usage metadata"
```

### Task 7: Upgrade The Codex UI From Key CRUD To Team-Member Management

**Files:**
- Modify: `src/components/openai/CodexServerDialog.vue`
- Modify: `src/components/openai/CodexUsageChart.vue`
- Modify: `src/locales/en-US.js`
- Modify: `src/locales/zh-CN.js`

**Step 1: Replace anonymous key rows with member-oriented rows**

Show:

- name
- member code
- role title
- enabled state
- key input and regenerate button
- notes
- current usage summary

**Step 2: Add the team-template entry point**

Expose an obvious action to import the built-in 10-member squad when none exists.

**Step 3: Add person-oriented visualization**

Update the chart and ranking area so they render one series per member with stable colors and labels.

**Step 4: Add member filters to the logs tab**

Allow the operator to filter logs by one person.

**Step 5: Verify the UI**

Run:

```bash
npm run build
```

Expected: PASS. Because there is no dedicated Vue test harness for this screen today, use build success plus a manual walkthrough of:

- import template
- regenerate one member key
- disable one member
- filter logs by one member
- confirm the trend chart shows member names and colors

**Step 6: Commit**

```bash
git add src/components/openai/CodexServerDialog.vue src/components/openai/CodexUsageChart.vue src/locales/en-US.js src/locales/zh-CN.js
git commit -m "feat(team): add person-centric gateway UI and analytics"
```

### Task 8: Full Verification

**Files:**
- Verify only

**Step 1: Run Rust tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: PASS

**Step 2: Run lightweight Node tests**

```bash
make test
```

Expected: PASS

**Step 3: Run frontend build**

```bash
npm run build
```

Expected: PASS

**Step 4: Manual verification**

Check:

- importing the 10-member squad creates 10 stable profiles
- each member key follows `sk-team-<code>-<random8>`
- logs visibly show member name, code, and role title
- daily trend chart groups by person
- disabled members stop authenticating
- legacy custom profiles still work

**Step 5: Commit**

```bash
git add .
git commit -m "feat(team): complete team-centric gateway management"
```
