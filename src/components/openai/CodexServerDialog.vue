<template>
  <BaseModal
    :visible="true"
    :title="''"
    :modal-class="'max-w-[1100px]'"
    :body-scroll="false"
    @close="$emit('close')"
  >
    <template #header>
      <div class="flex w-full flex-wrap items-center justify-between gap-2 pr-2">
        <div class="flex items-center gap-1 rounded-md border border-border p-1">
          <button
            class="btn btn--sm"
            :class="activeTab === 'overview' ? 'btn--primary' : 'btn--ghost'"
            @click="activeTab = 'overview'"
          >
            {{ $t('platform.openai.codexDialog.tabOverview') }}
          </button>
          <button
            class="btn btn--sm"
            :class="activeTab === 'logs' ? 'btn--primary' : 'btn--ghost'"
            @click="activeTab = 'logs'"
          >
            {{ $t('platform.openai.codexDialog.tabRequestLogs') }}
          </button>
        </div>
        <div class="flex items-center gap-2">
          <button class="btn btn--secondary btn--sm" :disabled="isLoading" @click="manualRefresh">{{ $t('platform.openai.codexDialog.refresh') }}</button>
          <button
            class="btn btn--sm"
            :class="serverStatus.running ? 'btn--danger' : 'btn--primary'"
            :disabled="isToggling"
            @click="toggleServer"
          >
            <span v-if="isToggling" class="btn-spinner" aria-hidden="true"></span>
            {{ serverStatus.running ? $t('platform.openai.codexDialog.stopService') : $t('platform.openai.codexDialog.startService') }}
          </button>
        </div>
      </div>
    </template>

    <div class="h-[80vh] overflow-hidden">
      <div v-if="activeTab === 'overview'" class="h-full space-y-4 overflow-y-auto p-1 pr-2">
        <div class="grid gap-4 md:grid-cols-2">
          <div class="space-y-2 rounded-lg border border-border p-3">
            <label class="label mb-0">{{ $t('platform.openai.codexDialog.localServerUrl') }}</label>
            <div class="flex gap-2">
              <input class="input font-mono" :value="accessConfig.serverUrl" readonly />
              <button
                class="btn btn--icon btn--ghost !h-[34px] !w-[34px] shrink-0"
                v-tooltip="$t('common.copy')"
                @click="copyText(accessConfig.serverUrl)"
              >
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
                  <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
                </svg>
              </button>
            </div>
          </div>

          <div class="space-y-2 rounded-lg border border-border p-3">
            <div class="flex gap-2">
              <div class="w-full">
                <label class="label mb-0">{{ $t('platform.openai.codexDialog.publicServerUrl') }}</label>
                <input class="input font-mono" :value="publicServerUrl" readonly />
              </div>
              <button
                class="btn btn--icon btn--ghost !mt-[22px] !h-[34px] !w-[34px] shrink-0"
                v-tooltip="$t('common.copy')"
                @click="copyText(publicServerUrl)"
              >
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
                  <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
                </svg>
              </button>
            </div>
          </div>
        </div>

        <div class="space-y-3 rounded-lg border border-border p-3">
          <div class="flex flex-wrap items-center justify-between gap-2">
            <div>
              <label class="label mb-0">{{ $t('platform.openai.codexDialog.gatewayKeys') }}</label>
              <p class="mt-1 text-[12px] text-text-muted">
                {{ $t('platform.openai.codexDialog.gatewayKeysHint') }}
              </p>
            </div>
            <button class="btn btn--secondary btn--sm" :disabled="isCreatingProfile" @click="createGatewayProfile">
              {{ $t('platform.openai.codexDialog.addKey') }}
            </button>
          </div>

          <div v-if="gatewayProfiles.length === 0" class="rounded-lg border border-dashed border-border bg-muted/10 px-3 py-5 text-center text-[12px] text-text-muted">
            {{ $t('platform.openai.codexDialog.noGatewayKeys') }}
          </div>

          <div v-else class="space-y-2">
            <div
              v-for="profile in gatewayProfiles"
              :key="profile.id"
              class="space-y-3 rounded-lg border border-border bg-muted/10 p-3"
            >
              <div class="flex flex-wrap items-center justify-between gap-2">
                <div class="flex flex-wrap items-center gap-2">
                  <span class="text-[13px] font-medium">{{ profile.name || profile.id }}</span>
                  <span
                    v-if="profile.isPrimary"
                    class="rounded-full bg-primary/15 px-2 py-0.5 text-[11px] font-medium text-primary"
                  >
                    {{ $t('platform.openai.codexDialog.primaryKey') }}
                  </span>
                  <span
                    v-if="!profile.enabled"
                    class="rounded-full bg-danger/15 px-2 py-0.5 text-[11px] font-medium text-danger"
                  >
                    {{ $t('platform.openai.codexDialog.disabledKey') }}
                  </span>
                </div>
                <label class="flex items-center gap-2 text-[12px] text-text-secondary">
                  <input
                    type="checkbox"
                    class="h-4 w-4"
                    :checked="profile.enabled"
                    :disabled="isProfileBusy(profile.id)"
                    @change="toggleGatewayProfile(profile, $event.target.checked)"
                  />
                  <span>{{ profile.enabled ? $t('platform.openai.codexDialog.enabledKey') : $t('platform.openai.codexDialog.disabledKey') }}</span>
                </label>
              </div>

              <div class="grid gap-2 xl:grid-cols-[220px_minmax(0,1fr)_auto]">
                <input
                  v-model="profile.name"
                  class="input"
                  :disabled="isProfileBusy(profile.id)"
                  :placeholder="$t('platform.openai.codexDialog.profileNamePlaceholder')"
                />

                <div class="flex gap-2">
                  <input
                    v-model="profile.apiKey"
                    :type="isProfileKeyVisible(profile.id) ? 'text' : 'password'"
                    class="input font-mono"
                    :disabled="isProfileBusy(profile.id)"
                    :placeholder="$t('platform.openai.codexDialog.apiKeyPlaceholder')"
                  />
                  <button
                    class="btn btn--icon btn--ghost !h-[34px] !w-[34px] shrink-0"
                    :disabled="isProfileBusy(profile.id)"
                    v-tooltip="isProfileKeyVisible(profile.id) ? $t('platform.openai.codexDialog.hideApiKey') : $t('platform.openai.codexDialog.showApiKey')"
                    @click="toggleProfileKeyVisibility(profile.id)"
                  >
                    <svg v-if="isProfileKeyVisible(profile.id)" width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
                      <path d="M12 7c2.76 0 5 2.24 5 5 0 .65-.13 1.26-.36 1.83l2.92 2.92c1.51-1.26 2.7-2.89 3.43-4.75-1.73-4.39-6-7.5-11-7.5-1.4 0-2.74.25-3.98.7l2.16 2.16C10.74 7.13 11.35 7 12 7zM2 4.27l2.28 2.28.46.46C3.08 8.3 1.78 10.02 1 12c1.73 4.39 6 7.5 11 7.5 1.55 0 3.03-.3 4.38-.84l.42.42L19.73 22 21 20.73 3.27 3 2 4.27z"/>
                    </svg>
                    <svg v-else width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
                      <path d="M12 4.5C7 4.5 2.73 7.61 1 12c1.73 4.39 6 7.5 11 7.5s9.27-3.11 11-7.5c-1.73-4.39-6-7.5-11-7.5zM12 17c-2.76 0-5-2.24-5-5s2.24-5 5-5 5 2.24 5 5-2.24 5-5 5zm0-8c-1.66 0-3 1.34-3 3s1.34 3 3 3 3-1.34 3-3-1.34-3-3-3z"/>
                    </svg>
                  </button>
                </div>

                <div class="flex flex-wrap gap-2">
                  <button class="btn btn--secondary btn--sm" :disabled="isProfileBusy(profile.id)" @click="saveGatewayProfile(profile)">
                    {{ $t('platform.openai.codexDialog.saveKey') }}
                  </button>
                  <button class="btn btn--ghost btn--sm" :disabled="isProfileBusy(profile.id)" @click="regenerateGatewayProfileKey(profile)">
                    {{ $t('platform.openai.codexDialog.generateApiKey') }}
                  </button>
                  <button class="btn btn--ghost btn--sm" :disabled="isProfileBusy(profile.id)" @click="copyText(profile.apiKey)">
                    {{ $t('common.copy') }}
                  </button>
                  <button class="btn btn--ghost btn--sm text-danger" :disabled="isProfileBusy(profile.id)" @click="deleteGatewayProfile(profile)">
                    {{ $t('common.delete') }}
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- 策略选择器 + 快捷切换 -->
        <div class="grid grid-cols-2 gap-4">
          <div class="flex items-center justify-between gap-2 rounded-lg border border-border bg-muted/20 px-3 py-2">
            <span class="text-[13px] text-text-secondary">{{ $t('platform.openai.codexDialog.poolStrategy') }}</span>
            <div class="flex items-center gap-2">
              <FloatingDropdown placement="bottom-end" :offset="4" :disabled="isChangingStrategy">
                <template #trigger="{ isOpen }">
                  <button
                    class="btn btn--secondary btn--sm h-8 flex items-center gap-1 px-2"
                    :class="{ 'btn--light': !isOpen }"
                    type="button"
                  >
                    <span class="text-[13px]">{{ getStrategyLabel(poolStrategy) }}</span>
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M6 9l6 6 6-6"/>
                    </svg>
                  </button>
                </template>
                <template #default="{ close }">
                  <div class="py-1">
                    <button
                      v-for="strategy in strategyOptions"
                      :key="strategy.value"
                      class="dropdown-item flex items-center gap-2 px-3 py-1.5 text-[13px]"
                      :class="{ 'bg-primary/10': strategy.value === poolStrategy }"
                      :disabled="isChangingStrategy"
                      @click="selectStrategy(strategy.value, close)"
                    >
                      <span>{{ strategy.label }}</span>
                    </button>
                  </div>
                </template>
              </FloatingDropdown>
              <!-- 单个模式下的账号选择器 -->
              <FloatingDropdown v-if="poolStrategy === 'single'" placement="bottom-end" :offset="4">
                <template #trigger="{ isOpen }">
                  <button
                    class="btn btn--secondary btn--sm h-8 flex items-center gap-1 px-2"
                    v-tooltip="poolStatus.selectedAccountEmail || $t('platform.openai.codexDialog.selectAccount')"
                    :class="{ 'btn--light': !isOpen }"
                    type="button"
                  >
                    <span class="truncate max-w-[100px]">{{ poolStatus.selectedAccountEmail || $t('platform.openai.codexDialog.selectAccount') }}</span>
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M6 9l6 6 6-6"/>
                    </svg>
                  </button>
                </template>
                <template #default="{ close }">
                  <div class="py-1">
                    <button
                      v-for="account in availableAccounts"
                      :key="account.id"
                      class="dropdown-item flex items-center gap-2 px-3 py-1.5 text-[13px]"
                      :class="{ 'bg-primary/10': account.id === selectedAccountId }"
                      @click="selectAccount(account.id, close)"
                    >
                      <span class="truncate">{{ account.email }}</span>
                    </button>
                  </div>
                </template>
              </FloatingDropdown>
            </div>
          </div>
          <!-- 快捷切换 -->
          <div class="flex items-center justify-between gap-2 rounded-lg border border-border bg-muted/20 px-3 py-2">
            <span class="text-[13px] text-text-secondary">{{ $t('platform.openai.codexDialog.quickSwitch') }}</span>
            <div class="flex items-center gap-2">
              <button
                class="btn btn--secondary btn--sm h-8 px-3"
                v-tooltip="$t('platform.openai.codexDialog.clickToUseAccount')"
                @click="showQuickSwitchModal = 'codex'"
              >
                Codex
              </button>
              <button
                class="btn btn--secondary btn--sm h-8 px-3"
                v-tooltip="$t('platform.openai.codexDialog.clickToUseAccount')"
                @click="showQuickSwitchModal = 'droid'"
              >
                Droid
              </button>
            </div>
          </div>
        </div>

        <div class="grid gap-4 md:grid-cols-3">
          <div class="rounded-lg border border-border p-3">
            <div class="text-[12px] text-text-muted">{{ $t('platform.openai.codexDialog.totalAccounts') }}</div>
            <div class="text-[18px] font-semibold">{{ poolStatus.totalAccounts }}</div>
          </div>
          <div class="rounded-lg border border-border p-3">
            <div class="flex items-center justify-between">
              <div class="text-[12px] text-text-muted">{{ $t('platform.openai.codexDialog.totalRequests') }}</div>
              <div class="text-[11px] text-success">+{{ formatNumber(periodStats.todayRequests) }}</div>
            </div>
            <div class="text-[18px] font-semibold">{{ formatNumber(allTimeStats.requests) }}</div>
          </div>
          <div class="rounded-lg border border-border p-3">
            <div class="flex items-center justify-between">
              <div class="text-[12px] text-text-muted">{{ $t('platform.openai.codexDialog.allTimeTokens') }}</div>
              <div class="text-[11px] text-success">+{{ formatTokens(periodStats.todayTokens) }}</div>
            </div>
            <div class="text-[18px] font-semibold">{{ formatTokens(allTimeStats.tokens) }}</div>
          </div>
        </div>

        <!-- 月度趋势图 -->
        <CodexUsageChart :loading="isLoadingChart" :chart-data="dailyStatsSeries" />
      </div>

      <div v-else class="flex h-full flex-col gap-2 p-1">
        <div class="flex min-h-0 flex-1 flex-col gap-2 rounded-lg border border-border p-3">
          <div class="flex flex-wrap items-center justify-between gap-2">
            <h4 class="text-[13px] font-semibold">{{ $t('platform.openai.codexDialog.logsTitle') }}</h4>
            <div class="flex flex-wrap items-center gap-2">
              <!-- 时间范围筛选 -->
              <FloatingDropdown placement="bottom-end" :offset="4">
                <template #trigger="{ isOpen }">
                  <button
                    class="btn btn--secondary btn--sm h-8 flex items-center gap-1 px-2"
                    :class="{ 'btn--light': !isOpen }"
                    type="button"
                  >
                    <span class="text-[13px]">{{ getLogRangeLabel(logRange) }}</span>
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M6 9l6 6 6-6"/>
                    </svg>
                  </button>
                </template>
                <template #default="{ close }">
                  <div class="py-1">
                    <button
                      v-for="range in logRangeOptions"
                      :key="range.value"
                      class="dropdown-item flex items-center gap-2 px-3 py-1.5 text-[13px]"
                      :class="{ 'bg-primary/10': range.value === logRange }"
                      @click="selectLogRange(range.value, close)"
                    >
                      <span>{{ range.label }}</span>
                    </button>
                  </div>
                </template>
              </FloatingDropdown>
              <!-- 账号筛选 -->
              <FloatingDropdown placement="bottom-end" :offset="4">
                <template #trigger="{ isOpen }">
                  <button
                    class="btn btn--secondary btn--sm h-8 flex items-center gap-1 px-2"
                    :class="{ 'btn--light': !isOpen }"
                    type="button"
                  >
                    <span class="text-[13px] truncate max-w-[120px]">{{ getLogAccountLabel() }}</span>
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M6 9l6 6 6-6"/>
                    </svg>
                  </button>
                </template>
                <template #default="{ close }">
                  <div class="py-1">
                    <button
                      class="dropdown-item flex items-center gap-2 px-3 py-1.5 text-[13px]"
                      :class="{ 'bg-primary/10': !logAccountFilter }"
                      @click="selectLogAccount('', close)"
                    >
                      <span>{{ $t('platform.openai.codexDialog.allAccounts') }}</span>
                    </button>
                    <button
                      v-for="account in availableAccounts"
                      :key="account.id"
                      class="dropdown-item flex items-center gap-2 px-3 py-1.5 text-[13px]"
                      :class="{ 'bg-primary/10': account.id === logAccountFilter }"
                      @click="selectLogAccount(account.id, close)"
                    >
                      <span class="truncate">{{ account.email }}</span>
                    </button>
                  </div>
                </template>
              </FloatingDropdown>
              <!-- 模型筛选 -->
              <input v-model="logModelFilter" class="input h-8 w-[140px]" :placeholder="$t('platform.openai.codexDialog.modelFilterPlaceholder')" />
              <!-- 状态筛选 -->
              <FloatingDropdown placement="bottom-end" :offset="4">
                <template #trigger="{ isOpen }">
                  <button
                    class="btn btn--secondary btn--sm h-8 flex items-center gap-1 px-2"
                    :class="{ 'btn--light': !isOpen }"
                    type="button"
                  >
                    <span class="text-[13px]">{{ getLogStatusLabel(logStatusFilter) }}</span>
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M6 9l6 6 6-6"/>
                    </svg>
                  </button>
                </template>
                <template #default="{ close }">
                  <div class="py-1">
                    <button
                      v-for="status in logStatusOptions"
                      :key="status.value"
                      class="dropdown-item flex items-center gap-2 px-3 py-1.5 text-[13px]"
                      :class="{ 'bg-primary/10': status.value === logStatusFilter }"
                      @click="selectLogStatus(status.value, close)"
                    >
                      <span>{{ status.label }}</span>
                    </button>
                  </div>
                </template>
              </FloatingDropdown>
            </div>
          </div>

          <div class="min-h-0 flex-1 overflow-y-auto rounded-lg">
            <table class="table table-fixed">
              <thead class="sticky top-0 z-10 bg-bg-base rounded-t-lg overflow-hidden">
                <tr>
                  <th class="w-[12%] first:rounded-tl-lg">{{ $t('platform.openai.codexDialog.time') }}</th>
                  <th class="w-[16%]">{{ $t('platform.openai.codexDialog.account') }}</th>
                  <th class="w-[15%]">{{ $t('platform.openai.codexDialog.model') }}</th>
                  <th class="w-[8%]">{{ $t('platform.openai.codexDialog.format') }}</th>
                  <th class="w-[8%] text-right">{{ $t('platform.openai.codexDialog.inputTokens') }}</th>
                  <th class="w-[8%] text-right">{{ $t('platform.openai.codexDialog.outputTokens') }}</th>
                  <th class="w-[8%] text-right">{{ $t('platform.openai.codexDialog.totalTokens') }}</th>
                  <th class="w-[7%]">{{ $t('platform.openai.codexDialog.status') }}</th>
                  <th class="w-[7%] text-right">{{ $t('platform.openai.codexDialog.duration') }}</th>
                  <th class="w-[11%] last:rounded-tr-lg">{{ $t('platform.openai.codexDialog.error') }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-if="logPage.items.length === 0">
                  <td colspan="10" class="text-center text-text-muted">{{ $t('platform.openai.codexDialog.noLogs') }}</td>
                </tr>
                <tr v-for="log in logPage.items" :key="log.id">
                  <td class="font-mono text-[11px]">{{ formatTs(log.timestamp) }}</td>
                  <td class="text-[11px] truncate"><span class="inline-block -mb-1" v-tooltip="log.accountEmail">{{ log.accountEmail || '-' }}</span></td>
                  <td class="font-mono text-[11px] truncate"><span class="inline-block -mb-1" v-tooltip="log.model">{{ log.model }}</span></td>
                  <td class="text-[11px]">{{ log.format }}</td>
                  <td class="text-right text-[11px]">{{ formatTokens(log.inputTokens) }}</td>
                  <td class="text-right text-[11px]">{{ formatTokens(log.outputTokens) }}</td>
                  <td class="text-right text-[11px] font-semibold">{{ formatTokens(log.totalTokens) }}</td>
                  <td>
                    <span :class="['badge badge--sm', log.status === 'success' ? 'badge--success-tech' : 'badge--danger-tech']">{{ log.status }}</span>
                  </td>
                  <td class="text-right text-[11px]">{{ formatDuration(log.requestDurationMs) }}</td>
                  <td class="text-[11px] text-danger truncate"><span class="inline-block -mb-1" v-tooltip="log.errorMessage || ''">{{ log.errorMessage || '-' }}</span></td>
                </tr>
              </tbody>
            </table>
          </div>

          <div class="flex items-center justify-between">
            <span class="text-[12px] text-text-muted">{{ $t('platform.openai.codexDialog.totalRecords', { total: logPage.total }) }}</span>
            <div class="flex items-center gap-2">
              <button class="btn btn--secondary btn--sm" :disabled="logOffset === 0" @click="prevLogPage">{{ $t('platform.openai.codexDialog.prevPage') }}</button>
              <button
                class="btn btn--secondary btn--sm"
                :disabled="logOffset + logLimit >= logPage.total"
                @click="nextLogPage"
              >{{ $t('platform.openai.codexDialog.nextPage') }}</button>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 快捷切换 Modal -->
    <CodexQuickSwitchModal
      v-if="showQuickSwitchModal"
      :type="showQuickSwitchModal"
      :base-url="accessConfig.serverUrl"
      :api-key="primaryGatewayApiKey"
      @close="showQuickSwitchModal = ''"
      @switched="showQuickSwitchModal = ''"
    />
  </BaseModal>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import BaseModal from '@/components/common/BaseModal.vue'
import FloatingDropdown from '@/components/common/FloatingDropdown.vue'
import CodexUsageChart from '@/components/openai/CodexUsageChart.vue'
import CodexQuickSwitchModal from '@/components/openai/CodexQuickSwitchModal.vue'

defineEmits(['close'])

const { t: $t } = useI18n()

const isLoading = ref(false)
const isToggling = ref(false)
const isCreatingProfile = ref(false)
const activeTab = ref('overview')
const showQuickSwitchModal = ref('') // 'codex' | 'droid' | ''
const SHARED_PORT = 8766
const publicServerUrl = 'https://lingkong.xyz/v1'

const serverStatus = ref({ running: false, address: `http://127.0.0.1:${SHARED_PORT}`, port: SHARED_PORT, poolStatus: null })
const accessConfig = ref({
  serverUrl: `http://127.0.0.1:${SHARED_PORT}/v1`,
  apiKey: ''
})
const poolStatus = ref({
  totalAccounts: 0,
  activeAccounts: 0,
  expiredAccounts: 0,
  coolingAccounts: 0,
  unauthorizedAccounts: 0,
  paymentRequiredAccounts: 0,
  totalRequestsToday: 0,
  totalTokensUsed: 0,
  strategy: 'round-robin',
  selectedAccountId: ''
})
const periodStats = ref({ todayRequests: 0, todayTokens: 0, weekRequests: 0, weekTokens: 0, monthRequests: 0, monthTokens: 0 })
const allTimeStats = ref({ requests: 0, tokens: 0 })

const gatewayProfiles = ref([])
const profileBusyState = ref({})
const visibleProfileKeys = ref({})
const poolStrategy = ref('round-robin')
const selectedAccountId = ref('')
const isChangingStrategy = ref(false)
const availableAccounts = ref([])
const primaryGatewayApiKey = computed(() => {
  const primary = gatewayProfiles.value.find(profile => profile.isPrimary)
  return primary?.apiKey || accessConfig.value.apiKey || ''
})

const applyPoolStatus = (rawStatus) => {
  poolStatus.value = toCamel(rawStatus)
  selectedAccountId.value = poolStatus.value.selectedAccountId || ''

  if (poolStatus.value.strategy) {
    const strategyMap = {
      RoundRobin: 'round-robin',
      Single: 'single',
      Smart: 'smart'
    }
    poolStrategy.value = strategyMap[poolStatus.value.strategy] || 'round-robin'
  }
}

// 策略选项
const strategyOptions = [
  { value: 'round-robin', label: $t('platform.openai.codexDialog.strategyRoundRobin') },
  { value: 'single', label: $t('platform.openai.codexDialog.strategySingle') },
  { value: 'smart', label: $t('platform.openai.codexDialog.strategySmart') }
]

// 日志时间范围选项
const logRangeOptions = [
  { value: 'today', label: $t('platform.openai.codexDialog.rangeToday') },
  { value: '7d', label: $t('platform.openai.codexDialog.range7d') },
  { value: '30d', label: $t('platform.openai.codexDialog.range30d') },
  { value: 'all', label: $t('platform.openai.codexDialog.rangeAll') }
]

// 日志状态选项
const logStatusOptions = [
  { value: '', label: $t('platform.openai.codexDialog.allStatus') },
  { value: 'success', label: 'success' },
  { value: 'error', label: 'error' }
]

const logPage = ref({ total: 0, items: [] })
const logLimit = ref(50)
const logOffset = ref(0)
const logAccountFilter = ref('')
const logModelFilter = ref('')
const logStatusFilter = ref('')
const logRange = ref('7d')

// 图表相关状态
const isLoadingChart = ref(false)
const dailyStatsSeries = ref([])

let pollTimer = null

const toCamel = (obj) => {
  if (Array.isArray(obj)) return obj.map(toCamel)
  if (!obj || typeof obj !== 'object') return obj
  const out = {}
  for (const [key, value] of Object.entries(obj)) {
    const camel = key.replace(/_([a-z])/g, (_, c) => c.toUpperCase())
    out[camel] = toCamel(value)
  }
  return out
}

const getLogRange = () => {
  const now = Math.floor(Date.now() / 1000)
  if (logRange.value === 'all') {
    return { startTs: null, endTs: null }
  }
  if (logRange.value === 'today') {
    const startDate = new Date()
    startDate.setHours(0, 0, 0, 0)
    return { startTs: Math.floor(startDate.getTime() / 1000), endTs: now }
  }
  if (logRange.value === '30d') {
    return { startTs: now - 30 * 24 * 3600, endTs: now }
  }
  return { startTs: now - 7 * 24 * 3600, endTs: now }
}

const formatCompactNumber = (v) => {
  const n = Number(v || 0)
  if (n < 1000) return n.toLocaleString()
  if (n < 1000000) return (n / 1000).toFixed(1).replace(/\.0$/, '') + 'K'
  if (n < 1000000000) return (n / 1000000).toFixed(2).replace(/\.00$/, '') + 'M'
  if (n < 1000000000000) return (n / 1000000000).toFixed(2).replace(/\.00$/, '') + 'B'
  return (n / 1000000000000).toFixed(2).replace(/\.00$/, '') + 'T'
}

const formatNumber = (v) => formatCompactNumber(v)

const formatTokens = (v) => formatCompactNumber(v)

const formatTs = (ts) => {
  if (!ts) return '-'
  const date = new Date(ts * 1000)
  if (Number.isNaN(date.getTime())) return '-'
  const pad = (n) => String(n).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`
}

const formatDuration = (ms) => {
  const n = Number(ms || 0)
  if (n < 1000) return `${n}ms`
  if (n < 60000) return `${(n / 1000).toFixed(1)}s`
  if (n < 3600000) return `${(n / 60000).toFixed(1)}m`
  return `${(n / 3600000).toFixed(1)}h`
}

const copyText = async (text) => {
  const value = String(text || '').trim()
  if (!value) {
    window.$notify?.warning($t('platform.openai.codexDialog.copyEmpty'))
    return
  }
  try {
    await navigator.clipboard.writeText(value)
    window.$notify?.success($t('common.copySuccess'))
  } catch (error) {
    window.$notify?.error($t('common.copyFailed'))
  }
}

const randomBytesHex = (size = 20) => {
  if (typeof crypto !== 'undefined' && typeof crypto.getRandomValues === 'function') {
    const bytes = new Uint8Array(size)
    crypto.getRandomValues(bytes)
    return Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('')
  }
  let out = ''
  for (let i = 0; i < size; i += 1) {
    out += Math.floor(Math.random() * 256).toString(16).padStart(2, '0')
  }
  return out
}

const generateApiKeyValue = () => `sk-${randomBytesHex(24)}`

const normalizeGatewayProfile = (profile) => {
  const data = toCamel(profile)
  return {
    id: data.id || '',
    name: data.name || '',
    apiKey: data.apiKey || '',
    enabled: data.enabled !== false,
    isPrimary: !!data.isPrimary
  }
}

const cleanupProfileMaps = (profiles) => {
  const validIds = new Set(profiles.map(profile => profile.id))
  const filterByIds = (source) =>
    Object.fromEntries(Object.entries(source).filter(([id]) => validIds.has(id)))

  profileBusyState.value = filterByIds(profileBusyState.value)
  visibleProfileKeys.value = filterByIds(visibleProfileKeys.value)
}

const setProfileBusy = (profileId, busy) => {
  const next = { ...profileBusyState.value }
  if (busy) {
    next[profileId] = true
  } else {
    delete next[profileId]
  }
  profileBusyState.value = next
}

const isProfileBusy = (profileId) => !!profileBusyState.value[profileId]

const toggleProfileKeyVisibility = (profileId) => {
  visibleProfileKeys.value = {
    ...visibleProfileKeys.value,
    [profileId]: !visibleProfileKeys.value[profileId]
  }
}

const isProfileKeyVisible = (profileId) => !!visibleProfileKeys.value[profileId]

const ensureGatewayProfileApiKey = (profile) => {
  const apiKey = String(profile?.apiKey || '').trim()
  if (apiKey) {
    return apiKey
  }

  window.$notify?.error($t('platform.openai.codexDialog.apiKeyRequired'))
  return null
}

const loadServerStatus = async () => {
  const raw = await invoke('get_codex_server_status')
  const data = toCamel(raw)
  serverStatus.value = {
    running: !!data.running,
    address: data.address || `http://127.0.0.1:${SHARED_PORT}`,
    port: data.port || SHARED_PORT,
    poolStatus: data.poolStatus || null
  }
}

