anti_path = "src/components/antigravity/AntigravityServerDialog.vue"

with open(anti_path, 'r', encoding='utf-8') as f:
    anti_content = f.read()

import_stmt = "import FloatingDropdown from '@/components/common/FloatingDropdown.vue'\n"
anti_content = anti_content.replace(
    "import BaseModal from '@/components/common/BaseModal.vue'",
    "import BaseModal from '@/components/common/BaseModal.vue'\n" + import_stmt
)

with open(anti_path, 'w', encoding='utf-8') as f:
    f.write(anti_content)
