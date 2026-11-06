#!/bin/sh
# Exercises `install.sh` without a network.
#
# A fake `curl` in front of PATH serves a fixture archive, and a fake `uname`
# fixes the platform, so the happy path, a mismatched checksum, a missing
# archive and an unsupported machine are all real runs of the installer.
#
#   sh scripts/test-install.sh
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
installer="${root}/install.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT INT TERM

fail() {
    printf 'test-install: FAIL: %s\n' "$*" >&2
    if [ -f "${work}/stderr" ]; then
        sed 's/^/  | /' "${work}/stderr" >&2
    fi
    exit 1
}

pass() {
    printf 'test-install: ok: %s\n' "$*" >&2
}

[ -f "$installer" ] || fail "no installer at ${installer}"

# --- fixtures -------------------------------------------------------------

mkdir -p "${work}/payload" "${work}/empty" "${work}/broken"
cat > "${work}/payload/herdstead" <<'FAKE'
#!/bin/sh
printf 'herdstead 9.9.9\n'
FAKE
chmod 0755 "${work}/payload/herdstead"
cat > "${work}/broken/herdstead" <<'FAKE'
#!/bin/sh
# Stands in for a binary this machine cannot run at all.
exit 1
FAKE
chmod 0755 "${work}/broken/herdstead"
tar -czf "${work}/archive.tar.gz" -C "${work}/payload" herdstead
tar -czf "${work}/empty.tar.gz" -C "${work}/empty" .
tar -czf "${work}/broken.tar.gz" -C "${work}/broken" herdstead

sha() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

fixture_sha="$(sha "${work}/archive.tar.gz")"
empty_sha="$(sha "${work}/empty.tar.gz")"
broken_sha="$(sha "${work}/broken.tar.gz")"

# --- stand-ins for the commands the installer calls -----------------------

mkdir -p "${work}/shim"

# Pins the platform, so one fixture serves macOS and Linux runners alike.
cat > "${work}/shim/uname" <<'SHIM'
#!/bin/sh
case "${1:-}" in
    -s) printf '%s\n' "${FAKE_UNAME_S:-Darwin}" ;;
    -m) printf '%s\n' "${FAKE_UNAME_M:-arm64}" ;;
    *) printf '%s %s\n' "${FAKE_UNAME_S:-Darwin}" "${FAKE_UNAME_M:-arm64}" ;;
esac
SHIM

# Serves the fixture for any archive URL, with the matching `.sha256` beside it.
cat > "${work}/shim/curl" <<'SHIM'
#!/bin/sh
url=""
out=""
while [ $# -gt 0 ]; do
    case "$1" in
        --output)
            out="$2"
            shift 2
            ;;
        -*) shift ;;
        *)
            url="$1"
            shift
            ;;
    esac
done
[ -n "$out" ] || exit 2
if [ -n "${FAKE_LOG:-}" ]; then
    printf '%s\n' "$url" >> "$FAKE_LOG"
fi

case "${FAKE_MODE:-ok}" in
    missing) exit 22 ;;
esac

case "$url" in
    *.tar.gz.sha256)
        case "${FAKE_MODE:-ok}" in
            corrupt) printf '%064d  %s\n' 0 "${url##*/}" > "$out" ;;
            nosums) exit 22 ;;
            nobinary) printf '%s  %s\n' "${FAKE_EMPTY_SHA}" "${url##*/}" > "$out" ;;
            broken) printf '%s  %s\n' "${FAKE_BROKEN_SHA}" "${url##*/}" > "$out" ;;
            *) printf '%s  %s\n' "${FAKE_SHA}" "${url##*/}" > "$out" ;;
        esac
        ;;
    *.tar.gz)
        case "${FAKE_MODE:-ok}" in
            nobinary) cp "${FAKE_EMPTY}" "$out" ;;
            broken) cp "${FAKE_BROKEN}" "$out" ;;
            *) cp "${FAKE_ARCHIVE}" "$out" ;;
        esac
        ;;
    *) exit 22 ;;
esac
SHIM

# `ldd` only matters on Linux; report glibc unless a case says otherwise.
cat > "${work}/shim/ldd" <<'SHIM'
#!/bin/sh
printf '%s\n' "${FAKE_LDD:-ldd (GNU libc) 2.39}"
SHIM

chmod 0755 "${work}/shim/uname" "${work}/shim/curl" "${work}/shim/ldd"

# --- running the installer ------------------------------------------------

# Each case sets these, then calls expect_ok/expect_failure. `run` passes them
# to the installer explicitly: a leading assignment on a function call is not
# reliably exported to the function's children.
CASE_VERSION=""
CASE_MODE=""
CASE_UNAME_S=""
CASE_UNAME_M=""
CASE_LDD=""
CASE_PATH_FIRST=""

