anti_path = "src/components/antigravity/AntigravityServerDialog.vue"

with open(anti_path, 'r', encoding='utf-8') as f:
    anti_content = f.read()

anti_content = anti_content.replace(
    "    <AntigravityTeamMemberEditorModal",
    "    </div>\n\n    <AntigravityTeamMemberEditorModal"
)

with open(anti_path, 'w', encoding='utf-8') as f:
    f.write(anti_content)
