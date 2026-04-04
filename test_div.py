import re
with open("patch_antigravity.py", "r") as f:
    text = f.read()

v_else = text.split('new_v_else = """')[1].split('"""')[0]

open_tags = len(re.findall(r'<div\b', v_else))
close_tags = len(re.findall(r'</div\b', v_else))
print(f"Open: {open_tags}, Close: {close_tags}")
