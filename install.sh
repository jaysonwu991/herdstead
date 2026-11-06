#!/bin/sh
# Install herdstead from a GitHub release.
#
#   curl -fsSL https://raw.githubusercontent.com/jaysonwu991/herdstead/main/install.sh | sh
#
# Downloads the archive for this machine, checks it against the `.sha256`
# published beside it, and installs the binary without `sudo`.
#
# Environment overrides:
#   HERDSTEAD_VERSION      release to install, with or without the leading "v"
#                          (default: the latest release)
#   HERDSTEAD_INSTALL_DIR  directory to install the binary into
#                          (default: $HOME/.local/bin)
#   HERDSTEAD_REPO         GitHub repository slug (default: jaysonwu991/herdstead)
#
# `sh install.sh --help` prints these overrides too.
set -eu

REPO="${HERDSTEAD_REPO:-jaysonwu991/herdstead}"
VERSION="${HERDSTEAD_VERSION:-}"
BASE_URL="https://github.com/${REPO}"

# The archives carry the platform and no version, so one URL per platform is
# permanent: `releases/latest/download` keeps pointing at the current build.
PLATFORMS="macos-arm64, macos-x64, linux-x64, linux-arm64"

err() {
    printf 'herdstead-install: error: %s\n' "$*" >&2
    exit 1
}

warn() {
    printf 'herdstead-install: warning: %s\n' "$*" >&2
}

info() {
    printf 'herdstead-install: %s\n' "$*" >&2
}

usage() {
    cat <<'EOF'
Install herdstead from a GitHub release.

  curl -fsSL https://raw.githubusercontent.com/jaysonwu991/herdstead/main/install.sh | sh
  sh install.sh --help        # this text

Environment:
  HERDSTEAD_VERSION      release to install, with or without the leading "v"
                         (default: the latest release)
  HERDSTEAD_INSTALL_DIR  directory to install the binary into
                         (default: $HOME/.local/bin)
  HERDSTEAD_REPO         GitHub repository slug
                         (default: jaysonwu991/herdstead)
EOF
}

download() {
    if command -v curl >/dev/null 2>&1; then
        curl --fail --silent --show-error --location "$1" --output "$2"
    elif command -v wget >/dev/null 2>&1; then
        wget --quiet --output-document="$2" "$1"
    else
        err "curl or wget is required"
    fi
}

# `uname` names the machine the way the archives do, except for the two places
# they disagree: Darwin/Linux and arm64/x86_64.
detect_platform() {
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Darwin) os_part="macos" ;;
        Linux) os_part="linux" ;;
        *) err "unsupported operating system: ${os}; herdstead runs on macOS and Linux" ;;
    esac

    case "$arch" in
        arm64 | aarch64) arch_part="arm64" ;;
        x86_64 | amd64) arch_part="x64" ;;
        *) err "unsupported architecture: ${arch}; supported: ${PLATFORMS}" ;;
    esac

    if [ "$os_part" = "linux" ] && ldd --version 2>&1 | grep -qi musl; then
        err "this is a musl system; the release binaries are built against glibc. \
Build from source instead: cargo install --git ${BASE_URL}.git herdstead"
    fi

    printf '%s-%s' "$os_part" "$arch_part"
}

archive_url() {
    if [ -n "$VERSION" ]; then
        ver="${VERSION#v}"
        printf '%s/releases/download/v%s/herdstead-%s.tar.gz' "$BASE_URL" "$ver" "$1"
    else
        printf '%s/releases/latest/download/herdstead-%s.tar.gz' "$BASE_URL" "$1"
    fi
}

verify_checksum() {
    file="$1"
    sums="$2"
    expected="$(cut -d' ' -f1 < "$sums" | head -n1)"
    if [ -z "$expected" ]; then
        warn "checksum file is empty; skipping verification"
        return 0
    fi

    if command -v sha256sum >/dev/null 2>&1; then
        actual="$(sha256sum "$file" | cut -d' ' -f1)"
    elif command -v shasum >/dev/null 2>&1; then
        actual="$(shasum -a 256 "$file" | cut -d' ' -f1)"
    elif command -v openssl >/dev/null 2>&1; then
        actual="$(openssl dgst -sha256 "$file" | sed 's/^.*= //')"
    else
        warn "no sha256 tool found; skipping checksum verification"
        return 0
    fi

    [ "$expected" = "$actual" ] || err "checksum verification failed for ${file##*/}"
}

main() {
    case "${1:-}" in
        -h | --help)
            usage
            exit 0
            ;;
        "") ;;
        *) err "unknown argument: $1 (try --help)" ;;
    esac

    command -v uname >/dev/null 2>&1 || err "uname is required"
    command -v tar >/dev/null 2>&1 || err "tar is required"
    command -v mktemp >/dev/null 2>&1 || err "mktemp is required"
    command -v curl >/dev/null 2>&1 || command -v wget >/dev/null 2>&1 \
        || err "curl or wget is required"

    [ -n "${HERDSTEAD_INSTALL_DIR:-}" ] || [ -n "${HOME:-}" ] \
        || err "HOME is not set; set HERDSTEAD_INSTALL_DIR to choose a folder"
    install_dir="${HERDSTEAD_INSTALL_DIR:-${HOME:-}/.local/bin}"

    platform="$(detect_platform)"
    url="$(archive_url "$platform")"
    archive="${url##*/}"

    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT INT TERM

    info "downloading ${archive}"
    download "$url" "${tmp}/${archive}" \
        || err "could not download ${url}. \
Check that a release exists with an archive for ${platform} (supported: ${PLATFORMS})"

    # The checksum is published by the same release; a missing one is worth a
    # warning, a mismatched one is fatal.
    if download "${url}.sha256" "${tmp}/${archive}.sha256" 2>/dev/null; then
        verify_checksum "${tmp}/${archive}" "${tmp}/${archive}.sha256"
    else
        warn "checksum file unavailable; skipping verification"
    fi

    tar -xzf "${tmp}/${archive}" -C "$tmp" || err "could not unpack ${archive}"
    [ -f "${tmp}/herdstead" ] || err "${archive} did not contain a herdstead binary"
    chmod 0755 "${tmp}/herdstead"

    # Run the new binary before it replaces anything: a glibc older than the one
    # the release was built against fails here rather than after the swap, and
    # the version comes from the binary itself, not from what was asked for.
    if ! installed="$("${tmp}/herdstead" --version 2>/dev/null)"; then
        err "${archive} does not run on this machine. \
Build from source instead: cargo install --git ${BASE_URL}.git herdstead"
    fi

    mkdir -p "$install_dir" 2>/dev/null \
        || err "cannot create ${install_dir}; set HERDSTEAD_INSTALL_DIR to a folder you own"
    mv -f "${tmp}/herdstead" "${install_dir}/herdstead" 2>/dev/null \
        || err "cannot write to ${install_dir}; set HERDSTEAD_INSTALL_DIR to a folder you own"
    chmod 0755 "${install_dir}/herdstead"

    info "installed ${installed} to ${install_dir}/herdstead"

    case ":${PATH:-}:" in
        *":${install_dir}:"*) ;;
        *)
            warn "${install_dir} is not on your PATH"
            printf 'herdstead-install: add it with: export PATH="%s:$PATH"\n' "$install_dir" >&2
            ;;
    esac

    # An older copy earlier on PATH would keep answering `herdstead`.
    on_path="$(command -v herdstead 2>/dev/null || true)"
    if [ -n "$on_path" ] && [ "$on_path" != "${install_dir}/herdstead" ]; then
        warn "a different herdstead comes first on your PATH: ${on_path}"
    fi
}

main "$@"