const loadAccessConfig = async () => {
  const raw = await invoke('get_codex_access_config')
  const data = toCamel(raw)
  accessConfig.value = {
    serverUrl: data.serverUrl || `http://127.0.0.1:${SHARED_PORT}/v1`,
    apiKey: data.apiKey || ''
  }
}

const loadGatewayProfiles = async () => {
  const raw = await invoke('list_codex_gateway_profiles')
  const profiles = (Array.isArray(raw) ? raw : []).map(normalizeGatewayProfile)
  gatewayProfiles.value = profiles
  cleanupProfileMaps(profiles)
}

const loadPoolStatus = async () => {
  try {
    const raw = await invoke('get_codex_pool_status')
    applyPoolStatus(raw)

    // 加载账号列表
    await loadAccounts()
  } catch {
    applyPoolStatus(serverStatus.value.poolStatus || poolStatus.value)
  }
}

const loadAccounts = async () => {
  try {
    const raw = await invoke('openai_list_accounts')
    const accounts = toCamel(raw)
    // 过滤出可用的 OAuth 账号
    availableAccounts.value = accounts.filter((a) =>
      a.accountType === 'oauth' && a.token && !a.token.isExpired
    )
  } catch {
    availableAccounts.value = []
  }
}

const persistGatewayProfile = async (profile, { notifySuccess = false, successMessage = '' } = {}) => {
  const apiKey = ensureGatewayProfileApiKey(profile)
  if (!apiKey || !profile?.id) {
    return null
  }

  setProfileBusy(profile.id, true)
  try {
    const updated = await invoke('update_codex_gateway_profile', {
      profileId: profile.id,
      name: profile.name,
      apiKey,
      enabled: profile.enabled
    })
    await Promise.all([loadGatewayProfiles(), loadAccessConfig()])

    if (notifySuccess && successMessage) {
      window.$notify?.success(successMessage)
    }

    return normalizeGatewayProfile(updated)
  } finally {
    setProfileBusy(profile.id, false)
  }
}

