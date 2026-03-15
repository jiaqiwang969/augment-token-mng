# ATM One-Command Relay Deploy Design

## Goal

Make the public relay path deployable with one local command:

`make deploy`

The deploy flow must keep secrets and infrastructure details out of git while still being easy to rerun on the same machine.

## Approved Direction

Use a local, untracked `.env.relay` file as the single source of truth for relay deployment settings.

`make deploy` should:

1. load local relay settings from `.env.relay`
2. render the nginx relay snippet from a template
3. fetch the current remote nginx site file
4. upsert a managed ATM relay block inside that file
5. upload the updated site file back to the relay server
6. run `nginx -t` and reload nginx on the server
7. start or reuse the reverse SSH tunnel
8. run an end-to-end health check

ATM itself stays a separate process and is not started by `make deploy`.

## Why This Fits

The relay is operationally tied to one developer machine and one server. That makes a local `.env` file the simplest safe control plane.

This keeps the repo clean:

- no real server IP in tracked deploy scripts
- no real SSH target in tracked files
- no external access keys in tracked files
- no need to maintain a second secrets system yet

## Configuration Model

Tracked file:

- `.env.relay.example`

Untracked file:

- `.env.relay`

The example file documents the required keys. The real file stores the actual values for the current operator.

Minimum fields:

- `ATM_RELAY_HOST`
- `ATM_RELAY_SERVER_NAME`
- `ATM_RELAY_PUBLIC_BASE_URL`
- `ATM_RELAY_REMOTE_PORT`
- `ATM_RELAY_LOCAL_PORT`
- `ATM_RELAY_NGINX_SITE_PATH`
- `ATM_RELAY_API_KEY`

Optional fields:

- `ATM_RELAY_CONTROL_SOCKET`
- `ATM_RELAY_LOCAL_BASE_URL`
- `ATM_RELAY_NGINX_RELOAD_COMMAND`

## Deployment Flow

### Local Env Loading

All relay-related scripts should automatically load `.env.relay` when present.

Manual `export` should remain supported, but it becomes optional.

### Template Rendering

The nginx relay block should be stored as a parameterized template, not a hard-coded example.

The rendered output must at least substitute:

- remote loopback port

The deploy flow should not overwrite the whole site blindly. It should update only a managed relay section in the configured nginx site file.

### Server Apply

The deploy command should fetch the current nginx site file, replace or insert the managed relay block, upload the updated file back to the target path, and then execute:

```bash
nginx -t
sudo systemctl reload nginx
```

The reload command should be configurable because some hosts use `service nginx reload`.

### Tunnel Handling

Deploy should ensure the reverse tunnel is running after the nginx config is applied.

If an existing control-socket session is healthy, reuse it.

### Final Verification

Deploy should fail if any of these checks fail:

- local ATM `/v1/models`
- remote loopback `/v1/models`
- public HTTPS `/v1/models`

If `ATM_RELAY_API_KEY` is set, the checks should include authorization.

## Make Targets

Primary target:

- `make deploy`

Supporting targets:

- `make deploy-check`
- `make relay-start`
- `make relay-stop`
- `make relay-check`

## Non-Goals

- do not manage the ATM desktop process lifecycle
- do not provision nginx or certbot from scratch
- do not add persistent launchd/systemd management in this slice
- do not store secrets in tracked files

## Testing Strategy

Automated coverage should focus on the pure logic:

- `.env.relay` parsing/merging behavior
- nginx template rendering
- required-setting validation

Operational verification should cover the real deploy flow:

- deploy command renders config successfully
- remote nginx config test passes
- remote nginx reload succeeds
- relay tunnel is running
- public endpoint answers through the tunnel
