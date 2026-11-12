# Studio test harnesses

## Automated regressions

From `Desktop_App`, `npm test` runs Node's `electron/*.test.mjs` tests and Vitest's `tests/**/*.test.ts` tests. `npm run test:host` runs Rust tests separately. These commands do not automatically run the standalone UI scripts below.

Use the established Docker workbench when host-local dependencies are unavailable:

```sh
cd dev-container
./studio exec -T workbench env TMPDIR=/tmp npm test
```

## Standalone UI checks

The `request-visibility-*.mjs`, `notes-save-explorer-electron.mjs`, and `model-sheet-delete-*.mjs` scripts launch Electron/Vite and exercise current direct-workflow UI behavior. Several deliberately intercept host calls or native dialogs to create deterministic pending/error states. They test those boundaries, not live provider integration or the real picker. Run them sequentially because their listener ports are fixed.

Each run creates a fresh output directory outside the checkout by default. `STUDIO_TEST_OUT` selects an explicit absolute output path; it must not already exist. Prior output is never removed to make a run fit. Logs, screenshots, disposable projects and app profiles belong to that run. To retain results in the existing container runtime volume:

```sh
# From dev-container. Choose an output path that does not already exist:
./studio --profile ui run --rm --no-deps \
  -e STUDIO_TEST_OUT=/runtime/my-new-duplicate-click-run \
  ui desktop node tests/request-visibility-duplicate-click.mjs
```

Use the `ui desktop` launcher for these checks, not a root workbench Electron launch: it supplies the unprivileged identity, X11 and private session bus. Each standalone check starts and stops its own Vite server. Fixture teardown force-exits only its disposable Electron instance, avoiding close-confirmation prompts from intentionally pending requests; it does not exercise normal user shutdown.

The current container scripts use its prepared `/runtime/target/debug/workbench-host`; `WORKBENCH_HOST_BIN` can select a different prepared binary. Changing the renderer does not rebuild that binary. These are disposable offline tests: do not supply personal provider settings, credentials or original projects. Container-only `--no-sandbox` fixture launches are not evidence that the application's normal sandboxed launcher works.

## Native picker and container smoke

See [native tracer commands and options](UI_NATIVE_TRACER.md) for the attachment scenario and deliberately injected failure check.

`sb14-native-ui.mjs` exercises attachment through the real native picker using committed fixtures. It needs X11, the native Python helper dependencies, a private Secret Service session and a prepared host binary. Its shared runner owns and cleans up its processes, sockets and synthetic keyring entries. It must not use the retired chat controls as setup prerequisites.

The maintained container smoke and keyring probes are in `../../dev-container/`; use that directory's [operating guide](../../dev-container/README.md) for their display, session and sandbox requirements. The smoke opens the actual application and verifies the preload bridge and renderer sandbox without provider calls.

## Shared helpers and retired scenarios

Reusable CDP, bounded polling and process-cleanup functions live under `helpers/` and have Node regression tests under `../electron/`. Reuse those helpers rather than copying a whole runner.

The old T01 Director-chat tracer, A01 chat-driven assembly scenario and A02 mixed chat lifecycle scenario are retired, along with their old execution guides. Their historical results are not a current runnable suite. Relevant direct-workflow regression coverage remains, including `sb22-operation-approval.test.ts`; retirement does not imply equivalent coverage of every old interaction. The native-project-chooser coverage limitation remains documented in the [current reference](../docs/studio-reference.md).
