import assert from 'node:assert/strict'

import {
  buildAntigravityAccessBundle,
  buildAntigravityAnalyticsByProfileId,
  formatAntigravityBytes,
  buildAntigravityMaintenanceSummary,
  buildAntigravityModelUsageRows,
  buildAntigravityTokenShareChartData,
  buildAntigravityLogMemberOptions,
  toAntigravityDeleteBeforeDateKey
} from '../src/utils/antigravityApiServiceUi.js'

const now = Date.parse('2026-03-15T12:00:00Z') / 1000

const logs = [
  {
    id: '1',
    timestamp: now,
    gatewayProfileId: 'ant-jdd',
    gatewayProfileName: '姜大大',
    memberCode: 'jdd',
    roleTitle: '产品与方法论',
    totalTokens: 200,
    status: 'success',
    requestDurationMs: 200,
    color: '#4c6ef5'
  },
  {
    id: '2',
    timestamp: now,
    gatewayProfileId: 'ant-jdd',
    gatewayProfileName: '姜大大',
    memberCode: 'jdd',
    roleTitle: '产品与方法论',
    totalTokens: 150,
    status: 'error',
    requestDurationMs: 400,
    color: '#4c6ef5'
  },
  {
    id: '3',
    timestamp: now,
    gatewayProfileId: 'ant-cr',
    gatewayProfileName: 'CR',
    memberCode: 'cr',
    roleTitle: '执行导向',
    totalTokens: 300,
    status: 'success',
    requestDurationMs: 300,
    color: '#12b886'
  }
]

const analyticsMap = buildAntigravityAnalyticsByProfileId(logs, {
  nowTs: now
})

assert.equal(analyticsMap.get('ant-jdd').requests, 2)
assert.equal(analyticsMap.get('ant-jdd').todayTokens, 350)
assert.equal(Math.round(analyticsMap.get('ant-jdd').successRate), 50)
assert.equal(analyticsMap.get('ant-cr').totalTokens, 300)

const memberRows = [
  {
    id: 'ant-jdd',
    memberCode: 'jdd',
    displayLabel: '姜大大 · jdd',
    rowColor: '#4c6ef5',
    totalTokens: 350
  },
  {
    id: 'ant-cr',
    memberCode: 'cr',
    displayLabel: 'CR · cr',
    rowColor: '#12b886',
    totalTokens: 300
  }
]

const pie = buildAntigravityTokenShareChartData({
  memberRows,
  selectedProfileIds: ['ant-jdd']
})

assert.deepEqual(pie.labels, ['jdd'])
assert.deepEqual(pie.datasets[0].data, [350])
assert.equal(pie.totalTokens, 350)

const accessBundle = buildAntigravityAccessBundle({
  baseUrl: 'https://lingkong.xyz/v1',
  profiles: [
    {
      name: '佳琪',
      memberCode: 'jqw',
      apiKey: 'sk-ant-jqw-01ad3e0b'
    }
  ]
})

assert.match(accessBundle, /# 佳琪 · jqw/)
assert.match(accessBundle, /ANTIGRAVITY_BASE_URL=https:\/\/lingkong\.xyz\/v1/)
assert.match(accessBundle, /ANTIGRAVITY_API_KEY=sk-ant-jqw-01ad3e0b/)
assert.doesNotMatch(accessBundle, /OPENAI_BASE_URL|OPENAI_API_KEY/)

const memberOptions = buildAntigravityLogMemberOptions(memberRows)

assert.deepEqual(memberOptions, [
  { value: 'cr', label: 'cr' },
  { value: 'jdd', label: 'jdd' }
])

const maintenance = buildAntigravityMaintenanceSummary({
  serviceStatus: {
    totalAccounts: 10,
    availableAccounts: 8
  },
  storageStatus: {
    totalLogs: 128,
    dbSizeBytes: 1024 * 1024 * 12
  },
  allTimeStats: {
    requests: 88,
    tokens: 4567
  },
  periodStats: {
    todayRequests: 5,
    todayTokens: 400,
    weekRequests: 21,
    weekTokens: 2100,
    monthRequests: 77,
    monthTokens: 4300
  }
})

assert.equal(maintenance.totalLogs, 128)
assert.equal(maintenance.dbSizeBytes, 1024 * 1024 * 12)
assert.equal(maintenance.monthTokens, 4300)
assert.equal(maintenance.accountCoverageLabel, '8 / 10')
assert.equal(formatAntigravityBytes(0), '0 B')
assert.equal(formatAntigravityBytes(1536), '1.5 KB')
assert.equal(formatAntigravityBytes(1024 * 1024 * 12), '12 MB')

const modelRows = buildAntigravityModelUsageRows([
  { model: 'claude-haiku', requests: 10, totalTokens: 1000 },
  { model: 'claude-sonnet', requests: 2, totalTokens: 3000 },
  { model: 'gemini-2.5-pro', requests: 7, totalTokens: 2000 }
])

assert.deepEqual(
  modelRows.map(item => item.model),
  ['claude-sonnet', 'gemini-2.5-pro', 'claude-haiku']
)
assert.equal(modelRows[0].share, 50)

assert.equal(toAntigravityDeleteBeforeDateKey('2026-03-15'), 20260315)
assert.equal(toAntigravityDeleteBeforeDateKey('2026-3-15'), null)
assert.equal(toAntigravityDeleteBeforeDateKey(''), null)

console.log('antigravity api service ui helpers ok')
