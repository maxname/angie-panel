#!/usr/bin/env bash
# Write the copy of packaging/install.sh that is published with a release:
# the same file, with the release version stamped into ONE line —
#
#     PANEL_VERSION="__PANEL_VERSION__"   ->   PANEL_VERSION="0.3.0"
#
# Only that assignment. The placeholder also appears in the installer's own
# check for an unstamped copy and in its comments, and those must ship as
# committed. A file-wide replacement turned the check into
# `[ "$PANEL_VERSION" = "0.3.0" ]` — always true — so the installers published
# with v0.2.3 and v0.3.0 refuse to download the package.
#
# The release workflow and packaging/test-install-version.sh both stamp through
# this script, so what CI exercises is the substitution that ships.
#
# Usage: scripts/stamp-install-sh.sh <version> <output-file>
#        <version> is the tag without its leading "v", e.g. 0.3.0
set -euo pipefail

if [ $# -ne 2 ]; then
    echo "usage: $0 <version> <output-file>" >&2
    exit 2
fi
VERSION="$1"
OUT="$2"
SRC="$(dirname "$0")/../packaging/install.sh"
PLACEHOLDER_LINE='PANEL_VERSION="__PANEL_VERSION__"'

# The version becomes part of the download URL (and of the sed replacement
# below). Anything that is not a plain version — a tag still carrying its "v",
# an empty string — would publish an installer that points nowhere.
if ! [[ "$VERSION" =~ ^[0-9][0-9A-Za-z.+~-]*$ ]]; then
    echo "stamp-install-sh: not a release version: '$VERSION' (expected e.g. 0.3.0, without the leading v)" >&2
    exit 1
fi

# Exactly one line to rewrite. None: install.sh changed shape and nothing would
# be stamped. Several: this script no longer knows which one is the assignment.
count=$(grep -cxF "$PLACEHOLDER_LINE" "$SRC" || true)
if [ "$count" -ne 1 ]; then
    echo "stamp-install-sh: expected exactly one line '$PLACEHOLDER_LINE' in $SRC, found $count" >&2
    exit 1
fi

# cp first so the output keeps install.sh's mode; then replace its contents.
cp "$SRC" "$OUT"
sed "s/^PANEL_VERSION=\"__PANEL_VERSION__\"\$/PANEL_VERSION=\"$VERSION\"/" "$SRC" > "$OUT"
