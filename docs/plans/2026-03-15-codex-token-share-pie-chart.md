# Codex Token Share Pie Chart Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a member-level doughnut chart that shows the selected members' 30-day token share inside the Codex dashboard.

**Architecture:** Reuse the existing member-filtered daily series that already powers the monthly trend chart. Add one utility that aggregates per-member token totals, render those totals in a new doughnut-chart component, and place that component beside the existing line chart so both views stay synchronized with the same member selection state.

**Tech Stack:** Vue 3, vue-chartjs, Chart.js, Node test runner, existing Codex team UI helpers

---

### Task 1: Document the expected token-share aggregation

**Files:**
- Modify: `tests/codexTeamUi.test.js`
- Modify: `src/utils/codexTeamUi.js`

**Step 1: Write the failing test**

Add a test that passes selected member series into a new helper and expects:
- token totals to be summed per member
- percentages to be computed from the selected members' total tokens
- zero-token members to be excluded from the pie slices
- compact member codes to remain available for chart labels

**Step 2: Run test to verify it fails**

Run: `node --test tests/codexTeamUi.test.js`
Expected: FAIL because the token-share helper does not exist yet.

**Step 3: Write minimal implementation**

Add a helper in `src/utils/codexTeamUi.js` that converts filtered daily series into doughnut-chart rows.

**Step 4: Run test to verify it passes**

Run: `node --test tests/codexTeamUi.test.js`
Expected: PASS.

### Task 2: Add the doughnut-chart component contract

**Files:**
- Create: `src/components/openai/CodexUsagePieChart.vue`
- Modify: `tests/codexUsageChart.test.js`

**Step 1: Write the failing test**

Add a source-level test that expects the new component to:
- import and register the Chart.js doughnut primitives
- render compact labels in the chart/legend
- expose full member label, token total, and percentage in the tooltip
- show an empty state when there are no non-zero members

**Step 2: Run test to verify it fails**

Run: `node --test tests/codexUsageChart.test.js`
Expected: FAIL because the new component file does not exist.

**Step 3: Write minimal implementation**

Create `CodexUsagePieChart.vue` with the doughnut chart, empty state, and tooltip formatting.

**Step 4: Run test to verify it passes**

Run: `node --test tests/codexUsageChart.test.js`
Expected: PASS.

### Task 3: Wire the token-share chart into the Codex dashboard

**Files:**
- Modify: `src/components/openai/CodexServerDialog.vue`
- Modify: `src/locales/en-US.js`
- Modify: `src/locales/zh-CN.js`
- Test: `tests/codexServerDialog.test.js`

**Step 1: Write the failing test**

Add a dialog source test that expects:
- the new pie-chart component import and render block
- computed token-share data derived from the existing filtered member series
- localized title/hint strings for the token share panel

**Step 2: Run test to verify it fails**

Run: `node --test tests/codexServerDialog.test.js`
Expected: FAIL because the dialog does not render the new chart yet.

**Step 3: Write minimal implementation**

Import the helper and new pie-chart component, compute token-share rows from `filteredDailyStatsSeries`, and render the chart beside the existing monthly trend chart.

**Step 4: Run test to verify it passes**

Run: `node --test tests/codexServerDialog.test.js`
Expected: PASS.

### Task 4: Verify the integrated dashboard

**Files:**
- Test: `tests/codexTeamUi.test.js`
- Test: `tests/codexUsageChart.test.js`
- Test: `tests/codexServerDialog.test.js`

**Step 1: Run targeted test suite**

Run: `node --test tests/codexTeamUi.test.js tests/codexUsageChart.test.js tests/codexServerDialog.test.js`
Expected: PASS.

**Step 2: Run production build**

Run: `npm run build`
Expected: PASS, with only existing non-fatal Vite chunk warnings if any.

**Step 3: Commit**

```bash
git add docs/plans/2026-03-15-codex-token-share-pie-chart.md \
  tests/codexTeamUi.test.js \
  tests/codexUsageChart.test.js \
  tests/codexServerDialog.test.js \
  src/utils/codexTeamUi.js \
  src/components/openai/CodexUsagePieChart.vue \
  src/components/openai/CodexServerDialog.vue \
  src/locales/en-US.js \
  src/locales/zh-CN.js
git commit -m "feat: add codex token share chart"
```
