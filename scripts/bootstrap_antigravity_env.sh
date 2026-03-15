#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
target_env_file="${ATM_ANTIGRAVITY_ENV_FILE:-${repo_root}/.env.antigravity}"
oauth_module_path="src-tauri/src/platforms/antigravity/modules/oauth.rs"
force_bootstrap="${ATM_ANTIGRAVITY_BOOTSTRAP_FORCE:-}"

looks_like_placeholder() {
  local value="${1:-}"
  [[ -z "${value}" ]] && return 0
  [[ "${value}" == *REDACTED* ]] && return 0
  [[ "${value}" == your-* ]] && return 0
  [[ "${value}" == example* ]] && return 0
  return 1
}

resolve_from_current_env() {
  if [[ -n "${ATM_ANTIGRAVITY_OAUTH_CLIENT_ID:-}" && -n "${ATM_ANTIGRAVITY_OAUTH_CLIENT_SECRET:-}" ]]; then
    printf '%s\n%s\n%s\n' \
      "${ATM_ANTIGRAVITY_OAUTH_CLIENT_ID}" \
      "${ATM_ANTIGRAVITY_OAUTH_CLIENT_SECRET}" \
      "ATM environment"
    return 0
  fi

  if [[ -n "${CLIPROXY_ANTIGRAVITY_OAUTH_CLIENT_ID:-}" && -n "${CLIPROXY_ANTIGRAVITY_OAUTH_CLIENT_SECRET:-}" ]]; then
    printf '%s\n%s\n%s\n' \
      "${CLIPROXY_ANTIGRAVITY_OAUTH_CLIENT_ID}" \
      "${CLIPROXY_ANTIGRAVITY_OAUTH_CLIENT_SECRET}" \
      "legacy CLIProxy environment"
    return 0
  fi

  return 1
}

resolve_from_git_history() {
  local revision content client_id client_secret

  git -C "${repo_root}" rev-parse --git-dir >/dev/null 2>&1 || return 1

  while IFS= read -r revision; do
    content="$(git -C "${repo_root}" show "${revision}:${oauth_module_path}" 2>/dev/null || true)"
    [[ -n "${content}" ]] || continue

    client_id="$(
      printf '%s\n' "${content}" | perl -ne 'if (/^const CLIENT_ID: &str = "(.*)";$/) { print "$1\n"; exit }'
    )"
    client_secret="$(
      printf '%s\n' "${content}" | perl -ne 'if (/^const CLIENT_SECRET: &str = "(.*)";$/) { print "$1\n"; exit }'
    )"

    if looks_like_placeholder "${client_id}" || looks_like_placeholder "${client_secret}"; then
      continue
    fi

    if [[ -n "${client_id}" && -n "${client_secret}" ]]; then
      printf '%s\n%s\n%s\n' "${client_id}" "${client_secret}" "local git history"
      return 0
    fi
  done < <(git -C "${repo_root}" rev-list --all -- "${oauth_module_path}")

  return 1
}

write_env_file() {
  local client_id="${1}"
  local client_secret="${2}"
  local tmp_file

  mkdir -p "$(dirname "${target_env_file}")"
  umask 077
  tmp_file="$(mktemp "${target_env_file}.XXXXXX")"

  {
    printf '# Local-only Antigravity OAuth credentials. Do not commit.\n'
    printf 'ATM_ANTIGRAVITY_OAUTH_CLIENT_ID=%q\n' "${client_id}"
    printf 'ATM_ANTIGRAVITY_OAUTH_CLIENT_SECRET=%q\n' "${client_secret}"
  } > "${tmp_file}"

  mv "${tmp_file}" "${target_env_file}"
  chmod 600 "${target_env_file}" 2>/dev/null || true
}

main() {
  local output client_id client_secret source_label

  if [[ -e "${target_env_file}" && "${1:-}" != "--force" && -z "${force_bootstrap}" ]]; then
    printf 'Refusing to overwrite existing %s. Re-run with --force or ATM_ANTIGRAVITY_BOOTSTRAP_FORCE=1.\n' "${target_env_file}" >&2
    exit 1
  fi

  if output="$(resolve_from_current_env)"; then
    :
  elif output="$(resolve_from_git_history)"; then
    :
  else
    printf 'Unable to resolve Antigravity OAuth credentials from current env or local git history.\n' >&2
    printf 'Populate ATM_ANTIGRAVITY_OAUTH_CLIENT_ID / ATM_ANTIGRAVITY_OAUTH_CLIENT_SECRET, then re-run this bootstrap.\n' >&2
    exit 1
  fi

  client_id="$(printf '%s\n' "${output}" | sed -n '1p')"
  client_secret="$(printf '%s\n' "${output}" | sed -n '2p')"
  source_label="$(printf '%s\n' "${output}" | sed -n '3p')"

  write_env_file "${client_id}" "${client_secret}"
  printf 'Wrote %s from %s. Secrets were not printed.\n' "${target_env_file}" "${source_label}"
}

main "$@"
