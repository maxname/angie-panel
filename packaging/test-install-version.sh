#!/usr/bin/env bash
# Test the release stamping of install.sh: the copy published with a release
# must get as far as downloading the package, and a plain checkout copy must
# still refuse to.
#
# Why this exists: the installer tells the two apart by comparing PANEL_VERSION
# with its placeholder. The release workflow used to replace the placeholder
# everywhere in the file — the comparison included — so the installers
# published with v0.2.3 and v0.3.0 compare the version with itself and stop,
# after installing Angie, with "not tied to a release". Nothing had ever
# executed a stamped copy before users did.
#
# What it does:
#   1. Stamps a copy with scripts/stamp-install-sh.sh, the script the release
#      workflow runs, and checks that only the PANEL_VERSION assignment changed.
#   2. Runs the stamped and the unstamped copy the documented way — as root,
#      no argument — in a throwaway Debian container. The container has no
#      network: apt-get, systemctl and curl are stubs, and the curl stub fails
#      the package download, which is as far as either run may get.
#
# Usage: packaging/test-install-version.sh   (requires docker)
#        IMAGE=debian:13-slim packaging/test-install-version.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IMAGE="${IMAGE:-debian:12-slim}"
# Any well-formed version will do; this one is not a release.
VERSION="9.9.9"
# From the installer's message for an unstamped copy.
REFUSAL="не привязана к релизу"

pass() { printf '\033[32m✓ %s\033[0m\n' "$1"; }
fail() { printf '\033[31m✗ %s\033[0m\n' "$1" >&2; exit 1; }

command -v docker >/dev/null 2>&1 || fail "docker is required"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# --- 1. Stamping -------------------------------------------------------------
mkdir "$WORK/unstamped" "$WORK/stamped"
cp "$ROOT/packaging/install.sh" "$WORK/unstamped/install.sh"
"$ROOT/scripts/stamp-install-sh.sh" "$VERSION" "$WORK/stamped/install.sh"

# Apart from the assignment the two copies must be identical: the installer's
# check for the placeholder, and the comments that name it, ship as committed.
if ! diff <(grep -vxF 'PANEL_VERSION="__PANEL_VERSION__"' "$WORK/unstamped/install.sh") \
          <(grep -vxF "PANEL_VERSION=\"$VERSION\"" "$WORK/stamped/install.sh"); then
    fail "stamping changed more than the PANEL_VERSION assignment (diff above)"
fi
if [ "$(grep -cxF "PANEL_VERSION=\"$VERSION\"" "$WORK/stamped/install.sh")" != 1 ]; then
    fail "the stamped copy does not carry PANEL_VERSION=\"$VERSION\" exactly once"
fi
pass "stamping rewrites the PANEL_VERSION assignment and nothing else"

# The likeliest mistake in a workflow: passing the tag (v0.3.0) as it is.
if "$ROOT/scripts/stamp-install-sh.sh" "v$VERSION" "$WORK/rejected.sh" 2>/dev/null; then
    fail "stamp-install-sh.sh accepted \"v$VERSION\"; it must refuse a tag that still has its v"
fi
pass "stamping refuses a version that still carries the tag's v"

# --- 2. Run both copies ------------------------------------------------------
mkdir "$WORK/stubs"
# Every call is echoed, so the transcript shows what the installer asked for.
# A request for a release asset fails like a 404; anything else "succeeds",
# leaving an empty file where one was asked for with -o.
cat > "$WORK/stubs/curl" <<'EOF'
#!/bin/sh
echo "stub-curl: $*" >&2
out=""
prev=""
for arg in "$@"; do
    if [ "$prev" = "-o" ]; then
        out="$arg"
    fi
    case "$arg" in
        */releases/download/*) exit 22 ;;
    esac
    prev="$arg"
done
if [ -n "$out" ]; then
    : > "$out"
fi
exit 0
EOF
for cmd in apt-get systemctl; do
    printf '#!/bin/sh\nexit 0\n' > "$WORK/stubs/$cmd"
done
chmod +x "$WORK/stubs/"*

docker image inspect "$IMAGE" >/dev/null 2>&1 || docker pull -q "$IMAGE" >/dev/null

# Run the install.sh in directory $1; leaves its exit status in RC and its
# combined output in OUT.
run_installer() {
    RC=0
    OUT="$(docker run --rm --network none \
        -v "$WORK/stubs:/stubs:ro" -v "$1:/installer:ro" \
        "$IMAGE" \
        env PATH=/stubs:/usr/sbin:/usr/bin:/sbin:/bin \
        bash /installer/install.sh 2>&1)" || RC=$?
}

show_and_fail() {
    printf -- '--- installer output (exit %s) ---\n%s\n---\n' "$RC" "$OUT" >&2
    fail "$1"
}

# Both runs are expected to end in one of the installer's own errors (exit 1):
# anything else means it never got to the part under test.
DOWNLOAD_RE="^stub-curl: .*/releases/download/"

run_installer "$WORK/unstamped"
if [ "$RC" -ne 1 ]; then
    show_and_fail "unstamped copy: expected the installer to stop with its own error"
fi
if ! grep -qF "$REFUSAL" <<<"$OUT"; then
    show_and_fail "unstamped copy: did not refuse with «$REFUSAL»"
fi
if grep -Eq "$DOWNLOAD_RE" <<<"$OUT"; then
    show_and_fail "unstamped copy: tried to download a release it does not belong to"
fi
pass "unstamped copy refuses to download"

# The package URL the stamped copy must ask for, as an ERE.
V_RE="${VERSION//./\\.}"
URL_RE="https://github\.com/maxname/angie-panel/releases/download/v${V_RE}/angie-panel_${V_RE}_(amd64|arm64)\.deb"

run_installer "$WORK/stamped"
if [ "$RC" -ne 1 ]; then
    show_and_fail "stamped copy: expected the run to stop at the stubbed download"
fi
if grep -qF "$REFUSAL" <<<"$OUT"; then
    show_and_fail "stamped copy: took the unstamped-copy branch — the published installer would refuse to install"
fi
if ! grep -Eq "^stub-curl: .* ${URL_RE}\$" <<<"$OUT"; then
    show_and_fail "stamped copy: never asked for its release package"
fi
pass "stamped copy reaches the package download for v$VERSION"