const createGatewayProfile = async () => {
  if (isCreatingProfile.value) {
    return
  }

  isCreatingProfile.value = true
  try {
    const created = normalizeGatewayProfile(await invoke('create_codex_gateway_profile', {
      name: null,
      apiKey: null,
      enabled: true
    }))
    await Promise.all([loadGatewayProfiles(), loadAccessConfig()])
    visibleProfileKeys.value = {
      ...visibleProfileKeys.value,
      [created.id]: true
    }
    window.$notify?.success($t('platform.openai.codexDialog.createKeySuccess'))
  } catch (error) {
    console.error('Failed to create Codex gateway profile:', error)
    window.$notify?.error(
      $t('platform.openai.codexDialog.createKeyFailed', { error: error?.message || error })
    )
  } finally {
    isCreatingProfile.value = false
  }
}

const saveGatewayProfile = async (profile) => {
  try {
    await persistGatewayProfile(profile, {
      notifySuccess: true,
      successMessage: $t('platform.openai.codexDialog.saveKeySuccess')
    })
  } catch (error) {
    console.error('Failed to save Codex gateway profile:', error)
    window.$notify?.error(
      $t('platform.openai.codexDialog.saveKeyFailed', { error: error?.message || error })
    )
  }
}

const toggleGatewayProfile = async (profile, enabled) => {
  const previousEnabled = profile.enabled
  profile.enabled = enabled

  try {
    const updated = await persistGatewayProfile(profile)
    if (!updated) {
      profile.enabled = previousEnabled
    }
  } catch (error) {
    profile.enabled = previousEnabled
    console.error('Failed to toggle Codex gateway profile:', error)
    window.$notify?.error(
      $t('platform.openai.codexDialog.updateKeyFailed', { error: error?.message || error })
    )
  }
}

