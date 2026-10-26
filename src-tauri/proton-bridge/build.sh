#!/usr/bin/env bash
# Build the AuraMail Proton bridge sidecar from the vendored Hydroxide source.
#
# Provenance:
#   Upstream: https://codeberg.org/emersion/hydroxide
#   Pinned commit: 02b3b6c8aff726d2404b6ed5f6ba1b7937e1f30e
#   License: MIT (see LICENSE)
#
# Kept upstream packages: auth, config, events, imap, protonmail, smtp.
# Removed from upstream: carddav/, exports/, imports/, cmd/hydroxide/,
# protonmail/calendar.go, protonmail/contacts.go, protonmail/conversations.go,
# protonmail/import.go, config/tls.go, and the contact surface of
# protonmail/events.go. go.mod and go.sum are tidied to match.
#
# Local patches:
#   go.mod: module path renamed from the snapshot's GitHub module path to
#   codeberg.org/emersion/hydroxide, matching the Codeberg origin.
#   protonmail/protonmail.go: drop the raw request-failure log from doJSON so
#   Proton error text is never written to stderr.
#   auth/auth.go: create and chmod the encrypted auth file as 0600. Upstream
#   uses os.Create, which leaves new files at 0644.
#   auth/auth.go: pass the login username to the client factory so serve can
#   associate Proton 2028 status events with the account username.
#   events/events.go: keep the upstream 30s poll logic but log a fixed message
#   on a failed event poll, so raw Proton error text never reaches stderr.
#   imap/database: persist per-mailbox UIDVALIDITY and UIDNEXT in bbolt and
#   allocate message UIDs from that metadata. The upstream cache reset on every
#   IMAP session renumbered UIDs; forcing a full resync now increments
#   UIDVALIDITY instead.
#   imap/mailbox.go: after paging the full mailbox, reconcile the UID map
#   against a stable remote API ID set so messages deleted while offline
#   disappear without renumbering survivors. Only mappings present before the
#   sync are pruned, and moving listings (page totals differ) skip
#   reconciliation until the next sync.
#   imap/mailbox.go: expose the persisted UIDVALIDITY. MOVE remains a
#   label/unlabel operation and never falls back to COPY + STORE + EXPUNGE.
#
# Builds only read these local files. They never clone, pull, or otherwise
# fetch upstream Hydroxide. Updating the snapshot is a deliberate re-vendoring
# task that bumps the pinned commit, reviews these exclusions, and reruns the
# sidecar tests.
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
cd "$script_dir"

if ! command -v go >/dev/null 2>&1; then
	echo "error: go not found; run this script inside 'nix develop'" >&2
	exit 1
fi

out="$script_dir/dist/auramail-proton-bridge"
mkdir -p "$script_dir/dist"

export CGO_ENABLED=0
export GOOS=linux
export GOARCH=amd64

# Compile every vendored package so a broken patch fails the build, even when
# the wrapper does not import every package yet.
go build -trimpath -buildvcs=false ./...
go build -trimpath -buildvcs=false -ldflags='-s -w' -o "$out" ./cmd/auramail-proton-bridge

echo "built $out"
