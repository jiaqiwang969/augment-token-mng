#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
SOURCE_DIR="${REPO_ROOT}/sidecars/cliproxy/apps/server-go"
BUILD_TARGET="./cmd/server/main.go"
OUTPUT_PATH="${OUTPUT_PATH:-${REPO_ROOT}/src-tauri/resources/cliproxy-server}"

if ! command -v go >/dev/null 2>&1; then
  echo "go is required but was not found in PATH" >&2
  exit 1
fi

GOOS_VALUE="${GOOS:-$(go env GOOS)}"
GOARCH_VALUE="${GOARCH:-$(go env GOARCH)}"
CGO_ENABLED_VALUE="${CGO_ENABLED:-0}"

if [ ! -d "${SOURCE_DIR}" ]; then
  echo "cliproxy source directory not found: ${SOURCE_DIR}" >&2
  exit 1
fi

mkdir -p "$(dirname "${OUTPUT_PATH}")"

echo "cliproxy source: ${SOURCE_DIR}"
echo "cliproxy target: ${BUILD_TARGET}"
echo "cliproxy output: ${OUTPUT_PATH}"
echo "cliproxy goos: ${GOOS_VALUE}"
echo "cliproxy goarch: ${GOARCH_VALUE}"
echo "cliproxy cgo_enabled: ${CGO_ENABLED_VALUE}"

(
  cd "${SOURCE_DIR}"
  GOOS="${GOOS_VALUE}" \
  GOARCH="${GOARCH_VALUE}" \
  CGO_ENABLED="${CGO_ENABLED_VALUE}" \
  go build -trimpath -o "${OUTPUT_PATH}" "${BUILD_TARGET}"
)
