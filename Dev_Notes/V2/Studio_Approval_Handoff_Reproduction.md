# Director approval handoff — reproduction and repair evidence

## Scope and baseline

This document preserves the initial reproduction, followed by the bounded repair and its review results, for the first approval/result handoff in the [usable filmmaking recovery plan](Studio_Usable_Filmmaking_Recovery_Plan.md). It does not establish the full dialogue-scene capability map or runtime acceptance.

- Branch: `v1-baseline-phase8-deferred`
- HEAD: `6ab6d9a8a5f8a6cb28d55a5b9726f268072f0fa0`
- At the original reproduction checkpoint, production source was unchanged. The repair below modifies only `src/main.ts` in production. No commit, branch change, live provider call or original Black_Hole write is part of this work.

## Reproduction

Test file: `Desktop_App/tests/director-approval-continuation.test.ts`.

From `Desktop_App`:

```sh
npx vitest run tests/director-approval-continuation.test.ts --reporter=verbose
```

The harness loads the real `index.html` body and imports the real `src/main.ts`. It opens a synthetic project through the Home button, submits through the chat form and clicks the actual rendered Approve button. IPC, settings, voice and workflow-desk panels are fixtures. The synthetic `/fixture/black-hole-repro` path is not a copied project or an actual disk write. `fetch` is forbidden; the HTML head is excluded to avoid happy-dom fetching Vite stylesheets.

Request:

> Save the approved production note for the Mom/Sam car exchange, then read it back and report whether the dialogue is preserved. Do not generate media.

Sequence:

1. The first `director_turn` returns an approval card.
2. Clicking Approve invokes `director_approve` exactly once with `fixtureAction1`.
3. The IPC fixture returns a completed local-action message and `continueSuggested: false`, matching the existing completed-specialist convention.
4. The real renderer displays that result, removes the consumed approval marker and becomes idle.
5. **Failure:** no second `director_turn` occurs. The outstanding read-back step never reaches the planner.

The fixture's completion message and planned read-back answer are test data, not claims that a file was written/read or that the host/model executed those operations.

### Assertion witnessed failing

```text
The completed action must reach the planner for the outstanding read-back step
Expected: 2 director_turn calls
Received: 1 director_turn call
```

Expected behavior comes from the explicit two-step request and recovery-plan result-handoff requirement, not from the current implementation. A flag-only repair (`if continueSuggested`) would still fail this case because completed specialists return false. The test also requires the next request to include the original user task, the completed-action result, the same project root and the stable scene/shot IDs. Those later assertions have not yet been reached in the failing case.

## Passing controls

- Ordinary chat continuation works through the same real renderer harness. This rules out a broken timer/paint setup as the explanation for the missing second turn.
- A pending approval marker plus `continueSuggested: true` remains paused for another explicit approval. No second action is auto-approved.
- A rejected `director_approve` shows the error, releases busy state and does not continue.

These controls do not yet prove cancellation races, scope changes during approval, reload recovery, hop-limit handling, running-job handling or duplicate provider submission prevention. Those are required repair gates, not established by a mocked IPC boundary.

## Source trace and interpretation

- `Desktop_App/src/main.ts`: `approveDirectorAction` displays `result.reply` and refreshes views, but does not hand the result back to planning.
- `Desktop_App/src/main.ts`: `sendChatMessage` has separate bounded ordinary-turn continuation and guards against pending approval markers.
- `Desktop_App/host/src/director.rs`: `director_approve` forwards a specialist's `continue_suggested`; `finish_director_reply` publishes pending actions into one-time approval markers.
- `Desktop_App/host/src/specialists.rs`: `SpecialistResult::done` returns false; `SpecialistResult::cont` returns true. Completed media paths use `done`, while inspected unfinished media/model-sheet paths pair `cont` with another pending action.

**Do not interpret specialist completion as completion of the original user task. Do not interpret a continuation flag as permission to approve or rerun another action.** The narrow repair must restore planning after a completed action while keeping pending/failed/running/stopped states distinct. This checkpoint deliberately leaves that repair unimplemented.

## Real host characterization

Test file: `Desktop_App/host/tests/director_approval_continuation.rs`.

From `Desktop_App/host`:

```sh
cargo test --test director_approval_continuation -- --nocapture
```