const deleteGatewayProfile = async (profile) => {
  if (!profile?.id) {
    return
  }

  const confirmed = window.confirm(
    $t('platform.openai.codexDialog.deleteKeyConfirm', {
      name: profile.name || profile.id
    })
  )
  if (!confirmed) {
    return
  }

  setProfileBusy(profile.id, true)
  try {
    await invoke('delete_codex_gateway_profile', {
      profileId: profile.id
    })
    await Promise.all([loadGatewayProfiles(), loadAccessConfig()])
    window.$notify?.success($t('platform.openai.codexDialog.deleteKeySuccess'))
  } catch (error) {
    console.error('Failed to delete Codex gateway profile:', error)
    window.$notify?.error(
      $t('platform.openai.codexDialog.deleteKeyFailed', { error: error?.message || error })
    )
  } finally {
    setProfileBusy(profile.id, false)
  }
}

const regenerateGatewayProfileKey = (profile) => {
  profile.apiKey = generateApiKeyValue()
  visibleProfileKeys.value = {
    ...visibleProfileKeys.value,
    [profile.id]: true
  }
  window.$notify?.success($t('platform.openai.codexDialog.generateApiKeySuccess'))
}

const onStrategyChange = async () => {
  isChangingStrategy.value = true
  try {
    await invoke('set_codex_pool_strategy', { strategy: poolStrategy.value })
    await loadPoolStatus()
  } catch (error) {
    window.$notify?.error($t('platform.openai.codexDialog.toggleFailed', { error: error?.message || error }))
  } finally {
    isChangingStrategy.value = false
  }
}

