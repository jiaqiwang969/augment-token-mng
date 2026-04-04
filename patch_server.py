server_path = "src-tauri/src/platforms/antigravity/api_service/server.rs"

with open(server_path, 'r', encoding='utf-8') as f:
    content = f.read()

import re

# Need to update build_request_log to include the actual duration and usage stats if they were missing or not correctly assigned
# Wait, the frontend is showing 0ms and 0 tokens and no errorMessage. But the SQLite query SHOWED that tokens are recorded (315, 5, 320), status is 'success', and request_duration_ms is NULL!
# Let me look at the SQLite query again.
# 315|5|320||success  -> request_duration_ms is empty (NULL).
# Where does it get duration?
