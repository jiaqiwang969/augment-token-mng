<template>
  <div class="border-b border-border bg-surface px-5 pb-4">
    <div class="rounded-xl border border-border bg-muted p-4">
      <div class="flex flex-wrap items-start justify-between gap-4">
        <div class="min-w-[280px] flex-1">
          <div class="flex flex-wrap items-center gap-2">
            <h3 class="m-0 text-[14px] font-semibold text-text">{{ t('tokenList.proxyPanel.title') }}</h3>
            <span :class="['badge badge--sm', overallStatus.badgeClass]">
              <span class="status-dot"></span>
              {{ overallStatus.label }}
            </span>
          </div>
          <p class="mt-2 mb-0 text-[12px] leading-5 text-text-secondary">
            {{ overallStatus.description }}
          </p>
        </div>

        <div class="flex flex-wrap items-center gap-2">
          <button class="btn btn--secondary btn--sm" :disabled="isRefreshing || isActionLoading" @click="refreshPanel(true)">
            <span v-if="isRefreshing" class="btn-spinner" aria-hidden="true"></span>
            <span v-else>{{ t('tokenList.proxyPanel.refreshStatus') }}</span>
          </button>
          <button
            v-if="!status.apiServerRunning"
            class="btn btn--primary btn--sm"
            :disabled="isActionLoading"
            @click="startServer"
          >
            <span v-if="isActionLoading" class="btn-spinner" aria-hidden="true"></span>
            <span v-else>{{ t('tokenList.proxyPanel.startServer') }}</span>
          </button>
          <button
            v-else
            class="btn btn--danger btn--sm"
            :disabled="isActionLoading"
            @click="stopServer"
          >
            <span v-if="isActionLoading" class="btn-spinner" aria-hidden="true"></span>
            <span v-else>{{ t('tokenList.proxyPanel.stopServer') }}</span>
          </button>
        </div>
      </div>

      <div v-if="loadError" class="mt-3 rounded-lg border border-danger/30 bg-danger/8 px-3 py-2 text-[12px] text-danger">
        {{ t('tokenList.proxyPanel.loadFailed') }}: {{ loadError }}
      </div>

      <div v-if="accessError" class="mt-3 rounded-lg border border-warning/30 bg-warning/8 px-3 py-2 text-[12px] text-warning">
        {{ t('tokenList.proxyPanel.loadAccessFailed') }}: {{ accessError }}
      </div>

      <div class="mt-4 grid gap-3 md:grid-cols-3">
        <div class="rounded-lg border border-border bg-surface px-3 py-3">
          <div class="text-[11px] uppercase tracking-[0.3px] text-text-muted">{{ t('tokenList.proxyPanel.apiServer') }}</div>
          <div class="mt-2 flex items-center gap-2">
            <span :class="['badge badge--sm', status.apiServerRunning ? 'badge--success-tech' : 'badge--danger-tech']">
              {{ status.apiServerRunning ? t('tokenList.proxyPanel.serverRunning') : t('tokenList.proxyPanel.serverStopped') }}
            </span>
          </div>
          <div class="mt-2 break-all font-mono text-[12px] text-text">
            {{ status.apiServerAddress || defaultApiServerAddress }}
          </div>
        </div>

        <div class="rounded-lg border border-border bg-surface px-3 py-3">
          <div class="text-[11px] uppercase tracking-[0.3px] text-text-muted">{{ t('tokenList.proxyPanel.sidecar') }}</div>
          <div class="mt-2 flex flex-wrap items-center gap-2">
            <span :class="['badge badge--sm', status.sidecarConfigured ? 'badge--accent-tech' : 'badge--danger-tech']">
              {{ status.sidecarConfigured ? t('tokenList.proxyPanel.sidecarConfigured') : t('tokenList.proxyPanel.sidecarMissing') }}
            </span>
            <span
              v-if="status.sidecarConfigured"
              :class="['badge badge--sm', status.sidecarRunning && status.sidecarHealthy ? 'badge--success-tech' : 'badge--accent-tech']"
            >
              {{ sidecarRuntimeText }}
            </span>
          </div>
          <div class="mt-2 text-[12px] leading-5 text-text-secondary">
            {{ t('tokenList.proxyPanel.sidecarAutoStart') }}
          </div>
        </div>

        <div class="rounded-lg border border-border bg-surface px-3 py-3">
          <div class="text-[11px] uppercase tracking-[0.3px] text-text-muted">{{ t('tokenList.proxyPanel.accounts') }}</div>
          <div class="mt-2 flex items-center gap-2">
            <span :class="['badge badge--sm', status.availableAccounts > 0 ? 'badge--success-tech' : 'badge--danger-tech']">
              {{ t('tokenList.proxyPanel.accountsSummary', { available: status.availableAccounts, total: status.totalAccounts }) }}
            </span>
          </div>
          <div class="mt-2 text-[12px] leading-5 text-text-secondary">
            {{ t('tokenList.proxyPanel.accountsHint') }}
          </div>
        </div>
      </div>

      <div class="mt-4 grid gap-3 xl:grid-cols-[minmax(0,0.96fr)_minmax(0,1.04fr)]">
        <div class="space-y-3">
          <div class="rounded-lg border border-border bg-surface px-3 py-3">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <div>
                <div class="text-[11px] uppercase tracking-[0.3px] text-text-muted">{{ t('tokenList.proxyPanel.baseUrl') }}</div>
                <div class="mt-1 text-[12px] leading-5 text-text-secondary">
                  {{ t('tokenList.proxyPanel.baseUrlHint') }}
                </div>
              </div>
              <button class="btn btn--secondary btn--sm" @click="copyText(currentBaseUrl, t('tokenList.proxyPanel.copyBaseUrlSuccess'))">
                {{ t('tokenList.proxyPanel.copyBaseUrl') }}
              </button>
            </div>
            <div class="mt-3 break-all rounded-lg border border-border bg-muted px-3 py-2 font-mono text-[12px] text-text">
              {{ currentBaseUrl }}
            </div>

            <div class="mt-3 flex flex-wrap items-center justify-between gap-2">
              <div class="text-[11px] uppercase tracking-[0.3px] text-text-muted">{{ t('tokenList.proxyPanel.envExport') }}</div>
              <button class="btn btn--secondary btn--sm" @click="copyText(exportCommand, t('tokenList.proxyPanel.copyEnvExportSuccess'))">
                {{ t('tokenList.proxyPanel.copyEnvExport') }}
              </button>
            </div>
            <pre class="mt-2 mb-0 overflow-x-auto rounded-lg border border-border bg-muted px-3 py-2"><code class="text-[12px] leading-5 text-text">{{ exportCommand }}</code></pre>
          </div>

          <div class="rounded-lg border border-border bg-surface px-3 py-3">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <div>
                <div class="text-[11px] uppercase tracking-[0.3px] text-text-muted">{{ t('tokenList.proxyPanel.accessKey') }}</div>
                <div class="mt-1 text-[12px] leading-5 text-text-secondary">
                  {{ t('tokenList.proxyPanel.accessKeyHint') }}
                </div>
              </div>
              <button
                class="btn btn--primary btn--sm"
                :disabled="isSavingAccessConfig || isAccessLoading"
                @click="saveAccessKey"
              >
                <span v-if="isSavingAccessConfig" class="btn-spinner" aria-hidden="true"></span>
                <span v-else>{{ t('tokenList.proxyPanel.saveKey') }}</span>
              </button>
            </div>

            <div class="mt-3 flex gap-2">
              <input
                v-model="apiKeyInput"
                :type="showApiKey ? 'text' : 'password'"
                class="input font-mono"
                :disabled="isAccessLoading || isSavingAccessConfig"
                :placeholder="t('tokenList.proxyPanel.keyPlaceholder')"
              />
              <button
                class="btn btn--icon btn--ghost !h-[34px] !w-[34px] shrink-0"
                :disabled="isAccessLoading"
                :title="showApiKey ? t('tokenList.proxyPanel.hideKey') : t('tokenList.proxyPanel.showKey')"
                @click="showApiKey = !showApiKey"
              >
                <svg v-if="showApiKey" width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
                  <path d="M12 7c2.76 0 5 2.24 5 5 0 .65-.13 1.26-.36 1.83l2.92 2.92c1.51-1.26 2.7-2.89 3.43-4.75-1.73-4.39-6-7.5-11-7.5-1.4 0-2.74.25-3.98.7l2.16 2.16C10.74 7.13 11.35 7 12 7zM2 4.27l2.28 2.28.46.46C3.08 8.3 1.78 10.02 1 12c1.73 4.39 6 7.5 11 7.5 1.55 0 3.03-.3 4.38-.84l.42.42L19.73 22 21 20.73 3.27 3 2 4.27z"/>
                </svg>
                <svg v-else width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
                  <path d="M12 4.5C7 4.5 2.73 7.61 1 12c1.73 4.39 6 7.5 11 7.5s9.27-3.11 11-7.5c-1.73-4.39-6-7.5-11-7.5zM12 17c-2.76 0-5-2.24-5-5s2.24-5 5-5 5 2.24 5 5-2.24 5-5 5zm0-8c-1.66 0-3 1.34-3 3s1.34 3 3 3 3-1.34 3-3-1.34-3-3-3z"/>
                </svg>
              </button>
              <button
                class="btn btn--icon btn--ghost !h-[34px] !w-[34px] shrink-0"
                :disabled="!currentApiKey"
                :title="t('tokenList.proxyPanel.copyKey')"
                @click="copyText(currentApiKey, t('tokenList.proxyPanel.copyKeySuccess'))"
              >
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
                  <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
                </svg>
              </button>
            </div>

            <div class="mt-3 flex flex-wrap items-center gap-2">
              <button
                class="btn btn--secondary btn--sm"
                :disabled="isAccessLoading || isSavingAccessConfig"
                @click="generateDraftKey"
              >
                {{ hasSavedApiKey ? t('tokenList.proxyPanel.rotateKey') : t('tokenList.proxyPanel.generateKey') }}
              </button>
              <span v-if="hasUnsavedKey" class="text-[12px] text-warning">
                {{ t('tokenList.proxyPanel.unsavedKeyHint') }}
              </span>
            </div>

            <div class="mt-3 rounded-lg border border-border bg-muted px-3 py-2">
              <div class="text-[11px] uppercase tracking-[0.3px] text-text-muted">{{ t('tokenList.proxyPanel.envKey') }}</div>
              <div class="mt-2 break-all font-mono text-[12px] text-text">{{ envKey }}</div>
            </div>
          </div>
        </div>

        <div class="rounded-lg border border-border bg-surface px-3 py-3">
          <div class="flex flex-wrap items-center justify-between gap-2">
            <div>
              <div class="text-[11px] uppercase tracking-[0.3px] text-text-muted">{{ t('tokenList.proxyPanel.codexIntegration') }}</div>
              <div class="mt-1 text-[12px] leading-5 text-text-secondary">
                {{ t('tokenList.proxyPanel.integrationHint') }}
              </div>
            </div>
          </div>

          <div class="mt-3 space-y-3">
            <div>
              <div class="mb-2 flex flex-wrap items-center justify-between gap-2">
                <div class="text-[11px] uppercase tracking-[0.3px] text-text-muted">{{ t('tokenList.proxyPanel.curlExample') }}</div>
                <button class="btn btn--secondary btn--sm" @click="copyText(curlExample, t('tokenList.proxyPanel.copyCurlSuccess'))">
                  {{ t('tokenList.proxyPanel.copyCurl') }}
                </button>
              </div>
              <pre class="mb-0 overflow-x-auto rounded-lg border border-border bg-muted px-3 py-2"><code class="text-[12px] leading-5 text-text">{{ curlExample }}</code></pre>
            </div>

            <div>
              <div class="mb-2 flex flex-wrap items-center justify-between gap-2">
                <div class="text-[11px] uppercase tracking-[0.3px] text-text-muted">{{ t('tokenList.proxyPanel.authPoolSnippet') }}</div>
                <button class="btn btn--secondary btn--sm" @click="copyText(authPoolSnippet, t('tokenList.proxyPanel.copyAuthPoolSuccess'))">
                  {{ t('tokenList.proxyPanel.copyAuthPool') }}
                </button>
              </div>
              <pre class="mb-0 overflow-x-auto rounded-lg border border-border bg-muted px-3 py-2"><code class="text-[12px] leading-5 text-text">{{ authPoolSnippet }}</code></pre>
            </div>

            <div>
              <div class="mb-2 flex flex-wrap items-center justify-between gap-2">
                <div class="text-[11px] uppercase tracking-[0.3px] text-text-muted">{{ t('tokenList.proxyPanel.configPoolSnippet') }}</div>
                <button class="btn btn--secondary btn--sm" @click="copyText(configPoolSnippet, t('tokenList.proxyPanel.copyConfigPoolSuccess'))">
                  {{ t('tokenList.proxyPanel.copyConfigPool') }}
                </button>
              </div>
              <pre class="mb-0 overflow-x-auto rounded-lg border border-border bg-muted px-3 py-2"><code class="text-[12px] leading-5 text-text">{{ configPoolSnippet }}</code></pre>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const defaultApiServerAddress = 'http://127.0.0.1:8766'
