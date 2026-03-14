# ATM Team Dashboard Refinement Design

## Goal

Tighten the Codex gateway team dashboard so it scales better for the fixed 10-person squad without changing the backend model.

The current panel already has the right data, but it spends too much vertical space by expanding every member at once and it does not let the operator link visibility between the member ranking and the monthly trend chart.

This refinement keeps the existing gateway profile, log, and analytics model intact while making the UI more usable for day-to-day team distribution and monitoring.

## Approved Scope

- replace the always-expanded 10-member card wall with a dropdown-driven single-member detail view
- add a shared visible-member filter for both monthly trend and member ranking
- add one-click export of all member access bundles using the public gateway URL `https://lingkong.xyz/v1`
- keep custom key management unchanged
- do not change backend storage, Tauri commands, or request logging semantics in this batch

## Why This Fits The Current Codebase

The current `CodexServerDialog.vue` already computes the right frontend-ready shapes:

- `teamMemberCards` for the 10 built-in members
- `memberRankingRows` for team analytics
- `dailyStatsSeries` for chart rendering
- `copyGatewayAccess(profile)` for one-member export

That means the shortest path is a UI-layer refinement, not a data-model rewrite.

The design introduces a thin presentation layer:

- one selected member id for the detail card
- one visible-member set for analytics filtering
- one pure helper module that filters chart/ranking data and builds the all-member export payload

## UI Design

### Team Member Management

The team section should no longer render every member card at once.

Instead it should show:

- a selected-member dropdown
- a compact summary strip for the selected member
- one full detail card for the selected member only
- the existing team-level actions such as sync template and add custom key

This preserves all member-management actions while dramatically reducing scrolling.

### Linked Analytics Visibility

The analytics area should get one shared visible-member dropdown placed above the monthly trend chart and ranking table.

Behavior:

- default state is all members visible
- operator can toggle individual members on or off
- one shortcut resets visibility back to all members
- the same visible-member selection filters both:
  - the monthly trend chart
  - the member ranking table

The selected-member dropdown for the detail card stays independent from the analytics visibility filter.

### All-Member Export

Add one button in the team-member management area:

- label: export all member access
- action: copy a text bundle to the clipboard

The exported content should include, for every built-in member:

- member display name
- member code
- public base URL: `https://lingkong.xyz/v1`
- dedicated API key

Recommended format:

```text
# 姜大大 · jdd
OPENAI_BASE_URL=https://lingkong.xyz/v1
OPENAI_API_KEY=sk-team-jdd-xxxxxxxx

# 佳琪 · jqw
OPENAI_BASE_URL=https://lingkong.xyz/v1
OPENAI_API_KEY=sk-team-jqw-xxxxxxxx
```

This is optimized for direct distribution in chat tools and direct copy into local shell profiles.

## Data Flow

No backend changes are required.

Frontend flow:

1. existing `gatewayProfiles` and `memberAnalytics` load as today
2. `teamMemberCards` continues to be the canonical member list for built-in members
3. new helper functions derive:
   - filtered chart series
   - filtered member ranking rows
   - clipboard text for all-member export
4. the chart and ranking consume filtered data only

## Testing Strategy

Use a small pure helper module to keep the new behavior testable without mounting Vue components.

Cover these behaviors with `node:test`:

- filtering chart series by visible member ids
- filtering ranking rows by visible member ids
- building the all-member access clipboard bundle with the public URL and member keys

After that, wire the helpers into `CodexServerDialog.vue` and verify with:

- targeted node tests
- existing frontend tests
- `npm run build`

## Non-Goals

- no backend schema changes
- no new download-file export flow in this batch
- no changes to custom-key ranking semantics
- no changes to relay deployment, nginx, or proxy timeout work already in progress
