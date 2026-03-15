import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import path from 'node:path'

const dialogPath = path.resolve('src/components/antigravity/AntigravityServerDialog.vue')
const managerPath = path.resolve('src/components/platform/AntigravityAccountManager.vue')
const editorPath = path.resolve('src/components/antigravity/AntigravityTeamMemberEditorModal.vue')

const readSource = async (targetPath, label) => {
  try {
    return await fs.readFile(targetPath, 'utf8')
  } catch (error) {
    assert.fail(`failed to read ${label}: ${error.message}`)
  }
}

test('Antigravity account manager exposes the API service entry and dialog wiring', async () => {
  const source = await readSource(managerPath, 'AntigravityAccountManager.vue')

  assert.match(source, /showAntigravityServerDialog = true/)
  assert.match(source, /platform\.antigravity\.apiService\.buttonTooltip/)
  assert.match(source, /<AntigravityServerDialog/)
  assert.match(source, /v-if="showAntigravityServerDialog"/)
  assert.match(source, /@close="showAntigravityServerDialog = false"/)
})

test('Antigravity server dialog defaults to overview and keeps storage analytics visible', async () => {
  const source = await readSource(dialogPath, 'AntigravityServerDialog.vue')
  const storageSection = source.match(
    /storageOverviewTitle[\s\S]*?maintenanceTitle/
  )?.[0]

  assert.match(source, /const activeTab = ref\('overview'\)/)
  assert.match(source, /platform\.antigravity\.apiService\.tabOverview/)
  assert.match(source, /platform\.antigravity\.apiService\.tabRequestLogs/)
  assert.ok(storageSection, 'storage overview section should exist')
  assert.match(storageSection, /platform\.antigravity\.apiService\.storageTotalLogs/)
  assert.match(storageSection, /platform\.antigravity\.apiService\.storageDbSize/)
  assert.match(storageSection, /platform\.antigravity\.apiService\.allTimeRequests/)
  assert.match(storageSection, /platform\.antigravity\.apiService\.allTimeTokens/)
  assert.match(storageSection, /platform\.antigravity\.apiService\.todayRequests/)
  assert.match(storageSection, /platform\.antigravity\.apiService\.todayTokens/)
  assert.match(storageSection, /platform\.antigravity\.apiService\.weekRequests/)
  assert.match(storageSection, /platform\.antigravity\.apiService\.weekTokens/)
  assert.match(storageSection, /platform\.antigravity\.apiService\.monthRequests/)
  assert.match(storageSection, /platform\.antigravity\.apiService\.monthTokens/)
})

test('Antigravity server dialog keeps maintenance actions and model usage table in the logs workflow', async () => {
  const source = await readSource(dialogPath, 'AntigravityServerDialog.vue')

  assert.match(source, /platform\.antigravity\.apiService\.maintenanceTitle/)
  assert.match(source, /@click="clearLogs"/)
  assert.match(source, /@click="deleteLogsBeforeDate"/)
  assert.match(source, /platform\.antigravity\.apiService\.modelUsageTitle/)
  assert.match(source, /const modelUsageRows = computed\(/)
  assert.match(source, /get_antigravity_model_stats_from_storage/)
})

test('Antigravity server dialog shows filtered log success and error summary cards', async () => {
  const source = await readSource(dialogPath, 'AntigravityServerDialog.vue')

  assert.match(source, /platform\.antigravity\.apiService\.logSummaryTitle/)
  assert.match(source, /platform\.antigravity\.apiService\.successfulRequests/)
  assert.match(source, /platform\.antigravity\.apiService\.failedRequests/)
  assert.match(source, /const logSummary = ref\(/)
  assert.match(source, /get_antigravity_log_summary_from_storage/)
})

test('Antigravity server dialog keeps a silent auto refresh loop while the modal is open', async () => {
  const source = await readSource(dialogPath, 'AntigravityServerDialog.vue')

  assert.match(source, /const AUTO_REFRESH_INTERVAL_MS = 15000/)
  assert.match(source, /let autoRefreshTimer = null/)
  assert.match(source, /refreshAllData\(\{ refreshGatewayProfiles: false, silent: true \}\)/)
  assert.match(source, /window\.setInterval\(/)
  assert.match(source, /clearInterval\(autoRefreshTimer\)/)
})

test('Antigravity access bundle UI uses dedicated ANTIGRAVITY env names', async () => {
  const dialogSource = await readSource(dialogPath, 'AntigravityServerDialog.vue')
  const editorSource = await readSource(editorPath, 'AntigravityTeamMemberEditorModal.vue')

  assert.match(dialogSource, /buildAntigravityAccessBundle/)
  assert.doesNotMatch(dialogSource, /buildAllMembersAccessBundle/)
  assert.match(editorSource, /ANTIGRAVITY_BASE_URL=/)
  assert.match(editorSource, /ANTIGRAVITY_GEMINI_BASE_URL=/)
  assert.match(editorSource, /buildAntigravityGeminiBaseUrl/)
  assert.match(editorSource, /ANTIGRAVITY_API_KEY=/)
  assert.doesNotMatch(editorSource, /OPENAI_BASE_URL=|OPENAI_API_KEY=/)
})

test('Antigravity server dialog overview exposes Gemini native gateway URLs', async () => {
  const source = await readSource(dialogPath, 'AntigravityServerDialog.vue')

  assert.match(source, /buildAntigravityGeminiBaseUrl/)
  assert.match(source, /geminiLocalServerUrl/)
  assert.match(source, /geminiPublicServerUrl/)
})