const defaultProxyBaseUrl = `${defaultApiServerAddress}/v1`
const defaultEnvKey = 'OPENAI_API_KEY_POOL_AUGMENT_1'
const defaultSmokeModel = 'gpt-5.4'

const status = ref({
  apiServerRunning: false,
  apiServerAddress: '',
  proxyBaseUrl: defaultProxyBaseUrl,
  sidecarConfigured: false,
  sidecarRunning: false,
  sidecarHealthy: false,
  totalAccounts: 0,
  availableAccounts: 0
})

const accessConfig = ref({
  baseUrl: defaultProxyBaseUrl,
  apiKey: '',
  envKey: defaultEnvKey,
  curlExample: '',
  authPoolSnippet: '',
  configPoolSnippet: ''
})

const apiKeyInput = ref('')
const showApiKey = ref(false)
const isRefreshing = ref(false)
const isActionLoading = ref(false)
const isAccessLoading = ref(false)
const isSavingAccessConfig = ref(false)
const loadError = ref('')
const accessError = ref('')

let unlistenApiServerStatus = null
let unlistenTokensUpdated = null

const currentBaseUrl = computed(() => accessConfig.value.baseUrl || status.value.proxyBaseUrl || defaultProxyBaseUrl)
const envKey = computed(() => accessConfig.value.envKey || defaultEnvKey)
const currentApiKey = computed(() => apiKeyInput.value.trim())
const hasSavedApiKey = computed(() => !!accessConfig.value.apiKey?.trim())
const hasUnsavedKey = computed(() => currentApiKey.value !== (accessConfig.value.apiKey || '').trim())
const exportCommand = computed(() => `export OPENAI_BASE_URL=${currentBaseUrl.value}`)

