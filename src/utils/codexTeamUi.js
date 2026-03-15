const normalizeMemberCode = (value) => String(value || '').trim().toLowerCase()
const normalizeProfileId = (value) => String(value || '').trim()
const DEFAULT_TEAM_MEMBER_ORDER = ['jdd', 'jqw', 'cr', 'lsb', 'will', 'cp', 'dlz', 'cw', 'xj', 'zdz']
const DAY_IN_MS = 24 * 60 * 60 * 1000

const buildVisibleMemberSet = (visibleMemberCodes) => {
  const normalized = Array.isArray(visibleMemberCodes)
    ? visibleMemberCodes.map(normalizeMemberCode).filter(Boolean)
    : []

  return new Set(normalized)
}

const shouldKeepMember = (memberCode, visibleMemberCodes) => {
  const visibleSet = buildVisibleMemberSet(visibleMemberCodes)
  if (visibleSet.size === 0) {
    return true
  }
  return visibleSet.has(normalizeMemberCode(memberCode))
}

export const filterTeamSeriesByVisibleMembers = (series, visibleMemberCodes) => {
  if (!Array.isArray(series)) {
    return []
  }

  return series.filter(entry => shouldKeepMember(entry?.memberCode, visibleMemberCodes))
}

export const filterMemberRankingByVisibleMembers = (rows, visibleMemberCodes) => {
  if (!Array.isArray(rows)) {
    return []
  }

  return rows.filter(entry => shouldKeepMember(entry?.memberCode, visibleMemberCodes))
}

const toProfileIds = (items) =>
  Array.isArray(items)
    ? items
        .map((item) => (typeof item === 'string' ? item : item?.id))
        .map(normalizeProfileId)
        .filter(Boolean)
    : []

export const resolveFocusedProfileId = ({
  profiles,
  focusedProfileId
} = {}) => {
  const currentIds = toProfileIds(profiles)
  if (currentIds.length === 0) {
    return ''
  }

  const normalizedFocusedId = normalizeProfileId(focusedProfileId)
  return currentIds.includes(normalizedFocusedId) ? normalizedFocusedId : currentIds[0]
}

const buildTrailingDateRange = ({ fallbackDays = 30, endDate = new Date() } = {}) => {
  const totalDays = Math.max(1, Number(fallbackDays) || 30)
  const lastDate = new Date(endDate)
  const lastDateUtc = new Date(Date.UTC(
    lastDate.getUTCFullYear(),
    lastDate.getUTCMonth(),
    lastDate.getUTCDate()
  ))
  const dates = []

  for (let offset = totalDays - 1; offset >= 0; offset -= 1) {
    const current = new Date(lastDateUtc.getTime() - offset * DAY_IN_MS)
    dates.push(current.toISOString().slice(0, 10))
  }

  return dates
}

const collectSeriesDates = (series) => {
  const dateSet = new Set()

  if (!Array.isArray(series)) {
    return []
  }

  series.forEach((entry) => {
    if (!Array.isArray(entry?.stats)) {
      return
    }
    entry.stats.forEach((point) => {
      const date = String(point?.date || '').trim()
      if (date) {
        dateSet.add(date)
      }
    })
  })

  return [...dateSet].sort((left, right) => left.localeCompare(right))
}

export const syncSelectedProfileIds = ({
  profiles,
  selectedProfileIds,
  previousProfiles,
  previousProfileIds
} = {}) => {
  const currentIds = toProfileIds(profiles)
  if (currentIds.length === 0) {
    return []
  }

  const nextValidSet = new Set(currentIds)
  const currentSelectedIds = toProfileIds(selectedProfileIds)
  const preservedIds = currentSelectedIds.filter(id => nextValidSet.has(id))
  const priorIds = previousProfileIds?.length ? toProfileIds(previousProfileIds) : toProfileIds(previousProfiles)

  if (currentSelectedIds.length === 0) {
    return priorIds.length === 0 ? currentIds : []
  }

  const wasAllSelected = priorIds.length > 0 && priorIds.every(id => currentSelectedIds.includes(id))
  if (wasAllSelected) {
    return currentIds
  }

  return preservedIds
}

