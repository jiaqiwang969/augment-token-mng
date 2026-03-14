const normalizeMemberCode = (value) => String(value || '').trim().toLowerCase()

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
