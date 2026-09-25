# Production Studio UI Redo Plan

Date: 2026-09-24  
Status: Phase 0 frozen 2026-09-24. Implementation plan for the Studio UI rewrite.  
Scope: `Desktop_App`  
Visual direction: `Dev_Notes/V2/UI_Redo.jpg`  
Builds on:

- `Dev_Notes/V2/Workflow_Migration_Plan.md`
- `Dev_Notes/V2/Traditional_Studio_Production_Model.md`

---

## 1. Decision

The workflow state is mature enough to begin the Studio UI rewrite.

Do not treat this as a CSS pass over the current Production Desk. The current
desk proves the workflow, but it is one large stage-oriented renderer inside
the older explorer/editor/chat shell. The new UI should introduce a stable
presentation layer over the same host-owned project state, then replace the
old desk one working surface at a time.

The first visible slice is the scene workspace from `UI_Redo.jpg`:

```text
project navigation
+ scene list and scene status
+ selected scene workspace
+ approved image and alternates
+ scoped Director
+ jobs
+ finish/readiness summary
```

Before that slice is called complete, close the remaining workflow contracts
that would otherwise make Jobs and Finish display incorrect state.

---

## 2. Current Baseline

Keep and reuse:

- `Preproduction/workflow_state.json` as the shared typed project map.
- The project folder as the source of truth for media and production files.
- Production profiles, requirements, stable scene and shot IDs, recipes,
  animatics, artifact versions, jobs, takes, selected takes, post state, and
  deliverable checks.
- Existing host commands for intent, look generation and approval, cut
  sketches, visual sources, production notes, rendering, take review, and
  assembly preparation.
- Existing Director and specialist execution.
- Existing media viewers, settings, file operations, and voice tools.
- `Lite_Challenge` Scene 1 as the first real UI fixture.

The repository currently builds, and the Rust host test suite passes. The real
Scene 1 fixture has approved planning, nine selected takes, and a continuity
frame linking Cut 8 to Cut 9.

The remaining weaknesses are at the presentation boundary:

- Assembly output is not reconciled into workflow state.
- The Assemble stage cannot become complete.
- A review export cannot be opened directly from the desk.
- Continuity extraction chooses a near-final decoded frame automatically
  instead of allowing a reviewed frame choice.
- Director context carries the selected scene and stage, but not the selected
  stable shot, asset, look slot, or requirement.
- Jobs have no timestamps, elapsed time, or cost metadata.
- The recipe graph describes workflow stages but not UI work-area grouping.
- `workflowDesk.ts` and `main.ts` are too coupled for a safe whole-screen
  restyle.

---

## 3. Product Outcome

The application should open into a production-studio shell rather than an
editor with a production tab embedded inside it.

### Global shell

- A narrow navigation rail for Home, Studio, Files, Assets, and Voices.
- A top bar for project identity, current work area, Jobs, and Settings.
- Studio is the primary production workspace.
- Files and Voices reuse the capabilities already present in the app.
- Assets may begin as a filtered project-media view; it must not introduce a
  second asset registry.

### Studio workspace

- Left: scene list with thumbnail, title, readiness, and warning state.
- Center: the selected scene and selected production artifact.
- Right: scoped Director context and the Finish/readiness surface.
- Jobs remain visible without taking over the asset workspace.
- The current stage rail becomes a compact work-area or detail navigation
  control. It is not the architecture.

### Scene Look first slice

- Show the selected approved look image prominently.
- Show alternate versions beside it.
- Approve, reject, attach, generate, and open an image from the same surface.
- Show the role, title, narrative purpose, approval state, and dependencies.
- Derive the scene summary from state: intent, references, cuts,
  requirements, characters, blockers, and warnings.
- Keep every action wired to an existing host transition.

### Director

- Show the exact context that will be sent with the next turn.
- Context entries are references into shared state, not copied prose.
- Scope can include project, scene, stable shot, look slot, asset version,
  character, requirement, and current work area.
- Chat remains optional assistance. Known transitions remain buttons.

### Finish

- Summarize what is complete, pending, blocked, stale, or awaiting approval.
- Open the latest review export when one exists.
- Expose take selection, cut-version, picture-lock, post, QC, and deliverable
  status without pretending to be an NLE.
- Never show “complete” only because a provider returned a file.

