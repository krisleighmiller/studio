# Bounded Task 2026-10-15 — Acceptance Criteria (Functional + Visual)

Scope of this bounded task: (A) close remaining production-plan Phase 4
verification gaps (Get_Studio_To_Production.md §8 items 8.1/8.3/Done-When)
including the inherited uncommitted §8.4 job-metadata work, and (B) produce
visible Studio-shell + Scene Look improvements against
Production_Studio_UI_Redo_Plan.md §3/§4/§7-Phase-3 and the measured mock
description in UI_Redo_Visual_Acceptance_Spec.md. Visual work is bounded to
Scene Look, Jobs, the spending-review panel placement, Director/Finish
accessibility, and shared workspace CSS; it does NOT complete production-plan
Phase 5/6 (full shell migration + surface-by-surface move), does NOT touch
Phase 8 governance (PARTIAL / DEFERRED), does not generalize recipes, and adds
no providers. Mock is direction, not a pixel contract; the mock is a light
paper theme while the shipped app theme is the app's own — theme is out of
scope; composition, prominence, density, and placement are in scope.

Starting state: branch `v1-baseline-phase8-deferred` @ 045fde4, inherited
uncommitted §8.4 lifecycle changes in 6 files (kept), pre-existing untracked
docs/. Reference image analysis spec created by a subagent from
UI_Redo.jpg (measured OCR/pixel method; pencil text gaps flagged there).

## A. Functional acceptance

- FA-1 (8.1 done-when c): a test proves the Scene Look / Assemble "Open review
  export" click delivers the project-resolved assembly path to the app
  file-open hook (onOpenPath spy with surface-host stub), and
  electron/commandAllowlist.test.mjs pins membership of
  `workflow_prepare_assemble`, `classify_file`, and the commands this task
  relies on; removal would fail CI.
- FA-2 (chain head): host tests exercise cmd_approve_intent and
  cmd_approve_animatic through the dispatch layer: guard errors (no intent →
  Err; no rough cut → Err) and success state (intent_approved; animatic
  approved with approved_at + checklist note), plus the intent-approved gate
  on planning exit-frame strategies.
- FA-3 (full chain): one new host integration test walks the Redo plan §8
  chain with REAL ffmpeg media: approved plan → approved/preplanned source
  strategies → render registration → real ffmpeg take clips on disk → check
  pending reconcile → take review selection → continuity frame bind (real
  frame extraction) → dependent render ready → successor prepare_render
  registers with Bound source → successor take media lands → job completed via
  production reconcile → assemble prepared → real assembled master written →
  check_pending completes assembly job → review export registered → cut
  version → check_deliverable with ffprobe truth. Disclosed substitution:
  the ffmpeg concat for assembly is executed by the test command (same
  binary/args the specialist path runs) rather than the Director approval
  card, and provider renders are represented by local media landing — no
  live billable requests.
- FA-4 (8.3): host scope tests resolve asset / look slot / requirement IDs
  (resolved + unresolved cases); a vitest cross-check pins the frontend
  describeDirectorContext lines to the host scope.rs format for the same
  state fixture (asset-role and requirement-status lines byte-equal;
  look-slot audio-role fallback reconciled or documented).
- FA-5: inherited §8.4 lifecycle population + its tests remain green.
- FA-6 regression: cargo fmt/build --tests/cargo test, npm test (vitest +
  electron), tsc -p, vite build all green; no Phase 8 behavior change
  (governance tests pass unedited).

## B. Visual acceptance (measured, not asserted by class names)

Baseline captured 2026-10-15 at 1440×900 on a scratch copy of Lite_Challenge
(scene with 7 look slots, 1 alternate visible):
scene_look_baseline.png. Metrics to beat (final screenshot at same viewport
and fixture, same measurement method):

- VB-1 alternates: every version of the selected look slot renders as a
  labeled tile in the right-side alternates strip (baseline: 1 of 7 slots
  surfaced, tile 190×117); each tile gets a caption and an action row
  (Approve/Open) in the strip — per mock, vertical strip beside the primary.
