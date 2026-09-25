# Desktop App Migration Plan

Date: 2026-09-23  
Status: Baseline frozen 2026-09-24. Steps 1–6 are on the desk. Open gaps are in the baseline section below, and they close in the UI redo foundation gate.  
Scope: `Desktop_App`, with `Lite_Challenge` as the narrative proving project  
Governs: `Dev_Notes/V2/Traditional_Studio_Production_Model.md`  
Next: `Dev_Notes/V2/Production_Studio_UI_Redo_Plan.md`

This revision replaces the 2026-09-17 stage list in this file. That list is kept below only as history. Section 5 is the record of the migration. The baseline section is the current status. Do not read earlier “missing” language as still open.

---

## 1. Why This Revision Exists

The September 17 plan fixed a real failure: Scene Plan wrote canonical production notes before a scene had intent, a look, or cut-specific images, so cinematic-balanced requests collapsed into `source_video_lipsync`.

The desk followed that plan. By 2026-09-23 it had typed workflow state, an importer, a stage rail, scene intent, cut sketches, strategy mix checks, a scene look board, wardrobe plates, and cut start frames.

At that date the work had stopped at the planning half of one narrative recipe. The production model, written after that plan, is the product direction. It treats the linear stage list as a useful backbone for narrative dialogue, not as the Workbench kernel.

Continuing to add look roles, plate types, character editors, or closed stage enums is the drift. The job this revision set was to finish one narrative production on the desk, then widen the state model in the order the production model already sets. That widening is the frozen baseline below.

Target, unchanged:

```text
one authoritative project
+ visible production artifacts
+ scoped creative agents
+ host-owned mechanics
+ human approval at meaningful gates
```

---

## 2. Baseline — 2026-09-24

Checked before the Studio UI rewrite. The current Production Desk stays runnable as the comparison surface.

Verification:

- Frontend `npm run build` (`tsc && vite build`) succeeded. Vite 6.4.3 transformed 15 modules and wrote `dist/` in 373ms.
- Host `cargo test --manifest-path Desktop_App/host/Cargo.toml` passed: 228 tests, 0 failed (211 library, 10 `scene_plan_commit_flow`, 7 `workflow_import`). Existing unused-assignment warnings in `luma.rs`, `replicate.rs`, `runway.rs`, and an unused import in `fs_ops.rs` remain.
- `Lite_Challenge` Scene 1 is the fixture: approved planning, nine selected takes, and a continuity frame from Cut 8 to Cut 9.
- `Dev_Notes/V2/UI_Redo.jpg` is visual direction for the UI stage. It is not a field-level data contract.

Mapped to the production model’s lifecycle:

| Production-model phase | On the desk at this checkpoint |
| --- | --- |
| Development and greenlight | Present. The production profile stores recipe, deliverables, audience, platforms, runtime, aspect ratios, resolution, frame rate, language, medium, style notes, audio plan, continuity rules, safety constraints, budget, schedule, and rights notes. Projects still open as a folder plus `Preproduction/workflow_state.json`. |
| Script and breakdown | Present. Script import, per-scene beats, and a requirements registry. Characters are one requirement kind. Scene 1 registers cast, the train interior, the parcel, and voices with stable IDs. |
| Casting and production design | Present. Character profiles, identity refs, voices, scene look slots, wardrobe plates, and artifact versions. Approving a replacement keeps the previous version. |
| Storyboard, previz, animatic | Present. Ordered shot timing, scratch audio, coverage warnings, and animatic approval. A timing change marks later shot packages for review and keeps existing renders. |
| Shot packages | Present. Approved sketches and visual-source strategies compile to canonical shot specifications through host derivation. A cut can declare a predecessor without a concrete path. Stable scene and shot IDs survive reorder. |
| Production / shoot | Present. A ready cut can be sent to Scene Generation or Text-Sync. The result is a job and an unreviewed take. |
| Dailies and take review | Present. Take Review can mark selected, alternate, rejected, needs edit, needs rerender, or used in assembly. |
| Editorial and post | Partial. Assemble prepares a versioned master from selected takes and records an assembly job. Cut versions, picture lock, post status, editorial path, and deliverable checks exist. A successful assemble output is not reconciled back into workflow state, so Assemble cannot become complete from that output, and the review export cannot be opened from the desk. |
| QC, delivery, archive | Partial. A deliverable check compares a cut version against resolution, aspect, and missing shots. How strict picture lock and QC should be is still the open question in section 9. |

