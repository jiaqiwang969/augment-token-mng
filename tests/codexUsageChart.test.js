import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import path from 'node:path'

const chartPath = path.resolve('src/components/openai/CodexUsageChart.vue')

const readChartSource = async () => {
  try {
    return await fs.readFile(chartPath, 'utf8')
  } catch (error) {
    assert.fail(`failed to read CodexUsageChart.vue: ${error.message}`)
  }
}

test('CodexUsageChart treats zero-filled selected series as chart data', async () => {
  const source = await readChartSource()

  assert.doesNotMatch(
    source,
    /Number\(point\.requests \|\| 0\) > 0 \|\| Number\(point\.tokens \|\| 0\) > 0/,
    'all-zero series should not be hidden as empty data'
  )
  assert.match(
    source,
    /normalizedSeries\.value\.some\(series\s*=>\s*series\.stats\.length > 0\)/,
    'any selected series with points should render even if the values are zero'
  )
})

test('CodexUsageChart uses compact legend labels while preserving rich tooltip member labels', async () => {
  const source = await readChartSource()

  assert.match(source, /const formatLegendLabel = \(series\) =>/)
  assert.match(source, /return memberCode \|\| profileName \|\| profileId/)
  assert.match(source, /label:\s*`\$\{formatLegendLabel\(series\)\} · \$\{metricLabel\}`/)
  assert.match(source, /memberLabel:\s*formatSeriesLabel\(series\)/)
  assert.match(source, /context\.dataset\?\.memberLabel \|\| context\.dataset\?\.label/)
})