run() {
    dest="$1"
    rm -rf "$dest"
    : > "${work}/urls"
    PATH="${CASE_PATH_FIRST:+${CASE_PATH_FIRST}:}${work}/shim:${PATH}" \
        HERDSTEAD_INSTALL_DIR="$dest" \
        HERDSTEAD_VERSION="$CASE_VERSION" \
        FAKE_LOG="${work}/urls" \
        FAKE_MODE="${CASE_MODE:-ok}" \
        FAKE_UNAME_S="${CASE_UNAME_S:-Darwin}" \
        FAKE_UNAME_M="${CASE_UNAME_M:-arm64}" \
        FAKE_LDD="${CASE_LDD:-ldd (GNU libc) 2.39}" \
        FAKE_ARCHIVE="${work}/archive.tar.gz" \
        FAKE_EMPTY="${work}/empty.tar.gz" \
        FAKE_BROKEN="${work}/broken.tar.gz" \
        FAKE_SHA="${fixture_sha}" \
        FAKE_EMPTY_SHA="${empty_sha}" \
        FAKE_BROKEN_SHA="${broken_sha}" \
        sh "$installer" > "${work}/stdout" 2> "${work}/stderr"
}

reset_case() {
    CASE_VERSION=""
    CASE_MODE=""
    CASE_UNAME_S=""
    CASE_UNAME_M=""
    CASE_LDD=""
    CASE_PATH_FIRST=""
}

expect_ok() {
    label="$1"
    dest="$2"
    if ! run "$dest"; then
        fail "${label}: expected success"
    fi
    pass "$label"
    reset_case
}

expect_failure() {
    label="$1"
    needle="$2"
    dest="$3"
    if run "$dest"; then
        fail "${label}: expected a non-zero exit"
    fi
    if ! grep -q "$needle" "${work}/stderr"; then
        fail "${label}: stderr never mentioned '${needle}'"
    fi
    pass "$label"
    reset_case
}

# --- the cases ------------------------------------------------------------

# The happy path installs the fixture, and reports the version it reads back.
expect_ok "latest release installs" "${work}/bin/latest"
"${work}/bin/latest/herdstead" > "${work}/version" 2>&1 || fail "the installed binary does not run"
grep -q 'herdstead 9.9.9' "${work}/version" || fail "the installed binary is not the fixture"
grep -q 'installed herdstead 9.9.9 to ' "${work}/stderr" \
    || fail "the installer did not report the installed version"
grep -q 'is not on your PATH' "${work}/stderr" \
    || fail "the installer did not warn about PATH"
grep -q 'releases/latest/download/herdstead-macos-arm64.tar.gz' "${work}/urls" \
    || fail "the latest release URL is wrong"

# Already on PATH: installed again, no warning.
CASE_PATH_FIRST="${work}/bin/onpath"
if ! run "${work}/bin/onpath"; then
    fail "reinstall into a folder already on PATH"
fi
if grep -q 'is not on your PATH' "${work}/stderr"; then
    fail "warned about PATH when the folder is on it"
fi
pass "reinstall into a folder already on PATH"
reset_case

# A pinned version swaps the tag in the URL.
CASE_VERSION=v0.0.1
expect_ok "pinned version" "${work}/bin/pinned"
grep -q 'releases/download/v0.0.1/herdstead-macos-arm64.tar.gz' "${work}/urls" \
    || fail "a pinned version did not request the tagged archive"

CASE_MODE=corrupt
expect_failure "mismatched checksum" "checksum verification failed" "${work}/bin/corrupt"
if [ -f "${work}/bin/corrupt/herdstead" ]; then
    fail "a mismatched checksum still installed a binary"
fi

CASE_MODE=nosums
expect_ok "missing checksum file still installs" "${work}/bin/nosums"
grep -q 'checksum file unavailable' "${work}/stderr" \
    || fail "a missing checksum file was not reported"

CASE_MODE=missing
expect_failure "missing archive" "could not download" "${work}/bin/missing"
if [ -d "${work}/bin/missing" ]; then
    fail "a failed run created its install folder"
fi

CASE_MODE=nobinary
expect_failure "archive without the binary" "did not contain a herdstead" "${work}/bin/nobinary"

CASE_MODE=broken
expect_failure "binary this machine cannot run" "does not run on this machine" "${work}/bin/broken"
if [ -f "${work}/bin/broken/herdstead" ]; then
    fail "a binary that does not run was installed anyway"
fi

# An older copy earlier on PATH is called out: it would keep answering.
mkdir -p "${work}/decoy"
printf '#!/bin/sh\nprintf "herdstead 0.0.1\\n"\n' > "${work}/decoy/herdstead"
chmod 0755 "${work}/decoy/herdstead"
CASE_PATH_FIRST="${work}/decoy"
expect_ok "another herdstead earlier on PATH" "${work}/bin/decoy"
grep -q 'a different herdstead comes first on your PATH' "${work}/stderr" \
    || fail "an older herdstead on PATH was not reported"

CASE_UNAME_M=riscv64
expect_failure "unsupported architecture" "unsupported architecture" "${work}/bin/riscv"

CASE_UNAME_S=FreeBSD
expect_failure "unsupported operating system" "unsupported operating system" "${work}/bin/freebsd"

CASE_UNAME_S=Linux
CASE_LDD='musl libc (x86_64) 1.2.5'
expect_failure "musl system" "built against glibc" "${work}/bin/musl"

# Unsupported machines fail before anything is downloaded.
CASE_UNAME_M=riscv64
run "${work}/bin/riscv-again" || true
if [ -s "${work}/urls" ]; then
    fail "an unsupported machine still downloaded an archive"
fi
reset_case

pass "all cases"