- VB-2 artifact prominence: primary image remains ≥500px wide at 1440
  viewport and its approval pill/state + slot title + narrative purpose are
  VISIBLE without opening any disclosure (baseline: Prompt & metadata
  closed; no header pill).
- VB-3 jobs collapse truth: in the collapsed state the jobs band's measured
  height is ≤48px (baseline: 228px because spend review sits outside the
  collapse wrapper), and when expanded every required disclosure remains
  reachable: budget summary line stays visible, spend-review panel is INSIDE
  the expandable jobs region with real CSS (baseline: orphaned sibling,
  zero style rules).
- VB-4 director + finish accessible: the attached Director context (the same
  entries a Director turn carries) is a persistent strip at the top of the
  right panel (`.studio-context-mount` in the chat aside, per the mock's
  right-zone Director chip column), visible without any 'Details' toggle;
  Finish remains reachable through its existing inspector tab. Baseline:
  zero persistent context; both states only inside tabbed/hidden panels.
  (Deviation from the first-draft approach — a third workspace grid column —
  reverted because it competed with the chat panel for the same width and
  crushed the hero image to ~104px.)
- VB-5 narrow window: at 1000px and 720px (CDP-resized, screenshotted), no
  horizontal overflow (scrollWidth ≤ clientWidth), jobs band still ≤48px
  collapsed, and approve/reject/open/generate actions remain reachable
  (mock rule: collapse may restack, never remove actions).
- VB-6 evidence: final screenshots exist for scene-look expanded-jobs and
  collapsed state at 1440×900 + the two narrow widths; a read-only reviewer
  inspects them.

## Out of scope (explicit)
Full shell restructure into a dedicated Studio surface (Redo Phase 2 /
prod plan Phase 5-6), moving other surfaces, theme/color redesign, recipe
work, spending governance changes, provider integration, retiring the old
desk renderer.

## Measured outcome (2026-10-15, final capture, same method as baseline)

Screenshots (1440×900, 1000×900, 720×900; collapsed + expanded jobs):
`Desktop_App/../scratch scene_look_final_*.png` captured via CDP on a dev
instance of THIS checkout (vite 1420, host sidecar spawned by main).

- VB-1: alternates strip = 2 tiles (all versions of the selected slot),
  captions + per-tile action bars (Approve / Reject / Open; version select
  on non-current tiles), persistent slot tab strip (7 tabs).
- VB-2: primary image 529×414 at 1440 (baseline 514×488) — ≥500 achieved;
  approval pill, slot title, narrative purpose visible with zero disclosures.
- VB-3: collapsed jobs band 38px (baseline 228px); expanded state contains
  budget line + spend review (spendInsideJobs true in DOM measurement).
- VB-4: `.studio-context-mount` renders at the chat-aside top (measured
  present and non-empty at all three widths); Finish via its inspector tab.
- VB-5: at 1000 and 720 hero restacks to horizontal alternates; measured
  scrollWidth == clientWidth (no horizontal overflow) at every width; jobs
  band ≤48px collapsed at all widths; action buttons present in DOM at
  each width (vitest action-bar pins).
- New in this pass: job rows with outputs render a visible
  "Open image/video/review export" pill (`.studio-job-action`); live app on
  the real fixture shows 52 pills over 50 historical jobs, all legacy rows
  without timestamps still display "time unknown".
- FA-4/FA-5: vitest desk-level test loads a canned state through the real
  intent router and asserts the completed assemble job hands
  `/projects/Lite/Final/Scene1_master.mp4` to onOpenPath (13/13 with the
  "time unknown" positive pin added on review).

## Results — Phase 4 chain heads and visual close-out (second pass, 2026-10-15)

