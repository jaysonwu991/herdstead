#!/usr/bin/env bash
# Sets the workspace version from a release tag.
#
# The repository keeps one version, `version` in `[workspace.package]` of the
# root `Cargo.toml`, and the binary reports it (`herdstead --version`). Release
# CI calls this with the pushed tag so a built binary names the tag it came
# from; the caller refreshes `Cargo.lock` afterwards (`cargo update
# --workspace`).
#
# A leading `v` is optional: `v1.2.3` and `1.2.3` both set 1.2.3.
#
# Usage: scripts/set-version.sh v1.2.3

set -euo pipefail

raw="${1:-}"
if [ -z "$raw" ]; then
  echo "usage: $(basename "$0") <version|tag>" >&2
  exit 2
fi

version="${raw#v}"
if ! printf '%s' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$'; then
  echo "error: '$raw' is not a semver version (expected like v1.2.3)" >&2
  exit 2
fi

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
manifest="${root}/Cargo.toml"

# Replace the version only inside `[workspace.package]`: a crate's own
# `version.workspace = true` is not a version, and a dependency line would be
# the wrong target.
staged="$(mktemp)"
trap 'rm -f "${staged}"' EXIT

if ! awk -v version="${version}" '
  /^\[/ { section = $0 }
  section == "[workspace.package]" && /^version[[:space:]]*=/ {
    sub(/"[^"]*"/, "\"" version "\"")
    found = 1
  }
  { print }
  END { exit(found ? 0 : 1) }
' "${manifest}" > "${staged}"; then
  echo "error: could not find the version in [workspace.package] of Cargo.toml" >&2
  exit 1
fi

mv "${staged}" "${manifest}"
echo "set version to ${version}"
