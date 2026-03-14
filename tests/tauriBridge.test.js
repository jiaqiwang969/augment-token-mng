import test from 'node:test'
import assert from 'node:assert/strict'

const loadModule = async () => {
  try {
    return await import('../src/utils/tauriBridge.js')
  } catch (error) {
    assert.fail(`tauriBridge helper is missing: ${error.message}`)
  }
}

test('hasTauriBridge returns false when window internals are unavailable', async () => {
  const { hasTauriBridge } = await loadModule()

  assert.equal(hasTauriBridge(undefined), false)
  assert.equal(hasTauriBridge({}), false)
  assert.equal(hasTauriBridge({ __TAURI_INTERNALS__: {} }), false)
})

test('hasTauriBridge returns true when invoke bridge exists', async () => {
  const { hasTauriBridge } = await loadModule()

  assert.equal(
    hasTauriBridge({
      __TAURI_INTERNALS__: {
        invoke() {}
      }
    }),
    true
  )
})
