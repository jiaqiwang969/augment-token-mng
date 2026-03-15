#!/usr/bin/env node

import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

export const DEFAULT_RELAY_ENV_FILE = '.env.relay'
export const RELAY_MANAGED_BLOCK_START = '# >>> ATM RELAY MANAGED BLOCK >>>'
export const RELAY_MANAGED_BLOCK_END = '# <<< ATM RELAY MANAGED BLOCK <<<'
export const RELAY_LEGACY_HEADER = '# ========== ATM Remote Relay =========='

export const REQUIRED_RELAY_KEYS = [
  'ATM_RELAY_HOST',
  'ATM_RELAY_SERVER_NAME',
  'ATM_RELAY_PUBLIC_BASE_URL',
  'ATM_RELAY_NGINX_SITE_PATH'
]

const stripWrappingQuotes = value => {
  if (
    (value.startsWith('"') && value.endsWith('"')) ||
    (value.startsWith("'") && value.endsWith("'"))
  ) {
    return value.slice(1, -1)
  }

  return value
}

export const parseDotenvText = text => {
  const result = {}

  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim()
    if (!line || line.startsWith('#')) {
      continue
    }

    const separatorIndex = line.indexOf('=')
    if (separatorIndex <= 0) {
      continue
    }

    const key = line.slice(0, separatorIndex).trim()
    const value = stripWrappingQuotes(line.slice(separatorIndex + 1).trim())

    if (!key) {
      continue
    }

    result[key] = value
  }

  return result
}

const readEnvFileIfPresent = async envPath => {
  try {
    return await readFile(envPath, 'utf8')
  } catch (error) {
    if (error?.code === 'ENOENT') {
      return ''
    }

    throw error
  }
}

const pickRelayOverrides = env => {
  const overrides = {}

  for (const [key, value] of Object.entries(env)) {
    if (!key.startsWith('ATM_RELAY_')) {
      continue
    }

    if (typeof value !== 'string') {
      continue
    }

    const trimmed = value.trim()
    if (!trimmed) {
      continue
    }

    overrides[key] = trimmed
  }

  return overrides
}

export const validateRelaySettings = settings => {
  const missing = REQUIRED_RELAY_KEYS.filter(key => !settings[key] || !String(settings[key]).trim())

  if (missing.length > 0) {
    throw new Error(`Missing required relay settings: ${missing.join(', ')}`)
  }

  return settings
}

export const resolveRelaySettings = async ({
  cwd = process.cwd(),
  envPath = path.join(cwd, DEFAULT_RELAY_ENV_FILE),
  env = process.env
} = {}) => {
  const fileText = await readEnvFileIfPresent(envPath)
  const fileValues = parseDotenvText(fileText)
  const envOverrides = pickRelayOverrides(env)

  const settings = {
    ATM_RELAY_REMOTE_PORT: '19090',
    ATM_RELAY_LOCAL_PORT: '8766',
    ATM_RELAY_NGINX_RELOAD_COMMAND: 'sudo systemctl reload nginx',
    ATM_RELAY_NGINX_VALIDATE_COMMAND: 'sudo nginx -t',
    ATM_RELAY_REMOTE_TMP_PATH: '/tmp/atm-relay-nginx.conf',
    ...fileValues,
    ...envOverrides
  }

  if (!settings.ATM_RELAY_LOCAL_BASE_URL) {
    settings.ATM_RELAY_LOCAL_BASE_URL = `http://127.0.0.1:${settings.ATM_RELAY_LOCAL_PORT}/v1`
  }

  if (!settings.ATM_RELAY_CONTROL_SOCKET) {
    settings.ATM_RELAY_CONTROL_SOCKET = path.join(
      env.HOME || '~',
      '.ssh',
      `atm-relay-${settings.ATM_RELAY_REMOTE_PORT}.sock`
    )
  }

  validateRelaySettings(settings)
  return settings
}

export const renderRelayNginxTemplate = (template, settings) => {
  const replacements = {
    '__ATM_RELAY_SERVER_NAME__': settings.ATM_RELAY_SERVER_NAME,
    '__ATM_RELAY_REMOTE_PORT__': settings.ATM_RELAY_REMOTE_PORT
  }

  return Object.entries(replacements).reduce(
    (output, [needle, value]) => output.split(needle).join(String(value)),
    template
  )
}

const shellQuote = value => `'${String(value).replace(/'/g, `'\\''`)}'`

export const emitShellExports = settings =>
  Object.entries(settings)
    .filter(([key]) => key.startsWith('ATM_RELAY_'))
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([key, value]) => `export ${key}=${shellQuote(value)}`)
    .join('\n')

const trimSingleTrailingNewline = text => text.replace(/\n$/, '')

const wrapManagedRelayBlock = relayBlock => {
  const normalizedBlock = trimSingleTrailingNewline(relayBlock)
  return [RELAY_MANAGED_BLOCK_START, normalizedBlock, RELAY_MANAGED_BLOCK_END].join('\n')
}

const replaceManagedBlock = (siteConfig, relayBlock) => {
  const managedPattern = new RegExp(
    `${RELAY_MANAGED_BLOCK_START.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}[\\s\\S]*?${RELAY_MANAGED_BLOCK_END.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}`
  )

  return siteConfig.replace(managedPattern, wrapManagedRelayBlock(relayBlock))
}

const replaceLegacyBlock = (siteConfig, relayBlock) => {
  const legacyPattern =
    /[ \t]*# ========== ATM Remote Relay ==========[\s\S]*?location \^~ \/v1\/ \{\s*return 404;\s*\}\s*/m

  return siteConfig.replace(legacyPattern, `${wrapManagedRelayBlock(relayBlock)}\n\n`)
}

export const upsertManagedRelayBlock = (siteConfig, relayBlock) => {
  if (siteConfig.includes(RELAY_MANAGED_BLOCK_START) && siteConfig.includes(RELAY_MANAGED_BLOCK_END)) {
    return replaceManagedBlock(siteConfig, relayBlock)
  }

  if (siteConfig.includes(RELAY_LEGACY_HEADER)) {
    return replaceLegacyBlock(siteConfig, relayBlock)
  }

  const managedBlock = `${wrapManagedRelayBlock(relayBlock)}\n\n`
  const defaultLocationPattern = /^([ \t]*location \/ \{)/m

  if (defaultLocationPattern.test(siteConfig)) {
    return siteConfig.replace(defaultLocationPattern, `${managedBlock}$1`)
  }

  const closingBraceIndex = siteConfig.lastIndexOf('}')
  if (closingBraceIndex === -1) {
    return `${siteConfig}\n\n${wrapManagedRelayBlock(relayBlock)}`
  }

  return `${siteConfig.slice(0, closingBraceIndex).trimEnd()}\n\n${wrapManagedRelayBlock(relayBlock)}\n${siteConfig.slice(closingBraceIndex)}`
}

const printShellEnv = async () => {
  const settings = await resolveRelaySettings()
  process.stdout.write(`${emitShellExports(settings)}\n`)
}

const isEntrypoint = () => process.argv[1] === fileURLToPath(import.meta.url)

if (isEntrypoint()) {
  const command = process.argv[2]

  if (command === 'print-shell-env') {
    printShellEnv().catch(error => {
      console.error(`[relay-config] ${error.message}`)
      process.exit(1)
    })
  } else {
    console.error('[relay-config] Supported commands: print-shell-env')
    process.exit(1)
  }
}