const overallStatus = computed(() => {
  if (!status.value.apiServerRunning) {
    return {
      badgeClass: 'badge--danger-tech',
      label: t('tokenList.proxyPanel.apiServerStopped'),
      description: t('tokenList.proxyPanel.apiServerStoppedHint')
    }
  }

  if (!status.value.sidecarConfigured) {
    return {
      badgeClass: 'badge--danger-tech',
      label: t('tokenList.proxyPanel.sidecarMissing'),
      description: t('tokenList.proxyPanel.sidecarMissingHint')
    }
  }

  if (status.value.availableAccounts === 0) {
    return {
      badgeClass: 'badge--danger-tech',
      label: t('tokenList.proxyPanel.noAccounts'),
      description: t('tokenList.proxyPanel.noAccountsHint')
    }
  }

  if (status.value.sidecarRunning && status.value.sidecarHealthy) {
    return {
      badgeClass: 'badge--success-tech',
      label: t('tokenList.proxyPanel.active'),
      description: t('tokenList.proxyPanel.activeHint')
    }
  }

  if (status.value.sidecarRunning && !status.value.sidecarHealthy) {
    return {
      badgeClass: 'badge--accent-tech',
      label: t('tokenList.proxyPanel.starting'),
      description: t('tokenList.proxyPanel.startingHint')
    }
  }

  return {
    badgeClass: 'badge--accent-tech',
    label: t('tokenList.proxyPanel.ready'),
    description: t('tokenList.proxyPanel.readyHint')
  }
})

