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

test('filterTeamSeriesBySelectedProfiles keeps only selected profile ids', async () => {
  const { filterTeamSeriesBySelectedProfiles } = await loadModule()

  const series = [
    { profileId: 'codex-jdd', memberCode: 'jdd', stats: [] },
    { profileId: 'codex-jqw', memberCode: 'jqw', stats: [] },
    { profileId: 'codex-cr', memberCode: 'cr', stats: [] }
  ]

  const output = filterTeamSeriesBySelectedProfiles(series, ['codex-jqw', 'codex-cr'])

  assert.deepEqual(
    output.map(entry => entry.profileId),
    ['codex-jqw', 'codex-cr']
  )
})

test('empty table selection returns no visible series', async () => {
  const { filterTeamSeriesBySelectedProfiles } = await loadModule()

  const series = [
    { profileId: 'codex-jdd', memberCode: 'jdd', stats: [] },
    { profileId: 'codex-jqw', memberCode: 'jqw', stats: [] }
  ]

  assert.equal(filterTeamSeriesBySelectedProfiles(series, []).length, 0)
})

test('filterTeamSeriesBySelectedProfiles pads selected series with zero-value dates', async () => {
  const { filterTeamSeriesBySelectedProfiles } = await loadModule()

  const series = [
    {
      profileId: 'codex-jdd',
      profileName: '姜大大',
      memberCode: 'jdd',
      stats: [
        { date: '2026-03-01', requests: 1, tokens: 100 },
        { date: '2026-03-03', requests: 3, tokens: 300 }
      ]
    },
    {
      profileId: 'codex-jqw',
      profileName: '佳琪',
      memberCode: 'jqw',
      stats: [
        { date: '2026-03-02', requests: 2, tokens: 200 }
      ]
    }
  ]

  const profiles = [
    { id: 'codex-jdd', name: '姜大大', memberCode: 'jdd', roleTitle: '产品与方法论', color: '#4c6ef5' },
    { id: 'codex-cr', name: '春荣', memberCode: 'cr', roleTitle: '交付与协作', color: '#f08c00' }
  ]

  const output = filterTeamSeriesBySelectedProfiles(series, ['codex-jdd', 'codex-cr'], profiles)

  assert.deepEqual(output.map(entry => entry.profileId), ['codex-jdd', 'codex-cr'])
  assert.deepEqual(output[0].stats.map(point => point.date), ['2026-03-01', '2026-03-02', '2026-03-03'])
  assert.deepEqual(output[0].stats.map(point => point.requests), [1, 0, 3])
  assert.deepEqual(output[1].stats.map(point => point.requests), [0, 0, 0])
  assert.equal(output[1].profileName, '春荣')
})

test('syncSelectedProfileIds selects all profiles on first load', async () => {
  const { syncSelectedProfileIds } = await loadModule()

  const output = syncSelectedProfileIds({
    profiles: [
      { id: 'codex-jdd' },
      { id: 'codex-jqw' }
    ],
    selectedProfileIds: [],
    previousProfileIds: []
  })

  assert.deepEqual(output, ['codex-jdd', 'codex-jqw'])
})

test('syncSelectedProfileIds keeps remaining profiles selected after a deletion when all were previously selected', async () => {
  const { syncSelectedProfileIds } = await loadModule()

  const output = syncSelectedProfileIds({
    profiles: [
      { id: 'codex-jdd' },
      { id: 'codex-cr' }
    ],
    selectedProfileIds: ['codex-jdd', 'codex-jqw', 'codex-cr'],
    previousProfileIds: ['codex-jdd', 'codex-jqw', 'codex-cr']
  })

  assert.deepEqual(output, ['codex-jdd', 'codex-cr'])
})

test('syncSelectedProfileIds preserves manual empty selection after initialization', async () => {
  const { syncSelectedProfileIds } = await loadModule()

  const output = syncSelectedProfileIds({
    profiles: [
      { id: 'codex-jdd' },
      { id: 'codex-jqw' }
    ],
    selectedProfileIds: [],
    previousProfileIds: ['codex-jdd', 'codex-jqw']
  })

  assert.deepEqual(output, [])
})

test('syncSelectedProfileIds defaults to all current profiles on first load', async () => {
  const { syncSelectedProfileIds } = await loadModule()

  const output = syncSelectedProfileIds({
    profiles: [
      { id: 'codex-jdd' },
      { id: 'codex-jqw' },
      { id: 'codex-cr' }
    ],
    previousProfiles: [],
    selectedProfileIds: []
  })

  assert.deepEqual(output, ['codex-jdd', 'codex-jqw', 'codex-cr'])
})

test('syncSelectedProfileIds removes deleted members from explicit selection', async () => {
  const { syncSelectedProfileIds } = await loadModule()

  const output = syncSelectedProfileIds({
    profiles: [
      { id: 'codex-jdd' },
      { id: 'codex-cr' }
    ],
    previousProfiles: [
      { id: 'codex-jdd' },
      { id: 'codex-jqw' },
      { id: 'codex-cr' }
    ],
    selectedProfileIds: ['codex-jdd', 'codex-jqw']
  })

  assert.deepEqual(output, ['codex-jdd'])
})