Stable IDs are durable identity. Scene and cut numbers are labels.

`WorkflowStage` is still the closed ten-value enum (`script` through `assemble`). Recipes select and relabel those stages. They do not add variants. Montage skips characters and mouth policy. The recipe graph describes stages, not UI work-area grouping.

Chat specialists remain the execution layer. The desk and chat share one job ledger and one selected-take record.

Still open, and owned by the UI redo foundation gate rather than a new migration step:

- Reconcile a successful assembly into a completed job, a review-export artifact, and Assemble readiness.
- Open that review export from the desk.
- Let the user choose a continuity frame time. Extraction still picks a near-final decoded frame automatically.
- Carry the selected stable shot, asset, look slot, or requirement in Director context. Context today is the selected scene and stage.
- Add job timestamps, elapsed time, and cost. Unknown timing and cost must not become zero.
- Keep `workflowDesk.ts` and `main.ts` coupled until the presentation layer replaces the desk one surface at a time.

---

## 3. What Stays

Keep, and do not rebuild:

- Project folder as source of truth, with `Preproduction/workflow_state.json` as the typed map.
- Import of existing `Scripts/`, `Characters/`, `prod.md`, and `Scene_Prompts/` without forcing a folder rename.
- Scene intent, cut sketches, shot roles, mouth policies, and host-side cinematic-balanced / performance-heavy / atmosphere-heavy checks.
- Scene look roles and the rule that an approved visual-source strategy determines the video source. When the strategy is a preproduction still, the approved cut image—not a character model-sheet closeup—is the source.
- Wardrobe plates as the scene-specific identity input for later start frames. Do not add more plate pipelines.
- Host-owned paths, IDs, schemas, provider calls, and approvals. Agents draft; the host commits.
- `source_video_lipsync` only for deliberate performance cuts.
- Existing chat specialists as the execution layer until the desk calls them and records the result. Do not replace them with new department agents.
- `Lite_Challenge` Scene 1 as the narrative proving scene.

The old opt-out modes (full pre-production, fast draft, direct production notes, manual assets) stay available as narrative shortcuts. Direct production notes remains a labeled shortcut. It is not the default, and it is not a second product.

---

## 4. What to Stop

These limits still hold:

- Add `WorkflowStage` variants, recipe enums, or style flags (`is_claymation`, `is_anime`, `is_live_action`).
- Add look roles, wardrobe-plate variants, or another character-generation pipeline.
- Grow the character editor, voice tab, or look board into a department UI.
- Write a new Scene Plan prompt to paper over take review, selection, or identity. Those are host state.
- Treat a successful provider call as a completed shot.
- Require every concrete start image to exist before production notes. Some sources can only be derived from selected takes during production.
- Reopen storyboard, animatic, production-profile, or recipe work as new pipelines. Steps 2–5 are in the baseline.
- Force non-dialogue projects through scene, dialogue, mouth, and character primitives. Those stay inside the narrative recipe.

---

## 5. Implementation Order

This is the production model’s order. The status line on each step is the 2026-09-24 reading. The paragraphs under it are the original work order.

### Step 1 — Finish the narrative vertical slice

Status: Done for planning, render, take review, selected takes, and the Cut 8 to Cut 9 continuity frame on `Lite_Challenge` Scene 1. Still open: a reviewed frame time, reconciling a successful assemble into workflow state, and opening the review export from the desk.

Prove the desk can complete a production, not only plan one.

Chain:

```text
approved intent + look + sketches + visual-source strategies
-> canonical shot specifications with declared dependencies
-> render currently ready shots
-> take registration
-> take review
-> selected take
-> exit-frame selection / continuity-frame binding
-> dependent shots become render ready
-> assembly
-> review export
```

Use `Lite_Challenge` Scene 1. Use the artifacts that already exist. Do not insert a new stage.

Work:

