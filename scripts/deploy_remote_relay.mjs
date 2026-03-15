#!/usr/bin/env node

import { spawn } from 'node:child_process'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

import {
  renderRelayNginxTemplate,
  resolveRelaySettings,
  upsertManagedRelayBlock
} from './relayConfig.mjs'

const scriptPath = fileURLToPath(import.meta.url)
const scriptDir = path.dirname(scriptPath)
const repoRoot = path.resolve(scriptDir, '..')
const templatePath = path.join(repoRoot, 'deploy/nginx/public-atm-relay.conf.template')

const shellQuote = value => `'${String(value).replace(/'/g, `'\\''`)}'`

const runCommand = (command, args, { env = process.env, capture = false } = {}) =>
  new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: repoRoot,
      env,
      stdio: capture ? ['ignore', 'pipe', 'pipe'] : 'inherit'
    })

    let stdout = ''
    let stderr = ''

    if (capture) {
      child.stdout.on('data', chunk => {
        stdout += chunk.toString()
      })
      child.stderr.on('data', chunk => {
        stderr += chunk.toString()
      })
    }

    child.on('error', reject)
    child.on('exit', code => {
      if (code === 0) {
        resolve({ stdout, stderr })
        return
      }

      reject(
        new Error(
          `${command} ${args.join(' ')} exited with code ${code ?? 'unknown'}${stderr ? `\n${stderr}` : ''}`
        )
      )
    })
  })

const runScript = (relativeScriptPath, settings) =>
  runCommand(relativeScriptPath, [], {
    env: {
      ...process.env,
      ...settings
    }
  })

const main = async () => {
  const settings = await resolveRelaySettings()
  const template = await readFile(templatePath, 'utf8')
  const relaySnippet = renderRelayNginxTemplate(template, settings)

  console.log(`[deploy] Fetching remote nginx site: ${settings.ATM_RELAY_NGINX_SITE_PATH}`)
  const { stdout: remoteSiteConfig } = await runCommand(
    'ssh',
    [settings.ATM_RELAY_HOST, `sudo cat ${shellQuote(settings.ATM_RELAY_NGINX_SITE_PATH)}`],
    { capture: true }
  )

  const nextSiteConfig = upsertManagedRelayBlock(remoteSiteConfig, relaySnippet)

  const tempDir = await mkdtemp(path.join(os.tmpdir(), 'atm-relay-deploy-'))
  const localRenderedPath = path.join(tempDir, 'nginx-site.conf')

  try {
    await writeFile(localRenderedPath, nextSiteConfig)

    console.log(`[deploy] Uploading rendered config to ${settings.ATM_RELAY_HOST}:${settings.ATM_RELAY_REMOTE_TMP_PATH}`)
    await runCommand('scp', [localRenderedPath, `${settings.ATM_RELAY_HOST}:${settings.ATM_RELAY_REMOTE_TMP_PATH}`])

    const remoteDir = path.posix.dirname(settings.ATM_RELAY_NGINX_SITE_PATH)
    const remoteApplyCommand = [
      `sudo install -d -m 755 ${shellQuote(remoteDir)}`,
      `sudo install -m 644 ${shellQuote(settings.ATM_RELAY_REMOTE_TMP_PATH)} ${shellQuote(settings.ATM_RELAY_NGINX_SITE_PATH)}`,
      `rm -f ${shellQuote(settings.ATM_RELAY_REMOTE_TMP_PATH)}`,
      settings.ATM_RELAY_NGINX_VALIDATE_COMMAND,
      settings.ATM_RELAY_NGINX_RELOAD_COMMAND
    ].join(' && ')

    console.log('[deploy] Applying nginx config and reloading server')
    await runCommand('ssh', [settings.ATM_RELAY_HOST, remoteApplyCommand])

    console.log('[deploy] Ensuring reverse tunnel is running')
    await runScript('./scripts/start_remote_relay.sh', settings)

    console.log('[deploy] Running end-to-end relay health check')
    await runScript('./scripts/check_remote_relay.sh', settings)
  } finally {
    await rm(tempDir, { recursive: true, force: true })
  }
}

main().catch(error => {
  console.error(`[deploy] ${error.message}`)
  process.exit(1)
})