const getStrategyLabel = (value) => {
  const option = strategyOptions.find(s => s.value === value)
  return option?.label || value
}

const selectStrategy = async (value, close) => {
  if (value === poolStrategy.value || isChangingStrategy.value) {
    close()
    return
  }
  isChangingStrategy.value = true
  try {
    await invoke('set_codex_pool_strategy', { strategy: value })
    poolStrategy.value = value
    await loadPoolStatus()
    close()
  } catch (error) {
    window.$notify?.error($t('platform.openai.codexDialog.toggleFailed', { error: error?.message || error }))
  } finally {
    isChangingStrategy.value = false
  }
}

const selectAccount = async (accountId, close) => {
  selectedAccountId.value = accountId
  try {
    await invoke('set_codex_selected_account', { accountId })
    await loadPoolStatus()
    close()
  } catch (error) {
    window.$notify?.error($t('platform.openai.codexDialog.toggleFailed', { error: error?.message || error }))
  }
}

// 日志筛选相关方法
const getLogRangeLabel = (value) => {
  const option = logRangeOptions.find(r => r.value === value)
  return option?.label || value
}

const getLogStatusLabel = (value) => {
  const option = logStatusOptions.find(s => s.value === value)
  return option?.label || value
}

const getLogAccountLabel = () => {
  if (!logAccountFilter.value) {
    return $t('platform.openai.codexDialog.allAccounts')
  }
  const account = availableAccounts.value.find(a => a.id === logAccountFilter.value)
  return account?.email || logAccountFilter.value
}