- Give each cut an approved visual-source strategy: approved preproduction start frame, previous selected take’s exit frame, shared reference, no start frame, or deferred manual source.
- Let a cut declare a predecessor dependency without inventing a path for an artifact that does not exist yet. A declared dependency counts as planned; it does not count as render ready.
- Generate the canonical shot specification from the approved cut sketch plus its source strategy, through the existing host derivation in `scene_plan` (IDs, aspect ratio, lane, timing, provider). The model fills creative fields only. Reaction and insert cuts do not become `source_video_lipsync`.
- Keep the shot specification provider-neutral. Compile the concrete provider request immediately before execution, after every required source has been bound.
- Write `prod.md` and sidecars from that typed state so the production-note mix matches the approved sketch mix.
- From the desk, send one approved and currently ready cut to the existing Scene Generation or Text-Sync specialist.
- Record the provider call on the cut as a job and a take. A returned file is a take.
- On Take Review, play the take and set `selected`, `alternate`, `rejected`, `needs_edit`, or `needs_rerender`.
- For a continuity-linked successor, let the user select a usable exit frame from the selected predecessor take. Default to the last usable frame, not blindly to the last decoded frame. Extract and register that frame as the successor’s concrete continuity asset.
- Unlock the dependent cut only after its continuity asset is bound. If the predecessor selection changes during this slice, require the successor source to be rebound before another render. Step 4 generalizes this into versioned stale-dependency propagation.
- Assemble the scene from selected takes with the existing ffmpeg assemble path. The host reads selection and order from workflow state. The model does not reconstruct the scene list.
- Write a review export the user can open from the desk.

Done when:

- Scene 1 can move from approved sketches and source strategies to a review export without a chat detour that bypasses workflow state.
- A cut can be production-note ready while its declared continuity source is unresolved, but it cannot render until that source is bound.
- Selecting Cut 1’s take and exit frame can unlock a continuity-linked Cut 2.
- The jobs drawer shows that render, not only look and start-frame stills.
- Take Review is the place a take becomes selected.
- Assemble uses `selected_take`, not a hand-written `Final/final_prompt.txt` scene list, for this slice.

Numeric scene and cut IDs are acceptable for this step only. Do not migrate them here.

### Step 2 — Production profile and script breakdown

Status: Done. Opening a project shows the recipe, deliverables, and requirement list. Registry records use stable IDs.

Make project-wide constraints explicit, and stop treating characters as the only requirement type.

Production profile, stored with the project, not as a style branch:

- Recipe (narrative dialogue is the only built-in recipe at this step).
- Deliverables, audience, platforms.
- Runtime, aspect ratios, resolution, frame rate, language.
- Medium and style notes, including clay or any other look as profile values.
- Audio plan, continuity rules, safety constraints, budget, schedule, rights notes.

Breakdown:

- A typed requirements registry for the script: cast, locations, props, wardrobe, sound, graphics, risky shots, and deferred items.
- Characters remain one requirement. A scene may be blocked on a location, prop, voice, or plate instead.

Done when:

- Opening a project shows recipe, deliverable, and the requirement list.
- Scene 1’s characters, train interior, parcel, and voices are registry entries with status, not only folders the importer happened to find.

New registry records get stable IDs immediately. Do not key them by display order.

### Step 3 — Storyboard and animatic state

Status: Done. Scene 1 can be reviewed as a timed rough cut. A timing change marks later shot packages for review without deleting renders.

Add the missing preproduction proof: the sequence works roughly before more final generation.

- Ordered shot timing on top of cut sketches.
- Scratch or temporary audio when the scene has dialogue.
- Coverage warnings: missing reactions, inserts, establishing shots, unplanned beats.
- A rough review the user can approve before treating shot packages as ready to shoot.

Cut sketches stay the shot cards. The animatic is their timing and order, not a second copy of the prompts.

Done when:

- Scene 1 can be reviewed as a timed rough cut, and a timing change marks later shot packages for review without deleting renders.

### Step 4 — Stabilize IDs, versions, artifacts, and jobs

Status: Done for stable IDs, artifact versions, the shared job ledger, and stale-dependency marking. Jobs still have no timestamps, elapsed time, or cost.

Do this before recipe graphs or post-production state. Otherwise every new stage deepens the current coupling.

