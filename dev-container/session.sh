#!/bin/bash
set -euo pipefail
umask 022
export HOME=/tmp/studio-home XDG_RUNTIME_DIR=/tmp/studio-runtime
mkdir -p "$HOME" "$XDG_RUNTIME_DIR"
chmod 0700 "$HOME" "$XDG_RUNTIME_DIR"
if [[ ${1:-} == desktop ]]; then
  shift
  if [[ ${STUDIO_MAP_UI:-0} == 1 && $(id -u) == 0 ]]; then
    export STUDIO_MAP_UI=0
    exec unshare --user --map-user=1000 --map-group=1000 /opt/studio/session.sh desktop "$@"
  fi
  if [[ ${STUDIO_NATIVE_DISPLAY:-0} == 1 ]]; then
    exec dbus-run-session -- "$@"
  fi
  exec xvfb-run -a -s '-screen 0 1600x1000x24 -nolisten tcp' dbus-run-session -- "$@"
fi
exec "$@"
