#!/bin/bash
set -euo pipefail
[[ ${STUDIO_INTERACTIVE:-0} == 1 ]] || { echo 'Explicit interactive mode required' >&2; exit 1; }
umask 077
mkdir -p "$XDG_DATA_HOME/keyrings" "$XDG_CONFIG_HOME/video-workbench"
chmod 0700 "$HOME" "$XDG_DATA_HOME" "$XDG_DATA_HOME/keyrings" "$XDG_CONFIG_HOME" "$XDG_CONFIG_HOME/video-workbench"
# Activation must see the container HOME and display, never the host bus/keyring.
dbus-update-activation-environment HOME XDG_CONFIG_HOME XDG_DATA_HOME XDG_RUNTIME_DIR DISPLAY XAUTHORITY
gnome-keyring-daemon --start --components=secrets >/dev/null
/usr/bin/python3 /source/Studio/dev-container/init-keyring.py
exec "$@"
