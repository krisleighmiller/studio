# Studio — Usable Filmmaking Recovery Plan

**Created:** 2026-10-24\
**Status:** Two narrow renderer repairs are independently reviewed, integrated and freshly verified: explicit-send Stop during pre-request paint, and best-effort Explorer refresh preserving continuation. The broader approval-handoff review still requests changes: real host cancellation is now behaviorally reproduced as ineffective; streamed-progress chat ownership and host yield-context retention remain open. Connected-app tracer attempts failed and are not acceptance evidence. Earlier media-inspection/custom-trim reviews remain separate evidence; the filmmaking milestone is not accepted.\
**Scope:** Restore assistant-coordinated filmmaking through existing Studio capabilities, starting with a protected Black_Hole derivative. Not a rewrite or another general platform build.

## 1. The outcome

Kris must be able to take a project like `/home/kris/Content_Creation/Projects/Black_Hole` from script through generation, review, revision and assembly without manually maintaining each prompt's character descriptions, reference paths, continuity instructions or timing calculations.

Kris directs the film, judges the creative result and authorizes spending. Studio carries the production context and operates the tools. Generating elsewhere and importing is a fallback, not fulfillment of this goal.

**First usable milestone:** one dialogue scene can be prepared, produced, reviewed, revised and exported through the app without Kris repairing its prompt or performing the connective handoffs. This milestone is deliberately smaller than a finished film; it must not be presented as end-to-end completion.

## 2. Keep the architecture; correct the division of work

- The Director interprets direction, gathers relevant context, prepares coherent requests, coordinates operations and explains actual results.
- Existing host code owns state, permissions, path confinement, approvals, provider execution, measurements and deterministic validation.
- The UI and Director operate the same validated capabilities. A button is not a reason to exclude an operation from the Director; Director access is not permission to bypass the host.
- Production state remains authoritative. Chat is not a parallel database.
- Deterministic QC measures file properties; human review judges performance, identity, continuity and comic timing. Model-assisted review may inform the discussion but must not replace host validation or human approval.
- Unknown duration and cost remain unknown. An estimated speech duration must not be relabeled as measured.

## 3. Boundaries against another expansion cycle

Pause unrelated feature expansion for this recovery slice. Preserve existing security and operation-approval protections. Project budgeting is optional: no-budget/testing work must not require provider prices or a verified charge ceiling. Keep unknown charges unknown rather than free; only enforce a numeric hard stop when the user deliberately configures one. Do not reactivate deferred advanced governance work as a prerequisite unless a concrete safety defect blocks this path.

Do not introduce a new agent framework, workflow builder, provider abstraction, project database or editor. First reuse the Director loop, specialists, host commands, provider adapters, media utilities and existing tests. Adopt a dependency only after identifying a specific missing capability, comparing the current implementation and documenting why the dependency is smaller and safer than the repair.

Do not make arbitrary shell execution or generic filesystem writes available to the model. Expose narrowly scoped existing operations as needed.

**Git baseline inspected:** branch `v1-baseline-phase8-deferred`, HEAD `6ab6d9a8a5f8a6cb28d55a5b9726f268072f0fa0`; working tree was clean before these documentation changes. Preserve this baseline. Do not switch branches, commit, merge or push under this plan without authorization.

**Project protection:** no changes to original Black_Hole files or outputs. Before runtime work, create a separate derivative, record copied inputs and remap or reject absolute paths that still point into the original. Writes and outputs must remain inside the derivative. Do not blindly copy a large project tree or its secrets.

**Cost protection:** source inspection and local tests first. Existing generated media can prove local coordination but cannot prove fresh generation. Real LLM or generation calls require an agreed live-test scope and explicit authorization; existing app approval gates remain mandatory.

## 4. Initial findings — source evidence, not runtime proof