- FA-2 (chain heads at the approval boundary): `host/tests/plan_approval.rs`,
  7 integration tests, all green. They drive the public command surface
  (`workflow_save_intent`, `workflow_approve_intent`,
  `workflow_set_animatic_duration`, `workflow_approve_animatic`,
  `workflow_reorder_cuts`) through draft → approve → reload-from-disk, and
  pin: approval persists; resaving a draft clears approval; durations outside
  the real bounds (0.4 s minimum, 120 s maximum in `animatic.rs`) are
  rejected as errors, not clamped; any duration change after approval
  demotes it; a
  scene without cut sketches cannot be approved; and reordering renumbers the
  positional `CutId` while stable ids and the animatic-shot-to-stable-id
  mapping survive. A first-draft file `full_production_chain.rs` from the
  delegated pass was discarded: it did not compile (imports of nonexistent
  command helpers) and its scope was claimed covered by
  `host/tests/vertical_slice.rs` plus the new file. SUPERSEDED (third pass
  below): discarding the chain draft and pointing at component tests is not
  accepted by the re-issued brief; the connected chain was repaired and now
  exists as `host/tests/production_chain.rs`. For accuracy:
  `vertical_slice.rs` contains exactly ONE test
  (`imported_takes_reach_finish_without_hand_edited_workflow_state`) — any
  "18 vertical slice tests" figure in earlier reports referred to
  `workflow/spend.rs` unit tests, not to this file.
- FA-1/FA-3 (frontend): `tests/phase4-closeout.test.ts`, 13 vitest tests,
  all green — review-export open delivers the project-resolved path to the
  app's file-open hook through the real desk seams; Scene Look composition
  is asserted without opening any disclosure; the frontend director scope
  lines are byte-equal to host `scope.rs` describe output; a completed job
  with no elapsed window renders "time unknown" while keeping its open
  affordance. `npm test` overall: 119/119 vitest, electron suite green.
  `tsc --noEmit`: clean.
- FA-1 close-out (post-review): `host/tests/classify_file.rs`, 4 tests —
  the review-export open path's classification is host-side and pinned at
  the public `classify_file` (video, transcript, unknown extension, missing
  file), closing the one false claim the review found in this document's
  status block.
- Visual composition (UI Redo plan): captured at 1440/1000/720, collapsed
  and expanded, against the live scratch copy of `Lite_Challenge`
  (`Dev_Notes/V2/evidence/2026-10-15-redo/`): hero image 529×414 at 1440
  (276×414 at 1000, 284×414 at 720), title and narrative purpose visible
  with zero disclosures open, approval pill rendered, 7 persistent slot
  tabs, `railFinish: false` at every width (no Finish row in the rail —
  it lives on the inspector tab), jobs band 38 px collapsed with the budget
  line and spend review inside the expanded panel only
  (`spendInsideJobs: true`, no duplicate spend chip), director context
  strip mounted right of the header at all three widths, and
  `scrollWidth == viewport width` everywhere (no horizontal overflow).
- Independent review: dispatched against the uncommitted working tree under
  the same read-only protocol as the first pass. Verdict: AC1 (public
  command surface, guards load-bearing) PASS; AC2 (no app-logic
  reimplementation) PASS; AC3 (full host + npm suites green) PASS; AC5
  (money/security boundaries untouched, allowlist diff additive) PASS; AC4
  (doc claims match reality) BLOCKER on one sentence — `classify_file` was
  claimed host-tested with no test behind it. Review also corrected three
  overstated claims in the dispatch brief itself (max duration is 120 s
  not 25 s; re-approval is not rejected host-side and is no longer claimed;
  the per-tile bar is Approve/Reject/Open plus version select). All fixed
  in this pass: `host/tests/classify_file.rs` added, both-duration-bounds
  assertion extended in `plan_approval.rs`, "time unknown" positive pin
  added, and every doc sentence above now matches what runs.


## Results — connected production chain (third pass, 2026-10-15)

Deliverable of the re-issued brief: ONE connected integration scenario from
an unapproved plan to a playable review export and truthful Finish facts,
driven through the real dispatcher (`workbench_host::dispatch`), with only
the external provider boundary mocked.

