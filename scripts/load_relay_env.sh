#!/usr/bin/env bash

load_relay_env() {
  local script_dir repo_root env_file
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  repo_root="$(cd "${script_dir}/.." && pwd)"
  env_file="${ATM_RELAY_ENV_FILE:-${repo_root}/.env.relay}"

  if [[ -f "${env_file}" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "${env_file}"
    set +a
  fi
}
