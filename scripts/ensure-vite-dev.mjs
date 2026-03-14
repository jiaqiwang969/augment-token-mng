#!/usr/bin/env node

import { spawn } from 'node:child_process'
import process from 'node:process'
import { fileURLToPath } from 'node:url'

export const DEV_SERVER_ORIGIN = 'http://localhost:1420'

const ATM_INDEX_MARKERS = ['<title>AI Tools Manager</title>']
const ATM_MAIN_ENTRY_PATTERN = /src="\/src\/main\.js(?:\?[^"]*)?"/

const RUN_DEV_COMMAND = ['run', 'dev']

export const classifyDevServerProbe = ({ viteClientOk = false, indexHtml = '' } = {}) => {
  const normalizedIndexHtml = indexHtml.trim()
  const isAtmDevServer =
    viteClientOk &&
    ATM_INDEX_MARKERS.every(marker => normalizedIndexHtml.includes(marker)) &&
    ATM_MAIN_ENTRY_PATTERN.test(normalizedIndexHtml)

  if (isAtmDevServer) {
    return {
      action: 'reuse',
      reason: 'Detected the current ATM Vite dev server on port 1420.'
    }
  }

  if (!viteClientOk && !normalizedIndexHtml) {
    return {
      action: 'start',
      reason: 'No reusable dev server is running on port 1420.'
    }
  }

  return {
    action: 'conflict',
    reason: 'Port 1420 is occupied, but it is not the ATM Vite dev server.'
  }
}

const readResponseText = async response => {
  try {
    return await response.text()
  } catch {
    return ''
  }
}

const probeUrl = async (url, fetchImpl = globalThis.fetch) => {
  if (typeof fetchImpl !== 'function') {
    throw new Error('Global fetch is unavailable in this Node runtime.')
  }

  try {
    const response = await fetchImpl(url, { redirect: 'manual' })
    return {
      reachable: true,
      ok: response.ok,
      status: response.status,
      body: await readResponseText(response)
    }
  } catch {
    return {
      reachable: false,
      ok: false,
      status: 0,
      body: ''
    }
  }
}

export const inspectDevServer = async (origin = DEV_SERVER_ORIGIN, fetchImpl = globalThis.fetch) => {
  const viteClient = await probeUrl(`${origin}/@vite/client`, fetchImpl)
  const index = await probeUrl(`${origin}/`, fetchImpl)

  return classifyDevServerProbe({
    viteClientOk: viteClient.ok,
    indexHtml: index.reachable ? index.body : ''
  })
}

const runNpmDev = async () =>
  new Promise((resolve, reject) => {
    const npmCommand = process.platform === 'win32' ? 'npm.cmd' : 'npm'
    const child = spawn(npmCommand, RUN_DEV_COMMAND, {
      stdio: 'inherit',
      env: process.env
    })

    child.on('error', reject)
    child.on('exit', code => {
      if (code === 0) {
        resolve()
        return
      }

      reject(new Error(`npm run dev exited with code ${code ?? 'unknown'}`))
    })
  })

export const ensureViteDevServer = async (origin = DEV_SERVER_ORIGIN, fetchImpl = globalThis.fetch) => {
  const decision = await inspectDevServer(origin, fetchImpl)

  if (decision.action === 'reuse') {
    console.log(`[tauri-dev] ${decision.reason}`)
    return decision
  }

  if (decision.action === 'conflict') {
    throw new Error(`${decision.reason} Stop the other process or free port 1420 before starting ATM.`)
  }

  console.log(`[tauri-dev] ${decision.reason} Starting npm run dev...`)
  await runNpmDev()
  return decision
}

const isEntrypoint = () => process.argv[1] === fileURLToPath(import.meta.url)

if (isEntrypoint()) {
  ensureViteDevServer().catch(error => {
    console.error(`[tauri-dev] ${error.message}`)
    process.exit(1)
  })
}
