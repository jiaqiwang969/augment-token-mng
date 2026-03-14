import test from 'node:test'
import assert from 'node:assert/strict'

const loadModule = async () => {
  try {
    return await import('../src/utils/codexTeamUi.js')
  } catch (error) {
    assert.fail(`codexTeamUi helper is missing: ${error.message}`)
  }
}

test('filterTeamSeriesByVisibleMembers keeps only visible member codes', async () => {
  const { filterTeamSeriesByVisibleMembers } = await loadModule()

  const input = [
    { profileId: 'codex-jdd', memberCode: 'jdd', stats: [{ date: '2026-03-01', requests: 3, tokens: 300 }] },
    { profileId: 'codex-jqw', memberCode: 'jqw', stats: [{ date: '2026-03-01', requests: 5, tokens: 500 }] },
    { profileId: 'codex-cr', memberCode: 'cr', stats: [{ date: '2026-03-01', requests: 2, tokens: 200 }] }
  ]

  const output = filterTeamSeriesByVisibleMembers(input, ['jqw', 'cr'])

  assert.deepEqual(
    output.map(entry => entry.memberCode),
    ['jqw', 'cr']
  )
})

test('filterMemberRankingByVisibleMembers keeps only visible member codes', async () => {
  const { filterMemberRankingByVisibleMembers } = await loadModule()

  const rows = [
    { id: 'codex-jdd', memberCode: 'jdd', totalTokens: 900 },
    { id: 'codex-jqw', memberCode: 'jqw', totalTokens: 1200 },
    { id: 'codex-cr', memberCode: 'cr', totalTokens: 600 }
  ]

  const output = filterMemberRankingByVisibleMembers(rows, ['jdd'])

  assert.equal(output.length, 1)
  assert.equal(output[0].memberCode, 'jdd')
})

test('empty visible-member selection leaves ranking and series unfiltered', async () => {
  const { filterTeamSeriesByVisibleMembers, filterMemberRankingByVisibleMembers } = await loadModule()

  const series = [
    { profileId: 'codex-jdd', memberCode: 'jdd', stats: [] },
    { profileId: 'codex-jqw', memberCode: 'jqw', stats: [] }
  ]
  const rows = [
    { id: 'codex-jdd', memberCode: 'jdd' },
    { id: 'codex-jqw', memberCode: 'jqw' }
  ]

  assert.equal(filterTeamSeriesByVisibleMembers(series, []).length, 2)
  assert.equal(filterMemberRankingByVisibleMembers(rows, []).length, 2)
})

test('buildAllMembersAccessBundle emits one public bundle block per member', async () => {
  const { buildAllMembersAccessBundle } = await loadModule()

  const output = buildAllMembersAccessBundle({
    baseUrl: 'https://lingkong.xyz/v1',
    profiles: [
      {
        name: '姜大大',
        memberCode: 'jdd',
        apiKey: 'sk-team-jdd-abc12345'
      },
      {
        name: '佳琪',
        memberCode: 'jqw',
        apiKey: 'sk-team-jqw-def67890'
      }
    ]
  })

  assert.match(output, /# 姜大大 · jdd/)
  assert.match(output, /OPENAI_BASE_URL=https:\/\/lingkong\.xyz\/v1/)
  assert.match(output, /OPENAI_API_KEY=sk-team-jdd-abc12345/)
  assert.match(output, /# 佳琪 · jqw/)
  assert.match(output, /OPENAI_API_KEY=sk-team-jqw-def67890/)
})