test('syncSelectedProfileIds keeps newly added members selected when previous state was all-selected', async () => {
  const { syncSelectedProfileIds } = await loadModule()

  const output = syncSelectedProfileIds({
    profiles: [
      { id: 'codex-jdd' },
      { id: 'codex-jqw' },
      { id: 'codex-cr' }
    ],
    previousProfiles: [
      { id: 'codex-jdd' },
      { id: 'codex-jqw' }
    ],
    selectedProfileIds: ['codex-jdd', 'codex-jqw']
  })

  assert.deepEqual(output, ['codex-jdd', 'codex-jqw', 'codex-cr'])
})

test('resolveFocusedProfileId keeps current focus when possible and falls back to first visible member', async () => {
  const { resolveFocusedProfileId } = await loadModule()

  assert.equal(
    resolveFocusedProfileId({
      profiles: [
        { id: 'codex-jdd' },
        { id: 'codex-jqw' }
      ],
      focusedProfileId: 'codex-jqw'
    }),
    'codex-jqw'
  )

  assert.equal(
    resolveFocusedProfileId({
      profiles: [
        { id: 'codex-jdd' },
        { id: 'codex-jqw' }
      ],
      focusedProfileId: 'codex-cr'
    }),
    'codex-jdd'
  )

  assert.equal(
    resolveFocusedProfileId({
      profiles: [],
      focusedProfileId: 'codex-jdd'
    }),
    ''
  )
})

test('buildSelectedProfileSeries fills zero-value stats for selected members without activity', async () => {
  const { buildSelectedProfileSeries } = await loadModule()

  const output = buildSelectedProfileSeries({
    series: [
      {
        profileId: 'codex-jdd',
        profileName: '姜大大',
        memberCode: 'jdd',
        roleTitle: '产品与方法论',
        color: '#4c6ef5',
        stats: [
          { date: '2026-03-13', requests: 2, tokens: 200 },
          { date: '2026-03-14', requests: 1, tokens: 100 }
        ]
      }
    ],
    selectedProfileIds: ['codex-jdd', 'codex-jqw'],
    profiles: [
      { id: 'codex-jdd', name: '姜大大', memberCode: 'jdd', roleTitle: '产品与方法论', rowColor: '#4c6ef5' },
      { id: 'codex-jqw', name: '佳琪', memberCode: 'jqw', roleTitle: '架构与趋势', rowColor: '#0ea5e9' }
    ]
  })

  assert.deepEqual(output.map(entry => entry.profileId), ['codex-jdd', 'codex-jqw'])
  assert.deepEqual(
    output[1].stats,
    [
      { date: '2026-03-13', requests: 0, tokens: 0 },
      { date: '2026-03-14', requests: 0, tokens: 0 }
    ]
  )
  assert.equal(output[1].profileName, '佳琪')
  assert.equal(output[1].memberCode, 'jqw')
  assert.equal(output[1].color, '#0ea5e9')
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

test('buildTeamMemberRows merges analytics and sorts built-in members before custom rows', async () => {
  const { buildTeamMemberRows } = await loadModule()

  const rows = buildTeamMemberRows({
    profiles: [
      {
        id: 'custom-1',
        name: '外部顾问',
        memberCode: 'mentor',
        roleTitle: '顾问',
        apiKey: 'sk-team-mentor-ff001122',
        enabled: true,
        color: '#a855f7'
      },
      {
        id: 'codex-jqw',
        name: '佳琪',
        memberCode: 'jqw',
        roleTitle: '架构与趋势',
        apiKey: 'sk-team-jqw-3f8d10ab',
        enabled: true,
        color: '#0ea5e9'
      },
      {
        id: 'codex-jdd',
        name: '姜大大',
        memberCode: 'jdd',
        roleTitle: '产品与方法论',
        apiKey: 'sk-team-jdd-a4f29c7e',
        enabled: false,
        color: '#4c6ef5'
      }
    ],
    analyticsByProfileId: new Map([
      ['codex-jdd', {
        todayRequests: 3,
        todayTokens: 300,
        requests: 20,
        totalTokens: 2000,
        successRate: 95.1,
        averageDurationMs: 1200,
        lastActiveTs: 1710000000
      }],
      ['custom-1', {
        todayRequests: 1,
        todayTokens: 80,
        requests: 4,
        totalTokens: 500,
        successRate: 100,
        averageDurationMs: 800,
        lastActiveTs: 1710000100
      }]
    ])
  })

  assert.deepEqual(
    rows.map(row => row.id),
    ['codex-jdd', 'codex-jqw', 'custom-1']
  )
  assert.equal(rows[0].displayLabel, '姜大大 · jdd · 产品与方法论')
  assert.equal(rows[0].keySuffix, 'a4f29c7e')
  assert.equal(rows[0].todayRequests, 3)
  assert.equal(rows[0].totalTokens, 2000)
  assert.equal(rows[1].todayRequests, 0)
  assert.equal(rows[2].isBuiltinMember, false)
  assert.equal(rows[2].keySuffix, 'ff001122')
})

test('buildTeamMemberRows tolerates missing arrays and maps', async () => {
  const { buildTeamMemberRows } = await loadModule()

  assert.deepEqual(buildTeamMemberRows({ profiles: null, analyticsByProfileId: null }), [])
})
