#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "${SCRIPT_DIR}/load_relay_env.sh"
load_relay_env

REMOTE_HOST="${ATM_RELAY_HOST:-}"
REMOTE_PORT="${ATM_RELAY_REMOTE_PORT:-19090}"
LOCAL_BASE_URL="${ATM_RELAY_LOCAL_BASE_URL:-http://127.0.0.1:8766/v1}"
PUBLIC_BASE_URL="${ATM_RELAY_PUBLIC_BASE_URL:-https://your-relay.example.com/v1}"
LOCAL_MODELS_URL="${LOCAL_BASE_URL%/}/models"
PUBLIC_MODELS_URL="${PUBLIC_BASE_URL%/}/models"
API_KEY="${ATM_RELAY_API_KEY:-}"
auth_args=()
tmp_dir="$(mktemp -d /tmp/atm-relay-check.XXXXXX)"

cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

validate_models_payload() {
  local body_file="$1"
  node -e '
    const fs = require("fs")
    const body = fs.readFileSync(process.argv[1], "utf8")
    const json = JSON.parse(body)
    const data = json.data
    if (!Array.isArray(data) || data.length === 0) {
      throw new Error("models payload is empty")
    }
  ' "$body_file"
}

run_http_probe() {
  local label="$1"
  local url="$2"
  local body_file="$3"
  local headers_file="$4"
  shift 4

  local http_code
  http_code="$(curl -sS -D "$headers_file" -o "$body_file" -w '%{http_code}' "$url" "$@")"

  echo "== ${label} =="
  cat "$headers_file"
  head -40 "$body_file"
  echo

  if [[ ! "$http_code" =~ ^2[0-9][0-9]$ ]]; then
    echo "Expected 2xx from ${label}, got HTTP ${http_code}" >&2
    return 1
  fi

  validate_models_payload "$body_file"
}

run_remote_probe() {
  local body_file="$1"
  local headers_file="$2"

  echo "== Remote Loopback =="
  if [[ -n "$API_KEY" ]]; then
    ssh "$REMOTE_HOST" "REMOTE_PORT='${REMOTE_PORT}' API_KEY='${API_KEY}' bash -s" <<'EOF'
set -euo pipefail
body_file="$(mktemp /tmp/atm-relay-remote-body.XXXXXX)"
headers_file="$(mktemp /tmp/atm-relay-remote-headers.XXXXXX)"
cleanup() {
  rm -f "$body_file" "$headers_file"
}
trap cleanup EXIT
auth_args=(-H "Authorization: Bearer ${API_KEY}")
http_code="$(curl -sS -D "$headers_file" -o "$body_file" -w '%{http_code}' "http://127.0.0.1:${REMOTE_PORT}/v1/models" "${auth_args[@]}")"
cat "$headers_file"
head -40 "$body_file"
echo
if [[ ! "$http_code" =~ ^2[0-9][0-9]$ ]]; then
  echo "Expected 2xx from Remote Loopback, got HTTP ${http_code}" >&2
  exit 1
fi
python3 - "$body_file" <<'PY'
import json, sys
with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    payload = json.load(fh)
if not isinstance(payload.get('data'), list) or not payload['data']:
    raise SystemExit('models payload is empty')
PY
EOF
  else
    ssh "$REMOTE_HOST" "REMOTE_PORT='${REMOTE_PORT}' bash -s" <<'EOF'
set -euo pipefail
body_file="$(mktemp /tmp/atm-relay-remote-body.XXXXXX)"
headers_file="$(mktemp /tmp/atm-relay-remote-headers.XXXXXX)"
cleanup() {
  rm -f "$body_file" "$headers_file"
}
trap cleanup EXIT
http_code="$(curl -sS -D "$headers_file" -o "$body_file" -w '%{http_code}' "http://127.0.0.1:${REMOTE_PORT}/v1/models")"
cat "$headers_file"
head -40 "$body_file"
echo
if [[ ! "$http_code" =~ ^2[0-9][0-9]$ ]]; then
  echo "Expected 2xx from Remote Loopback, got HTTP ${http_code}" >&2
  exit 1
fi
python3 - "$body_file" <<'PY'
import json, sys
with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    payload = json.load(fh)
if not isinstance(payload.get('data'), list) or not payload['data']:
    raise SystemExit('models payload is empty')
PY
EOF
  fi
}

if [[ -z "$REMOTE_HOST" ]]; then
  echo "ATM_RELAY_HOST is required, for example: export ATM_RELAY_HOST='ubuntu@your-relay-host'"
  exit 1
fi

if [[ -n "$API_KEY" ]]; then
  auth_args=(-H "Authorization: Bearer ${API_KEY}")
fi

run_http_probe \
  "Local ATM" \
  "$LOCAL_MODELS_URL" \
  "${tmp_dir}/local-body.json" \
  "${tmp_dir}/local-headers.txt" \
  "${auth_args[@]}"

run_remote_probe "${tmp_dir}/remote-body.json" "${tmp_dir}/remote-headers.txt"

run_http_probe \
  "Public HTTPS" \
  "$PUBLIC_MODELS_URL" \
  "${tmp_dir}/public-body.json" \
  "${tmp_dir}/public-headers.txt" \
  "${auth_args[@]}"
