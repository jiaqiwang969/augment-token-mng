#!/usr/bin/env bash

load_relay_env() {
  local script_dir repo_root env_file raw_line line key value
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  repo_root="$(cd "${script_dir}/.." && pwd)"
  env_file="${ATM_RELAY_ENV_FILE:-${repo_root}/.env.relay}"

  if [[ -f "${env_file}" ]]; then
    while IFS= read -r raw_line || [[ -n "${raw_line}" ]]; do
      line="${raw_line#"${raw_line%%[![:space:]]*}"}"
      line="${line%"${line##*[![:space:]]}"}"

      [[ -z "${line}" || "${line}" == \#* ]] && continue
      [[ "${line}" == *=* ]] || continue

      key="${line%%=*}"
      value="${line#*=}"
      key="${key#"${key%%[![:space:]]*}"}"
      key="${key%"${key##*[![:space:]]}"}"
      value="${value#"${value%%[![:space:]]*}"}"
      value="${value%"${value##*[![:space:]]}"}"

      [[ "${key}" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || continue
      [[ -n "${!key+x}" ]] && continue

      if [[ "${value}" == \"*\" && "${value}" == *\" ]]; then
        value="${value:1:${#value}-2}"
      elif [[ "${value}" == \'*\' && "${value}" == *\' ]]; then
        value="${value:1:${#value}-2}"
      fi

      printf -v "${key}" '%s' "${value}"
      export "${key}"
    done < "${env_file}"
  fi
}