- Stable IDs for sources, sequences, scenes, shots, characters, assets, versions, jobs, attempts, takes, edits, and deliverables. Scene and cut numbers become labels.
- An approval points at an artifact version. Replacing an approved still or take creates a new version.
- One job ledger for look generation, start frames, video, audio, and local ffmpeg. Same job, attempt, error, and output registration the desk already started for stills.
- Provider-neutral shot specs stay separate from provider request bodies. Adapters compile late.
- Shot dependencies and source bindings use stable IDs. The symbolic “previous selected take” relation resolves to an exact take and continuity-frame artifact version at execution time.
- Changing an approved upstream artifact marks dependents stale, needs review, invalid, or safe to keep. It does not delete downstream files.

Done when:

- Reordering Scene 1’s cuts does not change their IDs.
- Approving a new start frame does not overwrite the previously approved version in place.
- A render and a look generation show up as the same kind of job record.

### Step 5 — Recipe-defined stage graphs

Status: Done in the host. Narrative dialogue and montage are recipe graphs, and montage skips characters and lip-sync without a dead stage. The graph does not group stages into UI work areas. No second fixture production was added.

The narrative dialogue path remains the first recipe. The kernel reads stages, dependencies, required roles, and readiness from the recipe.

Prove one non-dialogue recipe before calling the abstraction done. The production model’s montage or music-video path is the smallest contrast: no character registry and no mouth policy required.

Do not add a stage enum or a UI switch per recipe.

Done when:

- Narrative dialogue still runs the slice from step 1.
- A second fixture can skip characters and lip-sync without dead stages or special-case branches in the host.

### Step 6 — Post-production and delivery state

Status: The records exist: cut versions, picture lock, audio, VFX, captions, QC, and deliverable checks. A successful assembly is not yet written back as a completed job and review-export artifact, so Assemble cannot become complete from output alone.

Represent, without building an NLE:

- Cut versions from selects through picture lock.
- Audio mix status.
- VFX and compositing status.
- Captions, mastering, QC, and named deliverables.

Straightforward ffmpeg stays in the host. Taste-heavy or frame-specific editorial work opens an external NLE, and the project stores that decision.

Done when:

- A selected-take assembly can be marked as a cut version, and a deliverable can be checked against resolution, aspect, and missing shots.

### Step 7 — Fixture productions

Status: Not started. This stays after the UI redo. The next product stage is the Studio shell, not another workflow-state expansion.

Only after steps 1–6. Use fixtures to test breadth, not to invent features during the slice:

- Clay dialogue comedy (style lives in the production profile).
- No-dialogue atmospheric montage.
- Presenter explainer.
- Product advertisement with no character registry.
- Documentary from existing footage and voice-over.
- Abstract animation driven by music.

---

## 6. Desk Shape

The narrative slice is closed except for the gaps in the baseline section. This stage rail stays on screen as the comparison surface for the UI rewrite. It is a narrative view, not the architecture.

```text
Script
Characters
Scene Intent
Scene Look
Cut Sketches
Reference Images
Production Notes
Render
Take Review
Assemble
```

These are views over shared production state, not a one-way waterfall. A continuity-linked cut legitimately crosses them:

```text
Reference Images: source strategy approved; predecessor frame pending
Production Notes: shot specification approved; concrete source unresolved
Render: blocked until predecessor selection
Take Review: predecessor take and exit frame selected
Render: continuity source bound; dependent cut ready
```

For step 1, Production Notes, Render, Take Review, and Assemble become working panels. Each Cut Sketch card also embeds that cut’s visual-source strategy, start-frame prompt, image versions, and approval actions, so planning and source work do not require tab switching. Reference Images remains as a filtered scene-wide review queue and can show a correctly planned but unresolved continuity source.

Chat stays a drawer scoped to the selected stage, scene, cut, or asset. Buttons own the known transitions: draft intent, draft sketches, choose a source strategy, generate a preproduction start frame, bind a production-derived continuity frame, write a shot specification, render, mark take, assemble.

The production desk should be able to answer, for the open scene:

- What is approved?
- What is blocked?
- Which take is selected?
- What has been spent?
- What would go stale if this still changes?

---

## 7. Asset and Take Rules That Already Apply

These rules are in force. Step 1 must not weaken them.

Identity assets establish who someone is. Scene look assets establish the world. A cut’s source strategy establishes how its opening image will be obtained. A concrete cut asset may exist in preproduction or may be derived later from a selected take. Render outputs are takes until review says otherwise.