const sidecarRuntimeText = computed(() => {
  if (!status.value.sidecarConfigured) {
    return t('tokenList.proxyPanel.sidecarMissing')
  }

  if (status.value.sidecarRunning && status.value.sidecarHealthy) {
    return t('tokenList.proxyPanel.sidecarActive')
  }

  if (status.value.sidecarRunning) {
    return t('tokenList.proxyPanel.sidecarStarting')
  }

  return t('tokenList.proxyPanel.sidecarStandby')
})

const snippetApiKey = computed(() => currentApiKey.value || accessConfig.value.apiKey || 'sk-your-key')

const curlExample = computed(() => {
  if (accessConfig.value.curlExample) {
    return accessConfig.value.curlExample
  }

  const chatUrl = `${currentBaseUrl.value}/chat/completions`
  return `curl ${chatUrl} \\
  -H "Authorization: Bearer ${snippetApiKey.value}" \\
  -H "Content-Type: application/json" \\
  -d '{
    "model": "${defaultSmokeModel}",
    "messages": [{"role": "user", "content": "Hello"}]
  }'`
})

const authPoolSnippet = computed(() => JSON.stringify({
  [envKey.value]: snippetApiKey.value
}, null, 2))

const configPoolSnippet = computed(() => `[model_providers.atm-augment]
base_url = "${currentBaseUrl.value}"
env_key = "${envKey.value}"

[[model_providers.atm-augment.account_pool]]
base_url = "${currentBaseUrl.value}"
env_key = "${envKey.value}"`)

