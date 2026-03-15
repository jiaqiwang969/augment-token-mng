import test from 'node:test'
import assert from 'node:assert/strict'
import os from 'node:os'
import path from 'node:path'
import { mkdtemp, writeFile } from 'node:fs/promises'

const loadModule = async () => {
  try {
    return await import('../scripts/relayConfig.mjs')
  } catch (error) {
    assert.fail(`relayConfig helper is missing: ${error.message}`)
  }
}

test('parseDotenvText reads relay settings and ignores comments', async () => {
  const { parseDotenvText } = await loadModule()

  const parsed = parseDotenvText(`
# relay config
ATM_RELAY_HOST=ubuntu@example-host
ATM_RELAY_REMOTE_PORT=19090

ATM_RELAY_SERVER_NAME=relay.example.com
  `)

  assert.deepEqual(parsed, {
    ATM_RELAY_HOST: 'ubuntu@example-host',
    ATM_RELAY_REMOTE_PORT: '19090',
    ATM_RELAY_SERVER_NAME: 'relay.example.com'
  })
})

test('resolveRelaySettings merges .env file with process env overrides', async () => {
  const { resolveRelaySettings } = await loadModule()
  const tempDir = await mkdtemp(path.join(os.tmpdir(), 'atm-relay-config-'))
  const envPath = path.join(tempDir, '.env.relay')

  await writeFile(
    envPath,
    [
      'ATM_RELAY_HOST=ubuntu@from-file',
      'ATM_RELAY_SERVER_NAME=file.example.com',
      'ATM_RELAY_PUBLIC_BASE_URL=https://file.example.com/v1',
      'ATM_RELAY_REMOTE_PORT=19090',
      'ATM_RELAY_LOCAL_PORT=8766',
      'ATM_RELAY_NGINX_SITE_PATH=/etc/nginx/sites-enabled/relay'
    ].join('\n')
  )

  const settings = await resolveRelaySettings({
    envPath,
    env: {
      ATM_RELAY_HOST: 'ubuntu@from-env',
      ATM_RELAY_REMOTE_PORT: '29090'
    }
  })

  assert.equal(settings.ATM_RELAY_HOST, 'ubuntu@from-env')
  assert.equal(settings.ATM_RELAY_REMOTE_PORT, '29090')
  assert.equal(settings.ATM_RELAY_SERVER_NAME, 'file.example.com')
})

test('resolveRelaySettings rejects missing required values', async () => {
  const { resolveRelaySettings } = await loadModule()
  const tempDir = await mkdtemp(path.join(os.tmpdir(), 'atm-relay-missing-config-'))
  const envPath = path.join(tempDir, '.env.relay')

  await assert.rejects(
    () =>
      resolveRelaySettings({
        envPath,
        env: {
          ATM_RELAY_HOST: 'ubuntu@example-host'
        }
      }),
    /ATM_RELAY_SERVER_NAME/
  )
})

test('renderRelayNginxTemplate substitutes public host and remote port', async () => {
  const { renderRelayNginxTemplate } = await loadModule()

  const output = renderRelayNginxTemplate(
    `
server_name __ATM_RELAY_SERVER_NAME__;
proxy_pass http://127.0.0.1:__ATM_RELAY_REMOTE_PORT__;
`.trim(),
    {
      ATM_RELAY_SERVER_NAME: 'relay.example.com',
      ATM_RELAY_REMOTE_PORT: '19090'
    }
  )

  assert.match(output, /server_name relay\.example\.com;/)
  assert.match(output, /proxy_pass http:\/\/127\.0\.0\.1:19090;/)
  assert.doesNotMatch(output, /__ATM_RELAY_/)
})

test('upsertManagedRelayBlock replaces the legacy relay section', async () => {
  const { upsertManagedRelayBlock } = await loadModule()

  const siteConfig = `
server {
    location = /health {
        proxy_pass http://gateway;
    }

    # ========== ATM Remote Relay ==========
    location = /v1/models {
        proxy_pass http://127.0.0.1:19090;
    }

    location ^~ /v1/ {
        return 404;
    }

    location / {
        proxy_pass http://webui;
    }
}
`.trim()

  const updated = upsertManagedRelayBlock(siteConfig, '    location = /v1/models {\n        proxy_pass http://127.0.0.1:29090;\n    }')

  assert.match(updated, /ATM RELAY MANAGED BLOCK/)
  assert.match(updated, /127\.0\.0\.1:29090/)
  assert.doesNotMatch(updated, /127\.0\.0\.1:19090/)
})

test('upsertManagedRelayBlock inserts relay block before the default location when missing', async () => {
  const { upsertManagedRelayBlock } = await loadModule()

  const siteConfig = `
server {
    location /api/ {
        proxy_pass http://gateway;
    }

    location / {
        proxy_pass http://webui;
    }
}
`.trim()

  const updated = upsertManagedRelayBlock(siteConfig, '    location = /v1/models {\n        proxy_pass http://127.0.0.1:19090;\n    }')

  assert.match(updated, /ATM RELAY MANAGED BLOCK/)
  assert.match(updated, /location = \/v1\/models/)
  assert.match(updated, /location \/ \{\n        proxy_pass http:\/\/webui;/)
  assert.ok(updated.indexOf('/v1/models') < updated.indexOf('location / {'))
})
