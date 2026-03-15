#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "${SCRIPT_DIR}/load_relay_env.sh"
load_relay_env

REMOTE_HOST="${ATM_RELAY_HOST:-}"
REMOTE_PORT="${ATM_RELAY_REMOTE_PORT:-19090}"
LOCAL_PORT="${ATM_RELAY_LOCAL_PORT:-8766}"
CONTROL_SOCKET="${ATM_RELAY_CONTROL_SOCKET:-$HOME/.ssh/atm-relay-${REMOTE_PORT}.sock}"

if [[ -z "$REMOTE_HOST" ]]; then
  echo "ATM_RELAY_HOST is required, for example: export ATM_RELAY_HOST='ubuntu@your-relay-host'"
  exit 1
fi

mkdir -p "$(dirname "$CONTROL_SOCKET")"

if ssh -S "$CONTROL_SOCKET" -O check "$REMOTE_HOST" >/dev/null 2>&1; then
  echo "Relay already running: $REMOTE_HOST -> 127.0.0.1:$REMOTE_PORT -> 127.0.0.1:$LOCAL_PORT"
  exit 0
fi

rm -f "$CONTROL_SOCKET"

ssh -fN \
  -M \
  -S "$CONTROL_SOCKET" \
  -o ExitOnForwardFailure=yes \
  -o ServerAliveInterval=30 \
  -o ServerAliveCountMax=3 \
  -R "127.0.0.1:${REMOTE_PORT}:127.0.0.1:${LOCAL_PORT}" \
  "$REMOTE_HOST"

echo "Relay started: $REMOTE_HOST -> 127.0.0.1:$REMOTE_PORT -> 127.0.0.1:$LOCAL_PORT"