const normalizeStatus = (raw = {}) => ({
  apiServerRunning: raw.apiServerRunning ?? false,
  apiServerAddress: raw.apiServerAddress ?? '',
  proxyBaseUrl: raw.proxyBaseUrl || defaultProxyBaseUrl,
  sidecarConfigured: raw.sidecarConfigured ?? false,
  sidecarRunning: raw.sidecarRunning ?? false,
  sidecarHealthy: raw.sidecarHealthy ?? false,
  totalAccounts: raw.totalAccounts ?? 0,
  availableAccounts: raw.availableAccounts ?? 0
})

const normalizeAccessConfig = (raw = {}) => ({
  baseUrl: raw.baseUrl || defaultProxyBaseUrl,
  apiKey: raw.apiKey || '',
  envKey: raw.envKey || defaultEnvKey,
  curlExample: raw.curlExample || '',
  authPoolSnippet: raw.authPoolSnippet || '',
  configPoolSnippet: raw.configPoolSnippet || ''
})

const loadStatus = async (showErrorToast = false) => {
  loadError.value = ''

  try {
    const nextStatus = await invoke('get_augment_proxy_status')
    status.value = normalizeStatus(nextStatus)
  } catch (error) {
    loadError.value = String(error)
    if (showErrorToast) {
      window.$notify.error(`${t('tokenList.proxyPanel.loadFailed')}: ${error}`)
    }
  }
}

