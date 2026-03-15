import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import path from 'node:path'
import os from 'node:os'
import { execFile as execFileCallback } from 'node:child_process'
import { promisify } from 'node:util'

const repoRoot = path.resolve('.')
const makefilePath = path.join(repoRoot, 'Makefile')
const gitignorePath = path.join(repoRoot, '.gitignore')
const agentsPath = path.join(repoRoot, 'AGENTS.md')
const envExamplePath = path.join(repoRoot, '.env.antigravity.example')
const envLoaderPath = path.join(repoRoot, 'scripts/load_antigravity_env.sh')
const envBootstrapPath = path.join(repoRoot, 'scripts/bootstrap_antigravity_env.sh')
const envCheckPath = path.join(repoRoot, 'scripts/check_antigravity_env.sh')
const oauthModulePath = path.join(repoRoot, 'src-tauri/src/platforms/antigravity/modules/oauth.rs')
const execFile = promisify(execFileCallback)

const readFile = async (targetPath, label = path.basename(targetPath)) => {
  try {
    return await fs.readFile(targetPath, 'utf8')
  } catch (error) {
    assert.fail(`failed to read ${label}: ${error.message}`)
  }
}

test('Makefile dev targets load antigravity env files before starting Tauri', async () => {
  const makefile = await readFile(makefilePath, 'Makefile')

  assert.match(makefile, /load_antigravity_env\.sh/, 'Makefile should source the Antigravity env loader')
  assert.match(makefile, /load_antigravity_env && ATM_SKIP_CLIPROXY_BUILD=1 npx tauri dev/)
  assert.match(makefile, /load_antigravity_env && npx tauri dev/)
})

test('Makefile exposes Antigravity bootstrap and env check targets', async () => {
  const makefile = await readFile(makefilePath, 'Makefile')

  assert.match(makefile, /^\.PHONY: .*antigravity-env-bootstrap.*antigravity-env-check/m)
  assert.match(makefile, /^antigravity-env-bootstrap:\s*$/m)
  assert.match(makefile, /^antigravity-env-check:\s*$/m)
})

test('.gitignore excludes real antigravity env files while example file remains tracked', async () => {
  const gitignore = await readFile(gitignorePath, '.gitignore')

  assert.match(gitignore, /^\.env\.antigravity$/m)
  assert.doesNotMatch(gitignore, /^\.env\.antigravity\.example$/m)
})

test('antigravity env example exposes the required OAuth keys', async () => {
  const envExample = await readFile(envExamplePath, '.env.antigravity.example')

  assert.match(envExample, /ATM_ANTIGRAVITY_OAUTH_CLIENT_ID=/)
  assert.match(envExample, /ATM_ANTIGRAVITY_OAUTH_CLIENT_SECRET=/)
})

test('repository AGENTS documents the antigravity env workflow and secret hygiene', async () => {
  const agents = await readFile(agentsPath, 'AGENTS.md')

  assert.match(agents, /\.env\.antigravity/)
  assert.match(agents, /make dev/)
  assert.match(agents, /make antigravity-env-bootstrap/)
  assert.match(agents, /make antigravity-env-check/)
  assert.match(agents, /never commit/i)
  assert.match(agents, /never print/i)
  assert.match(agents, /\.env\.antigravity\.example/)
})

test('antigravity env loader supports dedicated and fallback env files', async () => {
  const loader = await readFile(envLoaderPath, 'scripts/load_antigravity_env.sh')

  assert.match(loader, /load_antigravity_env\(\)/)
  assert.match(loader, /\.env\.antigravity/)
  assert.match(loader, /\.env/)
  assert.match(loader, /set -a/)
})

test('Antigravity env bootstrap and check scripts are present', async () => {
  const [bootstrapStat, checkStat] = await Promise.all([
    fs.stat(envBootstrapPath),
    fs.stat(envCheckPath),
  ])

  assert.ok(bootstrapStat.isFile(), 'bootstrap script should exist')
  assert.ok(checkStat.isFile(), 'check script should exist')
})

test('bootstrap script materializes a local env file without echoing secret values', async () => {
  const tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'atm-antigravity-bootstrap-'))
  const envFile = path.join(tempDir, '.env.antigravity')
  const clientId = 'atm-client-id-from-test'
  const clientSecret = 'atm-client-secret-from-test'

  const { stdout, stderr } = await execFile('bash', [envBootstrapPath], {
    cwd: repoRoot,
    env: {
      ...process.env,
      ATM_ANTIGRAVITY_ENV_FILE: envFile,
      ATM_ANTIGRAVITY_OAUTH_CLIENT_ID: clientId,
      ATM_ANTIGRAVITY_OAUTH_CLIENT_SECRET: clientSecret,
    },
  })

  const envContents = await fs.readFile(envFile, 'utf8')

  assert.match(envContents, /ATM_ANTIGRAVITY_OAUTH_CLIENT_ID=atm-client-id-from-test/)
  assert.match(envContents, /ATM_ANTIGRAVITY_OAUTH_CLIENT_SECRET=atm-client-secret-from-test/)
  assert.doesNotMatch(stdout, /atm-client-id-from-test|atm-client-secret-from-test/)
  assert.doesNotMatch(stderr, /atm-client-id-from-test|atm-client-secret-from-test/)
})

test('check script validates configured env without leaking secret values', async () => {
  const tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'atm-antigravity-check-'))
  const envFile = path.join(tempDir, '.env.antigravity')
  const clientId = 'atm-check-client-id'
  const clientSecret = 'atm-check-client-secret'

  await fs.writeFile(
    envFile,
    [
      'ATM_ANTIGRAVITY_OAUTH_CLIENT_ID=atm-check-client-id',
      'ATM_ANTIGRAVITY_OAUTH_CLIENT_SECRET=atm-check-client-secret',
      '',
    ].join('\n'),
    'utf8',
  )

  const { stdout, stderr } = await execFile('bash', [envCheckPath], {
    cwd: repoRoot,
    env: {
      ...process.env,
      ATM_ANTIGRAVITY_ENV_FILE: envFile,
      ATM_ANTIGRAVITY_OAUTH_CLIENT_ID: '',
      ATM_ANTIGRAVITY_OAUTH_CLIENT_SECRET: '',
      CLIPROXY_ANTIGRAVITY_OAUTH_CLIENT_ID: '',
      CLIPROXY_ANTIGRAVITY_OAUTH_CLIENT_SECRET: '',
    },
  })

  assert.match(stdout, /configured|ready|loaded/i)
  assert.doesNotMatch(stdout, new RegExp(`${clientId}|${clientSecret}`))
  assert.doesNotMatch(stderr, new RegExp(`${clientId}|${clientSecret}`))
})

test('Antigravity OAuth module accepts both ATM and legacy CLIProxy env names', async () => {
  const oauthModule = await readFile(oauthModulePath, 'oauth.rs')

  assert.match(oauthModule, /ATM_ANTIGRAVITY_OAUTH_CLIENT_ID/)
  assert.match(oauthModule, /ATM_ANTIGRAVITY_OAUTH_CLIENT_SECRET/)
  assert.match(oauthModule, /CLIPROXY_ANTIGRAVITY_OAUTH_CLIENT_ID/)
  assert.match(oauthModule, /CLIPROXY_ANTIGRAVITY_OAUTH_CLIENT_SECRET/)
})
