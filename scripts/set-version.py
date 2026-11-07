#!/usr/bin/env python3
"""Keep the bundled runtime, native app and npm package on one release version."""
import json
from pathlib import Path
import re
import sys

version = sys.argv[1].removeprefix("v") if len(sys.argv) == 2 else ""
if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?", version):
    sys.exit("usage: scripts/set-version.sh v1.2.3 (optional prerelease suffix)")
root = Path(__file__).resolve().parent.parent
for name, section in [("Cargo.toml", "workspace.package"), ("desktop/src-tauri/Cargo.toml", "package")]:
    path = root / name
    text = path.read_text()
    pattern = r'(\[' + re.escape(section) + r'\][^\[]*?^version\s*=\s*)"[^"]+"'
    text, count = re.subn(pattern, lambda match: match[1] + '"' + version + '"', text, count=1, flags=re.M)
    if count != 1:
        sys.exit(f"missing version in {name}")
    path.write_text(text)
for name in ["desktop/package.json", "desktop/package-lock.json", "desktop/src-tauri/tauri.conf.json"]:
    path = root / name
    data = json.loads(path.read_text())
    data["version"] = version
    if "packages" in data:
        data["packages"][""]["version"] = version
    path.write_text(json.dumps(data, indent=2) + "\n")
print(f"set desktop and runtime version to {version}")