---

## 4. UI Rules

1. The UI reads shared workflow state; it does not create parallel state.
2. Stable IDs are used for selection and action targets. Scene and cut numbers
   are labels.
3. Approval always applies to a concrete artifact version.
4. Provider output is a take until review selects it.
5. Every status shown in the shell must have one host-owned derivation.
6. Recipe data controls stage availability and labels.
7. `Plan`, `Produce`, and `Finish` are UI lenses over recipe stages, not new
   workflow enums.
8. Chat context is visible and inspectable before sending.
9. Jobs link to their input and output artifacts when those records exist.
10. Responsive behavior may collapse panels, but it must not remove actions or
    hide blockers.

---

## 5. Foundation Gate

Complete this gate before calling the new Jobs and Finish panels reliable.

### 5.1 Complete the assembly lifecycle

- Reconcile a successful assemble output on import, as render jobs are
  reconciled today.
- Mark the assembly job and current attempt completed.
- Register the output as an assembly or review-export artifact.
- Associate the output with the scene and cut version.
- Mark the selected takes as used in that assembly without losing their
  selected identity.
- Derive Assemble readiness from a real output or cut version instead of
  hard-coding it to missing.
- Add an Open Review Export action.
- Preserve failed and previous assembly attempts.

Done when:

- A prepared local assembly that writes its output becomes a completed job
  after refresh.
- The Assemble stage becomes present or approved from state.
- The review export opens from Studio.
- Reassembling creates a new version rather than overwriting the previous one.

### 5.2 Make continuity-frame choice reviewable

- Allow a selected predecessor take to be previewed.
- Default the candidate time near the end, but expose a frame-time control.
- Extract the frame at the chosen time.
- Persist `frame_time_seconds`, source take ID, parent asset ID, and artifact
  version.
- Require explicit binding or confirmation after the predecessor selection
  changes.
- Show dependent shots that will become stale before replacing the binding.

Done when:

- The user can choose a usable frame instead of accepting the last decoded
  frame blindly.
- The successor stays render-blocked until that exact frame is bound.
- Changing the predecessor selection does not silently approve a replacement
  continuity frame.

### 5.3 Extend Director scope

Add optional stable identifiers to the UI-to-host context:

- `workflowSceneStableId`
- `workflowShotStableId`
- `workflowAssetId`
- `workflowLookSlotId`
- `workflowRequirementId`
- `workflowWorkArea`

The host resolves these IDs against the current project state before composing
context. The UI may display friendly labels, but it must send stable identity.

Done when:

- The Director panel and the host prompt report the same attached context.
- Reordering cuts does not change the shot attached to Director.

### 5.4 Add minimum job presentation metadata

Add optional fields without blocking older state:

- Created, started, and completed timestamps.
- Operation scope: project, scene, stable shot, asset, or deliverable.
- Progress summary where the provider exposes one.
- Actual or estimated cost and currency when known.
- Retry parent or lineage.

Unknown timing and cost must display as unknown, not zero.

Done when:

- Jobs can truthfully show status and elapsed time.
- A render job links to its shot and take.
- An assembly job links to its review export.

### 5.5 Decide the alternate-angle action

The mockup includes Add Alternate Angle. Before enabling it, add a host
transition that:

- Allocates a stable shot ID.
- Inserts a display label without renumbering identity.
- Creates an editable cut sketch.
- Adds animatic timing.
- Leaves the shot package and source strategy incomplete.
- Does not invent a production-note path.

Until this transition exists, render the control as unavailable with a clear
reason. Do not create UI-only cuts.

Foundation stop line:

> Jobs, continuity, Director context, and Finish can all report truthful,
> durable state for `Lite_Challenge` Scene 1.

---

## 6. Presentation Model

Introduce a pure TypeScript presentation layer between `ProjectState` and DOM
rendering. It owns no durable state.

Suggested modules:

```text
src/studio/
  controller.ts
  selection.ts
  viewModel.ts
  selectors/
    project.ts
    scenes.ts
    workspace.ts
    directorContext.ts
    jobs.ts
    finish.ts
  components/
    StudioShell.ts
    NavigationRail.ts
    StudioTopBar.ts
    SceneList.ts
    SceneWorkspace.ts
    SceneLookBoard.ts
    DirectorPanel.ts
    JobsPanel.ts
    FinishPanel.ts
```

