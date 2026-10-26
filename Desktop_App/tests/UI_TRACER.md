# Minimal Electron/UI Director tracer

This Linux-only local gate opens an isolated project through the existing native
**Open Recent** submenu, selects the fixture model with the actual `change`
handler, submits the real chat form and waits for a terminal Director reply from
the real Rust host. Its planner is a separate ephemeral loopback HTTP server.
It does not inject application dispatch or approval calls, replace the bridge,
exercise media, use live Studio providers or establish creative acceptance.

## Prerequisites

From `Desktop_App`, with dependencies already installed:

```sh
cargo build --offline --manifest-path host/Cargo.toml
npx tsc -p tsconfig.json
npm run build
node --test electron/uiTracer.test.mjs
```

Required: Node with built-in WebSocket, Electron, `ss`, `/usr/bin/python3` with
GI/Secret Service bindings, libX11/libXtst, an unlocked Secret Service and an
accessible X11/Xwayland display. `DISPLAY`/`XAUTHORITY` may be supplied; otherwise
the tracer reads only the display-related environment fields of this user's
Xwayland process. Port **1420 must be free**, because the existing development
preload allowlist requires that origin. Inspector/planner/proxy ports are
allocated separately. The tracer fails rather than adopting another server.
Do not run concurrently with another Studio builder/runtime in this checkout.
Native menu interaction requires the owned window to receive focus.

## Run

Each `--out` must be a new absolute directory inside persistent, card-owned
storage. Example (replace the output path for every invocation):

```sh
node tests/ui-tracer.mjs --out /absolute/persistent/T01/success
node tests/ui-tracer.mjs --out /absolute/persistent/T01/failure --inject-failure
node tests/ui-tracer.mjs --out /absolute/persistent/T01/rerun
```

Expected process exit codes: **0, 1, 0**, respectively. Do not append `|| true`,
pipe to `head`/`tee` without capturing the real status, or interpret any generic
nonzero exit as the intended failure. The negative run must contain the exact
`T01 deliberate bounded failure after actual chat submission` marker and an
actual planner request. Every result must show zero remaining owned processes,
closed owned ports, both unique fixture keyring IDs absent and no cleanup errors.
Inspect the failed run from a separate process before the independent rerun.

## What is observed

- `result.json`: exact command/environment, runner/main/renderer/host PIDs and
  birth identities, selected source and executable SHA-256/size/inode, main
  inspector and matching renderer `/json/list` target, actual Electron userData
  and sidecar settings directory, all five listener owners, project identities,
  serialized host terminal response, DOM result and unconditional cleanup.
- `planner-requests.json`: actual `/v1/chat/completions` request, preserved user
  intent/project scope and the open TCP socket mapped by `ss` to the Rust PID.
- `terminal.png`, `electron.log`, `vite.log`, `chromium-netlog.json`.

The main-module startup breakpoint changes only this owned runtime's userData
and spellchecker configuration before application startup. The existing native
Open Recent submenu is displayed through Electron's `popup`, then selected with
real XTest Home/Return events (not a menu callback or synthetic IPC). The test
therefore proves native recent-item selection and the renderer transition, not
manual navigation from the File menu or native folder-dialog interaction.
Read-only debugger pauses capture actual `open_project` and Director responses;
no test-side host command is sent. Model/form events are DOM automation, not
native typing.

## Isolation and cleanup limits

Settings and recents live at the actual `<userData>/config/settings.json` and
`<userData>/recent-projects.json`. Fixture credentials use UUID-only API IDs;
only those exact keyring entries are queried/deleted, without reading values.
The synthetic credential is not a real provider key. A disabled Ollama entry
suppresses automatic discovery. Empty spellchecker languages suppress dictionary
downloads. Host/Chromium HTTP(S) proxies fail closed at a local deny server;
non-loopback attempts fail the run. Chromium HTTP URL records are checked after
shutdown. This is a bounded fixture route, **not an OS-wide network sandbox**.

Vite and Electron start in owned private sessions/groups, with root birth
identities captured immediately. Discovery includes reparented group members,
rejects reused leaders/stale groups and preserves zombie identities until reap.
Signals target only recorded birth-matched PIDs. Independent teardown actions
are attempted even after an earlier failure; errors remain in the report.
This does not promise cleanup after SIGKILL/power loss, arbitrary hostile
processes deliberately escaping their session, or unavailable OS permissions.
A separate verifier must audit the actual negative-run resources after exit.

Stop at this minimal gate. Media workflow, approval execution, live-provider
success and listening/creative/final Studio acceptance require separate evidence.