export const buildSelectedProfileSeries = ({
  series,
  selectedProfileIds,
  profiles,
  fallbackDays = 30,
  endDate
} = {}) => {
  const selectedIds = toProfileIds(selectedProfileIds)
  if (selectedIds.length === 0) {
    return []
  }

  const sourceSeries = Array.isArray(series) ? series : []
  const sourceMap = new Map(
    sourceSeries
      .map(entry => [normalizeProfileId(entry?.profileId), entry])
      .filter(([profileId]) => profileId)
  )
  const profileMap = new Map(
    (Array.isArray(profiles) ? profiles : [])
      .map(profile => [normalizeProfileId(profile?.id), profile])
      .filter(([profileId]) => profileId)
  )
  const dates = collectSeriesDates(sourceSeries)
  const resolvedDates = dates.length > 0
    ? dates
    : buildTrailingDateRange({ fallbackDays, endDate })

  return selectedIds.map((profileId) => {
    const sourceEntry = sourceMap.get(profileId)
    const profile = profileMap.get(profileId)
    const perDate = new Map()

    if (Array.isArray(sourceEntry?.stats)) {
      sourceEntry.stats.forEach((point) => {
        const date = String(point?.date || '').trim()
        if (!date) {
          return
        }
        perDate.set(date, {
          requests: Number(point?.requests || 0),
          tokens: Number(point?.tokens || 0)
        })
      })
    }

    return {
      profileId,
      profileName: String(sourceEntry?.profileName || sourceEntry?.name || profile?.name || profileId).trim() || profileId,
      memberCode: String(sourceEntry?.memberCode || profile?.memberCode || '').trim(),
      roleTitle: String(sourceEntry?.roleTitle || profile?.roleTitle || '').trim(),
      color: String(sourceEntry?.color || profile?.rowColor || profile?.color || '').trim(),
      stats: resolvedDates.map((date) => {
        const point = perDate.get(date)
        return {
          date,
          requests: point?.requests || 0,
          tokens: point?.tokens || 0
        }
      })
    }
  })
}

export const filterTeamSeriesBySelectedProfiles = (series, selectedProfileIds, profiles) =>
  buildSelectedProfileSeries({
    series,
    selectedProfileIds,
    profiles
  })

export const buildAllMembersAccessBundle = ({ baseUrl, profiles }) => {
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
        `OPENAI_BASE_URL=${publicBaseUrl}`,
        `OPENAI_API_KEY=${String(profile.apiKey || '').trim()}`
      ].join('\n')
    })
    .join('\n\n')
}

const extractKeySuffix = (apiKey) => {
  const trimmed = String(apiKey || '').trim()
  if (!trimmed) {
    return ''
  }

  const segments = trimmed.split('-').filter(Boolean)
  return segments[segments.length - 1] || trimmed.slice(-8)
}

const buildProfileDisplayLabel = (profile) => {
  const parts = [profile?.name, profile?.memberCode, profile?.roleTitle]
    .map(value => String(value || '').trim())
    .filter(Boolean)

  return parts.length > 0 ? parts.join(' · ') : profile?.id || 'Profile'
}

const sortProfilesForTable = (left, right, teamMemberOrder) => {
  const leftCode = normalizeMemberCode(left?.memberCode)
  const rightCode = normalizeMemberCode(right?.memberCode)
  const leftIndex = teamMemberOrder.indexOf(leftCode)
  const rightIndex = teamMemberOrder.indexOf(rightCode)
  const leftOrder = leftIndex >= 0 ? leftIndex : Number.MAX_SAFE_INTEGER
  const rightOrder = rightIndex >= 0 ? rightIndex : Number.MAX_SAFE_INTEGER

  if (leftOrder !== rightOrder) {
    return leftOrder - rightOrder
  }

  return String(left?.name || left?.id || '').localeCompare(String(right?.name || right?.id || ''), 'zh-CN')
}

export const buildTeamMemberRows = ({
  profiles,
  analyticsByProfileId,
  teamMemberOrder = DEFAULT_TEAM_MEMBER_ORDER
}) => {
  if (!Array.isArray(profiles)) {
    return []
  }

  const analyticsMap = analyticsByProfileId instanceof Map ? analyticsByProfileId : new Map()
  const normalizedOrder = Array.isArray(teamMemberOrder)
    ? teamMemberOrder.map(normalizeMemberCode).filter(Boolean)
    : DEFAULT_TEAM_MEMBER_ORDER

  return profiles
    .slice()
    .sort((left, right) => sortProfilesForTable(left, right, normalizedOrder))
    .map((profile) => {
      const analytics = analyticsMap.get(profile?.id) || null
      const memberCode = String(profile?.memberCode || '').trim()

      return {
        ...profile,
        memberCode,
        displayLabel: buildProfileDisplayLabel(profile),
        keySuffix: extractKeySuffix(profile?.apiKey),
        todayRequests: analytics?.todayRequests || 0,
        todayTokens: analytics?.todayTokens || 0,
        totalRequests: analytics?.requests || 0,
        totalTokens: analytics?.totalTokens || 0,
        successRate: analytics?.successRate || 0,
        averageDurationMs: analytics?.averageDurationMs ?? null,
        lastActiveTs: analytics?.lastActiveTs || null,
        rowColor: String(profile?.color || analytics?.color || '#4c6ef5').trim() || '#4c6ef5',
        isBuiltinMember: normalizedOrder.includes(normalizeMemberCode(memberCode))
      }
    })
}
