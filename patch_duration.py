import re

server_path = "src-tauri/src/platforms/antigravity/api_service/server.rs"

with open(server_path, 'r', encoding='utf-8') as f:
    content = f.read()

# Add a start_time to the handle function
# First, let's check where the handle is.
# Wait, there are multiple handles maybe? handle_antigravity_request etc?
