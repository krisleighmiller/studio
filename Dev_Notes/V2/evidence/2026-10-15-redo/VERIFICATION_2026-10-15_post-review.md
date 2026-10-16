# Scene Look verification — 2026-10-15 (post-review re-run)

Method: packaged build (`dist/`, no dev server) launched via Electron with
`--remote-debugging-port=9222` / `--inspect=9229`; scratch project copy at
`~/.hermes/profiles/studio/cache/scratch/Lite_Challenge_ui` (rsync'd fresh from
`~/Content_Creation/Projects/Lite_Challenge` before the run, restored after).
Scripts: `verify_capture.mjs`, `verify_interact.mjs`, `verify_probe2..4.mjs`
(scratch dir). No billable requests were approved.

## Viewport captures (Jobs collapsed + expanded)

Files: `scene_look_verify_{1440,1000,720}x900_{collapsed,expanded}.png`,
`verify_post_interact_1440x900.png`.

DOM metrics at capture time:

| viewport | hero img (w×h) | alt tiles | slot tabs | overflowX | jobs collapsed h | jobs expanded h |
|---|---|---|---|---|---|---|
| 1440×900 | 529×414 | 1 | 7 | no | 38 | 367 |
| 1000×900 | 276×414 | 1 | 7 | no | 38 | 383 |
| 720×900  | 284×414 | 1 | 7 | no | 38 | 363 |

At all widths: hero title "Warm Compartment on the Last Train" present,
narrative purpose visible, no horizontal overflow, `#studio-context-mount`
absent (single Director context presentation), spend disclosure hidden when
Jobs collapsed and present inside expanded Jobs.

## Interaction results

- **Baseline render**: scene list rows show deduped identity (`Scene 1 — The
  last train / Blocked`); Director "Attached context" lists Scene 1, Cut 1,
  cut_start_frame exactly once.
- **Alternate select**: single alternate is the approved current; clicking it
  re-selects without changing the primary image (expected — already active).
- **Approve**: no approvable alternate exists (sole version already approved) —
  button correctly absent/disabled; no no-op approve emitted.
- **Reject**: clicked on the one alternate → tile now shows `rejected`, hero
  pill changed to `Draft still`. Real host transition on the scratch copy
  (restored afterwards).
- **Open**: opens the Files surface media view on the image
  (`surface-files`, `media-view`, status "Image"). Works.