const selectLogRange = async (value, close) => {
  logRange.value = value
  close()
  await reloadLogs()
}

const selectLogStatus = async (value, close) => {
  logStatusFilter.value = value
  close()
  await reloadLogs()
}

const selectLogAccount = async (value, close) => {
  logAccountFilter.value = value
  close()
  await reloadLogs()
}

// 模型筛选防抖
let modelFilterTimer = null
watch(logModelFilter, () => {
  if (modelFilterTimer) {
    clearTimeout(modelFilterTimer)
  }
  modelFilterTimer = setTimeout(() => {
    reloadLogs()
  }, 500)
})

const loadPeriodStats = async () => {
  const raw = await invoke('get_codex_period_stats_from_storage')
  periodStats.value = toCamel(raw)
}

const loadAllTimeStats = async () => {
  try {
    const raw = await invoke('get_codex_all_time_stats')
    allTimeStats.value = toCamel(raw)
  } catch {
    allTimeStats.value = { requests: 0, tokens: 0 }
  }
}

const loadDailyStats = async () => {
  isLoadingChart.value = true
  try {
    const raw = await invoke('get_codex_daily_stats_by_gateway_profile_from_storage', { days: 30 })
    dailyStatsSeries.value = toCamel(raw).series || []
  } catch {
    dailyStatsSeries.value = []
  } finally {
    isLoadingChart.value = false
  }
}

