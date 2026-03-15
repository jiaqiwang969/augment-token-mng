# Codex Member Table Details Revert Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Restore the always-visible member overview table, keep chart visibility driven by row checkboxes, and show member details in a card below the table while simplifying chart legend labels to short member codes.

**Architecture:** Keep the current analytics and explicit chart-selection helpers. Rework only the dialog composition so the table remains the primary overview and a single focused member row drives a detail card beneath it. Update the chart label formatter so legend text stays compact while tooltip text keeps the full member identity.

**Tech Stack:** Vue 3, plain JavaScript helpers, node:test source assertions, Vite build

---

### Task 1: Lock the restored overview structure with failing tests

**Files:**
- Modify: `tests/codexServerDialog.test.js`
- Modify: `tests/codexUsageChart.test.js`

**Step 1: Write the failing test**

Add assertions for:
- the member overview table returning as the primary layout
- a focused member detail card rendered below the table
- table row clicks driving `setFocusedMember`
- chart legend labels using compact member-code labels instead of long display labels

**Step 2: Run test to verify it fails**

Run: `node --test tests/codexServerDialog.test.js tests/codexUsageChart.test.js`
Expected: FAIL on the current dropdown-focused layout and verbose legend labeling.

### Task 2: Restore table-first member layout

**Files:**
- Modify: `src/components/openai/CodexServerDialog.vue`
- Modify: `src/utils/codexTeamUi.js`

**Step 1: Write minimal implementation**

- keep `focusedMemberProfileId`
- restore the overview table block
- make row click set the focused member
- render one detail card below the table for `focusedMemberRow`
- keep checkbox selection dedicated to chart visibility only

**Step 2: Run test to verify it passes**

Run: `node --test tests/codexServerDialog.test.js`
Expected: PASS

### Task 3: Simplify chart legend labels

**Files:**
- Modify: `src/components/openai/CodexUsageChart.vue`
- Modify: `tests/codexUsageChart.test.js`

**Step 1: Write minimal implementation**

- use `memberCode` as the primary compact legend label when available
- keep tooltip titles rich with full member name and role
- avoid changing the data model or chart series shape

**Step 2: Run test to verify it passes**

Run: `node --test tests/codexUsageChart.test.js`
Expected: PASS

### Task 4: Verify the restored dialog end to end

**Files:**
- Modify: `src/locales/zh-CN.js`
- Modify: `src/locales/en-US.js`

**Step 1: Run focused verification**

Run: `node --test tests/codexTeamUi.test.js tests/codexServerDialog.test.js tests/codexUsageChart.test.js tests/tauriBridge.test.js tests/ensureViteDev.test.js`
Expected: PASS

**Step 2: Run build verification**

Run: `npm run build`
Expected: PASS