- **Generate (mock attempt)**: `window.workbench` bridge is frozen by the
  contextBridge (not writable/configurable), so page-level mocking is
  impossible — a security property, not a gap. Clicking "Generate still" for
  real produced a Director **approval card** ("Scene look generate · … Host
  will save to `Preproduction/Scene1/references/interior_4.png`") with an
  Approve button and stopped there: per-operation authorization gate engaged,
  no provider call, nothing billed. Card was left unapproved; fixture restored.
- **Jobs panel (expanded)**: disclosure line intact — "No budget · 0.00 USD
  spent · 0.00 USD committed · 12 unverified/unknown-cost jobs · exposure not
  bounded · Accounted jobs only; LLM/chat costs are not included; chat is not
  budget-enforced." Filters All 51 / Active 4 / Failed 0 / Awaiting 5 /
  Complete 42. Legacy rows show truthful unknowns (Created/Started/Completed/
  Estimate/Actual unknown); Cancel/Resume truthfully marked "Provider support
  unknown · unavailable in Studio"; "Open image" output link present.
- **Navigation round trip**: Studio → Files → Assets → Voices → Settings →
  Studio (Settings is the top-bar `#settings-btn`, not a rail item — earlier
  "MISSING" was a probe selector error; verified `surface-settings` renders).
  Project `Lite_Challenge_ui` and the selected scene/hero survived the round
  trip.

## Follow-up corrections (same day, second pass)

**1. Heading duplication was NOT fully fixed — now fixed.** Stored scene titles
in `Lite_Challenge` already carry the prefix (`Scene 1 — The last train`), and
`sceneLookModel` prepended `Scene N — ` again, rendering
`Scene 1 — Scene 1 — The last train`. Added `sceneHeadingTitle()` in
`src/studio/selectors/scenes.ts` (strips a matching stored prefix, handles
bare `Scene N`); used by the Scene Look header. The Director scope label was
deliberately left in host `scope.rs` format (`Scene 1 Scene 1 — …` parity is
pinned by `tests/phase4-closeout.test.ts`). Regression tests in
`tests/studio-acceptance.test.ts`. Verified live: zero occurrences of
`Scene 1 — Scene 1` in the rendered workspace.

**2. Approve + completed generation exercised against a mocked provider.**
`window.workbench` is frozen, but the provider boundary is not: added
`WORKBENCH_ATLAS_BASE_URL` override and a loopback-only http exception to the
download guard in `host/src/atlas.rs` (with unit tests, incl. a
`127.0.0.1.evil.com` bypass case the test itself caught). Ran a local mock
Atlas (`mock_atlas.py`): uploadMedia × 7 refs → spend-authorization prompt →
authorized → generateImage submit → prediction poll → download of the mock
32×32 PNG to `references/interior_4.png` → spend ledger job
`spend-4531… completed`. Separately, version **Approve** was exercised for
real: draft → `approved · current`, prior version → `superseded`, hero pill
`✓ Approved`.

**Findings surfaced:**
- The Electron app loads the **release** host binary in non-dev mode; it was
  stale (Sep 27) during earlier verification — debug builds were never what
  the app ran. Rebuilt; `verify_capture`/`verify_interact` results above were
  re-confirmed against the current release binary.
- During the first mock attempt (before the rebuild), one generation executed
  against the **real** Atlas provider and completed (`job_52`, one image,
  ~0.07 USD per the pricing registry). Reported honestly; fixture restored.
- The fixture's `.png` files are JPEG content; the host upload integrity
  check correctly refuses them (verified). Scratch-copy conversion to real
  PNGs was required for the upload path.
- **Reconciliation gap (new, unfixed):** when the spend gate interrupts a
  scene-look generation, the authorized replay downloads the file and
  completes the spend ledger entry, but the `scene_look` job stays `pending`
  and no asset version is registered — the in-turn path registers fine
  (`job_52`), but the gated path has no UI recovery: refresh does not
  reconcile, and asking the Director to continue is refused by the fail-closed
  chat guard ("chat cost is unknown…"). The user currently has no in-app way
  to finish registering a gated generation's output.
- The spend-confirmation dialog is a native `dialog.showMessageBox`; the
  spendConfirmation handler drops further spend events while one ask is
  unresolved (`pending` flag), which can silently strand a second operation's
  authorization.

## Notes

- One rejected alternate + one pending approval card were the only mutations;
  both confined to the scratch copy, which was restored from the pristine
  project afterwards. App process terminated cleanly.
- Pixel-level mockup comparison remains tool-unverified (no vision backend in
  this environment); metrics and OCR-level checks above stand as recorded.

---

# Third pass — generation regressions repair (2026-10-16)

Follow-up to the "reconciliation gap" and "dropped second spend event"
findings above, plus review items raised against the second pass.

## Root causes and fixes

1. **Gated replay never completed the workflow job.** Registration lived only
   in the Director specialist turn, which the spend gate terminates. Fixed:
   `media_generate_confirmed` (`host/src/lib.rs`) now runs the same post-save
   registration hooks on success, and fails the pending scene-look job on a
   terminal provider failure. `look.rs` gained a shared
   `register_look_output` core plus `on_still_failed`.
2. **No recovery for downloaded-but-unregistered outputs.**
   `import.rs::restore_scene_look_jobs` registers such outputs on project
   load (idempotent; missing files stay pending; invalid content fails the
   job — validated via `media_util::ensure_media_file` magic-byte checks).
3. **Silent drop of concurrent spend events.** `spendConfirmation.ts` now
   keeps a bounded (4) deduplicated queue with per-request revalidation
   against the live project, explicit overflow rejection with an accurate
   re-trigger instruction (the previous text referenced a Jobs resume action
   that does not exist), and a specific stale-quote explanation.
4. **Test-only provider redirect leaked into production builds.** The
   unguarded `WORKBENCH_ATLAS_BASE_URL` override and loopback exception were
   removed from ordinary builds and re-gated behind an off-by-default
   `--features mock-provider` cargo feature. Mock builds: `reqwest::Url`-
   validated loopback-only base (rejects userinfo, lookalikes, hex/octal
   IPv4, external hosts), fail closed when the env var is missing/invalid,
   hermetic downloads (loopback only, http or https — arbitrary https
   destinations are refused), and `host_build_info` exposes
   `mock_provider: true` so harnesses can refuse a stale ordinary binary.
   `settings.rs::fetch_atlas_catalog` had a second hardcoded URL bypassing
   the gate; it now routes through the same check (fail-closed in mock
   builds).
5. **Terminal provider failure misclassified.** `status=failed` was treated
   as retriable "not ready", causing three wasted poll windows and a job
   left misleadingly pending. `atlas::collect_media` now emits a
   `provider-failed:` terminal error; the media router stops polling
   immediately and returns Err, the spend ledger records a failed
   (unverified) attempt, and the scene-look job is marked failed.

## Verification

- Unit: cargo default 425 passed / 0 failed; cargo `--features mock-provider`
  426 / 0 (final counts after the terminal-classification test was added;
  an earlier draft of this note said 424/425). Includes: recovery
  idempotency, invalid-output rejection
  (garbage bytes → job failed, nothing registered), mock URL negative
  tests, hermetic-download tests, and
  `second_queued_quote_goes_stale_after_first_operation_settles` (host-level
  proof that a second quote against the pre-settlement revision is rejected
  without dispatch, and a fresh quote dispatches with its own request
  fingerprint).
- Frontend: tsc clean; vitest 10 files pass incl. 12/12 spendConfirmation
  (queue, dedup, project-switch, stale-quote guidance, overflow wording);
  Electron 19 pass; production build clean.
- App-level (mock-provider binary, isolated `--user-data-dir` profile,
  dummy keyring credential, loopback mock; routing proven by a non-billable
  catalog probe; binary identity `mock_provider: true`):
  - **Two-operation concurrency:** op1's dialog held, op2's event queued
    (dialog count stayed 1). On release, op1's quote was stale (op2's
    prepare had already changed the ledger revision) — its confirm was
    rejected with **zero** provider submissions for op1. The queued op2 was
    then presented, confirmed against its current revision, and completed
    independently (own job `job_52`, own output `interior_5.png`, own spend
    entry). A fresh-quote re-run completed op3 (`job_54` / `interior_7.png`).
    Total successful submissions: exactly 2 for 2 completed operations; no
    request, token, or output crossed operations. Log:
    `mock_atlas_concurrency.log`; driver output: `e2e_concurrency.out`.
    Evidence caveat (from independent review): the submission counts, the
    held/queued dialog counts, and the stale-rejection outcome are in those
    artifacts; the specific job ids and output filenames (job_52/job_54,
    interior_5.png/interior_7.png) were read live from the scratch project's
    `workflow_state.json` during the run and are not persisted in any
    artifact. The driver script's "STATUS (expect …)" lines are soft
    observations, not assertions — the spend queue/stale status messages are
    transient and were overwritten by later statuses; their text is pinned
    by the vitest suite instead.
  - Earlier single-op flow (second pass) re-verified with the repaired
    binary: gated generation → draft registered → approved via UI →
    persisted across reload and project reopen; exactly 1 submit.
  - Failed-provider flow (app run, pre-fix binary): terminal failure, no
    download, no asset, job left pending — this run *exposed* the
    misclassification. Post-fix behavior is unit-covered
    (`terminal_failure_is_distinguished_from_still_processing`): the router
    stops polling immediately and returns Err, the spend ledger records a
    failed unverified attempt, and `media_generate_confirmed` fails the
    scene-look job. The failed-provider app scenario was not re-run after
    the fix; that remains an open verification item.
- Restored ordinary release binary answers `host_build_info` with
  `mock_provider: false` (stale-mock masquerade protection verified).

## Known limitations (recorded, not fixed here)

- A quote is invalidated by **any** ledger-changing event, including another
  operation's prepare creating a job — deliberately conservative; the
  rejected operation must be re-triggered (the UI now says exactly this).
  `job_51` in the concurrency run remains legitimately pending.
- `media_recovery_status` reports provider status only; there is no
  in-app collect-to-disk for Atlas predictions without Director chat.
- Approve buttons on consumed Director cards remain enabled; clicking one
  surfaces a visible "approval is no longer active" error but creates
  confusion (observed during the concurrency run).
- The mock build's chat provider path (`chat.rs`) is not routed through the
  mock gate; chat remains fail-closed in all tests.