const loadLogs = async () => {
  try {
    const range = getLogRange()
    const query = {
      limit: logLimit.value,
      offset: logOffset.value,
      startTs: range.startTs,
      endTs: range.endTs,
      model: logModelFilter.value.trim() || null,
      status: logStatusFilter.value || null,
      accountId: logAccountFilter.value.trim() || null
    }
    const raw = await invoke('query_codex_logs_from_storage', { query })
    logPage.value = toCamel(raw)
  } catch {
    // 静默失败
  }
}

const reloadLogs = async () => {
  logOffset.value = 0
  await loadLogs()
}

const prevLogPage = async () => {
  logOffset.value = Math.max(0, logOffset.value - logLimit.value)
  await loadLogs()
}

const nextLogPage = async () => {
  if (logOffset.value + logLimit.value >= logPage.value.total) return
  logOffset.value += logLimit.value
  await loadLogs()
}

onMounted(async () => {
  await refreshAllData({ refreshGatewayProfiles: true })
  pollTimer = window.setInterval(() => {
    refreshAllData()
  }, 1000)
})

onBeforeUnmount(() => {
  if (pollTimer) {
    window.clearInterval(pollTimer)
    pollTimer = null
  }
  if (modelFilterTimer) {
    clearTimeout(modelFilterTimer)
    modelFilterTimer = null
  }
})

