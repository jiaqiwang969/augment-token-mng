# Antigravity Service Dialog Manual Checklist

Use this after `make dev` starts successfully.

## Entry

- Antigravity account manager header shows a dedicated API-service button.
- Clicking the button opens an `Antigravity API 服务` dialog.
- Closing the dialog returns to the Antigravity account manager without losing page state.

## Overview

- The dialog defaults to the overview tab on first open.
- The overview shows both local `/v1` URL and public relay `/v1` URL.
- The overview shows service status cards for total accounts, available accounts, and enabled keys.
- The overview shows storage cards for total logs, DB size, all-time requests/tokens, and recent requests/tokens.
- The overview shows maintenance controls for clearing all logs and pruning logs before a selected date.

## Team Keys

- The overview lists Antigravity gateway profiles with member name, member code, status, and key suffix.
- The operator can import the built-in team template.
- The operator can export all member access bundles with `OPENAI_BASE_URL` and `OPENAI_API_KEY`.
- The operator can create, edit, regenerate, and delete a member key from the dialog.

## Logs

- The logs tab shows time-range filters, member/status/model filters, and paginated request logs.
- The logs tab shows a model-usage summary table for the active time range.
- Changing the log time range refreshes both the logs list and the model-usage summary.
