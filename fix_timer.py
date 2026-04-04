anti_path = "src/components/antigravity/AntigravityServerDialog.vue"

with open(anti_path, 'r', encoding='utf-8') as f:
    anti_content = f.read()

# I added this:
to_remove = """// 模型筛选防抖
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

anti_content = anti_content.replace(to_remove, "")

with open(anti_path, 'w', encoding='utf-8') as f:
    f.write(anti_content)