The implementation may use smaller or differently named files, but preserve
these boundaries:

- Controller invokes host commands and refreshes state.
- Selection stores stable IDs and ephemeral UI state.
- Selectors derive view models from state and report data.
- Components render view models and emit user intents.
- Components do not call `invoke` directly.

### Required view models

Project summary:

- Project name and profile.
- Active recipe.
- Current work area.
- Global blockers and pending approvals.

Scene summary:

- Stable ID and display number.
- Title and thumbnail.
- Intent, look, cut, reference, take, and finish counts.
- Characters and requirements scoped to the scene.
- Warning and blocker summaries.

Workspace:

- Selected artifact type and stable ID.
- Primary version and alternates.
- Approval and dependency state.
- Available actions with disabled reasons.

Director context:

- Resolved context entries with stable ID, type, label, and status.
- Current stage/work-area scope.
- Explicit user attachments.

Job summary:

- Kind, status, attempt, scope, timing, cost, output, and error.
- A target action such as Open output, Review take, or Retry.

Finish summary:

- Missing selected takes.
- Latest assembly and cut version.
- Picture-lock and post status.
- QC and deliverable checks.
- Stale or invalid downstream artifacts.

Presentation stop line:

> The target Studio screen can be rendered from deterministic view models
> without querying the DOM or duplicating workflow rules.

---

## 7. Implementation Order

### Phase 0 — Freeze the migration baseline

Status: Done, 2026-09-24.

- The migration checkpoint is `4d7c5f5`. `Workflow_Migration_Plan.md` describes
  the desk that exists, including the gaps that section 5 of this plan still
  has to close.
- Frontend `npm run build` succeeded.
- Host `cargo test` passed: 228 tests, 0 failed.
- `UI_Redo.jpg` stays visual direction. It is not a field-level data contract.
- The current Production Desk stays runnable as the comparison surface.

Done when:

- Workflow and UI work can be reviewed separately. Met by separate commits.
- The old desk remains runnable as a comparison surface. Met. No desk
  renderer was removed.

### Phase 1 — Close the foundation gate

Implement section 5 and add host tests for:

- Completed assembly reconciliation.
- Review-export registration and versioning.
- Assemble readiness.
- Explicit continuity-frame time and rebinding.
- Stable Director selection context.
- Job timing and scope serialization.
- Alternate-shot allocation, if that control will ship enabled.

Do not start visual polish in this phase.

### Phase 2 — Add the Studio shell

- Add the global navigation rail and top bar.
- Make Studio a full editor-body workspace rather than nesting another
  three-column desk inside the editor.
- Preserve Files, media viewing, Settings, and Voices.
- Keep the current Director implementation working while moving its visual
  container.
- Establish responsive panel behavior and keyboard focus order.

Done when:

- Navigation between Studio, Files, Assets, Voices, and Settings preserves the
  open project and current Studio selection.
- No workflow feature has been removed.

### Phase 3 — Ship Scene Look as the first complete workspace

- Implement the scene list and scene header.
- Derive scene thumbnails from approved look assets, then approved cut sources,
  with a neutral fallback.
- Implement the large approved image, alternates, and approval actions.
- Show the slot role, title, prompt, narrative purpose, and approval state.
- Add an alternate-generation tile wired to the existing generation flow.
- Show scene-level status counts and warnings.

Done when:

- All existing Scene Look actions work from the new screen.
- Selecting a scene or alternate uses stable identity.
- The old Scene Look panel is no longer needed for normal use.
- The screen remains usable with no image, one image, many alternates, missing
  files, and failed jobs.

### Phase 4 — Add scoped Director

- Render attached context explicitly.
- Allow context entries to be added or removed without deleting project state.
- Keep the current model picker, stop behavior, approval cards, and specialist
  execution.
- Scope Director to the selected scene, shot, asset, or requirement.
- Refresh Studio after successful Director actions without losing selection or
  scroll position.

Done when:

- The context shown in the panel matches the context sent to the host.
- A generation approval updates the associated job and artifact in Studio.

### Phase 5 — Add Jobs and Finish

- Implement the compact Jobs panel from the shared ledger.
- Add filters for active, failed, awaiting approval, and completed jobs.
- Link job outputs to image, video, audio, or review views.
- Implement Finish from selected takes, cut versions, assembly, post status,
  QC, and deliverables.
