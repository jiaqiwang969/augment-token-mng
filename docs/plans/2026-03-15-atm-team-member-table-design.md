# ATM Team Member Table Design

## Goal

Replace the current multi-card Codex team management surface with a simpler Apple-style layout built around one primary trend chart and one primary team member table.

This design keeps the current gateway-profile-backed data model, but changes the operating surface so the team roster, per-member usage, and curve visibility all converge into one compact flow.

## Approved Decisions

- the main team surface becomes `curve + single member table`
- member identity and member usage live in the same table
- member editing moves into a modal instead of always-expanded cards
- adding team members happens by creating normal Codex gateway profiles with member metadata
- chart visibility is driven by table row selection, not a separate filter block
- keep the current storage model based on `GatewayAccessProfile`

## Why This Fits The Current Codebase

The backend already supports arbitrary Codex gateway profiles with:

- `name`
- `member_code`
- `role_title`
- `persona_summary`
- `color`
- `notes`
- `api_key`
- `enabled`

That means the missing piece is not backend capability to create team members. The missing piece is that the current UI still treats the original 10 presets as the first-class team and pushes everything else into a separate custom-key section.

The right move is to simplify the UI and promote member-bearing gateway profiles into a single roster table.

## Information Architecture

### Top Toolbar

Keep one slim action row:

- add member
- import default team
- export all member access
- refresh

This row should stay compact and avoid the current layered card feel.

### Primary Trend Chart

Show one chart for the recent 30-day member trend.

Behavior:

- default shows all members
- selecting rows in the table filters the curve
- if nothing is selected, the chart falls back to all members
- selected rows visually highlight their matching series

### Team Member Table

Use one table that merges identity and usage columns:

- name
- member code
- role title
- status
- key suffix
- today requests
- 30-day tokens
- last active
- actions

The table becomes the main management surface instead of a stack of cards plus a separate ranking panel.

### Editing Flow

Do not keep large editable cards open in the overview.

Use a modal for member editing and creation:

- name
- member code
- role title
- persona summary
- color
- notes
- api key
- enabled

Actions in the modal:

- save
- regenerate key
- copy access bundle
- delete

## Data Model

No second team-member storage table is introduced in this batch.

The source of truth remains `GatewayAccessProfile`.

The team table is built from Codex gateway profiles, with analytics merged in for:

- today requests
- today tokens
- total requests
- total tokens
- success rate
- last active

## Compatibility Strategy

Use soft migration.

- existing 10-member preset profiles remain valid
- arbitrary new members can be added through normal profile creation
- `import_codex_team_template` remains available as a convenience initializer
- logs and trends continue to rely on stored `member_code` and profile metadata

No destructive migration is required.

## UI Simplification Rules

- remove the always-expanded member card wall
- remove the separate member ranking block from the overview
- reduce the number of bordered sections in the team area
- keep colors only for member identity and status emphasis
- use one roster table as the visual anchor

## Testing Strategy

Keep the new selection/filter logic in a pure helper module where possible.

Cover:

- table-row selection filters chart series by profile id
- empty selection falls back to all chart series
- export-all access bundle still includes every member

Then verify the Vue refactor with:

- targeted node tests
- existing frontend bridge tests
- `npm run build`

## Non-Goals

- no new backend member table
- no multi-key-per-member model
- no changes to request-log schema
- no redesign of the logs tab in this batch
