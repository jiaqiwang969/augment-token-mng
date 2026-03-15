import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import path from 'node:path'

const dialogPath = path.resolve('src/components/openai/CodexServerDialog.vue')

const readDialogSource = async () => {
  try {
    return await fs.readFile(dialogPath, 'utf8')
  } catch (error) {
    assert.fail(`failed to read CodexServerDialog.vue: ${error.message}`)
  }
}

test('CodexServerDialog no longer references removed team dashboard state', async () => {
  const source = await readDialogSource()

  assert.doesNotMatch(source, /\bteamMemberCards\b/, 'stale teamMemberCards reference should be removed')
  assert.doesNotMatch(
    source,
    /\bsyncTeamDashboardState\b/,
    'stale syncTeamDashboardState reference should be removed'
  )
})

test('CodexServerDialog uses explicit member selection sync and chart series helpers', async () => {
  const source = await readDialogSource()

  assert.match(
    source,
    /watch\(memberTableRows,\s*\(profiles,\s*previousProfiles\)\s*=>\s*\{\s*selectedMemberProfileIds\.value\s*=\s*syncSelectedProfileIds\(\s*\{/s
  )
  assert.match(source, /buildSelectedProfileSeries\(\s*\{\s*series:\s*dailyStatsSeries\.value,\s*selectedProfileIds:\s*selectedMemberProfileIds\.value,\s*profiles:\s*memberTableRows\.value/s)
})

test('CodexServerDialog member overview table does not expose gateway key details', async () => {
  const source = await readDialogSource()
  const tableSegment = source.match(
    /<div class="max-h-\[420px\] overflow-auto">[\s\S]*?<\/table>/
  )?.[0]

  assert.ok(tableSegment, 'member overview table should exist')
  assert.doesNotMatch(tableSegment, /codexDialog\.memberKey/, 'member table should not render a dedicated key block')
  assert.doesNotMatch(tableSegment, /keySuffix/, 'member table should not render key suffix values')
})

test('CodexServerDialog exposes both select-all and deselect-all controls for member trends', async () => {
  const source = await readDialogSource()

  assert.match(source, /@click="selectAllMembers"/)
  assert.match(source, /codexDialog\.selectAllMembers/)
  assert.match(source, /@click="clearMemberSelection"/)
})

test('CodexServerDialog restores table-first member focus with detail card below the table', async () => {
  const source = await readDialogSource()

  assert.match(source, /<div class="max-h-\[420px\] overflow-auto">/)
  assert.match(source, /const focusedMemberProfileId = ref\(''\)/)
  assert.match(source, /const focusedMemberRow = computed\(/)
  assert.match(source, /@click="setFocusedMember\(profile\.id\)"/)
  assert.match(source, /codexDialog\.selectedMemberLabel/)
  assert.match(source, /codexDialog\.selectedMemberHint/)
  assert.doesNotMatch(source, /setFocusedMember\(profile\.id,\s*close\)/, 'focused member should no longer rely on a dropdown close handler')
})

test('CodexServerDialog renders a token-share pie chart from filtered member series', async () => {
  const source = await readDialogSource()

  assert.match(source, /import CodexUsagePieChart from '@\/components\/openai\/CodexUsagePieChart\.vue'/)
  assert.match(source, /buildTokenShareSeries/)
  assert.match(source, /const tokenShareSeries = computed\(\(\) =>\s*buildTokenShareSeries\(filteredDailyStatsSeries\.value\)\)/)
  assert.match(source, /<CodexUsagePieChart :chart-data="tokenShareSeries" \/>/)
})