const loadAccessConfig = async (showErrorToast = false) => {
  isAccessLoading.value = true
  accessError.value = ''

  try {
    const nextConfig = await invoke('get_augment_gateway_access_config')
    accessConfig.value = normalizeAccessConfig(nextConfig)
    apiKeyInput.value = accessConfig.value.apiKey
  } catch (error) {
    accessError.value = String(error)
    if (showErrorToast) {
      window.$notify.error(`${t('tokenList.proxyPanel.loadAccessFailed')}: ${error}`)
    }
  } finally {
    isAccessLoading.value = false
  }
}

const refreshPanel = async (showErrorToast = false) => {
  isRefreshing.value = true

  try {
    await Promise.allSettled([
      loadStatus(showErrorToast),
      loadAccessConfig(showErrorToast)
    ])
  } finally {
    isRefreshing.value = false
  }
}

const copyText = async (text, successMessage) => {
  try {
    await navigator.clipboard.writeText(text)
    window.$notify.success(successMessage)
  } catch (error) {
    console.error('Failed to copy Augment proxy value:', error)
    window.$notify.error(t('common.copyFailed'))
  }
}

const randomBytesHex = (size = 24) => {
  if (typeof crypto !== 'undefined' && typeof crypto.getRandomValues === 'function') {
    const bytes = new Uint8Array(size)
    crypto.getRandomValues(bytes)
    return Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('')
  }

  let output = ''
  for (let index = 0; index < size; index += 1) {
    output += Math.floor(Math.random() * 256).toString(16).padStart(2, '0')
  }
  return output
}

const generateDraftKey = () => {
  apiKeyInput.value = `sk-${randomBytesHex(24)}`
  showApiKey.value = true
  window.$notify.success(t('tokenList.proxyPanel.generateKeySuccess'))
}

const saveAccessKey = async () => {
  if (isSavingAccessConfig.value) {
    return
  }

  isSavingAccessConfig.value = true

  try {
    if (!apiKeyInput.value.trim()) {
      apiKeyInput.value = `sk-${randomBytesHex(24)}`
    }

    const nextConfig = await invoke('set_augment_gateway_access_config', {
      apiKey: apiKeyInput.value
    })
    accessConfig.value = normalizeAccessConfig(nextConfig)
    apiKeyInput.value = accessConfig.value.apiKey
    showApiKey.value = true
    window.$notify.success(t('tokenList.proxyPanel.saveKeySuccess'))
  } catch (error) {
    console.error('Failed to save Augment gateway access config:', error)
    window.$notify.error(`${t('tokenList.proxyPanel.saveKeyFailed')}: ${error}`)
  } finally {
    isSavingAccessConfig.value = false
  }
}

const startServer = async () => {
  isActionLoading.value = true

  try {
    await invoke('start_api_server_cmd')
    await refreshPanel(false)
    window.$notify.success(t('tokenList.proxyPanel.startSuccess'))
  } catch (error) {
    console.error('Failed to start API server from Augment proxy panel:', error)
    window.$notify.error(`${t('tokenList.proxyPanel.startFailed')}: ${error}`)
  } finally {
    isActionLoading.value = false
  }
}

const stopServer = async () => {
  isActionLoading.value = true

  try {
    await invoke('stop_api_server')
    await refreshPanel(false)
    window.$notify.success(t('tokenList.proxyPanel.stopSuccess'))
  } catch (error) {
    console.error('Failed to stop API server from Augment proxy panel:', error)
    window.$notify.error(`${t('tokenList.proxyPanel.stopFailed')}: ${error}`)
  } finally {
    isActionLoading.value = false
  }
}

onMounted(async () => {
  await refreshPanel(false)

  unlistenApiServerStatus = await listen('api-server-status-changed', async () => {
    await refreshPanel(false)
  })

  unlistenTokensUpdated = await listen('tokens-updated', async () => {
    await loadStatus(false)
  })
})

onUnmounted(() => {
  if (unlistenApiServerStatus) {
    unlistenApiServerStatus()
    unlistenApiServerStatus = null
  }

  if (unlistenTokensUpdated) {
    unlistenTokensUpdated()
    unlistenTokensUpdated = null
  }
})
</script>
