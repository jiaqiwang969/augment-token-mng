import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import path from 'node:path'

const appPath = path.resolve('src/App.vue')
const dialogPath = path.resolve('src/components/openai/CodexServerDialog.vue')

const readSource = async (targetPath) => {
  try {
    return await fs.readFile(targetPath, 'utf8')
  } catch (error) {
    assert.fail(`failed to read ${targetPath}: ${error.message}`)
  }
}

test('App sidebar renders a global Codex relay health status chip', async () => {
  const source = await readSource(appPath)

  assert.match(source, /import CodexRelayStatusChip from '\.\/components\/openai\/CodexRelayStatusChip\.vue'/)
  assert.match(source, /<CodexRelayStatusChip :collapsed="isSidebarCollapsed" \/>/)
})

test('App sidebar places the relay chip in the narrow footer rail below the control buttons', async () => {
  const source = await readSource(appPath)

  assert.match(source, /Theme Toggle[\s\S]*<CodexRelayStatusChip :collapsed="isSidebarCollapsed" \/>/)
})

test('CodexServerDialog wires relay health commands and event updates', async () => {
  const source = await readSource(dialogPath)

  assert.match(source, /invoke\('get_codex_relay_health_status'\)/)
  assert.match(source, /invoke\('refresh_codex_relay_health_status'\)/)
  assert.match(source, /invoke\('repair_codex_relay_health'\)/)
  assert.match(source, /listen\('codex-relay-health-changed'/)
})

test('CodexServerDialog renders a detailed relay health panel with repair actions', async () => {
  const source = await readSource(dialogPath)

  assert.match(source, /codexDialog\.relayHealthTitle/)
  assert.match(source, /codexDialog\.relayRepairNow/)
  assert.match(source, /codexDialog\.relayStatusLocal/)
  assert.match(source, /codexDialog\.relayStatusPublic/)
  assert.match(source, /relayHealthRows/)
})

test('CodexServerDialog uses the configured public relay URL instead of a hardcoded default', async () => {
  const source = await readSource(dialogPath)

  assert.doesNotMatch(source, /const publicServerUrl = 'https:\/\/lingkong\.xyz\/v1'/)
  assert.match(source, /publicBaseUrl/)
})

test('CodexServerDialog only shows relay repair success when the repaired snapshot is actually healthy', async () => {
  const source = await readSource(dialogPath)

  assert.match(source, /const relayRepairSucceeded = \(snapshot\) =>/)
  assert.match(source, /if \(relayRepairSucceeded\(snapshot\)\) \{\s*window\.\$notify\?\.success/s)
  assert.match(source, /else \{\s*window\.\$notify\?\.error\(/s)
})
