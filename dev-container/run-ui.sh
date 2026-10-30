#!/bin/bash
set -euo pipefail
cd /source/Studio/Desktop_App
export XDG_CACHE_HOME=/tmp/ui-cache npm_config_cache=/tmp/ui-npm
./node_modules/.bin/vite --host 127.0.0.1 &
vite=$!
trap 'kill "$vite" 2>/dev/null || true' EXIT
node_modules/.bin/wait-on http://127.0.0.1:1420
node_modules/.bin/electron . --disable-gpu
