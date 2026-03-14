#!/usr/bin/env bash
set -euo pipefail

REMOTE_HOST="${ATM_RELAY_HOST:-}"
REMOTE_PORT="${ATM_RELAY_REMOTE_PORT:-19090}"
CONTROL_SOCKET="${ATM_RELAY_CONTROL_SOCKET:-$HOME/.ssh/atm-relay-${REMOTE_PORT}.sock}"

if [[ -z "$REMOTE_HOST" ]]; then
  echo "ATM_RELAY_HOST is required, for example: export ATM_RELAY_HOST='ubuntu@your-relay-host'"
  exit 1
fi

if ! ssh -S "$CONTROL_SOCKET" -O check "$REMOTE_HOST" >/dev/null 2>&1; then
  echo "Relay is not running"
  rm -f "$CONTROL_SOCKET"
  exit 0
fi

ssh -S "$CONTROL_SOCKET" -O exit "$REMOTE_HOST"
rm -f "$CONTROL_SOCKET"

echo "Relay stopped: $REMOTE_HOST ($REMOTE_PORT)"