| Observation | Evidence | Consequence |
|---|---|---|
| A bounded Director action loop already exists; inspection results are fed back to the planner. | `Desktop_App/host/src/director.rs:972–1073,1196–1215` | Do not build another agent engine before testing this one. |
| A successful specialist invocation ends the current host turn and returns its message/continuation flag. | `Desktop_App/host/src/director.rs:1173–1185` | Trace continuation across the host/frontend boundary; do not assume specialist success means the whole user request is complete. |
| The approval endpoint forwards the specialist's continuation flag. The approval UI handler receives that flag but does not act on it. Ordinary chat has separate bounded continuation logic. | `Desktop_App/host/src/director.rs:1500–1514`; `Desktop_App/src/main.ts:3313–3364,3509–3525` | First repair candidate: continuation after approval. Prove the correct behavior with regression tests before changing it. |
| Production Helper reads a document dossier and returns advice or a digest. | `Desktop_App/host/src/specialists.rs:1194–1432` | It is not itself evidence of a complete production coordinator. Reuse useful context gathering, not a new advisory layer. |
| An existing connected production-chain integration test exercises real host commands with a mocked provider boundary. | `Desktop_App/host/tests/production_chain.rs:1–36` | Extend existing coverage rather than inventing a second harness. The test does not establish Director-driven coordination or live provider success; its media-delivery substitution is documented. |
| Black_Hole repeats character blocks and reference paths in generation handoffs, and records manually reconciled speech/trim timing. | `Projects/Black_Hole/prod.md:8–16,58–64,107–113,410–419` | These are concrete manual tasks the recovery must remove. |

These observations do not establish when the product drift began or prove that one continuation fix will solve it. Tests have been read, not executed during this planning pass.

## 5. Execution order

### A. Trace one dialogue scene — finish the capability map

Start with the Mom/Sam car exchange: production shots 1–2, corresponding to script Scene 1. Preserve the distinction between script scenes and production shots; do not infer identity from display numbering.

Trace each operation through Director action, validation, approval, specialist/host command, returned result and visible state:

1. Read dialogue, approved character references and scene context.
2. Prepare a generation request without Kris copying those facts into each prompt.
3. Obtain or reuse dialogue audio; measure actual media duration.
4. Reconcile speech, generation duration, head/tail padding and assembly trims against provider capabilities.
5. Present the exact paid request for approval, execute only with valid authorization and register the result.
6. Return the result to the coordinator without losing the original request or requiring a new instruction at every hop.
7. Review/select a take, assemble and export.
8. Apply a requested revision and invalidate/rebuild only affected outputs.

Record each gap as **already works**, **exists but inaccessible**, **result/context handoff missing**, **genuinely missing**, or **not yet tested**, with source locations and a reproduction. Do not create speculative tickets for the rest of the application.

**Exit:** a short operation-to-capability map and the first reproducible break. Initial source findings above are a starting point, not this exit condition.

### B. First bounded repair — preserve continuation across approval

First reproduce the observed approval/continuation discrepancy using the existing host and frontend test facilities. Include a case whose original user request requires another safe step after an approved action.

Required behavior:

- The completed action's real result is available to the next planning step with the original task and current project scope intact.
- A pending approval, failed action, queued/running job, completed user request, stop request or exhausted limit must not be mistaken for permission to proceed or rerun work.
- Continuing must not approve the next paid action, reuse an approval for a changed request, repeat a provider submission, or overwrite a file silently.
- Refresh/reload and project/scene changes must not resume against a different target. Reuse durable job/recovery state where it exists; do not invent a parallel task store.
- Preserve bounded continuation and visible pauses; reuse the existing continuation path where safe rather than adding another recursive loop.

Write failing regression coverage first, make the smallest repair, then rerun the focused tests and relevant approval/stop/security tests. If reproduction shows that the continuation flag is insufficient or semantically wrong, correct the result contract minimally rather than turning it on everywhere.

**Exit:** an exercised multi-step request resumes after approval when appropriate, stops when required, and cannot cause a duplicate paid action. This is a handoff repair, not a claim that timing or continuity is solved.

### C. Complete one scene and a real revision

Repair the next demonstrated gap from A, one at a time. Prefer calling an existing host operation or returning a better result over adding a new subsystem. Every change needs a reproducing test and a visible workflow check.

Use an explicit revision such as: “Tighten the pause before Sam's response without clipping his dialogue.” The app must inspect available timing information and apply an achievable change, or explain the missing measurement and obtain it. It must not invent word-level timestamps from total duration alone or require Kris to calculate a trim. Reuse the existing media tools; a genuinely missing analysis capability must be scoped before adding it.

