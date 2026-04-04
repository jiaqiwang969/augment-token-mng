import re

anti_path = "src/components/antigravity/AntigravityServerDialog.vue"

with open(anti_path, 'r', encoding='utf-8') as f:
    anti_content = f.read()

# Replace the existing selectLogRange if any
anti_content = re.sub(
    r'const selectLogRange = async \(value\) => \{.*?\}',
    '',
    anti_content,
    flags=re.DOTALL
)

methods_to_add = """
const logStatusOptions = [
  { value: '', label: $t('platform.antigravity.apiService.allStatus') },
  { value: 'success', label: 'success' },
  { value: 'error', label: 'error' }
]

const getLogRangeLabel = (value) => {
  const option = logRangeOptions.value.find(r => r.value === value)
  return option?.label || value
}

const getLogStatusLabel = (value) => {
  const option = logStatusOptions.find(s => s.value === value)
  return option?.label || value
}

const getLogMemberLabel = () => {
  if (!logMemberFilter.value) {
    return $t('platform.antigravity.apiService.allMembers')
  }
  const member = logMemberOptions.value.find(option => option.value === logMemberFilter.value)
  return member?.label || logMemberFilter.value
}

const selectLogRange = async (value, close) => {
  logRange.value = value
  if (close) close()
  await reloadLogs()
}

const selectLogStatus = async (value, close) => {
  logStatusFilter.value = value
  if (close) close()
  await reloadLogs()
}

const selectLogMember = async (value, close) => {
  logMemberFilter.value = value
  if (close) close()
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
"""

# Find a good place to insert these methods, like before `const loadDailyStats = async () => {`
# Or before `const loadLogs = async () => {`
if 'const loadLogs = async () => {' in anti_content:
    anti_content = anti_content.replace('const loadLogs = async () => {', methods_to_add + '\nconst loadLogs = async () => {')
else:
    print("Could not find loadLogs")

with open(anti_path, 'w', encoding='utf-8') as f:
    f.write(anti_content)
