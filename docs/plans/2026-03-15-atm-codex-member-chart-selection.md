# Codex Member Chart Selection Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the Codex member trend chart use explicit default-all selection, remove deleted members from the visible series immediately, and render zero-filled lines instead of blank gaps.

**Architecture:** Keep the behavior in frontend helpers so the dialog stays declarative. One helper will reconcile selected member ids against the current table rows, and another will build chart-ready series by zero-filling missing member/date data for the selected rows. The chart component will treat zero-valued series as valid data.

**Tech Stack:** Vue 3, plain JavaScript helpers, node:test source and utility tests

---

### Task 1: Lock the new selection behavior with tests

**Files:**
- Modify: `tests/codexTeamUi.test.js`
- Modify: `tests/codexServerDialog.test.js`
- Create: `tests/codexUsageChart.test.js`

**Step 1: Write the failing test**

Add tests for:
- initial empty selection + first profile load => all ids selected
- subset selection + deleted profile => deleted id removed
- previously all-selected + new profile added => new id auto-selected
- selected profiles with no stats => chart series contains zero-filled points
- dialog source uses the new helpers
- chart source no longer hides all-zero series

**Step 2: Run test to verify it fails**

Run: `node --test tests/codexTeamUi.test.js tests/codexServerDialog.test.js tests/codexUsageChart.test.js`
Expected: FAIL on missing helper logic and old source references.

### Task 2: Implement helper logic

**Files:**
- Modify: `src/utils/codexTeamUi.js`
- Test: `tests/codexTeamUi.test.js`

**Step 1: Write minimal implementation**

Add helpers for:
- syncing explicit selected profile ids from current + previous member rows
- building chart-ready selected series with zero-filled dates and metadata from member rows

Keep the output shape compatible with `CodexUsageChart`.

**Step 2: Run helper tests**

Run: `node --test tests/codexTeamUi.test.js`
Expected: PASS

### Task 3: Wire dialog and chart

**Files:**
- Modify: `src/components/openai/CodexServerDialog.vue`
- Modify: `src/components/openai/CodexUsageChart.vue`
- Test: `tests/codexServerDialog.test.js`
- Test: `tests/codexUsageChart.test.js`

**Step 1: Write minimal implementation**

- use the new selection sync helper in the member table watcher
- use the new chart series helper for `filteredDailyStatsSeries`
- change the selection label to treat “all selected” as the default badge
- let the chart render any series that has points, even if every point is zero

**Step 2: Run focused tests**

Run: `node --test tests/codexServerDialog.test.js tests/codexUsageChart.test.js`
Expected: PASS

### Task 4: Verify full frontend build

**Files:**
- No code changes expected

**Step 1: Run verification**

Run: `node --test tests/codexTeamUi.test.js tests/codexServerDialog.test.js tests/codexUsageChart.test.js tests/tauriBridge.test.js tests/ensureViteDev.test.js`
Expected: PASS

Run: `npm run build`
Expected: PASS
