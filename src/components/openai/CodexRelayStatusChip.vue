<template>
  <div
    :class="[
      'mx-2 mb-2 overflow-hidden rounded-full border border-border/80 bg-muted/10 text-left',
      collapsed ? 'h-2.5 px-0.5' : 'flex items-center gap-1.5 px-2 py-1'
    ]"
    :title="tooltipText"
  >
    <template v-if="collapsed">
      <div :class="['h-full w-full rounded-full transition-colors', railClass]"></div>
    </template>
    <template v-else>
      <span :class="['h-2 w-2 rounded-full shrink-0', indicatorClass]"></span>
      <div class="min-w-0 flex-1">
        <div class="h-1 rounded-full bg-bg-base/70">
          <div :class="['h-full rounded-full transition-colors', railClass]"></div>
        </div>
      </div>
      <div class="truncate text-[10px] font-medium" :class="textClass">
        {{ statusText }}
      </div>
    </template>
  </div>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useI18n } from 'vue-i18n'

const props = defineProps({
  collapsed: {
    type: Boolean,
    default: false
  }
})

const { t } = useI18n()

const relayHealth = ref({
  overall: { state: 'unknown' },
  local: {},
  public: {}
})

let unlistenRelayHealth = null

const toCamel = (value) => {
  if (Array.isArray(value)) {
    return value.map(toCamel)
  }
  if (!value || typeof value !== 'object') {
    return value
  }
  return Object.fromEntries(
    Object.entries(value).map(([key, nested]) => [
      key.replace(/_([a-z])/g, (_, char) => char.toUpperCase()),
      toCamel(nested)
    ])
  )
}

const normalizeRelayHealth = (payload) => {
  const data = toCamel(payload || {})
  return {
    overall: data.overall || { state: 'unknown' },
    local: data.local || {},
    public: data.public || {}
  }
}

const loadRelayHealth = async () => {
  try {
    const raw = await invoke('get_codex_relay_health_status')
    relayHealth.value = normalizeRelayHealth(raw)
  } catch {
    relayHealth.value = normalizeRelayHealth(null)
  }
}

const statusKey = computed(() => {
  switch (relayHealth.value?.overall?.state) {
    case 'healthy':
      return 'platform.openai.codexDialog.relayStatusHealthy'
    case 'local_down':
      return 'platform.openai.codexDialog.relayStatusLocalDown'
    case 'public_down':
      return 'platform.openai.codexDialog.relayStatusPublicDown'
    case 'in_progress':
      return 'platform.openai.codexDialog.relayStatusInProgress'
    default:
      return 'platform.openai.codexDialog.relayStatusUnknown'
  }
})

const statusText = computed(() => t(statusKey.value))

const indicatorClass = computed(() => {
  switch (relayHealth.value?.overall?.state) {
    case 'healthy':
      return 'bg-emerald-500'
    case 'public_down':
      return 'bg-amber-500'
    case 'local_down':
      return 'bg-rose-500'
    case 'in_progress':
      return 'bg-sky-500'
    default:
      return 'bg-slate-400'
  }
})

const textClass = computed(() => {
  switch (relayHealth.value?.overall?.state) {
    case 'healthy':
      return 'text-emerald-600'
    case 'public_down':
      return 'text-amber-600'
    case 'local_down':
      return 'text-rose-600'
    case 'in_progress':
      return 'text-sky-600'
    default:
      return 'text-text-muted'
  }
})

const railClass = computed(() => indicatorClass.value)

const tooltipText = computed(() => {
  const localError = relayHealth.value?.local?.lastError
  const publicError = relayHealth.value?.public?.lastError
  return [statusText.value, localError, publicError].filter(Boolean).join(' · ')
})

onMounted(async () => {
  await loadRelayHealth()
  unlistenRelayHealth = await listen('codex-relay-health-changed', (event) => {
    relayHealth.value = normalizeRelayHealth(event.payload)
  })
})

onBeforeUnmount(() => {
  if (typeof unlistenRelayHealth === 'function') {
    unlistenRelayHealth()
    unlistenRelayHealth = null
  }
})
</script>