For the no-cost pass, use copied takes/audio and a clearly labeled provider fixture where needed. Then, with explicit authorization, test fresh request preparation and live generation. Preserve the prepared request and recovery information when a provider fails. Distinguish model noncompliance from an incorrect Studio request.

**Exit:** a playable scene export plus its revised export, visible in the actual app, with reproducible request/reference/timing evidence and no manual prompt repair. Local replay and live generation must have separate results.

### D. Extend across Black_Hole, not across a new platform

Only after C passes, exercise location changes, carried props, dependent references, replacement takes and final assembly across the film. Check intentional creative changes rather than mechanically enforcing identical environments in every shot.

The finished-project check must include script-to-production preparation; reusing already hand-authored production prompts is useful for regression but does not prove that Studio can prepare them. Kris must not have to reconstruct the original manual handoffs to begin a new project.

**Exit:** the full project can be prepared, produced, revised and assembled through Studio with creative review and spend approval, but without manual continuity/prompt/timing bookkeeping.

## 6. Verification and evidence

For each repair retain: the request, relevant input IDs/paths, prepared provider parameters and reference bindings, measured versus estimated timing, approval/job events, outputs and failure/recovery result. Keep secrets out of logs. Add evidence links here as they are produced.

Use three distinct proof levels:

1. **Automated:** focused regression tests plus affected host/frontend/security suites. Test cancellation, pending approval, invalid scope, provider failure, bounded continuation and no duplicate submission.
2. **Actual app:** exercise the same scenario through the UI and Director, including approval, review and the revision. A dispatcher-only test cannot satisfy this level.
3. **Media:** probe the exported file, open it for playback and review that speech is preserved and the intended edit occurred. Existence of an MP4 or a successful job is not sufficient.

For source changes, run `cargo fmt --check`, `cargo build`, `cargo build --tests` and `cargo test` from `Desktop_App/host`; run `npx tsc -p tsconfig.json` and `npm test` from `Desktop_App`. Record actual commands, failures and environment limits. Start with focused tests; do not claim full-suite success until executed.

## 7. Stop rules and completion

Stop and rescope if the proposed repair requires a parallel state system, weakens a safety boundary, silently changes project semantics or turns into a broad framework migration. A missing provider capability is a visible limitation, not permission to fake a successful result.

The scene milestone is complete only when Kris can direct, approve, review and revise it without becoming the prompt engineer or production courier. The recovery is complete only after the whole-project check in D. Broader release/security requirements in `Get_Studio_To_Production.md` remain separate; passing this slice must not be labeled a complete production-readiness audit.

### Current checkpoint

- [x] Preserve and record the inspected baseline.
- [x] Record the recovery plan and initial code-backed findings.
- [ ] Finish the one-scene capability map and reproduce the first break.
- [ ] Repair and verify approval/continuation behavior.
- [ ] Complete local scene production and revision through the app.
- [ ] Complete an explicitly authorized live-generation check.
- [ ] Complete the Black_Hole whole-project workflow.

**Current evidence:** [Director approval handoff — reproduction and repair evidence](Studio_Approval_Handoff_Reproduction.md). The original RED regression now passes: an approved completed-action fixture reaches another planning turn with original task/result history and scene scope. Pending/spend/failure/Stop cases remain paused. All 16 focused renderer tests and the full frontend suites (19 Electron and 144 Vitest), typecheck and frontend build pass after correcting the host-event mismatch and project-transition race found in review. The host builds and all 426 Rust tests passed before those renderer-only corrections; host source/tests have not changed since that run. Repository-wide formatting still fails in unchanged baseline files; the added host test passes its formatting check. These are separate renderer-boundary and host tests, not live app/model proof. The one-scene capability map remains incomplete.

**Optional-budget repair (automated evidence):** Removed the unconditional unknown-cost chat block, exercised all five chat protocol transports and a Director turn against local HTTP fixtures, and added a first-class **No budget / testing** action saving `budget: null`. No-budget/blank-limit generation and ElevenLabs requests retain exact-request approvals and duplicate protection without requiring a price or verified ceiling; explicit numeric strict limits retain their existing restrictions. Unknown costs stay unknown, with truthful budget-summary and approval disclosures. Verification: `cargo test --offline --manifest-path host/Cargo.toml` passed 429 tests; debug host/build-tests passed; `npm test` passed 19 Electron plus 161 Vitest tests; typecheck, frontend production build, changed-Rust-file formatting and `git diff --check` passed. Repository-wide `cargo fmt --check` still fails in unchanged baseline files (`atlas.rs`, `lib.rs`, `workflow/import.rs`, `workflow/look.rs`). No live provider calls or original project changes; actual app/provider acceptance remains unverified.

