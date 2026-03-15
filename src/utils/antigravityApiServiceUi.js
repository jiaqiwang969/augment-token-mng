const normalizeMemberCode = (value) => String(value || '').trim().toLowerCase()
const normalizeProfileId = (value) => String(value || '').trim()

export const buildAntigravityAccessBundle = ({ baseUrl, profiles }) => {
  const publicBaseUrl = String(baseUrl || '').trim()
  const items = Array.isArray(profiles) ? profiles : []

  return items
    .filter(profile => String(profile?.apiKey || '').trim())
    .map((profile) => {
      const labelParts = [profile?.name, profile?.memberCode]
        .map(value => String(value || '').trim())
        .filter(Boolean)

      return [
        `# ${labelParts.join(' · ') || profile?.id || 'Member'}`,
        `ANTIGRAVITY_BASE_URL=${publicBaseUrl}`,
        `ANTIGRAVITY_API_KEY=${String(profile.apiKey || '').trim()}`
      ].join('\n')
    })
    .join('\n\n')
}

export const formatAntigravityBytes = (value) => {
  const bytes = Math.max(0, Number(value || 0))
  if (bytes < 1024) {
    return `${bytes} B`
  }

  const units = ['KB', 'MB', 'GB', 'TB']
  let size = bytes
  let unitIndex = -1

  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024
    unitIndex += 1
  }

  const rounded = size >= 10 ? Math.round(size * 10) / 10 : Math.round(size * 100) / 100
  return `${String(rounded).replace(/\.0$/, '')} ${units[unitIndex]}`
}

export const formatAntigravityMemberShortLabel = (entry) => {
  const memberCode = normalizeMemberCode(entry?.memberCode)
  if (memberCode) {
    return memberCode
  }

  const profileName = String(entry?.profileName || entry?.name || entry?.displayLabel || entry?.id || '')
    .trim()
  if (!profileName) {
    return 'member'
  }

  return profileName
    .split(/[\s·/]+/)
    .map(part => part[0] || '')
    .join('')
    .toLowerCase()
    .slice(0, 4) || 'member'
}

export const buildAntigravityLogDisplayLabel = (log) => {
  const rawLabel = String(log?.displayLabel || '').trim()
  if (rawLabel) {
    return rawLabel
  }

  const parts = [log?.gatewayProfileName, log?.memberCode, log?.roleTitle]
    .map(value => String(value || '').trim())
    .filter(Boolean)

  return parts.length > 0 ? parts.join(' · ') : '-'
}

const getTodayStartTs = (nowTs) => {
  const base = new Date(Number(nowTs || Date.now() / 1000) * 1000)
  base.setHours(0, 0, 0, 0)
  return Math.floor(base.getTime() / 1000)
}

