import re

anti_path = "src/components/antigravity/AntigravityServerDialog.vue"

with open(anti_path, 'r', encoding='utf-8') as f:
    anti_content = f.read()

# Replace the HTML v-else block
new_v_else = """      <div v-else class="flex h-full flex-col gap-2 p-1">
        <div class="flex min-h-0 flex-1 flex-col gap-2 rounded-lg border border-border p-3">
          <div class="flex flex-wrap items-center justify-between gap-2">
            <h4 class="text-[13px] font-semibold">{{ $t('platform.antigravity.apiService.logsTitle') }}</h4>
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
              <!-- 成员筛选 -->
              <FloatingDropdown placement="bottom-end" :offset="4">
                <template #trigger="{ isOpen }">
                  <button
                    class="btn btn--secondary btn--sm h-8 flex items-center gap-1 px-2"
                    :class="{ 'btn--light': !isOpen }"
                    type="button"
                  >
                    <span class="text-[13px] truncate max-w-[180px]">{{ getLogMemberLabel() }}</span>
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M6 9l6 6 6-6"/>
                    </svg>
                  </button>
                </template>
                <template #default="{ close }">
                  <div class="py-1">
                    <button
                      class="dropdown-item flex items-center gap-2 px-3 py-1.5 text-[13px]"
                      :class="{ 'bg-primary/10': !logMemberFilter }"
                      @click="selectLogMember('', close)"
                    >
                      <span>{{ $t('platform.antigravity.apiService.allMembers') }}</span>
                    </button>
                    <button
                      v-for="member in logMemberOptions"
                      :key="member.value"
                      class="dropdown-item flex items-center gap-2 px-3 py-1.5 text-[13px]"
                      :class="{ 'bg-primary/10': member.value === logMemberFilter }"
                      @click="selectLogMember(member.value, close)"
                    >
                      <span class="truncate">{{ member.label }}</span>
                    </button>
                  </div>
                </template>
              </FloatingDropdown>
              <!-- 模型筛选 -->
              <input v-model="logModelFilter" class="input h-8 w-[140px]" :placeholder="$t('platform.antigravity.apiService.modelFilterPlaceholder')" />
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
                  <th class="w-[12%] first:rounded-tl-lg">{{ $t('platform.antigravity.apiService.time') }}</th>
                  <th class="w-[21%]">{{ $t('platform.antigravity.apiService.member') }}</th>
                  <th class="w-[14%]">{{ $t('platform.antigravity.apiService.account') }}</th>
                  <th class="w-[14%]">{{ $t('common.model') }}</th>
                  <th class="w-[8%]">{{ $t('platform.antigravity.apiService.format') }}</th>
                  <th class="w-[10%] text-right">{{ $t('platform.antigravity.apiService.tokenBreakdown') }}</th>
                  <th class="w-[7%]">{{ $t('platform.antigravity.apiService.status') }}</th>
                  <th class="w-[7%] text-right">{{ $t('platform.antigravity.apiService.avgDuration') }}</th>
                  <th class="w-[7%] last:rounded-tr-lg">{{ $t('platform.antigravity.apiService.error') }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-if="logPage.items.length === 0">
                  <td colspan="9" class="text-center text-text-muted">{{ $t('platform.antigravity.apiService.noLogs') }}</td>
                </tr>
                <tr v-for="log in logPage.items" :key="log.id">
                  <td class="font-mono text-[11px]">{{ formatTs(log.timestamp) }}</td>
                  <td>
                    <div class="flex items-start gap-2">
                      <span
                        class="mt-1 h-2.5 w-2.5 rounded-full"
                        :style="{ backgroundColor: log.color || '#4c6ef5' }"
                      ></span>
                      <div class="min-w-0">
                        <div class="truncate text-[11px] font-medium" v-tooltip="buildLogDisplayLabel(log)">
                          {{ buildLogDisplayLabel(log) }}
                        </div>
                        <div class="mt-0.5 truncate text-[10px] text-text-muted">
                          {{ log.apiKeySuffix ? $t('platform.antigravity.apiService.keySuffixLabel', { suffix: log.apiKeySuffix }) : '-' }}
                        </div>
                      </div>
                    </div>
                  </td>
                  <td class="text-[11px] truncate"><span class="inline-block -mb-1" v-tooltip="log.accountEmail">{{ log.accountEmail || '-' }}</span></td>
                  <td class="font-mono text-[11px] truncate"><span class="inline-block -mb-1" v-tooltip="log.model">{{ log.model }}</span></td>
                  <td class="text-[11px]">{{ log.format }}</td>
                  <td class="text-right text-[10px] leading-5">
                    <div>{{ $t('platform.antigravity.apiService.inputTokensShort') }} {{ formatTokens(log.inputTokens) }}</div>
                    <div>{{ $t('platform.antigravity.apiService.outputTokensShort') }} {{ formatTokens(log.outputTokens) }}</div>
                    <div class="font-semibold">{{ $t('platform.antigravity.apiService.totalTokensShort') }} {{ formatTokens(log.totalTokens) }}</div>
                  </td>
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
            <span class="text-[12px] text-text-muted">{{ $t('platform.antigravity.apiService.pageInfo', { current: currentLogPage, total: totalLogPages }) }} · {{ formatNumber(logPage.total) }}</span>
            <div class="flex items-center gap-2">
              <button class="btn btn--secondary btn--sm" :disabled="logOffset === 0" @click="prevLogPage">{{ $t('common.previousPage') }}</button>
              <button
                class="btn btn--secondary btn--sm"
                :disabled="logOffset + logLimit >= logPage.total"
                @click="nextLogPage"
              >{{ $t('common.nextPage') }}</button>
            </div>
          </div>
        </div>
      </div>
"""

# Apply the regex substitution
anti_content = re.sub(
    r'<div v-else class="flex h-full flex-col gap-3 p-1">.*?<AntigravityTeamMemberEditorModal',
    new_v_else + "\n    <AntigravityTeamMemberEditorModal",
    anti_content,
    flags=re.DOTALL
)

with open(anti_path, 'w', encoding='utf-8') as f:
    f.write(anti_content)