**Assembly fade revision repair (automated evidence):** Assembly now accepts inline boolean `fade_in` / `fade_out` overrides; omitted inputs preserve the first/last selected takes' production-note settings. Explicit inputs override optional map values. These control only host-added 0.4s picture/audio edge fades, not baked-in fades or transitions between scenes. Director guidance now makes the map optional and explains the inline revision; the approval plan always shows both effective edge-fade values. No generic file-writing capability was added. The new `host/tests/director_assemble_fades.rs` drives the real Director HTTP/validation/one-time-approval/render path with a loopback planner fixture and real ffmpeg media: baseline white-frame brightness starts dark, disabling both keeps both edges bright, and opening-only disable preserves the closing fade. It also verifies no rendering before approval, replay rejection, and unchanged production/source bytes. `cargo test --offline` passed 450 Rust tests; debug host and test builds, changed-file Rust formatting, typecheck, frontend build, 19 Electron and 162 Vitest tests passed. Repository-wide formatting still fails in the previously identified unchanged baseline files. No live provider calls, real-project edits, or live UI acceptance are claimed.

**Media inspection and custom-trim repair (automated evidence):** The bounded, provider-free media inspector reports measured source-relative acoustic activity, not verified speech boundaries. Detector results now come from metadata-cleared machine stdout rather than attacker-controlled ffmpeg stderr. Independent re-review passed 15 module tests, the Director integration test, injected audio/video container metadata, EOF silence, shifted timestamps, process/output bounds and playlist refusal. Parent verified reviewed source hashes against the current checkout. Custom trims preserve inline/YAML start/duration fields and reject windows outside the separate audio actually used by the renderer before approval; independent review passed the original short-WAV reproduction and a 96-case validation matrix. Final full verification: 476 Rust tests, 19 Electron tests and 162 Vitest tests passed; debug host/test builds, frontend typecheck/build, focused Rust formatting and `git diff --check` passed. Repository-wide formatting retains the previously recorded baseline failures. No live provider calls, original Black_Hole edits, commits or branch changes; live app/Director execution and listening acceptance remain unverified.

**October 25 unattended verification:** [Durable evidence and runnable host Stop reproducer](evidence/2026-10-25-unattended/README.md). Parent witnessed two behavioral RED failures, then 25 focused renderer tests passed after the narrow explicit-send Stop / ordinary Explorer-refresh repairs. Independent GPT/Codex review accepted only those two fixes; they are integrated into the existing dirty checkout. Fresh integrated verification passed 19 Electron + 164 Vitest tests, frontend typecheck/build and `git diff --check`. Earlier in this run the unchanged host passed 476 Rust tests and its test build. A separate public-dispatch, shared-context, loopback-barrier test now proves host Stop remains ineffective: inspection and a second planner call occur after cancellation; parent replay and the portable disposable-source reproducer both exit 101 at the behavioral assertion. That intentionally failing test is retained outside the default host suite. Progress-chat ownership and genuine host-yield observations remain open. Connected tracer attempts failed; no valid UI/export/revision acceptance or listening result is claimed. Scoped workers and their owned runtime residue were stopped; no live Studio providers, original-project edits, commits or branch changes.

**Next work item:** Resume from the durable evidence README rather than repeat failed broad tracer attempts. Repair the proven host cancellation boundary with the reviewed admission/action/approval contract, then streamed-progress ownership and genuine yield-context retention, each with witnessed RED→GREEN. Establish a minimal correct Electron-inspector/UI tracer before expanding it. The connected opening-fade revision remains the acceptance scenario: same selected takes, no map/prod.md edit, fresh approval showing `edge fades: in=false` and the preserved closing setting. Keep the scene milestone unchecked. A new live-provider test scope and Kris's listening/creative acceptance remain separate gates.