export const buildAntigravityAnalyticsByProfileId = (logs, { nowTs } = {}) => {
  const grouped = new Map()
  const todayStartTs = getTodayStartTs(nowTs)

  for (const log of Array.isArray(logs) ? logs : []) {
    const profileId =
      normalizeProfileId(log?.gatewayProfileId) ||
      normalizeMemberCode(log?.memberCode) ||
      `legacy:${buildAntigravityLogDisplayLabel(log)}`
    const existing = grouped.get(profileId) || {
      profileId,
      profileName: String(log?.gatewayProfileName || log?.displayLabel || 'Legacy').trim() || 'Legacy',
      memberCode: normalizeMemberCode(log?.memberCode),
      roleTitle: String(log?.roleTitle || '').trim(),
      displayLabel: buildAntigravityLogDisplayLabel(log),
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

    if (String(log?.status || '').trim() === 'success') {
      existing.successCount += 1
    }

    const durationMs = Number(log?.requestDurationMs || 0)
    if (Number.isFinite(durationMs) && durationMs > 0) {
      existing.durationSum += durationMs
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

  return new Map(
    [...grouped.entries()].map(([profileId, entry]) => [
      profileId,
      {
        ...entry,
        successRate: entry.requests > 0 ? (entry.successCount / entry.requests) * 100 : 0,
        averageDurationMs:
          entry.durationCount > 0 ? Math.round(entry.durationSum / entry.durationCount) : null
      }
    ])
  )
}

export const buildAntigravityTokenShareChartData = ({
  memberRows,
  selectedProfileIds
} = {}) => {
  const hasExplicitSelection = Array.isArray(selectedProfileIds)
  const selectedSet = new Set(
    hasExplicitSelection
      ? selectedProfileIds.map(normalizeProfileId).filter(Boolean)
      : []
  )

  const filteredRows = (Array.isArray(memberRows) ? memberRows : [])
    .filter(row => {
      if (!hasExplicitSelection) {
        return true
      }
      return selectedSet.has(normalizeProfileId(row?.id))
    })
    .filter(row => Number(row?.totalTokens || 0) > 0)

  const labels = filteredRows.map(row => formatAntigravityMemberShortLabel(row))
  const data = filteredRows.map(row => Number(row?.totalTokens || 0))
  const backgroundColor = filteredRows.map(
    row => String(row?.rowColor || row?.color || '#4c6ef5').trim() || '#4c6ef5'
  )

  return {
    labels,
    datasets: [
      {
        data,
        backgroundColor,
        borderWidth: 2
      }
    ],
    totalTokens: data.reduce((sum, value) => sum + value, 0),
    items: filteredRows.map((row, index) => ({
      id: normalizeProfileId(row?.id),
      label: labels[index],
      value: data[index],
      color: backgroundColor[index]
    }))
  }
}

export const buildAntigravityLogMemberOptions = (memberRows) =>
  (Array.isArray(memberRows) ? memberRows : [])
    .filter(row => normalizeMemberCode(row?.memberCode))
    .reduce((options, row) => {
      const value = normalizeMemberCode(row?.memberCode)
      if (!options.some(option => option.value === value)) {
        options.push({
          value,
          label: formatAntigravityMemberShortLabel(row)
        })
      }
      return options
    }, [])
    .sort((left, right) => left.label.localeCompare(right.label, 'en'))

export const buildAntigravityMaintenanceSummary = ({
  serviceStatus,
  storageStatus,
  allTimeStats,
  periodStats
} = {}) => {
  const totalAccounts = Number(serviceStatus?.totalAccounts || 0)
  const availableAccounts = Number(serviceStatus?.availableAccounts || 0)

  return {
    totalAccounts,
    availableAccounts,
    accountCoverageLabel: `${availableAccounts} / ${totalAccounts}`,
    totalLogs: Number(storageStatus?.totalLogs || 0),
    dbSizeBytes: Number(storageStatus?.dbSizeBytes || 0),
    allTimeRequests: Number(allTimeStats?.requests || 0),
    allTimeTokens: Number(allTimeStats?.tokens || 0),
    todayRequests: Number(periodStats?.todayRequests || 0),
    todayTokens: Number(periodStats?.todayTokens || 0),
    weekRequests: Number(periodStats?.weekRequests || 0),
    weekTokens: Number(periodStats?.weekTokens || 0),
    monthRequests: Number(periodStats?.monthRequests || 0),
    monthTokens: Number(periodStats?.monthTokens || 0)
  }
}

export const buildAntigravityModelUsageRows = (stats) => {
  const rows = (Array.isArray(stats) ? stats : [])
    .map((entry) => ({
      model: String(entry?.model || '').trim(),
      requests: Number(entry?.requests || 0),
      inputTokens: Number(entry?.inputTokens || 0),
      outputTokens: Number(entry?.outputTokens || 0),
      totalTokens: Number(entry?.totalTokens || 0)
    }))
    .filter(entry => entry.model)
    .sort((left, right) => {
      if (right.totalTokens !== left.totalTokens) {
        return right.totalTokens - left.totalTokens
      }
      if (right.requests !== left.requests) {
        return right.requests - left.requests
      }
      return left.model.localeCompare(right.model, 'en')
    })

  const totalTokens = rows.reduce((sum, entry) => sum + entry.totalTokens, 0)

  return rows.map((entry) => ({
    ...entry,
    share: totalTokens > 0 ? Number(((entry.totalTokens / totalTokens) * 100).toFixed(1)) : 0
  }))
}

export const toAntigravityDeleteBeforeDateKey = (value) => {
  const trimmed = String(value || '').trim()
  if (!/^\d{4}-\d{2}-\d{2}$/.test(trimmed)) {
    return null
  }

  const dateKey = Number(trimmed.replaceAll('-', ''))
  return Number.isFinite(dateKey) ? dateKey : null
}
