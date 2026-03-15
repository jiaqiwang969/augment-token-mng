<template>
  <BaseModal
    :visible="true"
    :title="''"
    :modal-class="'max-w-[1280px]'"
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
        <div class="grid gap-3 lg:grid-cols-2">
          <div class="space-y-2 rounded-xl border border-border bg-bg-base/50 p-3">
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

          <div class="space-y-2 rounded-xl border border-border bg-bg-base/50 p-3">
            <label class="label mb-0">{{ $t('platform.openai.codexDialog.publicServerUrl') }}</label>
            <div class="flex gap-2">
              <input class="input font-mono" :value="publicServerUrl" readonly />
              <button
                class="btn btn--icon btn--ghost !h-[34px] !w-[34px] shrink-0"
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

        <div class="grid gap-3 lg:grid-cols-2">
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

        <div class="grid gap-3 sm:grid-cols-3">
          <div class="rounded-xl border border-border bg-bg-base/50 p-3">
            <div class="text-[12px] text-text-muted">{{ $t('platform.openai.codexDialog.enabledMembers') }}</div>
            <div class="mt-1 text-[18px] font-semibold">{{ formatNumber(teamSummaryCards.enabledMembers) }}</div>
          </div>
          <div class="rounded-xl border border-border bg-bg-base/50 p-3">
            <div class="text-[12px] text-text-muted">{{ $t('platform.openai.codexDialog.todayRequests') }}</div>
            <div class="mt-1 text-[18px] font-semibold">{{ formatNumber(teamSummaryCards.todayRequests) }}</div>
          </div>
          <div class="rounded-xl border border-border bg-bg-base/50 p-3">
            <div class="text-[12px] text-text-muted">{{ $t('platform.openai.codexDialog.todayTokens') }}</div>
            <div class="mt-1 text-[18px] font-semibold">{{ formatTokens(teamSummaryCards.todayTokens) }}</div>
          </div>
        </div>

        <section class="space-y-4 rounded-2xl border border-border bg-bg-base/40 p-4">
          <div class="flex flex-wrap items-start justify-between gap-3">
            <div>
              <label class="label mb-0">{{ $t('platform.openai.codexDialog.teamMembersTitle') }}</label>
              <p class="mt-1 text-[12px] text-text-muted">
                {{ $t('platform.openai.codexDialog.teamMembersHint') }}
              </p>
            </div>
            <div class="flex flex-wrap gap-2">
              <button class="btn btn--primary btn--sm" :disabled="isCreatingProfile" @click="openCreateMemberEditor">
                {{ $t('platform.openai.codexDialog.addMember') }}
              </button>
              <button class="btn btn--secondary btn--sm" :disabled="isImportingTeam" @click="importTeamTemplate">
                {{ $t('platform.openai.codexDialog.syncTeamTemplate') }}
              </button>
              <button class="btn btn--secondary btn--sm" :disabled="memberTableRows.length === 0" @click="copyAllGatewayAccess">
                {{ $t('platform.openai.codexDialog.exportAllMembersAccess') }}
              </button>
            </div>
          </div>

          <div
            v-if="memberTableRows.length === 0"
            class="rounded-xl border border-dashed border-border bg-muted/10 px-4 py-8 text-center text-[12px] text-text-muted"
          >
            <p class="m-0">{{ $t('platform.openai.codexDialog.noTeamMembers') }}</p>
            <p class="m-0 mt-2">{{ $t('platform.openai.codexDialog.noTeamMembersHint') }}</p>
          </div>

          <template v-else>
            <div
              v-if="memberAnalyticsTruncated"
              class="rounded-lg border border-warning/40 bg-warning/10 px-3 py-2 text-[11px] text-warning"
            >
              {{ $t('platform.openai.codexDialog.analyticsTruncatedHint', { limit: formatNumber(TEAM_ANALYTICS_LIMIT) }) }}
            </div>

            <div class="rounded-xl border border-border bg-muted/10 p-3">
              <div class="flex flex-wrap items-center justify-between gap-3">
                <div>
                  <h4 class="m-0 text-[13px] font-semibold text-text-secondary">
                    {{ $t('platform.openai.codexDialog.monthlyTrend') }}
                  </h4>
                  <p class="m-0 mt-1 text-[11px] text-text-muted">
                    {{ $t('platform.openai.codexDialog.monthlyTrendHint') }}
                  </p>
                </div>
                <div class="flex flex-wrap items-center gap-2">
                  <span class="rounded-full bg-primary/10 px-3 py-1 text-[11px] font-medium text-primary">
                    {{ memberSelectionLabel }}
                  </span>
                  <button
                    v-if="memberTableRows.length > 0"
                    class="btn btn--ghost btn--sm"
                    :disabled="allMembersSelected"
                    @click="selectAllMembers"
                  >
                    {{ $t('platform.openai.codexDialog.selectAllMembers') }}
                  </button>
                  <button
                    v-if="memberTableRows.length > 0"
                    class="btn btn--ghost btn--sm"
                    :disabled="selectedMemberCount === 0"
                    @click="clearMemberSelection"
                  >
                    {{ $t('platform.openai.codexDialog.clearSelection') }}
                  </button>
                </div>
              </div>
              <div class="mt-3 grid gap-3 xl:grid-cols-[minmax(0,1.8fr)_minmax(280px,1fr)]">
                <CodexUsageChart :loading="isLoadingChart" :chart-data="filteredDailyStatsSeries" />
                <CodexUsagePieChart :chart-data="tokenShareSeries" />
              </div>
            </div>

            <div class="rounded-xl border border-border bg-muted/10">
              <div class="flex flex-wrap items-center justify-between gap-3 border-b border-border/70 px-3 py-3">
                <div>
                  <h4 class="m-0 text-[13px] font-semibold text-text-secondary">
                    {{ $t('platform.openai.codexDialog.memberTableTitle') }}
                  </h4>
                  <p class="m-0 mt-1 text-[11px] text-text-muted">
                    {{ $t('platform.openai.codexDialog.memberTableHint') }}
                  </p>
                </div>
                <span v-if="isLoadingMemberAnalytics" class="spinner spinner--sm"></span>
              </div>

              <div class="max-h-[420px] overflow-auto">
                <table class="table table-fixed">
                  <thead class="sticky top-0 z-10 overflow-hidden rounded-t-lg bg-bg-base">
                    <tr>
                      <th class="w-[6%] first:rounded-tl-lg text-center">#</th>
                      <th class="w-[24%]">{{ $t('platform.openai.codexDialog.member') }}</th>
                      <th class="w-[10%]">{{ $t('platform.openai.codexDialog.memberCodeLabel') }}</th>
                      <th class="w-[14%]">{{ $t('platform.openai.codexDialog.roleTitleLabel') }}</th>
                      <th class="w-[9%]">{{ $t('platform.openai.codexDialog.status') }}</th>
                      <th class="w-[9%] text-right">{{ $t('platform.openai.codexDialog.todayRequests') }}</th>
                      <th class="w-[11%] text-right">{{ $t('platform.openai.codexDialog.thirtyDayTokens') }}</th>
                      <th class="w-[9%]">{{ $t('platform.openai.codexDialog.lastActive') }}</th>
                      <th class="w-[8%] last:rounded-tr-lg text-right">{{ $t('platform.openai.codexDialog.actions') }}</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr
                      v-for="profile in memberTableRows"
                      :key="profile.id"
                      :class="{
                        'bg-primary/8': profile.id === focusedMemberProfileId,
                        'bg-primary/4': profile.id !== focusedMemberProfileId && isMemberSelected(profile.id)
                      }"
                      @click="setFocusedMember(profile.id)"
                    >
                      <td class="text-center">
                        <input
                          type="checkbox"
                          class="h-4 w-4 accent-accent"
                          :checked="isMemberSelected(profile.id)"
                          @click.stop="toggleMemberSelection(profile.id)"
                        />
                      </td>
                      <td>
                        <div class="flex items-start gap-2">
                          <span
                            class="mt-1 h-2.5 w-2.5 rounded-full"
                            :style="{ backgroundColor: profile.rowColor || '#4c6ef5' }"
                          ></span>
                          <div class="min-w-0">
                            <div class="truncate text-[12px] font-medium">{{ profile.name || profile.id }}</div>
                            <div class="mt-0.5 truncate text-[11px] text-text-muted">
                              {{ profile.personaSummary || profile.displayLabel }}
                            </div>
                          </div>
                        </div>
                      </td>
                      <td class="font-mono text-[11px]">{{ profile.memberCode || '-' }}</td>
                      <td class="truncate text-[11px]">{{ profile.roleTitle || '-' }}</td>
                      <td>
                        <span :class="['badge badge--sm', profile.enabled ? 'badge--success-tech' : 'badge--danger-tech']">
                          {{ profile.enabled ? $t('platform.openai.codexDialog.enabledKey') : $t('platform.openai.codexDialog.disabledKey') }}
                        </span>
                      </td>
                      <td class="text-right text-[11px]">{{ formatNumber(profile.todayRequests) }}</td>
                      <td class="text-right text-[11px]">{{ formatTokens(profile.totalTokens) }}</td>
                      <td class="text-[11px]">{{ formatTs(profile.lastActiveTs) }}</td>
                      <td class="text-right">
                        <div class="flex justify-end gap-2">
                          <button class="btn btn--ghost btn--sm" @click.stop="copyGatewayAccess(profile)">
                            {{ $t('common.copy') }}
                          </button>
                          <button class="btn btn--ghost btn--sm" @click.stop="openEditMemberEditor(profile)">
                            {{ $t('common.edit') }}
                          </button>
                        </div>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </div>

            <div v-if="focusedMemberRow" class="rounded-xl border border-border bg-muted/10 p-4">
              <div class="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <h4 class="m-0 text-[13px] font-semibold text-text-secondary">
                    {{ $t('platform.openai.codexDialog.selectedMemberLabel') }}
                  </h4>
                  <p class="m-0 mt-1 text-[11px] text-text-muted">
                    {{ $t('platform.openai.codexDialog.selectedMemberHint') }}
                  </p>
                </div>
                <div class="flex items-center gap-2">
                  <span
                    class="rounded-full bg-primary/10 px-2 py-1 text-[11px] font-medium text-primary"
                  >
                    {{ focusedMemberRow.displayLabel }}
                  </span>
                </div>
              </div>

              <div class="mt-4 space-y-4">
                <div class="grid gap-4 xl:grid-cols-[minmax(0,1.2fr)_minmax(0,1fr)]">
                  <div class="rounded-2xl border border-border bg-bg-base/70 p-4">
                    <div class="flex items-start justify-between gap-3">
                      <div class="flex min-w-0 items-start gap-3">
                        <span
                          class="mt-1 h-3 w-3 shrink-0 rounded-full"
                          :style="{ backgroundColor: focusedMemberRow.rowColor || '#4c6ef5' }"
                        ></span>
                        <div class="min-w-0">
                          <div class="truncate text-[16px] font-semibold text-text-primary">
                            {{ focusedMemberRow.name || focusedMemberRow.id }}
                          </div>
                          <div class="mt-1 flex flex-wrap gap-2 text-[11px] text-text-muted">
                            <span class="rounded-full bg-muted/60 px-2 py-1 font-mono">{{ focusedMemberRow.memberCode || '-' }}</span>
                            <span class="rounded-full bg-muted/60 px-2 py-1">{{ focusedMemberRow.roleTitle || '-' }}</span>
                          </div>
                        </div>
                      </div>
                      <span :class="['badge badge--sm', focusedMemberRow.enabled ? 'badge--success-tech' : 'badge--danger-tech']">
                        {{ focusedMemberRow.enabled ? $t('platform.openai.codexDialog.enabledKey') : $t('platform.openai.codexDialog.disabledKey') }}
                      </span>
                    </div>
                    <p class="m-0 mt-4 text-[13px] leading-6 text-text-secondary">
                      {{ focusedMemberRow.personaSummary || $t('platform.openai.codexDialog.noPersonaSummary') }}
                    </p>
                    <p
                      v-if="focusedMemberRow.notes"
                      class="m-0 mt-3 rounded-xl border border-border/70 bg-muted/30 px-3 py-2 text-[11px] leading-5 text-text-muted"
                    >
                      {{ focusedMemberRow.notes }}
                    </p>
                  </div>

                  <div class="grid gap-3 sm:grid-cols-2">
                    <div class="rounded-2xl border border-border bg-bg-base/70 p-3">
                      <div class="text-[11px] text-text-muted">{{ $t('platform.openai.codexDialog.todayRequests') }}</div>
                      <div class="mt-2 text-[18px] font-semibold">{{ formatNumber(focusedMemberRow.todayRequests) }}</div>
                    </div>
                    <div class="rounded-2xl border border-border bg-bg-base/70 p-3">
                      <div class="text-[11px] text-text-muted">{{ $t('platform.openai.codexDialog.todayTokens') }}</div>
                      <div class="mt-2 text-[18px] font-semibold">{{ formatTokens(focusedMemberRow.todayTokens) }}</div>
                    </div>
                    <div class="rounded-2xl border border-border bg-bg-base/70 p-3">
                      <div class="text-[11px] text-text-muted">{{ $t('platform.openai.codexDialog.thirtyDayTokens') }}</div>
                      <div class="mt-2 text-[18px] font-semibold">{{ formatTokens(focusedMemberRow.totalTokens) }}</div>
                    </div>
                    <div class="rounded-2xl border border-border bg-bg-base/70 p-3">
                      <div class="text-[11px] text-text-muted">{{ $t('platform.openai.codexDialog.successRate') }}</div>
                      <div class="mt-2 text-[18px] font-semibold">{{ formatPercent(focusedMemberRow.successRate) }}</div>
                    </div>
                    <div class="rounded-2xl border border-border bg-bg-base/70 p-3">
                      <div class="text-[11px] text-text-muted">{{ $t('platform.openai.codexDialog.avgDuration') }}</div>
                      <div class="mt-2 text-[16px] font-semibold">
                        {{ focusedMemberRow.averageDurationMs ? formatDuration(focusedMemberRow.averageDurationMs) : '-' }}
                      </div>
                    </div>
                    <div class="rounded-2xl border border-border bg-bg-base/70 p-3">
                      <div class="text-[11px] text-text-muted">{{ $t('platform.openai.codexDialog.lastActive') }}</div>
                      <div class="mt-2 text-[13px] font-semibold">{{ formatTs(focusedMemberRow.lastActiveTs) }}</div>
                    </div>
                  </div>
                </div>

                <div class="flex flex-wrap justify-end gap-2">
                  <button class="btn btn--ghost btn--sm" @click="copyGatewayAccess(focusedMemberRow)">
                    {{ $t('common.copy') }}
                  </button>
                  <button class="btn btn--ghost btn--sm" @click="openEditMemberEditor(focusedMemberRow)">
                    {{ $t('common.edit') }}
                  </button>
                </div>
              </div>
            </div>
          </template>
        </section>
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
                      <span>{{ $t('platform.openai.codexDialog.allMembers') }}</span>
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
                  <th class="w-[21%]">{{ $t('platform.openai.codexDialog.member') }}</th>
                  <th class="w-[14%]">{{ $t('platform.openai.codexDialog.account') }}</th>
                  <th class="w-[14%]">{{ $t('platform.openai.codexDialog.model') }}</th>
                  <th class="w-[8%]">{{ $t('platform.openai.codexDialog.format') }}</th>
                  <th class="w-[10%] text-right">{{ $t('platform.openai.codexDialog.tokenBreakdown') }}</th>
                  <th class="w-[7%]">{{ $t('platform.openai.codexDialog.status') }}</th>
                  <th class="w-[7%] text-right">{{ $t('platform.openai.codexDialog.duration') }}</th>
                  <th class="w-[7%] last:rounded-tr-lg">{{ $t('platform.openai.codexDialog.error') }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-if="logPage.items.length === 0">
                  <td colspan="9" class="text-center text-text-muted">{{ $t('platform.openai.codexDialog.noLogs') }}</td>
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
                          {{ log.apiKeySuffix ? $t('platform.openai.codexDialog.keySuffixLabel', { suffix: log.apiKeySuffix }) : '-' }}
                        </div>
                      </div>
                    </div>
                  </td>
                  <td class="text-[11px] truncate"><span class="inline-block -mb-1" v-tooltip="log.accountEmail">{{ log.accountEmail || '-' }}</span></td>
                  <td class="font-mono text-[11px] truncate"><span class="inline-block -mb-1" v-tooltip="log.model">{{ log.model }}</span></td>
                  <td class="text-[11px]">{{ log.format }}</td>
                  <td class="text-right text-[10px] leading-5">
                    <div>{{ $t('platform.openai.codexDialog.inputTokensShort') }} {{ formatTokens(log.inputTokens) }}</div>
                    <div>{{ $t('platform.openai.codexDialog.outputTokensShort') }} {{ formatTokens(log.outputTokens) }}</div>
                    <div class="font-semibold">{{ $t('platform.openai.codexDialog.totalTokensShort') }} {{ formatTokens(log.totalTokens) }}</div>
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

    <CodexTeamMemberEditorModal
      v-if="memberEditorState.visible"
      :mode="memberEditorState.mode"
      :member="memberEditorState.profile"
      :busy="isMemberEditorBusy"
      :resettable="isResettableTeamMember(memberEditorState.profile)"
      :public-base-url="publicServerUrl"
      :local-base-url="accessConfig.serverUrl"
      @close="closeMemberEditor"
      @save="saveMemberEditor"
      @copy-access="copyGatewayAccess"
      @regenerate="regenerateGatewayProfileKey"
      @reset-defaults="resetGatewayProfileToTeamDefaults"
      @delete="deleteGatewayProfile"
    />

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
import CodexTeamMemberEditorModal from '@/components/openai/CodexTeamMemberEditorModal.vue'
import CodexUsageChart from '@/components/openai/CodexUsageChart.vue'
import CodexUsagePieChart from '@/components/openai/CodexUsagePieChart.vue'
import CodexQuickSwitchModal from '@/components/openai/CodexQuickSwitchModal.vue'
import {
  buildAllMembersAccessBundle,
  buildSelectedProfileSeries,
  buildTokenShareSeries,
  buildTeamMemberRows,
  resolveFocusedProfileId,
  syncSelectedProfileIds
} from '@/utils/codexTeamUi.js'

defineEmits(['close'])

const { t: $t } = useI18n()

const isLoading = ref(false)
const isToggling = ref(false)
const isCreatingProfile = ref(false)
const activeTab = ref('overview')
const showQuickSwitchModal = ref('') // 'codex' | 'droid' | ''
const SHARED_PORT = 8766
const publicServerUrl = 'https://lingkong.xyz/v1'
const TEAM_MEMBER_ORDER = ['jdd', 'jqw', 'cr', 'lsb', 'will', 'cp', 'dlz', 'cw', 'xj', 'zdz']
const TEAM_ANALYTICS_LIMIT = 5000
const TEAM_ANALYTICS_REFRESH_MS = 15000

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
const gatewayProfiles = ref([])
const profileBusyState = ref({})
const isImportingTeam = ref(false)
const poolStrategy = ref('round-robin')
const selectedAccountId = ref('')
const isChangingStrategy = ref(false)
const availableAccounts = ref([])
const memberAnalytics = ref([])
const isLoadingMemberAnalytics = ref(false)
const memberAnalyticsTruncated = ref(false)
const selectedMemberProfileIds = ref([])
const focusedMemberProfileId = ref('')
const memberEditorState = ref({
  visible: false,
  mode: 'create',
  profile: null
})
let lastMemberAnalyticsLoadedAt = 0
const primaryGatewayApiKey = computed(() => {
  const primary = gatewayProfiles.value.find(profile => profile.isPrimary)
  return primary?.apiKey || accessConfig.value.apiKey || ''
})
const memberAnalyticsByProfileId = computed(() =>
  new Map(memberAnalytics.value.map(entry => [entry.profileId, entry]))
)
const memberTableRows = computed(() =>
  buildTeamMemberRows({
    profiles: gatewayProfiles.value,
    analyticsByProfileId: memberAnalyticsByProfileId.value,
    teamMemberOrder: TEAM_MEMBER_ORDER
  })
)
const focusedMemberRow = computed(() =>
  memberTableRows.value.find(profile => profile.id === focusedMemberProfileId.value) || null
)
const filteredDailyStatsSeries = computed(() =>
  buildSelectedProfileSeries({
    series: dailyStatsSeries.value,
    selectedProfileIds: selectedMemberProfileIds.value,
    profiles: memberTableRows.value
  })
)
const tokenShareSeries = computed(() => buildTokenShareSeries(filteredDailyStatsSeries.value))
const selectedMemberCount = computed(() => selectedMemberProfileIds.value.length)
const allMembersSelected = computed(() =>
  memberTableRows.value.length > 0 && selectedMemberCount.value === memberTableRows.value.length
)
const memberSelectionLabel = computed(() =>
  allMembersSelected.value
    ? $t('platform.openai.codexDialog.allVisibleMembers')
    : $t('platform.openai.codexDialog.selectedMembersCount', { count: selectedMemberCount.value })
)
const teamSummaryCards = computed(() => ({
  enabledMembers: memberTableRows.value.filter(profile => profile.enabled).length,
  todayRequests: memberTableRows.value.reduce((sum, profile) => sum + profile.todayRequests, 0),
  todayTokens: memberTableRows.value.reduce((sum, profile) => sum + profile.todayTokens, 0)
}))
const logMemberOptions = computed(() =>
  memberTableRows.value
    .filter(profile => profile.memberCode)
    .reduce((options, profile) => {
      if (!options.some(option => option.value === profile.memberCode)) {
        options.push({
          value: profile.memberCode,
          label: profile.displayLabel
        })
      }
      return options
    }, [])
)
const activeEditorProfileId = computed(() =>
  memberEditorState.value.mode === 'edit' ? String(memberEditorState.value.profile?.id || '').trim() : ''
)
const isMemberEditorBusy = computed(() =>
  memberEditorState.value.mode === 'create'
    ? isCreatingProfile.value
    : isProfileBusy(activeEditorProfileId.value)
)

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
const logMemberFilter = ref('')
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

const formatPercent = (value) => `${Number(value || 0).toFixed(1)}%`

const normalizeMemberCode = (value) => String(value || '').trim().toLowerCase()

const buildProfileDisplayLabel = (profile) => {
  const parts = [profile?.name, profile?.memberCode, profile?.roleTitle]
    .map(value => String(value || '').trim())
    .filter(Boolean)
  return parts.length > 0 ? parts.join(' · ') : profile?.id || 'Profile'
}

const buildLogDisplayLabel = (log) => {
  const rawLabel = String(log?.displayLabel || '').trim()
  if (rawLabel) {
    return rawLabel
  }
  const parts = [log?.gatewayProfileName, log?.memberCode, log?.roleTitle]
    .map(value => String(value || '').trim())
    .filter(Boolean)
  return parts.length > 0 ? parts.join(' · ') : '-'
}

const isResettableTeamMember = (profile) =>
  TEAM_MEMBER_ORDER.includes(normalizeMemberCode(profile?.memberCode))

const getTodayStartTs = () => {
  const start = new Date()
  start.setHours(0, 0, 0, 0)
  return Math.floor(start.getTime() / 1000)
}

const buildMemberAnalytics = (logs) => {
  const grouped = new Map()
  const todayStartTs = getTodayStartTs()

  for (const log of logs) {
    const profileId = String(log?.gatewayProfileId || log?.memberCode || '').trim() || `legacy:${buildLogDisplayLabel(log)}`
    const existing = grouped.get(profileId) || {
      profileId,
      profileName: String(log?.gatewayProfileName || log?.displayLabel || 'Legacy').trim() || 'Legacy',
      memberCode: String(log?.memberCode || '').trim(),
      roleTitle: String(log?.roleTitle || '').trim(),
      displayLabel: buildLogDisplayLabel(log),
      apiKeySuffix: String(log?.apiKeySuffix || '').trim(),
      color: String(log?.color || '').trim(),
      requests: 0,
      totalTokens: 0,
      todayRequests: 0,
      todayTokens: 0,
      successCount: 0,
      durationSum: 0,
      durationCount: 0,
      lastActiveTs: null
    }

    existing.requests += 1
    existing.totalTokens += Number(log?.totalTokens || 0)
    if (log?.status === 'success') {
      existing.successCount += 1
    }
    if (Number.isFinite(Number(log?.requestDurationMs)) && Number(log?.requestDurationMs) > 0) {
      existing.durationSum += Number(log.requestDurationMs)
      existing.durationCount += 1
    }
    const timestamp = Number(log?.timestamp || 0)
    if (timestamp >= todayStartTs) {
      existing.todayRequests += 1
      existing.todayTokens += Number(log?.totalTokens || 0)
    }
    if (!existing.lastActiveTs || timestamp > existing.lastActiveTs) {
      existing.lastActiveTs = timestamp || existing.lastActiveTs
    }

    grouped.set(profileId, existing)
  }

  return [...grouped.values()]
    .map((entry) => ({
      ...entry,
      successRate: entry.requests > 0 ? (entry.successCount / entry.requests) * 100 : 0,
      averageDurationMs: entry.durationCount > 0 ? Math.round(entry.durationSum / entry.durationCount) : null
    }))
    .sort((left, right) => {
      if (right.totalTokens !== left.totalTokens) {
        return right.totalTokens - left.totalTokens
      }
      if (right.requests !== left.requests) {
        return right.requests - left.requests
      }
      return String(left.profileName).localeCompare(String(right.profileName), 'zh-CN')
    })
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

const normalizeGatewayProfile = (profile) => {
  const data = toCamel(profile)
  return {
    id: data.id || '',
    name: data.name || '',
    apiKey: data.apiKey || '',
    enabled: data.enabled !== false,
    isPrimary: !!data.isPrimary,
    memberCode: String(data.memberCode || '').trim(),
    roleTitle: String(data.roleTitle || '').trim(),
    personaSummary: String(data.personaSummary || '').trim(),
    color: String(data.color || '').trim(),
    notes: String(data.notes || '').trim()
  }
}

const cleanupProfileMaps = (profiles) => {
  const validIds = new Set(profiles.map(profile => profile.id))
  profileBusyState.value = Object.fromEntries(
    Object.entries(profileBusyState.value).filter(([id]) => validIds.has(id))
  )
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

const normalizeOptionalGatewayField = (value) => {
  const trimmed = String(value || '').trim()
  return trimmed || null
}

const buildEmptyMemberDraft = () => ({
  id: '',
  name: '',
  apiKey: '',
  enabled: true,
  isPrimary: false,
  memberCode: '',
  roleTitle: '',
  personaSummary: '',
  color: '#4c6ef5',
  notes: ''
})

const buildGatewayProfileMutationPayload = (profile, { allowGeneratedApiKey = false } = {}) => {
  const name = String(profile?.name || '').trim()
  const memberCode = String(profile?.memberCode || '').trim()
  const apiKey = normalizeOptionalGatewayField(profile?.apiKey)

  if (!name) {
    window.$notify?.error($t('platform.openai.codexDialog.memberNameRequired'))
    return null
  }
  if (!memberCode) {
    window.$notify?.error($t('platform.openai.codexDialog.memberCodeRequired'))
    return null
  }
  if (!allowGeneratedApiKey && !apiKey) {
    window.$notify?.error($t('platform.openai.codexDialog.apiKeyRequired'))
    return null
  }

  return {
    name,
    apiKey,
    enabled: profile?.enabled !== false,
    memberCode,
    roleTitle: normalizeOptionalGatewayField(profile?.roleTitle),
    personaSummary: normalizeOptionalGatewayField(profile?.personaSummary),
    color: normalizeOptionalGatewayField(profile?.color),
    notes: normalizeOptionalGatewayField(profile?.notes)
  }
}

const openCreateMemberEditor = () => {
  memberEditorState.value = {
    visible: true,
    mode: 'create',
    profile: buildEmptyMemberDraft()
  }
}

const openEditMemberEditor = (profile) => {
  memberEditorState.value = {
    visible: true,
    mode: 'edit',
    profile: normalizeGatewayProfile(profile)
  }
}

const closeMemberEditor = () => {
  if (isMemberEditorBusy.value) {
    return
  }
  memberEditorState.value = {
    visible: false,
    mode: 'create',
    profile: null
  }
}

const toggleGatewayProfile = async (profile, enabled) => {
  const payload = buildGatewayProfileMutationPayload(
    { ...profile, enabled },
    { allowGeneratedApiKey: false }
  )
  if (!payload || !profile?.id) {
    return
  }

  setProfileBusy(profile.id, true)
  try {
    await invoke('update_codex_gateway_profile', {
      profileId: profile.id,
      ...payload
    })
    await Promise.all([
      loadGatewayProfiles(),
      loadAccessConfig(),
      loadMemberAnalytics({ force: true })
    ])
  } catch (error) {
    console.error('Failed to toggle Codex gateway profile:', error)
    window.$notify?.error(
      $t('platform.openai.codexDialog.updateKeyFailed', { error: error?.message || error })
    )
  } finally {
    setProfileBusy(profile.id, false)
  }
}

const saveMemberEditor = async (profile) => {
  if (!profile) {
    return
  }

  if (memberEditorState.value.mode === 'create') {
    const payload = buildGatewayProfileMutationPayload(profile, { allowGeneratedApiKey: true })
    if (!payload) {
      return
    }

    isCreatingProfile.value = true
    try {
      await invoke('create_codex_gateway_profile', payload)
      await Promise.all([
        loadGatewayProfiles(),
        loadAccessConfig(),
        loadDailyStats(),
        loadMemberAnalytics({ force: true })
      ])
      closeMemberEditor()
      window.$notify?.success($t('platform.openai.codexDialog.createKeySuccess'))
    } catch (error) {
      console.error('Failed to create Codex gateway profile:', error)
      window.$notify?.error(
        $t('platform.openai.codexDialog.createKeyFailed', { error: error?.message || error })
      )
    } finally {
      isCreatingProfile.value = false
    }
    return
  }

  const payload = buildGatewayProfileMutationPayload(profile, { allowGeneratedApiKey: false })
  if (!payload || !profile?.id) {
    return
  }

  setProfileBusy(profile.id, true)
  try {
    await invoke('update_codex_gateway_profile', {
      profileId: profile.id,
      ...payload
    })
    await Promise.all([
      loadGatewayProfiles(),
      loadAccessConfig(),
      loadDailyStats(),
      loadMemberAnalytics({ force: true })
    ])
    closeMemberEditor()
    window.$notify?.success($t('platform.openai.codexDialog.saveKeySuccess'))
  } catch (error) {
    console.error('Failed to save Codex gateway profile:', error)
    window.$notify?.error(
      $t('platform.openai.codexDialog.saveKeyFailed', { error: error?.message || error })
    )
  } finally {
    setProfileBusy(profile.id, false)
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
    await Promise.all([
      loadGatewayProfiles(),
      loadAccessConfig(),
      loadDailyStats(),
      loadMemberAnalytics({ force: true })
    ])
    if (activeEditorProfileId.value === profile.id) {
      closeMemberEditor()
    }
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

const regenerateGatewayProfileKey = async (profile) => {
  if (!profile?.id) {
    return
  }

  setProfileBusy(profile.id, true)
  try {
    await invoke('regenerate_codex_gateway_profile_api_key', {
      profileId: profile.id
    })
    await Promise.all([
      loadGatewayProfiles(),
      loadAccessConfig(),
      loadDailyStats(),
      loadMemberAnalytics({ force: true })
    ])
    window.$notify?.success($t('platform.openai.codexDialog.generateApiKeySuccess'))
  } catch (error) {
    window.$notify?.error(
      $t('platform.openai.codexDialog.updateKeyFailed', { error: error?.message || error })
    )
  } finally {
    setProfileBusy(profile.id, false)
  }
}

const importTeamTemplate = async () => {
  if (isImportingTeam.value) {
    return
  }

  isImportingTeam.value = true
  try {
    await invoke('import_codex_team_template')
    await Promise.all([
      loadGatewayProfiles(),
      loadAccessConfig(),
      loadDailyStats(),
      loadMemberAnalytics({ force: true })
    ])
    window.$notify?.success($t('platform.openai.codexDialog.importTeamSuccess'))
  } catch (error) {
    window.$notify?.error(
      $t('platform.openai.codexDialog.importTeamFailed', { error: error?.message || error })
    )
  } finally {
    isImportingTeam.value = false
  }
}

const resetGatewayProfileToTeamDefaults = async (profile) => {
  if (!profile?.id) {
    return
  }

  setProfileBusy(profile.id, true)
  try {
    await invoke('reset_codex_gateway_profile_to_team_defaults', {
      profileId: profile.id
    })
    await Promise.all([
      loadGatewayProfiles(),
      loadAccessConfig(),
      loadDailyStats(),
      loadMemberAnalytics({ force: true })
    ])
    window.$notify?.success($t('platform.openai.codexDialog.resetTeamMemberSuccess'))
  } catch (error) {
    window.$notify?.error(
      $t('platform.openai.codexDialog.resetTeamMemberFailed', { error: error?.message || error })
    )
  } finally {
    setProfileBusy(profile.id, false)
  }
}

const copyGatewayAccess = async (profile) => {
  const payload = [
    `# ${buildProfileDisplayLabel(profile)}`,
    `OPENAI_BASE_URL=${publicServerUrl}`,
    `OPENAI_API_KEY=${profile.apiKey}`,
    '# Local fallback:',
    `# OPENAI_BASE_URL=${accessConfig.value.serverUrl}`
  ].join('\n')
  await copyText(payload)
}

const copyAllGatewayAccess = async () => {
  const payload = buildAllMembersAccessBundle({
    baseUrl: publicServerUrl,
    profiles: memberTableRows.value
  })
  await copyText(payload)
}

const toggleMemberSelection = (profileId) => {
  const next = new Set(selectedMemberProfileIds.value)
  if (next.has(profileId)) {
    next.delete(profileId)
  } else {
    next.add(profileId)
  }
  selectedMemberProfileIds.value = [...next]
}

const isMemberSelected = (profileId) => selectedMemberProfileIds.value.includes(profileId)

const clearMemberSelection = () => {
  selectedMemberProfileIds.value = []
}

const selectAllMembers = () => {
  selectedMemberProfileIds.value = memberTableRows.value.map(profile => profile.id)
}

const setFocusedMember = (profileId) => {
  focusedMemberProfileId.value = profileId
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

const getLogMemberLabel = () => {
  if (!logMemberFilter.value) {
    return $t('platform.openai.codexDialog.allMembers')
  }
  const member = logMemberOptions.value.find(option => option.value === logMemberFilter.value)
  return member?.label || logMemberFilter.value
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

const selectLogMember = async (value, close) => {
  logMemberFilter.value = value
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

watch(memberTableRows, (profiles, previousProfiles) => {
  selectedMemberProfileIds.value = syncSelectedProfileIds({
    profiles,
    previousProfiles,
    selectedProfileIds: selectedMemberProfileIds.value
  })
  focusedMemberProfileId.value = resolveFocusedProfileId({
    profiles,
    focusedProfileId: focusedMemberProfileId.value
  })
}, { immediate: true })

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

const loadMemberAnalytics = async ({ force = false } = {}) => {
  const now = Date.now()
  if (!force && now - lastMemberAnalyticsLoadedAt < TEAM_ANALYTICS_REFRESH_MS) {
    return
  }

  isLoadingMemberAnalytics.value = true
  try {
    const range = {
      startTs: Math.floor(now / 1000) - 30 * 24 * 3600,
      endTs: Math.floor(now / 1000)
    }
    const raw = await invoke('query_codex_logs_from_storage', {
      query: {
        limit: TEAM_ANALYTICS_LIMIT,
        offset: 0,
        startTs: range.startTs,
        endTs: range.endTs,
        model: null,
        status: null,
        accountId: null,
        memberCode: null
      }
    })
    const page = toCamel(raw)
    memberAnalytics.value = buildMemberAnalytics(page.items || [])
    memberAnalyticsTruncated.value = Number(page.total || 0) > Number((page.items || []).length)
    lastMemberAnalyticsLoadedAt = now
  } catch {
    memberAnalytics.value = []
    memberAnalyticsTruncated.value = false
  } finally {
    isLoadingMemberAnalytics.value = false
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
      accountId: logAccountFilter.value.trim() || null,
      memberCode: logMemberFilter.value.trim() || null
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
    await Promise.all([
      loadPoolStatus(),
      loadLogs(),
      loadDailyStats(),
      loadMemberAnalytics({ force: refreshGatewayProfiles || refreshPool })
    ])
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
  await loadMemberAnalytics({ force: true })
}
</script>
