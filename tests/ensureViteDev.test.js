import test from 'node:test'
import assert from 'node:assert/strict'

const loadModule = async () => {
  try {
    return await import('../scripts/ensure-vite-dev.mjs')
  } catch (error) {
    assert.fail(`ensure-vite-dev script is missing: ${error.message}`)
  }
}

test('classifies the current ATM Vite server as reusable', async () => {
  const { classifyDevServerProbe } = await loadModule()

  assert.deepEqual(
    classifyDevServerProbe({
      viteClientOk: true,
      indexHtml: `<!DOCTYPE html>
<html lang="en">
  <head>
    <script type="module" src="/@vite/client"></script>
    <title>AI Tools Manager</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.js?t=1773478572577"></script>
  </body>
</html>`
    }),
    {
      action: 'reuse',
      reason: 'Detected the current ATM Vite dev server on port 1420.'
    }
  )
})

test('classifies a missing server as start', async () => {
  const { classifyDevServerProbe } = await loadModule()

  assert.deepEqual(
    classifyDevServerProbe({
      viteClientOk: false,
      indexHtml: ''
    }),
    {
      action: 'start',
      reason: 'No reusable dev server is running on port 1420.'
    }
  )
})

test('classifies a foreign process on port 1420 as conflict', async () => {
  const { classifyDevServerProbe } = await loadModule()

  assert.deepEqual(
    classifyDevServerProbe({
      viteClientOk: false,
      indexHtml: '<title>Another App</title>'
    }),
    {
      action: 'conflict',
      reason: 'Port 1420 is occupied, but it is not the ATM Vite dev server.'
    }
  )
})
