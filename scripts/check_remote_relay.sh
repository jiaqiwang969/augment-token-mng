#!/usr/bin/env bash
set -euo pipefail

REMOTE_HOST="${ATM_RELAY_HOST:-}"
REMOTE_PORT="${ATM_RELAY_REMOTE_PORT:-19090}"
LOCAL_BASE_URL="${ATM_RELAY_LOCAL_BASE_URL:-http://127.0.0.1:8766/v1/models}"
PUBLIC_BASE_URL="${ATM_RELAY_PUBLIC_BASE_URL:-https://your-relay.example.com/v1/models}"
API_KEY="${ATM_RELAY_API_KEY:-}"
auth_args=()

if [[ -z "$REMOTE_HOST" ]]; then
  echo "ATM_RELAY_HOST is required, for example: export ATM_RELAY_HOST='ubuntu@your-relay-host'"
  exit 1
fi

if [[ -n "$API_KEY" ]]; then
  auth_args=(-H "Authorization: Bearer ${API_KEY}")
fi

echo "== Local ATM =="
curl -sS -D - "$LOCAL_BASE_URL" "${auth_args[@]}" -o /tmp/atm-relay-local-body
head -40 /tmp/atm-relay-local-body
echo

echo "== Remote Loopback =="
if [[ -n "$API_KEY" ]]; then
  ssh "$REMOTE_HOST" "REMOTE_PORT='${REMOTE_PORT}' API_KEY='${API_KEY}' bash -s" <<'EOF'
auth_args=(-H "Authorization: Bearer ${API_KEY}")
curl -sS -D - "http://127.0.0.1:${REMOTE_PORT}/v1/models" "${auth_args[@]}" -o /tmp/atm-relay-remote-body
head -40 /tmp/atm-relay-remote-body
EOF
else
  ssh "$REMOTE_HOST" "REMOTE_PORT='${REMOTE_PORT}' bash -s" <<'EOF'
curl -sS -D - "http://127.0.0.1:${REMOTE_PORT}/v1/models" -o /tmp/atm-relay-remote-body
head -40 /tmp/atm-relay-remote-body
EOF
fi
echo

echo "== Public HTTPS =="
curl -sS -D - "$PUBLIC_BASE_URL" "${auth_args[@]}" -o /tmp/atm-relay-public-body
head -40 /tmp/atm-relay-public-body