A character being lip-sync ready is not a reason to shoot them speaking.

Take statuses already in state:

- `selected`
- `alternate`
- `rejected`
- `needs_edit`
- `needs_rerender`
- `used_in_assembly`

Step 1 uses those values. It does not invent a parallel status in chat or in `prod.md`.

---

## 8. Testing

Step 1 tests, before any new stage:

- Handoff generation uses the approved visual-source strategy.
- A direct-start-frame cut binds its approved preproduction still.
- A continuity-linked cut preserves a symbolic predecessor dependency in its shot specification and is blocked from rendering while that dependency is unresolved.
- Selecting a predecessor take and exit frame registers a continuity-frame asset and makes the dependent cut render ready.
- Replacing the predecessor selection requires the dependent source to be rebound before another render.
- A character model sheet is not the video source unless it was explicitly approved as that cut’s source.
- A reaction or insert sketch does not become `source_video_lipsync`.
- The host derives timing, lane, paths, refs, provider, and IDs.
- A completed provider call creates a take and a job, and leaves the cut unselected until review.
- Assemble order follows selected takes.

Keep the existing importer and strategy tests. They guard the planning half that is already built.

Later steps add their own tests. Do not block step 1 on recipe or ID-migration coverage.

---

## 9. Decisions

Settled by the production model:

- One shared project state. Departments are responsibilities, not separate agent memories.
- Clay and other looks are production-profile values.
- Display numbers are labels. Stable IDs are required before new stages accumulate, and are introduced in step 4. Records created in steps 2 and 3 use stable IDs from the start.
- Workflow state stays in `Preproduction/workflow_state.json` until step 4. Do not split per-scene files during the slice.
- Scene intent and cut sketches stay editable as forms that write typed state.
- Cinematic-balanced validation stays in the host: warn on a lip-sync-heavy mix, reject only the obvious all-performance case, and keep an explicit override.
- Generated scene references are approved before they are treated as canonical. Cut sketches may be drafted earlier. Production notes require an approved visual-source strategy, not necessarily an already-existing concrete image.
- Start frames are generated for cuts that need an independent still source, not blindly for every cut. Continuity-linked cuts declare their predecessor during preproduction and receive their concrete frame after a predecessor take is selected.
- A shot specification may be approved with an unresolved production-derived source. A provider request may not execute until that source is bound to an exact artifact.
- The jobs drawer is the job history UI. It does not become an asset manager.
- External NLE remains the escape hatch.

Still open, and not required to start step 1:

- How strict picture-lock and QC need to be in step 6.
- Which non-dialogue recipe is the second fixture in step 5.
- Whether scratch dialogue for the animatic is recorded, generated, or both.

---

## 10. Near-Term Recommendation

Stop extending workflow state.

The chain below is on the desk through selected takes and a bound continuity frame. Assembly reconciliation, a reviewed frame time, and opening the review export are the remaining gaps, and they belong to the UI redo foundation gate:

```text
approved sketches + approved source strategies
-> canonical shot specifications
-> render ready predecessor
-> register and review takes
-> selected take + selected exit frame
-> bind continuity frame and unlock dependent cut
-> render dependent cut
-> scene assembly
-> review export
```

Next implementation target: `Dev_Notes/V2/Production_Studio_UI_Redo_Plan.md`, starting with the foundation gate after this baseline.

---

## Appendix: September 17 Plan

The previous plan’s diagnosis still holds. Its stage sequence is what the desk implemented through start frames, and what this revision stops extending.

It specified ten stages: Script, Characters, Scene Intent, Scene Look, Cut Sketches, Reference Images, Production Notes, Render, Take Review, Assemble. Phases 0–5 of that plan are in the app (state, importer, stage UI, intent, sketches, strategy checks, look board, start frames). Phases 6–8 (handoffs from approved sketches, desk render and take review, assemble from selected takes) were step 1 above. Render, take review, and selected-take assembly preparation are now on the desk. Reconciling the assemble output and opening the review export are still open, and they are listed in the baseline section.

The previous plan’s lip-sync rule, asset roles, host and model split, and `Lite_Challenge` importer remain in force. Its assumption that the ten-stage rail is the V2 product does not.
