# ATM Team Key Ownership Design

## Goal

Upgrade the current Codex gateway key management from generic API-key CRUD into a team-centric operating surface for a fixed 10-person squad.

The system should treat each external key as belonging to a specific person, so key issuance, request logs, monthly trends, and usage summaries all answer the same question:

- who is using the gateway
- how much they are using it
- whether their key is active
- when they were last active

## Approved Decisions

- `1 person = 1 dedicated key = 1 gateway profile`
- key format: `sk-team-<member_code>-<random8>`
- import the 10-person squad through a built-in template, not manual one-by-one creation
- UI shows `name + short code + role title` as the main identity
- logs and charts are person-centric by default
- first version keeps people stable in the system; disable instead of delete

## Why This Fits The Current Codebase

The current system already has the right backbone:

- `GatewayAccessProfile` already owns the API key used for gateway auth
- request logs already persist `gateway_profile_id` and `gateway_profile_name`
- daily usage series can already be grouped by gateway profile

That means the right move is not to introduce a second "member table" and a mapping layer.

The approved design upgrades the existing profile into a person-bearing entity. The authentication path stays the same, while the metadata attached to that profile becomes rich enough to drive logs, charts, and UI cards.

## Team Template

The built-in squad preset should define these 10 members:

| Name | Code | Role Title |
| --- | --- | --- |
| 姜大大 | `jdd` | 产品与方法论 |
| 佳琪 | `jqw` | 架构与趋势 |
| CR | `cr` | 实验执行 |
| 梁十本 | `lsb` | 流程落地 |
| Will | `will` | 节奏与执行 |
| CP 全栈负责人 | `cp` | 工程与产品 |
| 大栗子 | `dlz` | 风险与成本 |
| Camper Wu | `cw` | 务实反馈 |
| 小蒋 | `xj` | 趋势判断 |
| 钟大嘴 | `zdz` | 稳定执行 |

Each preset entry should also include:

- a short persona summary
- a default display color
- a stable sort order

The import action must be idempotent by `member_code`. Running the import twice should not create duplicates.

## Data Model

The current `GatewayAccessProfile` for Codex should be extended with person metadata:

- `member_code`
- `role_title`
- `persona_summary`
- `color`
- `notes`

Existing fields remain:

- `id`
- `name`
- `target`
- `api_key`
- `enabled`

Recommended shape:

```json
{
  "id": "codex-jdd",
  "name": "姜大大",
  "target": "codex",
  "api_key": "sk-team-jdd-placeholder1",
  "enabled": true,
  "memberCode": "jdd",
  "roleTitle": "产品与方法论",
  "personaSummary": "高频输出，偏产品与方法论视角，擅长比较工具优劣并推动落地。",
  "color": "#4c6ef5",
  "notes": ""
}
```

This keeps gateway auth, person identity, and UI display tied to the same record.

## Key Design

Each team member gets a readable but random key:

`sk-team-<member_code>-<random8>`

Examples:

- `sk-team-jdd-placeholder1`
- `sk-team-jqw-xxxxxxxx`

Rules:

- `member_code` is stable and comes from the built-in team template
- `random8` should be lowercase hex or another compact lowercase alphabet
- full keys are stored in the profile store
- logs must never store the full key
- logs may store only a masked suffix such as `29c7e`

## Lifecycle And Management

The first version should support these actions:

- import the 10-person team template
- regenerate one member's dedicated key
- enable or disable one member
- edit `notes`
- optionally edit display color and role title
- copy one member's base URL + key snippet
- reset one member back to the template defaults

The first version should not support deleting a built-in team member. If someone should stop using the system, disable that member and preserve the history.

## Logging

Request logs should persist person identity at write time, not infer it later in the UI.

In addition to the existing gateway profile fields, each request log should persist:

- `member_code`
- `role_title`
- `display_label`
- `api_key_suffix`

This ensures that:

- history remains stable even after a key is regenerated
- charts do not need to re-derive identity from current profile state
- exported or queried logs always carry a person label

## Stats And Analytics

The main analytics axis should be the person, not the model.

Required views:

- per-member daily/monthly request totals
- per-member daily/monthly token totals
- per-member latest activity time
- per-member success rate
- per-member average response duration

The existing profile-grouped daily stats response should be enriched so the frontend can render stable person-oriented charts with:

- member name
- member code
- role title
- color

## UI

The Codex sharing panel should become the operating surface for the team.

### Overview Area

Show a member-centric summary card for each person:

- name
- code
- role title
- enabled state
- current key suffix
- today requests
- today tokens
- last active time

### Trend Area

The existing monthly trend chart should default to person series, not generic key series.

Each line should use the member's stable color and display name.

### Ranking Area

Show a member ranking table for the current month:

- requests
- tokens
- success rate
- average duration
- last active

### Log Area

Each log row should visibly show:

- member name
- member code
- role title

The logs view should also allow filtering by one member.

## Compatibility

Existing custom or legacy Codex gateway profiles must not be destroyed.

Profiles without `member_code` should remain valid and continue to authenticate. In the UI and analytics they can be grouped under a fallback bucket such as:

- `Legacy`
- `Custom`

This keeps backward compatibility while the team template becomes the preferred path.

## Non-Goals

This slice does not include:

- multiple keys per person
- billing or cost allocation
- approval workflows
- quota budgets per person
- role-based permissions
- a dedicated person-detail page

## Testing Strategy

The implementation should cover:

- team preset import idempotency
- generated key prefix correctness
- profile metadata round-trip through storage
- log persistence of member metadata
- per-member daily stats aggregation
- per-member log filtering

Frontend verification can rely on `npm run build` plus manual walkthrough because the repo does not currently have a Vue component test harness for this screen.

## Rollout

1. Extend the shared gateway profile schema with person metadata.
2. Add the built-in 10-member team template and readable key generation.
3. Add member-centric commands for import, regenerate, and metadata updates.
4. Persist member identity in request logs.
5. Enrich per-member stats and filters.
6. Upgrade the Codex UI from key CRUD to team-member management.