This passing companion test uses the real staging, one-time approval, validation, specialist execution and reply-publication code. It stages an overwrite of `Research/production-note.md` in a temporary project, proves the old contents remain before approval, calls `director_approve`, and reads the actual file back. It verifies:

- the approved body really reaches disk;
- the response contains `approve:text_write`, the written path, no approval marker and serialized `continueSuggested: false`;
- replaying the consumed approval fails and leaves the written contents intact.

No planner is configured or called. The body is test input, not model-generated content; the read-back is performed by the test, not by the Director. This is a host integration test plus a separate renderer-boundary regression, **not** a combined Electron end-to-end test.

Historical fixture limitation: the first isolated test supplied one explicitly disabled, credential-free placeholder and no model routes to avoid settings migration. That concealed a real bug: every configured-provider local text-write approval compared its deliberately empty routing context with enabled provider settings and failed. The configured-provider regression and bounded host fix below supersede this fixture's earlier coverage.

The independent source audit traced every current `SpecialistResult::cont` producer: incomplete media collection/download, incomplete still-image batches, and model-sheet text-write approval. All carry an encoded pending action which `publish_user_text` converts to another approval marker. No intended successful current path was found with a true flag and no new approval marker. The executed local-write test confirms the false-flag completed-action case needed by the renderer regression.

## Execution results

- Focused run: **1 expected failure, 3 passes**; clean assertion failure, no runtime/import/network errors after fixture setup was corrected.
- Repeated focused run three times: same single failure on every run.
- `npm test`: **19 Electron tests passed; Vitest 131 passed, 1 expected failure**. The RED test is enabled normally, not skipped or marked as an expected failure.
- `npx tsc -p tsconfig.json`: passed. This project command checks `src`, not test files.
- `git diff --check`: passed.
- No-network host companion: **1 passed**; real approved local overwrite, response serialization and replay rejection verified. The new Rust file passes `rustfmt --check`.
- Full Rust suite/build and actual Electron/host/provider end-to-end execution: not run in this checkpoint.

The results above preserve the original RED checkpoint. The repair results below supersede its test status, not its limitations on runtime proof.

## Bounded repair — automated GREEN checkpoint

Production change: `Desktop_App/src/main.ts` only. After a successful approved action, return its response to a history-backed planning turn without adding a synthetic user instruction or resuming a stale host checkpoint. Preserve the context that produced the approval card and carry it through continuations. Do not continue after a pending approval/token, spend-approval event, structured failure, thrown error or Stop. Preserve unsent drafts; do not transfer continuation to another chat or project. Existing ordinary-turn continuation limits remain in use.

Executed verification:

- Initial focused renderer regression: **11 passed**. Independent review subsequently found two blocking holes in that coverage; see below.
- Latest focused renderer regression: **20 passed**. Includes original task/result history, false-flag completed action, exact host pending/spend/failure events, selection changes before and during approval, chat-tab changes, duplicate approval clicks, Stop and a later explicit approval, unsent draft preservation, dialog/recent project-transition timing, canceled/same-root invalidation and approval suppression during a pending dialog. Additional regressions cover Stop during pre-request paint, multiple automatic hops with and without planner pause/resume, and explorer-refresh failure after successful approval.
- Latest parent-run `npm test`: **19 Electron tests and 148 Vitest tests passed**.
- `npx tsc -p tsconfig.json` and `npm run build`: passed.
- `cargo build`, `cargo build --tests` and `cargo test`: passed; **426 Rust tests passed** across the emitted suite summaries. Existing dead-code warnings remain in `atlas.rs` and `tests/plan_approval.rs`.
- Real host local-write/replay test above is included in that full Rust run and passes.
- `git diff --check` and `rustfmt --check host/tests/director_approval_continuation.rs`: passed.
- Repository-wide `cargo fmt --check` fails in unchanged baseline files (`atlas.rs`, `lib.rs`, `workflow/import.rs`, `workflow/look.rs`, `workflow/spend.rs`). No unrelated formatting changes were made.

### Independent review corrections

The first review requested changes for two blockers:

1. Renderer guards and fixtures used nonexistent `approve:awaiting-spend` / `approve:failed` events. They now match the actual host events `approve:awaiting-spend-authorization` / `approve:error`. Updating the fixtures first produced two failing regressions; correcting the guards restored GREEN.
2. The host changes its project session before the renderer updates `projectRoot`. A delayed-directory-read regression reproduced an automatic planner call during that interval. The renderer now marks transitions in flight and increments a chain-invalidation epoch before dialog/recent/close operations. The epoch follows asynchronous continuation; new approvals and chat submissions are suppressed during transitions. The failing race regression now passes, with additional recent-project, canceled/same-root and pending-dialog coverage.