const toggleServer = async () => {
  isToggling.value = true
  try {
    if (serverStatus.value.running) {
      await invoke('stop_codex_server')
      window.$notify?.success($t('platform.openai.codexDialog.stopSuccess'))
    } else {
      await invoke('start_codex_server', {
        config: {
          port: SHARED_PORT,
          poolStrategy: poolStrategy.value || 'round-robin',
          logRequests: true,
          maxLogEntries: 3000,
          apiKey: primaryGatewayApiKey.value || null
        }
      })
      window.$notify?.success($t('platform.openai.codexDialog.startSuccess'))
    }
    await refreshAllData({ refreshGatewayProfiles: true })
  } catch (error) {
    window.$notify?.error($t('platform.openai.codexDialog.toggleFailed', { error: error?.message || error }))
  } finally {
    isToggling.value = false
  }
}

const refreshAllData = async ({ refreshPool = false, refreshGatewayProfiles = false } = {}) => {
  isLoading.value = true
  try {
    const topLevelTasks = [loadServerStatus(), loadAccessConfig()]
    if (refreshGatewayProfiles) {
      topLevelTasks.push(loadGatewayProfiles())
    }

    await Promise.all(topLevelTasks)
    if (refreshPool) {
      const refreshed = await invoke('refresh_codex_pool')
      applyPoolStatus(refreshed)
    }
    await Promise.all([loadPoolStatus(), loadPeriodStats(), loadAllTimeStats(), loadLogs(), loadDailyStats()])
  } catch (error) {
    console.error('Failed to load codex dialog data:', error)
  } finally {
    isLoading.value = false
  }
}

const manualRefresh = async () => {
  // 先 flush 内存日志到 SQLite 存储
  try {
    await invoke('flush_codex_logs')
  } catch {
    // 忽略 flush 错误
  }
  await refreshAllData({ refreshPool: true, refreshGatewayProfiles: true })
}
</script>
