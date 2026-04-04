# Codex Claude Relay Compatibility Design

**Problem**

公网 ATM relay 已经打通了 OpenAI `/v1/*` 的部分路由和 Gemini `/v1beta/*`，但 Claude/Codex 使用的 Anthropic-native `/v1/messages` 与 `/v1/messages/count_tokens` 没有被 nginx 放行，导致请求在公网入口被直接返回 nginx `404`，而不是进入 ATM sidecar。

**Goal**

让 `https://lingkong.xyz` 这类公网 relay 对 Claude-native surface 真正可用，保证 Codex/Claude 客户端访问 `claude-sonnet-4-6` 时可以穿过 nginx 到达 ATM 服务。

**Recommended Approach**

1. 在 `deploy/nginx/public-atm-relay.conf.template` 中显式代理 `/v1/messages` 与 `/v1/messages/count_tokens`
2. 在 `scripts/check_remote_relay.sh` 中新增 Claude-native POST 探针，发送一个“故意非法但稳定”的请求，并校验返回的是 ATM 的 Anthropic-style `400 invalid_request_error`，而不是 nginx `404`
3. 在 `tests/relayConfig.test.js` 中补上模板与健康检查的回归断言，防止今后再次只放行 `/v1/models` / `/v1/responses`

**Why This Approach**

- 比“放开整个 `/v1/*`”更稳，公网暴露面更小
- 比“改客户端走 `/v1/chat/completions`”更治本，直接修复 Claude-native surface
- 健康检查使用故意非法模型请求，不依赖真实上游账号状态，适合作为稳定回归探针

**Validation**

- 模板测试应覆盖 `/v1/messages` 与 `/v1/messages/count_tokens`
- relay 健康检查测试应覆盖 Claude-native POST 探针与 Anthropic 头
- 定向运行 `tests/relayConfig.test.js`