- New `host/tests/production_chain.rs` — 2 tests, both green:
  - `approved_plan_reaches_a_playable_review_export_through_production_boundaries`:
    a documented Lite_Challenge Scene 1 derivative (two b-roll cuts, dissolve
    transition, sidecar fixtures built with the shipped `scene_plan::fixtures`
    production builders) starting with NO media and NO approvals. The
    scenario drives, in order: `workflow_import` →
    `workflow_save_intent`/`workflow_approve_intent` (approved plan) →
    `workflow_set_strategy` → `workflow_backfill_sketches` →
    `workflow_save_production_prompt` ×2 → `workflow_set_visual_source` ×2
    (approved source strategies; cut 2's previous-selected-take-exit-frame
    strategy reported render-blocked) → initial render of cut 1 (one-use
    director card consumed → provider STILL untouched → unbounded quote →
    `confirm_unbounded_spend` one-use token → `media_generate_confirmed`
    through the real Replicate adapter protocol against a local in-process
    provider stub → durable prediction id recorded via the production
    recovery path) → `workflow_import` reconciliation registers the take and
    completes the job (state never hand-edited; takes arrive only through
    `restore_render_jobs`) → `workflow_set_take_status` selection →
    dependent render of cut 2 refused BEFORE continuity binding
    (`prepare_render` error, zero provider hits) →
    `workflow_bind_continuity_frame` at 0.4 s (real FFmpeg extraction; the
    bound frame asset is FFprobe-validated as a decodable image, parented to
    the selected take) → dependent render → selection →
    `workflow_prepare_assemble` (the pending assemble job is asserted to
    STAY Pending through a rescan while the master does not exist — the
    missing-output failure path) → `director_approve` executing the real
    host-side `assemble::run` FFmpeg pipeline → `Final/Scene1_master.mp4`
    FFprobe-verified: exactly one video stream, 1080×1920 (the 9:16 1080p
    delivery canvas the plan normalizes onto), duration in range, real byte
    size (never `b"master"`) → final rescan registers the review export
    asset, one cut version with the assembly path, both takes
    `used_in_assembly` with selected identity surviving, assemble job
    Completed, stage Present → `classify_file` on the master through the
    dispatcher returns `video` (the host hop the app's open uses) →
    `workflow_unsettled_spend` keeps the unbounded reservation visible.
    Provider-hit accounting asserted per render: exactly one authenticated
    submission per authorized operation, zero before authorization.
  - `failed_provider_outcome_never_completes_the_render_job_or_replays_blindly`:
    a FAILED provider prediction saves no artifact — the confirmed-generate
    outcome reports `saved: null` plus the failure, no take file exists, the
    render job stays Pending through a rescan, no phantom take is
    registered, the assemble stage stays Missing (no fake completion); a
    confirmation minted on a stale quote is refused ("stale or policy
    changed"); and the identical unresolved request is refused by the replay
    guard ("ambiguous previous provider outcome") BEFORE a second provider
    dispatch (stub posts stay at 1).
- Production-code change driven by the chain: `host/src/assemble.rs`
  `trim_scene` — the silent-take conformer appended
  `-f lavfi -i anullsrc=…` AFTER the `-vf` output option, which ffmpeg
  (n9.0.1 on this machine) rejects; the anullsrc input is now declared
  before the output options. A genuine latent bug: the real assembly path
  for silent takes had never been executed by any previous test (the older
  slice wrote `b"master"` bytes instead), so both the bug and the entire
  execute-assembly path were unproven until now. The has-audio and
  voice-over branches are unchanged.
- Disclosed substitution (unavoidable without weakening a security
  boundary): the stub reports a realistic prediction id and status over the
  real adapter protocol, but its final artifact is delivered by the stub
  writing the FFmpeg clip to the job's planned take path instead of an
  https fetch — `atlas::download_atlas_media` requires `https://` by policy
  and the test must preserve that boundary; the production download
  function still runs and accepts the delivered file through its
  scoped-path check. All authorization hops, reservation, recovery-metadata
  persistence, adapter request construction (the stub asserts the Bearer
  key and the production prompt inside `input`), polling, reconciliation,
  assembly, and classification are real host code.
- App-side opening hop: the chain pins `classify_file` on the produced
  master through the dispatcher. The desk-level routing (completed assemble
  job → project-resolved path → app file-open hook; disabled button with no
  export) is pinned by `tests/phase4-closeout.test.ts` (13 vitest). The
  live-app visual evidence remains the CDP captures in
  `Dev_Notes/V2/evidence/2026-10-15-redo/`; the dev app's CDP endpoint was
  NOT reachable during this pass, so no headless launch of the freshly
  assembled master was observed — the OS viewer launch against a
  chain-produced file is evidenced indirectly (current-build captures +
  pinned routing + host classification) and stated as a limitation, not
  claimed as observed this pass.
- Accurately reported counts (executed 2026-10-15 after these changes):
  - `cargo test --offline` (host): lib 386 passed / 0 failed; integration
    binaries — classify_file 4, lite_challenge 1, plan_approval 7,
    production_chain 2, provider_registry_metadata 1,
    scene_plan_commit_flow 10, vertical_slice 1, workflow_import 7;
    total 33 integration + 386 unit, all green.
  - `npm test` (Desktop_App): vitest 119 passed (10 files); Electron
    command-allowlist suite 19 pass / 0 fail. `tsc --noEmit`: clean.
- Phase 4 verdict: every mandatory §8.1–§8.6 "Done when" clause has a
  passing test, and the phase Done-When chain now executes as one connected
  scenario (independent read-only review completed 2026-10-15 — verdict in
  the Review section at the end of this document). Non-mandatory thin seams remain: predecessor-preview DOM,
  scope resolution beyond mirrored labels, a Known-price dispatch estimate
  assertion (not reachable in this environment — the two priced API kinds
  have compile-pinned provider hosts), and a dispatcher-level
  `workflow_add_alternate_shot` name test. Phase 8 remains
  PARTIAL / DEFERRED. No merge to `main`; no live billable request was made
  or could have been (all provider traffic was to 127.0.0.1;
  `atlas-cloud`/`elevenlabs` kinds were never configured).


## Review — independent read-only verdict (dispatched 18:13, returned 18:42)

A read-only reviewer traced every transition of `host/tests/production_chain.rs`
against the host source, re-ran the suites on the final tree, and delivered
per-criterion verdicts. Result: **(a) skipped transitions PASS, (b) boundary
mocking PASS with one minor finding, (c) authorization/security PASS, (d) real
assembly PASS, (e) claim accuracy PASS; `false_claims[]` limited to two minor
overstatements, both addressed in the revision under review** — the bind hop was
routed through the `workflow_bind_continuity_frame` dispatcher command and the
render request now carries the card's `wait_seconds` value so the dispatched
artifact matches the approved card byte-for-byte. The reviewer confirms spend
fingerprinting, the adapter request construction, the one-use confirm-token
consumption, reconciliation registration, assembly, and classification are all
REAL host code with only the provider HTTP endpoint stubbed on 127.0.0.1, that
the https-only download policy in `atlas.rs` is unchanged, and that the
`assemble.rs` `trim_scene` fix is correct (silent branch reordered to
input-options → input → output-options; has-audio branch untouched; no
behavioral change to trim selection).

Reviewer-noted nuances (recorded here, not to be re-claimed elsewhere):
- inside the render helper the spend-quote hop still calls the production
  `spend::quote_unbounded_scoped` directly rather than via the dispatcher route
  — the same code executes; the routing proof is weaker than the bind hop's;
- OS viewer launch and Finish remain evidenced by pinned routing, the
  `classify_file` host hop inside the chain, and the CDP captures under
  `evidence/2026-10-15-redo/`, not by a headless launch observed this pass;
- the tree churned during the 28-minute review (concurrent close-out edits;
  the reviewer observed one intermediate non-green state), and the verdict
  above reflects the reviewer's FINAL reads plus two final clean green runs.

Reviewer's own executed verification on the final tree (matches the numbers
recorded above): lib 386 passed / 0 failed; 33 integration including
production_chain 2/2; vitest closeout 13 passed.
