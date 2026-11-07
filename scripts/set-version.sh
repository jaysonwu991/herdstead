#!/usr/bin/env bash
# Stamp every desktop and runtime manifest from the release tag.
set -euo pipefail
cd "$(dirname "$0")/.."
python3 scripts/set-version.py "${1:-}"
