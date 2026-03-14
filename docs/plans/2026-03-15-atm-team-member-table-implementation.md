# ATM Team Member Table Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the current Codex team card wall with a simpler management flow built from one primary trend chart, one unified member table, and a modal for adding or editing team members.

**Architecture:** Keep `GatewayAccessProfile` as the only source of truth for team members. Extend the existing frontend helper layer so chart visibility follows selected table rows, then refactor `CodexServerDialog.vue` to render a single roster table and modal-driven editing instead of fixed built-in team cards plus a separate custom-key section.

**Tech Stack:** Vue 3 `<script setup>`, node:test, vite, vue-i18n, Tauri commands already present in Rust

---

### Task 1: Add Failing Tests For Member-Table Selection Helpers

**Files:**
- Modify: `src/utils/codexTeamUi.js`
- Modify: `tests/codexTeamUi.test.js`

**Step 1: Write the failing test**

Add focused tests for:

- filtering chart series by selected profile ids
- falling back to all series when no table rows are selected

Example target:

```js
test('filterTeamSeriesBySelectedProfiles keeps only selected profile ids', async () => {
  const { filterTeamSeriesBySelectedProfiles } = await loadModule()

  const output = filterTeamSeriesBySelectedProfiles(
    [{ profileId: 'a' }, { profileId: 'b' }],
    ['b']
  )

  assert.deepEqual(output.map(entry => entry.profileId), ['b'])
})
```

**Step 2: Run test to verify it fails**

Run:

```bash
node --test tests/codexTeamUi.test.js
```

Expected: FAIL because the new helper does not exist yet.

**Step 3: Commit**

```bash
git add src/utils/codexTeamUi.js tests/codexTeamUi.test.js
git commit -m "test(team-ui): cover member table selection helpers"
```

### Task 2: Implement The Minimal Selection Helper

**Files:**
- Modify: `src/utils/codexTeamUi.js`
- Modify: `tests/codexTeamUi.test.js`

**Step 1: Write minimal implementation**

Add `filterTeamSeriesBySelectedProfiles(series, selectedProfileIds)` and treat an empty selection as "show all".

**Step 2: Run test to verify it passes**

Run:

```bash
node --test tests/codexTeamUi.test.js
```

Expected: PASS

**Step 3: Commit**

```bash
git add src/utils/codexTeamUi.js tests/codexTeamUi.test.js
git commit -m "feat(team-ui): add table-driven series filtering"
```

### Task 3: Replace The Team Cards With One Unified Member Table

**Files:**
- Modify: `src/components/openai/CodexServerDialog.vue`
- Modify: `src/locales/zh-CN.js`
- Modify: `src/locales/en-US.js`

**Step 1: Build the table view model**

Derive one member-row list from Codex gateway profiles and analytics. Stop splitting the UI into fixed built-in team cards vs custom keys in the overview.

**Step 2: Render the simpler overview**

Replace the current team card wall and separate ranking block with:

- a compact toolbar
- one chart
- one roster table

Make row selection drive chart filtering.

**Step 3: Run focused verification**

Run:

```bash
npm run build
```

Expected: PASS

**Step 4: Commit**

```bash
git add src/components/openai/CodexServerDialog.vue src/locales/zh-CN.js src/locales/en-US.js
git commit -m "feat(team-ui): replace team cards with member table"
```

### Task 4: Add Modal-Driven Member Creation And Editing

**Files:**
- Create: `src/components/openai/CodexTeamMemberEditorModal.vue`
- Modify: `src/components/openai/CodexServerDialog.vue`
- Modify: `src/locales/zh-CN.js`
- Modify: `src/locales/en-US.js`

**Step 1: Add the modal component**

Implement a compact modal for member creation and editing using the existing Codex gateway profile commands.

**Step 2: Wire table actions**

Support:

- add member
- edit member
- regenerate key
- copy access bundle
- enable/disable member
- delete member

**Step 3: Run focused verification**

Run:

```bash
node --test tests/codexTeamUi.test.js
npm run build
```

Expected: PASS

**Step 4: Commit**

```bash
git add src/components/openai/CodexTeamMemberEditorModal.vue src/components/openai/CodexServerDialog.vue src/locales/zh-CN.js src/locales/en-US.js
git commit -m "feat(team-ui): add modal-based member editing"
```

### Task 5: Verify The Full Frontend Slice

**Files:**
- Modify: `tests/codexTeamUi.test.js`
- Modify: `src/components/openai/CodexServerDialog.vue`

**Step 1: Run the relevant frontend tests**

Run:

```bash
node --test tests/codexTeamUi.test.js tests/tauriBridge.test.js tests/ensureViteDev.test.js
```

Expected: PASS

**Step 2: Run build verification**

Run:

```bash
npm run build
```

Expected: PASS

**Step 3: Commit**

```bash
git add tests/codexTeamUi.test.js src/components/openai/CodexServerDialog.vue
git commit -m "test(team-ui): verify member table flow"
```
