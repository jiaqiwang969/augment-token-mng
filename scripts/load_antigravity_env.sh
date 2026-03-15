#!/usr/bin/env bash

load_antigravity_env() {
  local script_dir repo_root primary_env_file fallback_env_file env_file
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  repo_root="$(cd "${script_dir}/.." && pwd)"
  primary_env_file="${ATM_ANTIGRAVITY_ENV_FILE:-${repo_root}/.env.antigravity}"
  fallback_env_file="${repo_root}/.env"

  for env_file in "${fallback_env_file}" "${primary_env_file}"; do
    if [[ -f "${env_file}" ]]; then
      set -a
      # shellcheck disable=SC1090
      source "${env_file}"
      set +a
    fi
  done
}
