# Codex Member Chart Selection Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the member trend chart default to all-selected, remove deleted members immediately, and render zero-filled points instead of missing gaps.

**Architecture:** Keep the behavior in frontend helpers so the backend stats API stays unchanged. Use one helper to synchronize explicit member selection state and another helper to filter plus zero-pad chart series for selected members.

**Tech Stack:** Vue 3, plain JavaScript utility helpers, Node test runner

---

### Task 1: Add failing helper tests

- Cover explicit default-all selection initialization.
- Cover removing deleted members from selection.
- Cover zero-padding selected chart series across the shared date axis.

### Task 2: Implement helper logic and wire the dialog

- Add selection sync helper in `src/utils/codexTeamUi.js`.
- Update `src/components/openai/CodexServerDialog.vue` to track previous profile ids and initialize explicit selection state.
- Make the chart label reflect actual selected count instead of treating empty as “all”.

### Task 3: Verify

- Run `node --test tests/codexTeamUi.test.js tests/codexServerDialog.test.js`.
- Run `npm run build`.
