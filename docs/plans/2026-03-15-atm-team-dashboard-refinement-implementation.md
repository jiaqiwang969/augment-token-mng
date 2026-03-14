# ATM Team Dashboard Refinement Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Refine the Codex gateway team dashboard so the 10 built-in members are managed through a dropdown detail view, the ranking and trend chart share the same visibility filter, and operators can copy one clipboard bundle containing every member's public access config.

**Architecture:** Keep the backend and current Tauri command surface unchanged. Add one small pure frontend helper module for filtering and export generation, then connect it to `CodexServerDialog.vue` and the existing chart component with minimal template churn.

**Tech Stack:** Vue 3 `<script setup>`, node:test, vite, Chart.js, vue-i18n

---

### Task 1: Add Failing Tests For Team UI Helper Logic

**Files:**
- Create: `src/utils/codexTeamUi.js`
- Create: `tests/codexTeamUi.test.js`

**Step 1: Write the failing test**

Cover:

- chart series are filtered by visible member ids
- ranking rows are filtered by visible member ids
- all-member export text includes the public base URL and one block per member

Example target:

```js
test('buildAllMembersAccessBundle emits one block per member', async () => {
  const text = buildAllMembersAccessBundle({
    baseUrl: 'https://lingkong.xyz/v1',
    profiles: [
      { name: '姜大大', memberCode: 'jdd', apiKey: 'sk-team-jdd-abc12345' }
    ]
  })

  assert.match(text, /OPENAI_BASE_URL=https:\/\/lingkong\.xyz\/v1/)
  assert.match(text, /OPENAI_API_KEY=sk-team-jdd-abc12345/)
})
```

**Step 2: Run test to verify it fails**

Run:

```bash
node --test tests/codexTeamUi.test.js
```

Expected: FAIL because the helper module does not exist yet.

**Step 3: Commit**

```bash
git add src/utils/codexTeamUi.js tests/codexTeamUi.test.js
git commit -m "test(team-ui): cover dashboard filtering helpers"
```

### Task 2: Implement The Minimal Team UI Helper Module

**Files:**
- Create: `src/utils/codexTeamUi.js`
- Modify: `tests/codexTeamUi.test.js`

**Step 1: Write minimal implementation**

Implement these pure functions:

- `filterTeamSeriesByVisibleMembers(series, visibleMemberCodes)`
- `filterMemberRankingByVisibleMembers(rows, visibleMemberCodes)`
- `buildAllMembersAccessBundle({ baseUrl, profiles })`

Keep matching logic member-code based and treat an empty visible-member list as "show all".

**Step 2: Run test to verify it passes**

Run:

```bash
node --test tests/codexTeamUi.test.js
```

Expected: PASS

**Step 3: Commit**

```bash
git add src/utils/codexTeamUi.js tests/codexTeamUi.test.js
git commit -m "feat(team-ui): add filtering and export helpers"
```

### Task 3: Add The Dropdown-Driven Member Detail View

**Files:**
- Modify: `src/components/openai/CodexServerDialog.vue`
- Modify: `src/locales/zh-CN.js`
- Modify: `src/locales/en-US.js`

**Step 1: Add state for selected member**

Add a selected member id ref and compute the current selected member card from `teamMemberCards`.

**Step 2: Replace the 10-card wall**

Change the template so the team section shows:

- member selector dropdown
- export-all button
- one selected member detail card

Keep all existing member actions intact.

**Step 3: Run focused build verification**

Run:

```bash
npm run build
```

Expected: PASS with the new selector UI compiling cleanly.

**Step 4: Commit**

```bash
git add src/components/openai/CodexServerDialog.vue src/locales/zh-CN.js src/locales/en-US.js
git commit -m "feat(team-ui): switch team detail to dropdown view"
```

### Task 4: Add Linked Visibility Filtering For Trend And Ranking

**Files:**
- Modify: `src/components/openai/CodexServerDialog.vue`
- Modify: `src/components/openai/CodexUsageChart.vue`
- Modify: `src/locales/zh-CN.js`
- Modify: `src/locales/en-US.js`

**Step 1: Add shared visible-member state**

Create one visible-member selection for analytics and derive filtered data with the helper module.

**Step 2: Wire both analytics surfaces to the same filter**

Pass filtered chart data into `CodexUsageChart` and render the ranking table from filtered rows only.

**Step 3: Keep chart changes minimal**

Only update `CodexUsageChart.vue` if the current prop contract needs a tiny compatibility adjustment.

**Step 4: Run focused verification**

Run:

```bash
node --test tests/codexTeamUi.test.js
npm run build
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/components/openai/CodexServerDialog.vue src/components/openai/CodexUsageChart.vue src/locales/zh-CN.js src/locales/en-US.js
git commit -m "feat(team-ui): link ranking and trend visibility"
```

### Task 5: Verify The Full Frontend Slice

**Files:**
- Modify: `tests/codexTeamUi.test.js`
- Modify: `src/components/openai/CodexServerDialog.vue`

**Step 1: Run the relevant tests**

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
git commit -m "test(team-ui): verify dashboard refinement"
```