- Show stale dependencies and pickup/rerender needs.

Done when:

- The user can move from a failed or completed job to the affected artifact.
- The user can open the latest Scene 1 review export.
- Finish never reports complete while a required cut lacks a selected take.

### Phase 6 — Move the remaining workflow surfaces

Move one surface at a time:

1. Scene Intent
2. Cut Sketches and animatic
3. Reference Images and visual-source binding
4. Production Notes
5. Render queue
6. Take Review
7. Assemble and post
8. Project profile and requirements
9. Characters and voices

For each surface:

- Build a selector and view model.
- Reuse the existing host command.
- Reach action parity.
- Add empty, loading, error, and blocked states.
- Remove the corresponding old renderer only after parity.

Do not copy the old ten-stage rail into the new global navigation.

### Phase 7 — Retire the old desk

- Remove obsolete `workflowDesk.ts` rendering paths.
- Delete unused workflow CSS after visual comparison.
- Keep host commands and wire types only where still used.
- Update README and product notes.
- Make the new Studio surface the default production entry.

Done when:

- No normal production action requires the legacy desk.
- The old renderer and duplicated CSS can be removed without changing host
  behavior.

---

## 8. Testing

### Host tests

Add an end-to-end fixture test that performs the state transitions without
calling a paid provider:

```text
approved plan
-> ready shot specification
-> simulated render output
-> registered unreviewed take
-> selected take
-> reviewed continuity frame
-> dependent render ready
-> selected successor take
-> simulated assembly output
-> completed assembly job
-> review-export artifact
-> cut version
-> deliverable check
```

Also cover:

- Reassembly creates a new version.
- Changing a selected predecessor invalidates, but does not delete, dependent
  work.
- Reordering shots preserves Director context and take ownership.
- Old workflow-state files load with optional new job metadata absent.

### Selector tests

Add TypeScript tests for:

- Scene thumbnails and fallback order.
- Status and warning counts.
- Approved primary image and alternates.
- Director context resolution.
- Job grouping and elapsed-time labels.
- Finish readiness.
- Recipe-to-work-area grouping.

### UI tests

Use a deterministic fixture state and local media:

- Studio opens with the expected selected scene.
- Scene and alternate selection works.
- Approve/reject updates the visible primary.
- Director context follows selection.
- Jobs open the correct output.
- Finish opens the review export.
- Keyboard navigation and focus remain visible.
- Narrow layouts collapse panels without losing actions.

Capture a reference screenshot for the Scene Look fixture after interaction
tests pass. Visual comparison supports the tests; it does not replace them.

### Manual acceptance

Run the new UI against the real `Lite_Challenge` Scene 1:

- Inspect all scene-look versions.
- Review the nine selected takes.
- Confirm the Cut 8 to Cut 9 continuity source.
- Produce and open a versioned review export.
- Confirm no existing planning or render data is rewritten by opening Studio.

---

## 9. Non-Goals

Do not include in this stage:

- A full nonlinear editor.
- New generation providers.
- More look roles or wardrobe pipelines.
- Style-specific workflow branches.
- A new agent per department.
- A third recipe solely to exercise the UI.
- Replacing the project folder or workflow-state file.
- Moving host-owned validation into TypeScript.
- Rewriting working Rust workflow modules for visual consistency.
- Displaying invented cost, duration, readiness, or approval data.

---

## 10. Completion Criteria

This UI stage is complete when:

- The application presents the new Studio shell and Scene Look workspace.
- Scene selection, thumbnails, approved images, and alternates come from shared
  workflow state.
- Director displays and sends the same stable context.
- Jobs represent look, start-frame, render, and assembly work consistently.
- Finish derives from selected takes, review exports, cut versions, post, and
  deliverables.
- `Lite_Challenge` Scene 1 can be reviewed from planning through its review
  export without opening the legacy desk.
- Existing Files, media, Settings, and Voices workflows still work.
- The frontend builds, host tests pass, selector tests pass, and the Studio
  fixture passes UI smoke tests.
- The legacy Production Desk renderer can be removed without losing a
  production action.

The next product stage after this plan is broad fixture validation and UX
refinement, not another workflow-state expansion.
