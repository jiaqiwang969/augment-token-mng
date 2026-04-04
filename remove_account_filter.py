import re

anti_path = "src/components/antigravity/AntigravityServerDialog.vue"

with open(anti_path, 'r', encoding='utf-8') as f:
    anti_content = f.read()

# Remove the Account Filter dropdown block
anti_content = re.sub(
    r'<!-- 账号筛选 -->\s*<FloatingDropdown placement="bottom-end".*?</FloatingDropdown>\s*',
    '',
    anti_content,
    flags=re.DOTALL
)

with open(anti_path, 'w', encoding='utf-8') as f:
    f.write(anti_content)
