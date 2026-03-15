#!/usr/bin/env bash
set -euo pipefail

required_paths=(
  "apps/server-go"
  "apps/server-go/cmd/server/main.go"
  "README.md"
)

for path in "${required_paths[@]}"; do
  if [[ ! -e "$path" ]]; then
    echo "missing required path: $path" >&2
    exit 1
  fi
done

echo "smoke checks passed"