Subsequent independent review identified three renderer defects, now covered by witnessed RED→GREEN regressions: Stop during pre-request paint left stale progress/status; later automatic hops invented user `continue` input (including the planner-pause/resume path); an explorer-refresh rejection mislabeled a successful approval and suppressed its handoff. The fixes retain the original scope/chat/epoch and treat refresh as best-effort.

Parent verification also reproduced stale progress/status when a project or chat transition abandons the pre-request paint. Three additional regressions (canceled dialog, new project, new chat) failed before the cleanup change and pass afterward. Cleanup now removes the abandoned progress message and sets a paused status only if no newer status has replaced the working status, preserving a new project's display. The tests also assert that a later explicit request does not include the stale progress message in its history.

Latest parent verification: focused renderer file **23 passed**; full `npm test` **19 Electron + 151 Vitest tests passed**; `npx tsc -p tsconfig.json`, `npm run build`, and `git diff --check` passed. The earlier typecheck guard block is no longer outstanding. The bounded independent review of the final pre-request cleanup is complete: no blocking findings or concrete regressions were found; the reviewer independently reran the focused file with **23/23 passing**. This verdict is limited to the renderer cleanup, not live host/provider workflow acceptance.

## Configured-provider local-save approval repair

Live symptom: clicking Approve on the generated digest returned `Provider or model settings changed after approval was issued. Request a new plan.`

Cause: `text_write_approval_card` intentionally captures only the project context, not provider routes: this operation saves already-generated staged bytes and cannot call a model. `director_approve` nevertheless compared that empty routing snapshot with enabled provider settings. The previous disabled-provider fixture missed the real configuration.

Bounded change: exempt only `RunSpecialist` / `text_write` from the routing-snapshot comparison. Preserve one-time approval, project binding, staged target state, input fingerprints, file authorization and all other actions' routing validation. No renderer change, provider-setting change or original project write is part of this repair.

Executed evidence:

- Changed the real host local-write test to an enabled credential-free provider plus a configured text model. Before the production change it failed with the exact live error; afterward it wrote/read back the approved body and rejected replay.
- Added `director_text_write_binding.rs`: five passing controls cover a new nested save despite an unrelated provider change, existing-file edits, a newly created target, a project switch, and rejection of a model action after provider enablement changed.
- Full `cargo test --offline`: **444 passed**, no failures. `cargo build --offline` and `cargo build --offline --tests` passed; existing dead-code warnings remain.
- Focused renderer continuation tests: **23 passed**; full `npm test`: **19 Electron + 162 Vitest tests passed**. Typecheck and production frontend build passed.
- Formatting checks pass for the changed Rust files. Repository-wide formatting still fails in files outside this repair; no unrelated formatting was applied.
- Independent review accepted the routing exemption and found an existing destination-binding gap: a new target's parent could become an in-project symlink while approval was pending, redirecting the saved file. A real-host regression reproduced the unintended successful write. Commit now rejects a reauthorized destination differing from the staged resolved path. The regression verifies `approve:error` and no file at the redirected destination; execution errors use the response envelope rather than an IPC error.
- Final full `cargo test --offline` after this guard: **445 passed**, no failures; `cargo build --offline --bin workbench-host` passed. Changed-file `rustfmt --check` and `git diff --check` passed. The path comparison closes the reproduced pre-execution redirect, not all possible concurrent filesystem races or batch atomicity issues.
- Actual connected Electron/host/model save and automatic read-back remain for live verification. The running host's executable inode differs from the rebuilt binary and `/proc/412209/exe` is marked deleted: the app still runs the old host. Restart Studio, reopen the test project and request a fresh approval card; the failed token was consumed and cannot be replayed.

A scan of added production lines found no hardcoded credentials, eval/exec, shell invocation or pickle deserialization patterns; this is not a full security audit. This remains separate renderer-boundary and real host coverage, not a connected Electron/host/model run. Live request completion, reload recovery, running-job behavior and an actual scene export/revision remain unproven. No live provider request, original Black_Hole write, commit, branch change or push was performed for this repair.
