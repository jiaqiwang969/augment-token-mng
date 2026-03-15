#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
target_env_file="${ATM_ANTIGRAVITY_ENV_FILE:-${repo_root}/.env.antigravity}"

# shellcheck disable=SC1091
source "${script_dir}/load_antigravity_env.sh"
load_antigravity_env

file_mode() {
  local file_path="${1}"

  if [[ ! -e "${file_path}" ]]; then
    return 1
  fi

  if stat -f '%Lp' "${file_path}" >/dev/null 2>&1; then
    stat -f '%Lp' "${file_path}"
    return 0
  fi

  stat -c '%a' "${file_path}"
}

main() {
  local source_label mode_value

  if [[ -n "${ATM_ANTIGRAVITY_OAUTH_CLIENT_ID:-}" && -n "${ATM_ANTIGRAVITY_OAUTH_CLIENT_SECRET:-}" ]]; then
    source_label="ATM environment"
  elif [[ -n "${CLIPROXY_ANTIGRAVITY_OAUTH_CLIENT_ID:-}" && -n "${CLIPROXY_ANTIGRAVITY_OAUTH_CLIENT_SECRET:-}" ]]; then
    source_label="legacy CLIProxy environment"
  else
    printf 'Antigravity OAuth env is missing. Expected %s or legacy CLIProxy names. Secrets were not printed.\n' \
      "ATM_ANTIGRAVITY_OAUTH_CLIENT_ID / ATM_ANTIGRAVITY_OAUTH_CLIENT_SECRET" >&2
    exit 1
  fi

  if mode_value="$(file_mode "${target_env_file}")"; then
    printf 'Antigravity env file: %s (mode %s)\n' "${target_env_file}" "${mode_value}"
  else
    printf 'Antigravity env file: %s (not present, using process environment)\n' "${target_env_file}"
  fi

  printf 'Antigravity OAuth credentials are configured and ready from %s. Secrets were not printed.\n' "${source_label}"
}

main "$@"
