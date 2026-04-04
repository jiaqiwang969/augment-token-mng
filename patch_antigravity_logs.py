import re

codex_path = "src/components/openai/CodexServerDialog.vue"
anti_path = "src/components/antigravity/AntigravityServerDialog.vue"

with open(codex_path, 'r', encoding='utf-8') as f:
    codex_content = f.read()

# Extract the v-else block from CodexServerDialog.vue
codex_match = re.search(r'(<div v-else class="flex h-full flex-col gap-2 p-1">.*?</div>\s*</div>\s*</div>)', codex_content, re.DOTALL)
if not codex_match:
    print("Could not find v-else block in codex")
    exit(1)

codex_block = codex_match.group(1)

# Replace the i18n paths
# platform.openai.codexDialog -> platform.antigravity.apiService
anti_block = codex_block.replace("platform.openai.codexDialog.", "platform.antigravity.apiService.")

# Some specific replacements
anti_block = anti_block.replace("getLogAccountLabel()", "getLogAccountLabel()") 
anti_block = anti_block.replace("selectLogAccount(account.id, close)", "selectLogAccount(account.id, close)")
anti_block = anti_block.replace("availableAccounts", "availableAccounts")

# In Antigravity, we might not have availableAccounts! Let's check if it exists in AntigravityServerDialog.vue.
# Wait, let's just use the exact text replacements.

with open(anti_path, 'r', encoding='utf-8') as f:
    anti_content = f.read()

# Find the v-else block in AntigravityServerDialog.vue
anti_match = re.search(r'(<div v-else class="flex h-full flex-col gap-3 p-1">.*?)<AntigravityTeamMemberEditorModal', anti_content, re.DOTALL)
if not anti_match:
    print("Could not find v-else block in antigravity")
    exit(1)

anti_old_block = anti_match.group(1)

# Before replacing, let's check what variables we need to provide in script setup for AntigravityServerDialog.vue
# getLogRangeLabel, getLogAccountLabel, selectLogAccount, etc.
# Actually, I should just use `patch` or generate the file explicitly to have full control.
