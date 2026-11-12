# Native UI tracer (SB14)

`sb14-native-ui.mjs` is the maintained native attachment scenario. It shares
import-safe CDP/process/cleanup utilities in `helpers/ui.mjs` and the owned
Electron/Vite/Rust-host lifecycle in `helpers/native-runner.mjs`. There is no
default chat scenario, chat model selection, or chat-selector prerequisite.

The retired T01/A01/A02 chat/Director scenarios and their run guides are removed.
Their assembly-history assertions and chat approval lifecycle are not claimed as
coverage of the current direct workflow. The reusable confinement, CDP failure,
process-identity, independent-cleanup and bounded-request checks remain unit tested.

## Prerequisites

Linux `/proc`, Node with built-in WebSocket and `node:test`, installed app
Node dependencies, a built Rust host, an accessible X11 display, Python 3 with
Secret Service/GObject and XTest support (see `ui-tracer-native.py` and
`sb14-picker.py`), and a functioning session keyring. Vite port 1420 must be free;
the runner refuses to adopt an existing server or app. No provider is contacted:
the fixture endpoint rejects all requests and non-loopback HTTP(S) goes to a
local deny proxy. This proxy/netlog check is not an OS-level network sandbox.

From `Desktop_App`, with an otherwise new output directory:

```sh
node tests/sb14-native-ui.mjs /absolute/persistent/new-sb14-run
node tests/sb14-native-ui.mjs /absolute/persistent/new-failure-run --inject-failure
node --test electron/uiTracer.test.mjs electron/fixtureConfinement.test.mjs electron/nativeUiHelpers.test.mjs
```

Omitting the output argument uses `STUDIO_TEST_OUT`, otherwise the shared runtime
helper creates a disposable directory. A provided output directory must not
already exist. Fixture and Python-script paths resolve from the checkout, not the
launching shell's working directory.

### Explicit environment options

- `STUDIO_TEST_HOST_BIN`: host executable override; otherwise `WORKBENCH_HOST_BIN`,
  then `host/target/debug/workbench-host`. For a dev-container build, explicitly
  set `STUDIO_TEST_HOST_BIN=/runtime/target/debug/workbench-host`.
- `STUDIO_TEST_NO_SANDBOX=1`: explicitly add Electron's `--no-sandbox` for an
  environment that requires it. Default `0` preserves Electron sandboxing.
  A run with this option is **not** evidence of sandbox-enabled operation. The
  selected option is recorded in `result.json`; other values fail validation.
- `STUDIO_TEST_SOCKET_INSPECTOR=proc`: use Linux `/proc/net/tcp` plus the expected
  PID's `/proc/PID/fd` socket inode to verify each IPv4 loopback listener. Default
  `ss` uses `ss -H -ltnp`. Both modes require exactly one listener for each port
  and its expected owner. No missing-tool fallback silently skips ownership.
- `DISPLAY` and `XAUTHORITY`: explicit X11 environment; otherwise the helper
  attempts discovery from an accessible Xwayland process.

## Assertions and evidence

The native Recent menu opens only the disposable fixture. Main, host and renderer
project roots must agree. The host binary hash, Electron executable, renderer
ancestry, host settings directory, process birth identity and listener owners are
checked. File menu mnemonics are normalized; no native menu callback is invoked
programmatically. Desktop capture is available for native-picker screenshots.

The scenario attaches an external fixture PNG twice, to an empty then populated
Scene Look slot, through the native file picker. Each new draft asset must be
associated with the slot, not a cut; source and copied bytes must match; existing
jobs must remain unchanged; the actual selected image must decode visibly.
No provider requests are permitted.

`result.json` contains the run verdict, explicit options, identities, step and
failure details, owned processes and cleanup checks. `sb14-readback.json` contains
asset/state/UI observations. Screenshots include `native-menu.png`,
`empty-picker.png`, `populated-picker.png`, `empty.png` and `populated.png`.
The outer cleanup attempts every independent action, checks owned processes and
listeners are gone, clears only the two unique synthetic keyring entries, and
fails the verdict on async, CDP, network or cleanup errors. `--inject-failure`
fails after native project opening and key migration to exercise this same
cleanup path; its expected verdict/exit code is FAIL/1, not PASS/0.
