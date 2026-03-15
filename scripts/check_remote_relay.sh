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
if [[ "$LOCAL_BASE_URL" == */v1 ]]; then
  LOCAL_GEMINI_BASE_URL="${ATM_RELAY_LOCAL_GEMINI_BASE_URL:-${LOCAL_BASE_URL%/v1}/v1beta}"
else
  LOCAL_GEMINI_BASE_URL="${ATM_RELAY_LOCAL_GEMINI_BASE_URL:-${LOCAL_BASE_URL%/}/v1beta}"
fi
if [[ "$PUBLIC_BASE_URL" == */v1 ]]; then
  PUBLIC_GEMINI_BASE_URL="${ATM_RELAY_PUBLIC_GEMINI_BASE_URL:-${PUBLIC_BASE_URL%/v1}/v1beta}"
else
  PUBLIC_GEMINI_BASE_URL="${ATM_RELAY_PUBLIC_GEMINI_BASE_URL:-${PUBLIC_BASE_URL%/}/v1beta}"
fi
LOCAL_MODELS_URL="${LOCAL_BASE_URL%/}/models"
PUBLIC_MODELS_URL="${PUBLIC_BASE_URL%/}/models"
LOCAL_GEMINI_MODELS_URL="${LOCAL_GEMINI_BASE_URL%/}/models"
PUBLIC_GEMINI_MODELS_URL="${PUBLIC_GEMINI_BASE_URL%/}/models"
API_KEY="${ATM_RELAY_API_KEY:-}"
auth_args=()
tmp_dir="$(mktemp -d /tmp/atm-relay-check.XXXXXX)"

cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

validate_models_payload() {
  local body_file="$1"
  local payload_kind="$2"
  node -e '
    const fs = require("fs")
    const payloadKind = process.argv[2]
    const body = fs.readFileSync(process.argv[1], "utf8")
    const json = JSON.parse(body)
    if (payloadKind === "gemini") {
      const models = json.models
      if (!Array.isArray(models) || models.length === 0) {
        throw new Error("gemini models payload is empty")
      }
      process.exit(0)
    }
    const data = json.data
    if (!Array.isArray(data) || data.length === 0) {
      throw new Error("openai models payload is empty")
    }
  ' "$body_file" "$payload_kind"
}

run_http_probe() {
  local label="$1"
  local url="$2"
  local payload_kind="$3"
  local body_file="$4"
  local headers_file="$5"
  shift 5

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

  validate_models_payload "$body_file" "$payload_kind"
}

run_remote_probe() {
  local label="$1"
  local path_suffix="$2"
  local payload_kind="$3"

  echo "== ${label} =="
  if [[ -n "$API_KEY" ]]; then
    ssh "$REMOTE_HOST" "REMOTE_PORT='${REMOTE_PORT}' API_KEY='${API_KEY}' PATH_SUFFIX='${path_suffix}' PAYLOAD_KIND='${payload_kind}' PROBE_LABEL='${label}' bash -s" <<'EOF'
set -euo pipefail
body_file="$(mktemp /tmp/atm-relay-remote-body.XXXXXX)"
headers_file="$(mktemp /tmp/atm-relay-remote-headers.XXXXXX)"
cleanup() {
  rm -f "$body_file" "$headers_file"
}
trap cleanup EXIT
auth_args=(-H "Authorization: Bearer ${API_KEY}")
http_code="$(curl -sS -D "$headers_file" -o "$body_file" -w '%{http_code}' "http://127.0.0.1:${REMOTE_PORT}${PATH_SUFFIX}" "${auth_args[@]}")"
cat "$headers_file"
head -40 "$body_file"
echo
if [[ ! "$http_code" =~ ^2[0-9][0-9]$ ]]; then
  echo "Expected 2xx from ${PROBE_LABEL}, got HTTP ${http_code}" >&2
  exit 1
fi
python3 - "$body_file" "${PAYLOAD_KIND}" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    payload = json.load(fh)

payload_kind = sys.argv[2]
if payload_kind == "gemini":
    models = payload.get("models")
    if not isinstance(models, list) or not models:
        raise SystemExit("gemini models payload is empty")
else:
    data = payload.get("data")
    if not isinstance(data, list) or not data:
        raise SystemExit("openai models payload is empty")
PY
EOF
  else
    ssh "$REMOTE_HOST" "REMOTE_PORT='${REMOTE_PORT}' PATH_SUFFIX='${path_suffix}' PAYLOAD_KIND='${payload_kind}' PROBE_LABEL='${label}' bash -s" <<'EOF'
set -euo pipefail
body_file="$(mktemp /tmp/atm-relay-remote-body.XXXXXX)"
headers_file="$(mktemp /tmp/atm-relay-remote-headers.XXXXXX)"
cleanup() {
  rm -f "$body_file" "$headers_file"
}
trap cleanup EXIT
http_code="$(curl -sS -D "$headers_file" -o "$body_file" -w '%{http_code}' "http://127.0.0.1:${REMOTE_PORT}${PATH_SUFFIX}")"
cat "$headers_file"
head -40 "$body_file"
echo
if [[ ! "$http_code" =~ ^2[0-9][0-9]$ ]]; then
  echo "Expected 2xx from ${PROBE_LABEL}, got HTTP ${http_code}" >&2
  exit 1
fi
python3 - "$body_file" "${PAYLOAD_KIND}" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    payload = json.load(fh)

payload_kind = sys.argv[2]
if payload_kind == "gemini":
    models = payload.get("models")
    if not isinstance(models, list) or not models:
        raise SystemExit("gemini models payload is empty")
else:
    data = payload.get("data")
    if not isinstance(data, list) or not data:
        raise SystemExit("openai models payload is empty")
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
  "openai" \
  "${tmp_dir}/local-body.json" \
  "${tmp_dir}/local-headers.txt" \
  "${auth_args[@]}"

run_http_probe \
  "Local ATM Gemini" \
  "$LOCAL_GEMINI_MODELS_URL" \
  "gemini" \
  "${tmp_dir}/local-gemini-body.json" \
  "${tmp_dir}/local-gemini-headers.txt" \
  "${auth_args[@]}"

run_remote_probe "Remote Loopback" "/v1/models" "openai"
run_remote_probe "Remote Loopback Gemini" "/v1beta/models" "gemini"

run_http_probe \
  "Public HTTPS" \
  "$PUBLIC_MODELS_URL" \
  "openai" \
  "${tmp_dir}/public-body.json" \
  "${tmp_dir}/public-headers.txt" \
  "${auth_args[@]}"

run_http_probe \
  "Public HTTPS Gemini" \
  "$PUBLIC_GEMINI_MODELS_URL" \
  "gemini" \
  "${tmp_dir}/public-gemini-body.json" \
  "${tmp_dir}/public-gemini-headers.txt" \
  "${auth_args[@]}"
