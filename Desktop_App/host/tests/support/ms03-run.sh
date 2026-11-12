#!/bin/sh
# Private test-only Secret Service, no use of the user's keyring/session.
set -u
export TMPDIR=$(mktemp -d /runtime/ms03-fixture.XXXXXX)
export XDG_DATA_HOME="$TMPDIR/keyring"
mkdir -p "$XDG_DATA_HOME"
trap 'rm -rf "$TMPDIR"' EXIT
printf '\n' | gnome-keyring-daemon --unlock --components=secrets >/dev/null
cargo test --offline --manifest-path host/Cargo.toml --test cleanup_model_sheet_functions "${1:-ms03}"
