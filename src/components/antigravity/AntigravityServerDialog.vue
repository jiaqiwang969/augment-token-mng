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
            {{ $t('platform.antigravity.apiService.tabOverview') }}
          </button>
          <button
            class="btn btn--sm"
            :class="activeTab === 'logs' ? 'btn--primary' : 'btn--ghost'"
            @click="activeTab = 'logs'"
          >
            {{ $t('platform.antigravity.apiService.tabRequestLogs') }}
          </button>
        </div>
        <div class="flex items-center gap-2">
          <button class="btn btn--secondary btn--sm" :disabled="isLoading" @click="manualRefresh">
            {{ $t('platform.antigravity.apiService.refresh') }}
          </button>
        </div>
      </div>
    </template>

    <div class="h-[80vh] overflow-hidden">
      <div v-if="activeTab === 'overview'" class="h-full space-y-4 overflow-y-auto p-1 pr-2">
        <div class="grid gap-3 lg:grid-cols-2">
          <div class="space-y-2 rounded-xl border border-border bg-bg-base/50 p-3">
            <label class="label mb-0">{{ $t('platform.antigravity.apiService.localServerUrl') }}</label>
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
            <label class="label mb-0">{{ $t('platform.antigravity.apiService.publicServerUrl') }}</label>
            <div class="flex gap-2">
              <input class="input font-mono" :value="accessConfig.publicServerUrl" readonly />
              <button
                class="btn btn--icon btn--ghost !h-[34px] !w-[34px] shrink-0"
                v-tooltip="$t('common.copy')"
                @click="copyText(accessConfig.publicServerUrl)"
              >
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
                  <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2 2v1"></path>
                </svg>
              </button>
            </div>
          </div>
        </div>

        <div class="grid gap-3 sm:grid-cols-5">
          <div class="rounded-xl border border-border bg-bg-base/50 p-3">
            <div class="text-[12px] text-text-muted">{{ $t('platform.antigravity.apiService.totalAccounts') }}</div>
            <div class="mt-1 text-[18px] font-semibold">{{ formatNumber(serviceStatus.totalAccounts) }}</div>
          </div>
          <div class="rounded-xl border border-border bg-bg-base/50 p-3">
            <div class="text-[12px] text-text-muted">{{ $t('platform.antigravity.apiService.availableAccounts') }}</div>
            <div class="mt-1 text-[18px] font-semibold">{{ formatNumber(serviceStatus.availableAccounts) }}</div>
          </div>
          <div class="rounded-xl border border-border bg-bg-base/50 p-3">
            <div class="text-[12px] text-text-muted">{{ $t('platform.antigravity.apiService.enabledMembers') }}</div>
            <div class="mt-1 text-[18px] font-semibold">{{ formatNumber(teamSummaryCards.enabledMembers) }}</div>
          </div>
          <div class="rounded-xl border border-border bg-bg-base/50 p-3">
            <div class="text-[12px] text-text-muted">{{ $t('platform.antigravity.apiService.todayRequests') }}</div>
            <div class="mt-1 text-[18px] font-semibold">{{ formatNumber(teamSummaryCards.todayRequests) }}</div>
          </div>
          <div class="rounded-xl border border-border bg-bg-base/50 p-3">
            <div class="text-[12px] text-text-muted">{{ $t('platform.antigravity.apiService.thirtyDayTokens') }}</div>
            <div class="mt-1 text-[18px] font-semibold">{{ formatTokens(teamSummaryCards.thirtyDayTokens) }}</div>
          </div>
        </div>

        <section class="space-y-3 rounded-2xl border border-border bg-bg-base/40 p-4">
          <div class="flex flex-wrap items-center justify-between gap-3">
            <div>
              <label class="label mb-0">{{ $t('platform.antigravity.apiService.serviceStatus') }}</label>
              <p class="mt-1 text-[12px] text-text-muted">
                {{ serviceStatusSummary }}
              </p>
            </div>
            <div
              class="inline-flex items-center gap-2 rounded-full px-3 py-1 text-[12px] font-medium"
              :class="serviceStatus.sidecarHealthy ? 'bg-success/15 text-success' : 'bg-warning/15 text-warning'"
            >
              <span class="inline-block h-2 w-2 rounded-full bg-current"></span>
              <span>{{ serviceStatus.sidecarHealthy ? $t('platform.antigravity.apiService.healthy') : $t('platform.antigravity.apiService.notReady') }}</span>
            </div>
          </div>
        </section>

        <section class="space-y-4 rounded-2xl border border-border bg-bg-base/40 p-4">
          <div class="flex flex-wrap items-start justify-between gap-3">
            <div>
              <label class="label mb-0">{{ $t('platform.antigravity.apiService.storageOverviewTitle') }}</label>
              <p class="mt-1 text-[12px] text-text-muted">
                {{ $t('platform.antigravity.apiService.storageOverviewHint') }}
              </p>
            </div>
            <div
              v-if="storageStatus.dbPath"
              class="max-w-full truncate rounded-full bg-muted/30 px-3 py-1 text-[11px] font-mono text-text-muted"
              v-tooltip="storageStatus.dbPath"
            >
              {{ storageStatus.dbPath }}
            </div>
          </div>

          <div class="grid gap-3 md:grid-cols-2 xl:grid-cols-5">
            <div class="rounded-xl border border-border bg-bg-base/70 p-3">
              <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.storageTotalLogs') }}</div>
              <div class="mt-2 text-[18px] font-semibold">{{ formatNumber(maintenanceSummary.totalLogs) }}</div>
            </div>
            <div class="rounded-xl border border-border bg-bg-base/70 p-3">
              <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.storageDbSize') }}</div>
              <div class="mt-2 text-[18px] font-semibold">{{ formatBytes(maintenanceSummary.dbSizeBytes) }}</div>
            </div>
            <div class="rounded-xl border border-border bg-bg-base/70 p-3">
              <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.allTimeRequests') }}</div>
              <div class="mt-2 text-[18px] font-semibold">{{ formatNumber(maintenanceSummary.allTimeRequests) }}</div>
            </div>
            <div class="rounded-xl border border-border bg-bg-base/70 p-3">
              <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.allTimeTokens') }}</div>
              <div class="mt-2 text-[18px] font-semibold">{{ formatTokens(maintenanceSummary.allTimeTokens) }}</div>
            </div>
            <div class="rounded-xl border border-border bg-bg-base/70 p-3">
              <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.todayRequests') }}</div>
              <div class="mt-2 text-[18px] font-semibold">{{ formatNumber(maintenanceSummary.todayRequests) }}</div>
            </div>
            <div class="rounded-xl border border-border bg-bg-base/70 p-3">
              <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.todayTokens') }}</div>
              <div class="mt-2 text-[18px] font-semibold">{{ formatTokens(maintenanceSummary.todayTokens) }}</div>
            </div>
            <div class="rounded-xl border border-border bg-bg-base/70 p-3">
              <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.weekRequests') }}</div>
              <div class="mt-2 text-[18px] font-semibold">{{ formatNumber(maintenanceSummary.weekRequests) }}</div>
            </div>
            <div class="rounded-xl border border-border bg-bg-base/70 p-3">
              <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.weekTokens') }}</div>
              <div class="mt-2 text-[18px] font-semibold">{{ formatTokens(maintenanceSummary.weekTokens) }}</div>
            </div>
            <div class="rounded-xl border border-border bg-bg-base/70 p-3">
              <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.monthRequests') }}</div>
              <div class="mt-2 text-[18px] font-semibold">{{ formatNumber(maintenanceSummary.monthRequests) }}</div>
            </div>
            <div class="rounded-xl border border-border bg-bg-base/70 p-3">
              <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.monthTokens') }}</div>
              <div class="mt-2 text-[18px] font-semibold">{{ formatTokens(maintenanceSummary.monthTokens) }}</div>
            </div>
          </div>
        </section>

        <section class="space-y-4 rounded-2xl border border-border bg-bg-base/40 p-4">
          <div>
            <label class="label mb-0">{{ $t('platform.antigravity.apiService.maintenanceTitle') }}</label>
            <p class="mt-1 text-[12px] text-text-muted">
              {{ $t('platform.antigravity.apiService.maintenanceHint') }}
            </p>
          </div>

          <div class="grid gap-3 xl:grid-cols-[minmax(0,0.8fr)_minmax(0,1.2fr)]">
            <div class="rounded-xl border border-border bg-bg-base/70 p-3">
              <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.clearLogs') }}</div>
              <p class="m-0 mt-2 text-[12px] leading-5 text-text-muted">
                {{ $t('platform.antigravity.apiService.storageOverviewHint') }}
              </p>
              <button
                class="btn btn--danger btn--sm mt-3"
                :disabled="isClearingLogs || maintenanceSummary.totalLogs === 0"
                @click="clearLogs"
              >
                {{ $t('platform.antigravity.apiService.clearLogs') }}
              </button>
            </div>

            <div class="rounded-xl border border-border bg-bg-base/70 p-3">
              <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.deleteOldLogs') }}</div>
              <p class="m-0 mt-2 text-[12px] leading-5 text-text-muted">
                {{ $t('platform.antigravity.apiService.deleteBeforeDateHint') }}
              </p>
              <div class="mt-3 flex flex-wrap items-center gap-2">
                <label class="sr-only" for="antigravity-delete-before-date">
                  {{ $t('platform.antigravity.apiService.deleteBeforeDate') }}
                </label>
                <input
                  id="antigravity-delete-before-date"
                  v-model="deleteBeforeDate"
                  type="date"
                  class="input h-9 w-[180px]"
                />
                <button
                  class="btn btn--secondary btn--sm"
                  :disabled="isPruningLogs || maintenanceSummary.totalLogs === 0"
                  @click="deleteLogsBeforeDate"
                >
                  {{ $t('platform.antigravity.apiService.deleteOldLogs') }}
                </button>
              </div>
            </div>
          </div>
        </section>

        <section class="space-y-4 rounded-2xl border border-border bg-bg-base/40 p-4">
          <div class="flex flex-wrap items-start justify-between gap-3">
            <div>
              <label class="label mb-0">{{ $t('platform.antigravity.apiService.teamMembersTitle') }}</label>
              <p class="mt-1 text-[12px] text-text-muted">
                {{ $t('platform.antigravity.apiService.teamMembersHint') }}
              </p>
            </div>
            <div class="flex flex-wrap gap-2">
              <button class="btn btn--primary btn--sm" :disabled="isSavingMember" @click="openCreateMemberEditor">
                {{ $t('platform.antigravity.apiService.addMember') }}
              </button>
              <button class="btn btn--secondary btn--sm" :disabled="isImportingTeam" @click="importTeamTemplate">
                {{ $t('platform.antigravity.apiService.syncTeamTemplate') }}
              </button>
              <button class="btn btn--secondary btn--sm" :disabled="memberTableRows.length === 0" @click="copyAllGatewayAccess">
                {{ $t('platform.antigravity.apiService.exportAllMembersAccess') }}
              </button>
            </div>
          </div>

          <div
            v-if="memberTableRows.length === 0"
            class="rounded-xl border border-dashed border-border bg-muted/10 px-4 py-8 text-center text-[12px] text-text-muted"
          >
            <p class="m-0">{{ $t('platform.antigravity.apiService.noTeamMembers') }}</p>
            <p class="m-0 mt-2">{{ $t('platform.antigravity.apiService.noTeamMembersHint') }}</p>
          </div>

          <template v-else>
            <div
              v-if="memberAnalyticsTruncated"
              class="rounded-lg border border-warning/40 bg-warning/10 px-3 py-2 text-[11px] text-warning"
            >
              {{ $t('platform.antigravity.apiService.analyticsTruncatedHint', { limit: formatNumber(TEAM_ANALYTICS_LIMIT) }) }}
            </div>

            <div class="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-border bg-muted/10 px-3 py-2">
              <span class="rounded-full bg-primary/10 px-3 py-1 text-[11px] font-medium text-primary">
                {{ memberSelectionLabel }}
              </span>
              <div class="flex flex-wrap gap-2">
                <button
                  class="btn btn--ghost btn--sm"
                  :disabled="allMembersSelected"
                  @click="selectAllMembers"
                >
                  {{ $t('platform.antigravity.apiService.selectAllMembers') }}
                </button>
                <button
                  class="btn btn--ghost btn--sm"
                  :disabled="selectedMemberCount === 0"
                  @click="clearMemberSelection"
                >
                  {{ $t('platform.antigravity.apiService.clearSelection') }}
                </button>
              </div>
            </div>

            <div class="grid gap-4 xl:grid-cols-[minmax(0,1.55fr)_minmax(320px,0.95fr)]">
              <CodexUsageChart :loading="isLoadingChart" :chart-data="filteredDailyStatsSeries" />
              <AntigravityTokenShareChart
                :loading="isLoadingChart"
                :chart-data="tokenShareChartData"
                :title="$t('platform.antigravity.apiService.memberTokenShare')"
                :hint="$t('platform.antigravity.apiService.memberTokenShareHint')"
                :empty-label="$t('platform.antigravity.apiService.noData')"
              />
            </div>

            <div class="rounded-xl border border-border bg-muted/10">
              <div class="flex flex-wrap items-center justify-between gap-3 border-b border-border/70 px-3 py-3">
                <div>
                  <h4 class="m-0 text-[13px] font-semibold text-text-secondary">
                    {{ $t('platform.antigravity.apiService.memberUsageTitle') }}
                  </h4>
                  <p class="m-0 mt-1 text-[11px] text-text-muted">
                    {{ $t('platform.antigravity.apiService.memberUsageHint') }}
                  </p>
                </div>
                <span v-if="isLoadingMemberAnalytics" class="spinner spinner--sm"></span>
              </div>

              <div class="max-h-[420px] overflow-auto">
                <table class="table table-fixed">
                  <thead class="sticky top-0 z-10 overflow-hidden rounded-t-lg bg-bg-base">
                    <tr>
                      <th class="w-[6%] first:rounded-tl-lg text-center">#</th>
                      <th class="w-[25%]">{{ $t('platform.antigravity.apiService.member') }}</th>
                      <th class="w-[10%]">{{ $t('platform.antigravity.apiService.memberCodeLabel') }}</th>
                      <th class="w-[14%]">{{ $t('platform.antigravity.apiService.roleTitleLabel') }}</th>
                      <th class="w-[9%]">{{ $t('platform.antigravity.apiService.status') }}</th>
                      <th class="w-[9%] text-right">{{ $t('platform.antigravity.apiService.todayRequests') }}</th>
                      <th class="w-[11%] text-right">{{ $t('platform.antigravity.apiService.thirtyDayTokens') }}</th>
                      <th class="w-[9%]">{{ $t('platform.antigravity.apiService.lastActive') }}</th>
                      <th class="w-[7%] last:rounded-tr-lg text-right">{{ $t('platform.antigravity.apiService.actions') }}</th>
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
                          {{ profile.enabled ? $t('platform.antigravity.apiService.enabledKey') : $t('platform.antigravity.apiService.disabledKey') }}
                        </span>
                      </td>
                      <td class="text-right text-[11px]">{{ formatNumber(profile.todayRequests) }}</td>
                      <td class="text-right text-[11px]">{{ formatTokens(profile.totalTokens) }}</td>
                      <td class="text-[11px]">{{ formatTs(profile.lastActiveTs) }}</td>
                      <td class="text-right">
                        <div class="flex justify-end gap-2">
                          <button class="btn btn--ghost btn--sm" @click.stop="copySingleMemberAccess(profile)">
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
                    {{ $t('platform.antigravity.apiService.selectedMemberLabel') }}
                  </h4>
                  <p class="m-0 mt-1 text-[11px] text-text-muted">
                    {{ $t('platform.antigravity.apiService.selectedMemberHint') }}
                  </p>
                </div>
                <span class="rounded-full bg-primary/10 px-2 py-1 text-[11px] font-medium text-primary">
                  {{ focusedMemberRow.displayLabel }}
                </span>
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
                        {{ focusedMemberRow.enabled ? $t('platform.antigravity.apiService.enabledKey') : $t('platform.antigravity.apiService.disabledKey') }}
                      </span>
                    </div>
                    <p class="m-0 mt-4 text-[13px] leading-6 text-text-secondary">
                      {{ focusedMemberRow.personaSummary || $t('platform.antigravity.apiService.noPersonaSummary') }}
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
                      <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.todayRequests') }}</div>
                      <div class="mt-2 text-[18px] font-semibold">{{ formatNumber(focusedMemberRow.todayRequests) }}</div>
                    </div>
                    <div class="rounded-2xl border border-border bg-bg-base/70 p-3">
                      <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.todayTokens') }}</div>
                      <div class="mt-2 text-[18px] font-semibold">{{ formatTokens(focusedMemberRow.todayTokens) }}</div>
                    </div>
                    <div class="rounded-2xl border border-border bg-bg-base/70 p-3">
                      <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.thirtyDayTokens') }}</div>
                      <div class="mt-2 text-[18px] font-semibold">{{ formatTokens(focusedMemberRow.totalTokens) }}</div>
                    </div>
                    <div class="rounded-2xl border border-border bg-bg-base/70 p-3">
                      <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.successRate') }}</div>
                      <div class="mt-2 text-[18px] font-semibold">{{ formatPercent(focusedMemberRow.successRate) }}</div>
                    </div>
                    <div class="rounded-2xl border border-border bg-bg-base/70 p-3">
                      <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.avgDuration') }}</div>
                      <div class="mt-2 text-[16px] font-semibold">
                        {{ focusedMemberRow.averageDurationMs ? formatDuration(focusedMemberRow.averageDurationMs) : '-' }}
                      </div>
                    </div>
                    <div class="rounded-2xl border border-border bg-bg-base/70 p-3">
                      <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.lastActive') }}</div>
                      <div class="mt-2 text-[13px] font-semibold">{{ formatTs(focusedMemberRow.lastActiveTs) }}</div>
                    </div>
                  </div>
                </div>

                <div class="flex flex-wrap justify-end gap-2">
                  <button class="btn btn--ghost btn--sm" @click="copySingleMemberAccess(focusedMemberRow)">
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

      <div v-else class="flex h-full flex-col gap-3 p-1">
        <div class="rounded-xl border border-border bg-bg-base/40 p-3">
          <div class="flex flex-wrap items-center gap-2">
            <button
              v-for="option in logRangeOptions"
              :key="option.value"
              class="btn btn--sm"
              :class="logRange === option.value ? 'btn--primary' : 'btn--secondary'"
              @click="selectLogRange(option.value)"
            >
              {{ option.label }}
            </button>
            <select v-model="logMemberFilter" class="input h-8 w-[140px]" @change="reloadLogs">
              <option value="">{{ $t('platform.antigravity.apiService.allMembers') }}</option>
              <option v-for="member in logMemberOptions" :key="member.value" :value="member.value">
                {{ member.label }}
              </option>
            </select>
            <select v-model="logStatusFilter" class="input h-8 w-[140px]" @change="reloadLogs">
              <option value="">{{ $t('platform.antigravity.apiService.allStatus') }}</option>
              <option value="success">success</option>
              <option value="error">error</option>
            </select>
            <input
              v-model="logModelFilter"
              class="input h-8 min-w-[180px] flex-1"
              :placeholder="$t('platform.antigravity.apiService.modelFilterPlaceholder')"
            />
          </div>
        </div>

        <div class="rounded-xl border border-border bg-bg-base/40 p-3">
          <div class="mb-3 flex flex-wrap items-start justify-between gap-2">
            <div>
              <h4 class="m-0 text-[13px] font-semibold text-text-secondary">
                {{ $t('platform.antigravity.apiService.logSummaryTitle') }}
              </h4>
              <p class="m-0 mt-1 text-[11px] text-text-muted">
                {{ $t('platform.antigravity.apiService.logSummaryHint') }}
              </p>
            </div>
          </div>

          <div class="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
            <div class="rounded-xl border border-border bg-bg-base/70 p-3">
              <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.requests') }}</div>
              <div class="mt-2 text-[18px] font-semibold">{{ formatNumber(logSummary.totalRequests) }}</div>
            </div>
            <div class="rounded-xl border border-border bg-bg-base/70 p-3">
              <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.successfulRequests') }}</div>
              <div class="mt-2 text-[18px] font-semibold text-success">{{ formatNumber(logSummary.successRequests) }}</div>
            </div>
            <div class="rounded-xl border border-border bg-bg-base/70 p-3">
              <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.failedRequests') }}</div>
              <div class="mt-2 text-[18px] font-semibold text-danger">{{ formatNumber(logSummary.errorRequests) }}</div>
            </div>
            <div class="rounded-xl border border-border bg-bg-base/70 p-3">
              <div class="text-[11px] text-text-muted">{{ $t('platform.antigravity.apiService.successRate') }}</div>
              <div class="mt-2 text-[18px] font-semibold">{{ formatPercent(logSummary.successRate) }}</div>
            </div>
          </div>
        </div>

        <div class="rounded-xl border border-border bg-bg-base/40 p-3">
          <div class="mb-3 flex flex-wrap items-start justify-between gap-2">
            <div>
              <h4 class="m-0 text-[13px] font-semibold text-text-secondary">
                {{ $t('platform.antigravity.apiService.modelUsageTitle') }}
              </h4>
              <p class="m-0 mt-1 text-[11px] text-text-muted">
                {{ $t('platform.antigravity.apiService.modelUsageHint') }}
              </p>
            </div>
            <span v-if="isLoadingModelStats" class="spinner spinner--sm"></span>
          </div>

          <div v-if="modelUsageRows.length === 0" class="rounded-lg border border-dashed border-border bg-muted/10 px-3 py-6 text-center text-[12px] text-text-muted">
            {{ $t('platform.antigravity.apiService.noData') }}
          </div>

          <div v-else class="max-h-[220px] overflow-auto rounded-lg border border-border/70 bg-muted/10">
            <table class="table table-fixed">
              <thead class="sticky top-0 z-10 bg-bg-base">
                <tr>
                  <th class="w-[34%] first:rounded-tl-lg">{{ $t('common.model') }}</th>
                  <th class="w-[12%] text-right">{{ $t('platform.antigravity.apiService.requests') }}</th>
                  <th class="w-[14%] text-right">{{ $t('platform.antigravity.apiService.inputTokensShort') }}</th>
                  <th class="w-[14%] text-right">{{ $t('platform.antigravity.apiService.outputTokensShort') }}</th>
                  <th class="w-[14%] text-right">{{ $t('platform.antigravity.apiService.totalTokensShort') }}</th>
                  <th class="w-[12%] last:rounded-tr-lg text-right">{{ $t('platform.antigravity.apiService.share') }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="row in modelUsageRows" :key="row.model">
                  <td class="truncate font-mono text-[11px]">
                    <span class="inline-block -mb-1" v-tooltip="row.model">{{ row.model }}</span>
                  </td>
                  <td class="text-right text-[11px]">{{ formatNumber(row.requests) }}</td>
                  <td class="text-right text-[11px]">{{ formatTokens(row.inputTokens) }}</td>
                  <td class="text-right text-[11px]">{{ formatTokens(row.outputTokens) }}</td>
                  <td class="text-right text-[11px] font-semibold">{{ formatTokens(row.totalTokens) }}</td>
                  <td class="text-right text-[11px]">{{ row.share.toFixed(1) }}%</td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>

        <div class="min-h-0 flex-1 overflow-auto rounded-xl border border-border bg-bg-base/40">
          <table class="table table-fixed">
            <thead class="sticky top-0 z-10 overflow-hidden rounded-t-lg bg-bg-base">
              <tr>
                <th class="w-[12%] first:rounded-tl-lg">{{ $t('platform.antigravity.apiService.time') }}</th>
                <th class="w-[22%]">{{ $t('platform.antigravity.apiService.member') }}</th>
                <th class="w-[14%]">{{ $t('platform.antigravity.apiService.account') }}</th>
                <th class="w-[14%]">{{ $t('common.model') }}</th>
                <th class="w-[8%]">{{ $t('platform.antigravity.apiService.format') }}</th>
                <th class="w-[10%] text-right">{{ $t('platform.antigravity.apiService.tokenBreakdown') }}</th>
                <th class="w-[7%]">{{ $t('platform.antigravity.apiService.status') }}</th>
                <th class="w-[7%] text-right">{{ $t('platform.antigravity.apiService.avgDuration') }}</th>
                <th class="w-[6%] last:rounded-tr-lg">{{ $t('platform.antigravity.apiService.error') }}</th>
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
                <td class="truncate text-[11px]">
                  <span class="inline-block -mb-1" v-tooltip="log.accountEmail">{{ log.accountEmail || '-' }}</span>
                </td>
                <td class="truncate font-mono text-[11px]">
                  <span class="inline-block -mb-1" v-tooltip="log.model">{{ log.model || '-' }}</span>
                </td>
                <td class="text-[11px]">{{ log.format || '-' }}</td>
                <td class="text-right text-[10px] leading-5">
                  <div>{{ $t('platform.antigravity.apiService.inputTokensShort') }} {{ formatTokens(log.inputTokens) }}</div>
                  <div>{{ $t('platform.antigravity.apiService.outputTokensShort') }} {{ formatTokens(log.outputTokens) }}</div>
                  <div class="font-semibold">{{ $t('platform.antigravity.apiService.totalTokensShort') }} {{ formatTokens(log.totalTokens) }}</div>
                </td>
                <td>
                  <span :class="['badge badge--sm', log.status === 'success' ? 'badge--success-tech' : 'badge--danger-tech']">
                    {{ log.status }}
                  </span>
                </td>
                <td class="text-right text-[11px]">
                  {{ log.requestDurationMs ? formatDuration(log.requestDurationMs) : '-' }}
                </td>
                <td class="truncate text-[11px] text-danger" v-tooltip="log.errorMessage || ''">
                  {{ log.errorMessage || '-' }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>

        <div class="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border bg-bg-base/40 px-3 py-2">
          <div class="text-[12px] text-text-muted">
            {{ $t('platform.antigravity.apiService.pageInfo', { current: currentLogPage, total: totalLogPages }) }}
            · {{ formatNumber(logPage.total) }}
          </div>
          <div class="flex items-center gap-2">
            <button class="btn btn--secondary btn--sm" :disabled="logOffset === 0" @click="prevLogPage">
              {{ $t('common.previousPage') }}
            </button>
            <button
              class="btn btn--secondary btn--sm"
              :disabled="logOffset + logLimit >= logPage.total"
              @click="nextLogPage"
            >
              {{ $t('common.nextPage') }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <AntigravityTeamMemberEditorModal
      v-if="editor.visible"
      :mode="editor.mode"
      :member="editor.member"
      :busy="isSavingMember"
      :public-base-url="accessConfig.publicServerUrl"
      :local-base-url="accessConfig.serverUrl"
      @close="closeEditor"
      @save="saveMember"
      @regenerate="regenerateMemberApiKey"
      @delete="deleteMember"
      @copy-access="copyEditorAccess"
    />
  </BaseModal>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import BaseModal from '@/components/common/BaseModal.vue'
import CodexUsageChart from '@/components/openai/CodexUsageChart.vue'
import AntigravityTeamMemberEditorModal from './AntigravityTeamMemberEditorModal.vue'
import AntigravityTokenShareChart from './AntigravityTokenShareChart.vue'
import {
  buildSelectedProfileSeries,
  buildTeamMemberRows,
  resolveFocusedProfileId,
  syncSelectedProfileIds
} from '@/utils/codexTeamUi'
import {
  buildAntigravityAccessBundle,
  buildAntigravityAnalyticsByProfileId,
  buildAntigravityLogDisplayLabel,
  buildAntigravityLogMemberOptions,
  buildAntigravityMaintenanceSummary,
  buildAntigravityModelUsageRows,
  buildAntigravityTokenShareChartData,
  formatAntigravityBytes,
  toAntigravityDeleteBeforeDateKey
} from '@/utils/antigravityApiServiceUi'

defineEmits(['close'])

const { t: $t } = useI18n()

const TEAM_MEMBER_ORDER = ['jdd', 'jqw', 'cr', 'lsb', 'will', 'cp', 'dlz', 'cw', 'xj', 'zdz']
const TEAM_ANALYTICS_LIMIT = 5000
const AUTO_REFRESH_INTERVAL_MS = 15000
const SHARED_PORT = 8766

const activeTab = ref('overview')
const isLoading = ref(false)
const isImportingTeam = ref(false)
const isSavingMember = ref(false)
const isLoadingChart = ref(false)
const isLoadingMemberAnalytics = ref(false)
const isLoadingModelStats = ref(false)
const isClearingLogs = ref(false)
const isPruningLogs = ref(false)
const accessConfig = ref({
  serverUrl: `http://127.0.0.1:${SHARED_PORT}/v1`,
  publicServerUrl: '',
  apiKey: null
})
const serviceStatus = ref({
  apiServerRunning: false,
  apiServerAddress: null,
  serverUrl: `http://127.0.0.1:${SHARED_PORT}/v1`,
  publicServerUrl: '',
  sidecarConfigured: false,
  sidecarRunning: false,
  sidecarHealthy: false,
  totalAccounts: 0,
  availableAccounts: 0
})
const gatewayProfiles = ref([])
const memberAnalyticsByProfileId = ref(new Map())
const memberAnalyticsTruncated = ref(false)
const selectedMemberProfileIds = ref([])
const focusedMemberProfileId = ref('')
const dailyStatsSeries = ref([])
const storageStatus = ref({
  totalLogs: 0,
  dbSizeBytes: 0,
  dbPath: ''
})
const allTimeStats = ref({
  requests: 0,
  tokens: 0
})
const periodStats = ref({
  todayRequests: 0,
  todayTokens: 0,
  weekRequests: 0,
  weekTokens: 0,
  monthRequests: 0,
  monthTokens: 0
})
const modelStats = ref([])
const logPage = ref({ total: 0, items: [] })
const logSummary = ref({
  totalRequests: 0,
  successRequests: 0,
  errorRequests: 0,
  totalTokens: 0,
  successRate: 0
})
const logLimit = ref(50)
const logOffset = ref(0)
const logMemberFilter = ref('')
const logModelFilter = ref('')
const logStatusFilter = ref('')
const logRange = ref('7d')
const deleteBeforeDate = ref('')
const editor = reactive({
  visible: false,
  mode: 'create',
  member: {}
})

const logRangeOptions = computed(() => [
  { value: 'today', label: $t('platform.antigravity.apiService.rangeToday') },
  { value: '7d', label: $t('platform.antigravity.apiService.range7d') },
  { value: '30d', label: $t('platform.antigravity.apiService.range30d') },
  { value: 'all', label: $t('platform.antigravity.apiService.rangeAll') }
])

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

const tokenShareChartData = computed(() =>
  buildAntigravityTokenShareChartData({
    memberRows: memberTableRows.value,
    selectedProfileIds: selectedMemberProfileIds.value
  })
)

const maintenanceSummary = computed(() =>
  buildAntigravityMaintenanceSummary({
    serviceStatus: serviceStatus.value,
    storageStatus: storageStatus.value,
    allTimeStats: allTimeStats.value,
    periodStats: periodStats.value
  })
)

const modelUsageRows = computed(() =>
  buildAntigravityModelUsageRows(modelStats.value)
)

const logMemberOptions = computed(() =>
  buildAntigravityLogMemberOptions(memberTableRows.value)
)

const selectedMemberCount = computed(() => selectedMemberProfileIds.value.length)
const allMembersSelected = computed(() =>
  memberTableRows.value.length > 0 && selectedMemberCount.value === memberTableRows.value.length
)

const memberSelectionLabel = computed(() =>
  allMembersSelected.value
    ? $t('platform.antigravity.apiService.allVisibleMembers')
    : $t('platform.antigravity.apiService.selectedMembersCount', { count: selectedMemberCount.value })
)

const teamSummaryCards = computed(() => ({
  enabledMembers: memberTableRows.value.filter(profile => profile.enabled).length,
  todayRequests: memberTableRows.value.reduce((sum, profile) => sum + Number(profile.todayRequests || 0), 0),
  todayTokens: memberTableRows.value.reduce((sum, profile) => sum + Number(profile.todayTokens || 0), 0),
  thirtyDayTokens: memberTableRows.value.reduce((sum, profile) => sum + Number(profile.totalTokens || 0), 0)
}))

const currentLogPage = computed(() =>
  logPage.value.total === 0 ? 1 : Math.floor(logOffset.value / logLimit.value) + 1
)
const totalLogPages = computed(() =>
  Math.max(1, Math.ceil(Number(logPage.value.total || 0) / Number(logLimit.value || 1)))
)

const serviceStatusSummary = computed(() => {
  const parts = [
    serviceStatus.value.apiServerRunning
      ? $t('platform.antigravity.apiService.apiServerRunning')
      : $t('platform.antigravity.apiService.apiServerStopped'),
    serviceStatus.value.sidecarConfigured
      ? $t('platform.antigravity.apiService.sidecarConfigured')
      : $t('platform.antigravity.apiService.sidecarMissing'),
    serviceStatus.value.sidecarRunning
      ? $t('platform.antigravity.apiService.sidecarRunning')
      : $t('platform.antigravity.apiService.sidecarStopped')
  ]

  return parts.join(' · ')
})

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

const formatCompactNumber = (value) => {
  const number = Number(value || 0)
  if (number < 1000) return number.toLocaleString()
  if (number < 1000000) return `${(number / 1000).toFixed(1).replace(/\.0$/, '')}K`
  if (number < 1000000000) return `${(number / 1000000).toFixed(2).replace(/\.00$/, '')}M`
  if (number < 1000000000000) return `${(number / 1000000000).toFixed(2).replace(/\.00$/, '')}B`
  return `${(number / 1000000000000).toFixed(2).replace(/\.00$/, '')}T`
}

const formatNumber = (value) => formatCompactNumber(value)
const formatTokens = (value) => formatCompactNumber(value)
const formatBytes = (value) => formatAntigravityBytes(value)

const formatTs = (ts) => {
  if (!ts) return '-'
  const date = new Date(Number(ts) * 1000)
  if (Number.isNaN(date.getTime())) return '-'
  const pad = (part) => String(part).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`
}

const formatDuration = (ms) => {
  const value = Number(ms || 0)
  if (value < 1000) return `${value}ms`
  if (value < 60000) return `${(value / 1000).toFixed(1)}s`
  if (value < 3600000) return `${(value / 60000).toFixed(1)}m`
  return `${(value / 3600000).toFixed(1)}h`
}

const formatPercent = (value) => `${Number(value || 0).toFixed(1)}%`

const copyText = async (text) => {
  const value = String(text || '').trim()
  if (!value) {
    window.$notify?.warning($t('platform.antigravity.apiService.copyEmpty'))
    return
  }

  try {
    await navigator.clipboard.writeText(value)
    window.$notify?.success($t('common.copySuccess'))
  } catch {
    window.$notify?.error($t('common.copyFailed'))
  }
}

const buildSingleMemberAccessBundle = (profile) => buildAntigravityAccessBundle({
  baseUrl: accessConfig.value.publicServerUrl,
  profiles: [profile]
})

const buildLogDisplayLabel = (log) => buildAntigravityLogDisplayLabel(log)

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

const loadServiceStatus = async () => {
  const raw = await invoke('get_antigravity_api_service_status')
  serviceStatus.value = {
    ...serviceStatus.value,
    ...toCamel(raw)
  }
}

const loadAccessConfig = async () => {
  const raw = await invoke('get_antigravity_access_config')
  accessConfig.value = {
    ...accessConfig.value,
    ...toCamel(raw)
  }
}

const loadGatewayProfiles = async () => {
  const raw = await invoke('list_antigravity_gateway_profiles')
  gatewayProfiles.value = (Array.isArray(raw) ? raw : []).map(normalizeGatewayProfile)
}

const loadStorageStatus = async () => {
  try {
    const raw = await invoke('get_antigravity_log_storage_status')
    storageStatus.value = {
      ...storageStatus.value,
      ...toCamel(raw)
    }
  } catch {
    storageStatus.value = {
      totalLogs: 0,
      dbSizeBytes: 0,
      dbPath: ''
    }
  }
}

const loadAllTimeStats = async () => {
  try {
    const raw = await invoke('get_antigravity_all_time_stats')
    allTimeStats.value = {
      ...allTimeStats.value,
      ...toCamel(raw)
    }
  } catch {
    allTimeStats.value = {
      requests: 0,
      tokens: 0
    }
  }
}

const loadPeriodStats = async () => {
  try {
    const raw = await invoke('get_antigravity_period_stats_from_storage')
    periodStats.value = {
      ...periodStats.value,
      ...toCamel(raw)
    }
  } catch {
    periodStats.value = {
      todayRequests: 0,
      todayTokens: 0,
      weekRequests: 0,
      weekTokens: 0,
      monthRequests: 0,
      monthTokens: 0
    }
  }
}

const loadDailyStats = async () => {
  isLoadingChart.value = true
  try {
    const raw = await invoke('get_antigravity_daily_stats_by_gateway_profile_from_storage', { days: 30 })
    dailyStatsSeries.value = toCamel(raw).series || []
  } catch {
    dailyStatsSeries.value = []
  } finally {
    isLoadingChart.value = false
  }
}

const loadMemberAnalytics = async () => {
  isLoadingMemberAnalytics.value = true
  try {
    const nowTs = Math.floor(Date.now() / 1000)
    const raw = await invoke('query_antigravity_logs_from_storage', {
      query: {
        limit: TEAM_ANALYTICS_LIMIT,
        offset: 0,
        startTs: nowTs - 30 * 24 * 3600,
        endTs: nowTs,
        model: null,
        format: null,
        status: null,
        accountId: null,
        memberCode: null
      }
    })
    const page = toCamel(raw)
    memberAnalyticsByProfileId.value = buildAntigravityAnalyticsByProfileId(page.items || [], { nowTs })
    memberAnalyticsTruncated.value = Number(page.total || 0) > Number((page.items || []).length)
  } catch {
    memberAnalyticsByProfileId.value = new Map()
    memberAnalyticsTruncated.value = false
  } finally {
    isLoadingMemberAnalytics.value = false
  }
}

const loadModelStats = async () => {
  isLoadingModelStats.value = true
  try {
    const range = getLogRange()
    const raw = await invoke('get_antigravity_model_stats_from_storage', {
      startTs: range.startTs ?? 0,
      endTs: range.endTs ?? Math.floor(Date.now() / 1000)
    })
    modelStats.value = toCamel(raw)
  } catch {
    modelStats.value = []
  } finally {
    isLoadingModelStats.value = false
  }
}

const loadLogSummary = async () => {
  try {
    const range = getLogRange()
    const raw = await invoke('get_antigravity_log_summary_from_storage', {
      query: {
        limit: null,
        offset: null,
        startTs: range.startTs,
        endTs: range.endTs,
        model: logModelFilter.value.trim() || null,
        format: null,
        status: logStatusFilter.value || null,
        accountId: null,
        memberCode: logMemberFilter.value || null
      }
    })
    logSummary.value = {
      ...logSummary.value,
      ...toCamel(raw)
    }
  } catch {
    logSummary.value = {
      totalRequests: 0,
      successRequests: 0,
      errorRequests: 0,
      totalTokens: 0,
      successRate: 0
    }
  }
}

const loadLogs = async () => {
  try {
    const range = getLogRange()
    const raw = await invoke('query_antigravity_logs_from_storage', {
      query: {
        limit: logLimit.value,
        offset: logOffset.value,
        startTs: range.startTs,
        endTs: range.endTs,
        model: logModelFilter.value.trim() || null,
        format: null,
        status: logStatusFilter.value || null,
        accountId: null,
        memberCode: logMemberFilter.value || null
      }
    })
    logPage.value = toCamel(raw)
  } catch {
    logPage.value = { total: 0, items: [] }
  }
}

const refreshAllData = async ({ refreshGatewayProfiles = false, silent = false } = {}) => {
  if (!silent) {
    isLoading.value = true
  }
  try {
    const tasks = [
      loadServiceStatus(),
      loadAccessConfig(),
      loadStorageStatus(),
      loadAllTimeStats(),
      loadPeriodStats()
    ]
    if (refreshGatewayProfiles) {
      tasks.push(loadGatewayProfiles())
    }
    await Promise.all(tasks)
    await Promise.all([loadDailyStats(), loadMemberAnalytics(), loadLogs(), loadModelStats(), loadLogSummary()])
  } catch (error) {
    if (!silent) {
      window.$notify?.error(
        $t('platform.antigravity.apiService.loadFailed', { error: error?.message || error })
      )
    }
  } finally {
    if (!silent) {
      isLoading.value = false
    }
  }
}

const manualRefresh = async () => {
  await refreshAllData({ refreshGatewayProfiles: true })
}

const isMemberSelected = (profileId) => selectedMemberProfileIds.value.includes(profileId)

const toggleMemberSelection = (profileId) => {
  if (!profileId) {
    return
  }
  if (isMemberSelected(profileId)) {
    selectedMemberProfileIds.value = selectedMemberProfileIds.value.filter(id => id !== profileId)
  } else {
    selectedMemberProfileIds.value = [...selectedMemberProfileIds.value, profileId]
  }
}

const clearMemberSelection = () => {
  selectedMemberProfileIds.value = []
}

const selectAllMembers = () => {
  selectedMemberProfileIds.value = memberTableRows.value.map(profile => profile.id)
}

const setFocusedMember = (profileId) => {
  focusedMemberProfileId.value = profileId
}

const reloadLogs = async () => {
  logOffset.value = 0
  await Promise.all([loadLogs(), loadLogSummary()])
}

const selectLogRange = async (value) => {
  logRange.value = value
  logOffset.value = 0
  await Promise.all([loadLogs(), loadModelStats(), loadLogSummary()])
}

const prevLogPage = async () => {
  logOffset.value = Math.max(0, logOffset.value - logLimit.value)
  await loadLogs()
}

const nextLogPage = async () => {
  if (logOffset.value + logLimit.value >= logPage.value.total) {
    return
  }
  logOffset.value += logLimit.value
  await loadLogs()
}

const openCreateMemberEditor = () => {
  editor.visible = true
  editor.mode = 'create'
  editor.member = {
    id: '',
    name: '',
    apiKey: '',
    enabled: true,
    memberCode: '',
    roleTitle: '',
    personaSummary: '',
    color: '#4c6ef5',
    notes: ''
  }
}

const openEditMemberEditor = (profile) => {
  editor.visible = true
  editor.mode = 'edit'
  editor.member = { ...profile }
}

const closeEditor = () => {
  editor.visible = false
}

const saveMember = async (payload) => {
  if (!String(payload?.name || '').trim()) {
    window.$notify?.error($t('platform.antigravity.apiService.memberNameRequired'))
    return
  }
  if (!String(payload?.memberCode || '').trim()) {
    window.$notify?.error($t('platform.antigravity.apiService.memberCodeRequired'))
    return
  }

  isSavingMember.value = true
  try {
    if (editor.mode === 'create') {
      await invoke('create_antigravity_gateway_profile', {
        name: payload.name,
        apiKey: payload.apiKey,
        enabled: payload.enabled,
        memberCode: payload.memberCode,
        roleTitle: payload.roleTitle,
        personaSummary: payload.personaSummary,
        color: payload.color,
        notes: payload.notes
      })
      window.$notify?.success($t('platform.antigravity.apiService.createKeySuccess'))
    } else {
      await invoke('update_antigravity_gateway_profile', {
        profileId: payload.id,
        name: payload.name,
        apiKey: payload.apiKey,
        enabled: payload.enabled,
        memberCode: payload.memberCode,
        roleTitle: payload.roleTitle,
        personaSummary: payload.personaSummary,
        color: payload.color,
        notes: payload.notes
      })
      window.$notify?.success($t('platform.antigravity.apiService.saveKeySuccess'))
    }

    await refreshAllData({ refreshGatewayProfiles: true })
    closeEditor()
  } catch (error) {
    const messageKey = editor.mode === 'create'
      ? 'platform.antigravity.apiService.createKeyFailed'
      : 'platform.antigravity.apiService.saveKeyFailed'
    window.$notify?.error($t(messageKey, { error: error?.message || error }))
  } finally {
    isSavingMember.value = false
  }
}

const regenerateMemberApiKey = async (payload) => {
  isSavingMember.value = true
  try {
    await invoke('regenerate_antigravity_gateway_profile_api_key', {
      profileId: payload.id
    })
    window.$notify?.success($t('platform.antigravity.apiService.generateApiKeySuccess'))
    await refreshAllData({ refreshGatewayProfiles: true })
    const updated = gatewayProfiles.value.find(profile => profile.id === payload.id)
    if (updated) {
      editor.member = { ...updated }
    }
  } catch (error) {
    window.$notify?.error(
      $t('platform.antigravity.apiService.updateKeyFailed', { error: error?.message || error })
    )
  } finally {
    isSavingMember.value = false
  }
}

const deleteMember = async (payload) => {
  const confirmed = window.confirm(
    $t('platform.antigravity.apiService.deleteKeyConfirm', {
      name: payload.name || payload.memberCode || payload.id
    })
  )
  if (!confirmed) {
    return
  }

  isSavingMember.value = true
  try {
    await invoke('delete_antigravity_gateway_profile', {
      profileId: payload.id
    })
    window.$notify?.success($t('platform.antigravity.apiService.deleteKeySuccess'))
    await refreshAllData({ refreshGatewayProfiles: true })
    closeEditor()
  } catch (error) {
    window.$notify?.error(
      $t('platform.antigravity.apiService.deleteKeyFailed', { error: error?.message || error })
    )
  } finally {
    isSavingMember.value = false
  }
}

const copyEditorAccess = async (payload) => {
  await copyText(buildSingleMemberAccessBundle(payload))
}

const copySingleMemberAccess = async (profile) => {
  await copyText(buildSingleMemberAccessBundle(profile))
}

const copyAllGatewayAccess = async () => {
  try {
    const text = await invoke('export_antigravity_access_bundle')
    await copyText(text)
  } catch (error) {
    window.$notify?.error(
      $t('platform.antigravity.apiService.exportAllMembersAccessFailed', {
        error: error?.message || error
      })
    )
  }
}

const importTeamTemplate = async () => {
  isImportingTeam.value = true
  try {
    const raw = await invoke('import_antigravity_team_template')
    gatewayProfiles.value = (Array.isArray(raw) ? raw : []).map(normalizeGatewayProfile)
    window.$notify?.success($t('platform.antigravity.apiService.importTeamSuccess'))
    await refreshAllData({ refreshGatewayProfiles: false })
  } catch (error) {
    window.$notify?.error(
      $t('platform.antigravity.apiService.importTeamFailed', { error: error?.message || error })
    )
  } finally {
    isImportingTeam.value = false
  }
}

const clearLogs = async () => {
  const confirmed = window.confirm($t('platform.antigravity.apiService.clearLogsConfirm'))
  if (!confirmed) {
    return
  }

  isClearingLogs.value = true
  try {
    await invoke('clear_antigravity_logs_in_storage')
    window.$notify?.success($t('platform.antigravity.apiService.clearLogsSuccess'))
    await refreshAllData({ refreshGatewayProfiles: false })
  } catch (error) {
    window.$notify?.error(
      $t('platform.antigravity.apiService.clearLogsFailed', { error: error?.message || error })
    )
  } finally {
    isClearingLogs.value = false
  }
}

const deleteLogsBeforeDate = async () => {
  const dateKey = toAntigravityDeleteBeforeDateKey(deleteBeforeDate.value)
  if (!dateKey) {
    window.$notify?.warning($t('platform.antigravity.apiService.deleteBeforeDateRequired'))
    return
  }

  const confirmed = window.confirm(
    $t('platform.antigravity.apiService.deleteOldLogsConfirm', { date: deleteBeforeDate.value })
  )
  if (!confirmed) {
    return
  }

  isPruningLogs.value = true
  try {
    await invoke('delete_antigravity_logs_before', { dateKey })
    window.$notify?.success(
      $t('platform.antigravity.apiService.deleteOldLogsSuccess', { date: deleteBeforeDate.value })
    )
    await refreshAllData({ refreshGatewayProfiles: false })
  } catch (error) {
    window.$notify?.error(
      $t('platform.antigravity.apiService.deleteOldLogsFailed', { error: error?.message || error })
    )
  } finally {
    isPruningLogs.value = false
  }
}

let modelFilterTimer = null
let autoRefreshTimer = null
let autoRefreshInFlight = false

const stopAutoRefresh = () => {
  if (autoRefreshTimer) {
    clearInterval(autoRefreshTimer)
    autoRefreshTimer = null
  }
}

const startAutoRefresh = () => {
  stopAutoRefresh()
  autoRefreshTimer = window.setInterval(async () => {
    if (autoRefreshInFlight || isLoading.value) {
      return
    }

    autoRefreshInFlight = true
    try {
      await refreshAllData({ refreshGatewayProfiles: false, silent: true })
    } finally {
      autoRefreshInFlight = false
    }
  }, AUTO_REFRESH_INTERVAL_MS)
}

watch(logModelFilter, () => {
  if (modelFilterTimer) {
    clearTimeout(modelFilterTimer)
  }
  modelFilterTimer = window.setTimeout(() => {
    reloadLogs()
  }, 400)
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

onMounted(async () => {
  await refreshAllData({ refreshGatewayProfiles: true })
  startAutoRefresh()
})

onBeforeUnmount(() => {
  if (modelFilterTimer) {
    clearTimeout(modelFilterTimer)
    modelFilterTimer = null
  }
  stopAutoRefresh()
})
</script>
