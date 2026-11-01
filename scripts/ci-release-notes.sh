#!/usr/bin/env bash
# Print the hand-written release notes for a version.
#
# Usage: ci-release-notes.sh [version]
# Without an argument the version is read from package.json.
# The notes must live at release-notes/v<version>.md; CI fails without them.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VERSION="${1:-}"
if [ -z "$VERSION" ]; then
  VERSION="$(sed -n 's/^[[:space:]]*"version":[[:space:]]*"\([^"]*\)".*/\1/p' package.json | head -n1)"
fi
test -n "$VERSION"

NOTES="release-notes/v$VERSION.md"
if [ ! -s "$NOTES" ]; then
  echo "error: missing or empty $NOTES" >&2
  echo "Add the release notes for v$VERSION before publishing." >&2
  exit 1
fi

cat "$NOTES"
