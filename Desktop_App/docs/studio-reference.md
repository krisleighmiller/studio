# Studio: product and code reference

**Source baseline:** `e617ffe` (November 12, 2026), plus the documentation cleanup that introduces this reference. This describes the source, not the version loaded in a running window. It is not a release certification, execution plan, or list of approved future work.

## What Studio is for

Studio is a human-directed video production workspace. Scripts, characters, voices, reference images, shots, generated takes, revisions and assembly belong to one project instead of being passed manually between disconnected tools.

The reason for keeping these together is **revision**: changing dialogue, an image, a selected take or timing should not require reconstructing all the context or losing the original work. Generated output is candidate material, not an automatic creative decision. The creator chooses what to keep; the application owns files, identities, execution and validation.

This is the durable product direction, not a claim that every workflow is complete. Studio is neither a one-click autonomous filmmaker nor a full nonlinear video editor. A named production or fixture is an example, not the definition of the product.

### Reading this reference

- **Current behavior** below is grounded in the linked implementation and tests. Those tests identify coverage; this document does not claim they were all rerun for its publication.
- **Intent** explains the human purpose or an explicit design decision. It does not imply every part of that intention is implemented.
- **Limits** distinguish missing or partial behavior from a requirement to build it next.
- Source links use files and named concepts rather than fragile line numbers. Paths are relative to this document.

For ordinary button-by-button help, use the bundled [workflow guide](../src/docs/workbench-guide.md), also opened by F1. For maintained build and launch commands, use the [Docker workbench guide](../../dev-container/README.md). Project scheduling, approval holds and historical acceptance receipts live outside this reference.

## 1. The application and its records

### Shell and screens

The Electron shell contains Home, Studio, Files, Assets and Voices. Settings configures APIs/models. Studio groups recipe stages into **Plan, Produce and Finish**. Its inspector offers Finish, Saved conversations and Project state; Jobs records asynchronous work.

The embedded conversational Director is **retired, hidden and inert**. Prompts and operation approvals use dialogs associated with the initiating screen. Saved conversations open as ordinary files, not executable approvals. Backend names such as `director_approve` survive because the direct workflows reuse that execution machinery; they do not imply a reachable Director chat.

**Code:** [shell markup](../index.html), [main renderer](../src/main.ts), [workflow desk](../src/workflowDesk.ts), [surface navigation](../src/studio/surfaces.ts), [surface façade](../src/studio/surfaces/productionSurfaces.ts).

### Project truth and identities

A project is a folder containing authored files, media and a typed workflow map at `Preproduction/workflow_state.json`. The Rust host owns persisted workflow state. The renderer presents it, holds transient edits and sends typed intents; a visible UI field is not itself persisted authority.

Scenes, shots, assets, jobs and artifact versions have identities. Shot identity must survive reordering; a display number is not a substitute for a stable shot ID. Source assets, selected outputs, approvals and derived work are distinct records. Import/reconciliation can update the map from project files, but does not make every arbitrary file in the folder a production asset.

**Code:** [host state types](../host/src/workflow/state.rs), [import and reconciliation](../host/src/workflow/import.rs), [artifact identity/versioning](../host/src/workflow/identity.rs), [frontend mirror](../src/workflowTypes.ts), [intents](../src/studio/surfaces/surfaceIntents.ts).

### Recipes are dependency maps, not a mandatory waterfall

Built-in recipes are Narrative dialogue and Montage. They define stage IDs, labels, work areas, dependencies and required roles. The host supports project recipe documents and validation; the frontend derives navigation from the returned recipe/navigation data. Screen order and dependency order are not necessarily identical: Reference Images is deliberately moved earlier in Plan navigation.

**Why:** different productions should reuse the same project/asset machinery without hardwiring one story's sequence of steps. A registry does not establish that every conceivable production or custom stage has a complete UI.

**Code:** [built-in recipes](../host/src/workflow/recipes.json), [recipe loading/validation](../host/src/workflow/recipe.rs), [navigation derivation](../src/studio/surfaces.ts).

## 2. Planning and production surfaces

### Script and Notes

**Intent:** start from an idea, imported script or manual text without requiring a model. Keep working notes separate from the screenplay.

**Current behavior:** Title, Notes and Script are separate draft fields. Load notes reads a supported text file into Notes. Save notes writes project-root `Notes.md`; local draft recovery is a separate safety net, not a portable project backup. Save script writes/registers a script under `Scripts/`. Import script acquires/registers external text; merely opening a file in Files does not import it. Supported script controls accept `.md`, `.markdown` and `.txt`, with numbered `Scene N` headings.

Generate script from notes is an explicit text-model request with disclosure/confirmation and an editable unsaved result. Generate details drafts missing scene intent/look descriptions for the saved script through a scoped batch, preserving existing authored records; it is not image/video generation.

Saving an existing supported project script refreshes planning. Reimport/edits preserve identity and authored shot work where supported and can create dialogue review requirements or make dependent preparation stale. Written dialogue and generated dialogue audio are different things.

**Code:** [draft form](../src/studio/surfaces/scriptDraftForm.ts), [draft recovery](../src/studio/scriptDraftRecovery.ts), [script files](../src/studio/scriptFiles.ts), [script details](../src/studio/scriptDetails.ts), [host parser](../host/src/script.rs), [host import](../host/src/workflow/import.rs). **Coverage:** [script functions](../tests/cleanup-script-functions.test.ts), [saved script edits](../tests/saved-script-edit.test.ts), [dialogue review](../tests/script-dialogue-review.test.ts), [notes file](../host/tests/notes_file.rs).

### Characters, references, wardrobes and voices

**Intent:** retain reusable visual identity and voice material instead of rebuilding them for every scene.

**Current behavior:** Characters saves a named profile and selected reference images. Saved visual description/appearance, selected references, or both can support model-sheet preparation; a full biography or an Identity prompt is not a universal prerequisite. Model sheets have individual images, approval and recoverable deletion. The whole-pack action discloses a bounded batch of image requests, not one composite image or permission for unlimited retries.

Character-owned wardrobe images can be attached/generated without a scene. Multiple outfits are distinct assets. The older scene-based wardrobe-plate builder is a separate route with its own prerequisites and reuse rules; these are not interchangeable workflows.

Voices supports recording, library selection and design, with audition and reusable voice artifacts. Voice choice is deliberate, not something a Director conversation must choose first. Standalone speech preparation accepts a saved voice sample or profile and uses the explicitly selected one.

**Limits:** explicit multi-reference selection is not uniformly complete across every image creation/editing route. Do not infer model-sheet, legacy wardrobe-plate or video behavior from the Scene Look picker.

**Code:** [Characters](../src/studio/surfaces/charactersStage.ts), [character wardrobe UI](../src/studio/surfaces/characterWardrobe.ts), [host character wardrobe](../host/src/workflow/character_wardrobe.rs), [identity/model sheets](../host/src/workflow/identity.rs), [Voices UI](../src/voicePanel.ts), [voice host](../host/src/voice.rs). **Coverage:** [character-owned wardrobe](../tests/character-owned-wardrobe.test.ts), [wardrobe host](../host/tests/character_wardrobe_host.rs).

### Scene Intent, Reference Images and Scene Look

**Intent:** distinguish gathering potentially useful visual material from deciding how the production should look and which image belongs to each shot. Reference Images can contain alternatives, not just a final chosen scene image.

**Current behavior:** Scene Intent stores scene-level intent. Reference Images presents editable look descriptions/categories with image creation, attachment, preview and approval beside saved entries. A new entry appears first and defaults to User Defined. Save a changed description before image actions. Deleting a look removes its slot, not an instruction to delete its registered image files; dependent use can block removal.

Scene Look is a separate view over the **same look-board slots/assets**, not an independent image library. Reference Images embeds its existing image workspace per slot. Scene Look exposes hero/alternate images, image actions and per-request model capability choice from saved Settings. Ordered optional reference images are explicitly chosen; attaching character/model-sheet and approved wardrobe references is a separate opt-in.

**Open design boundary:** the precise future division between Scene Look, the reference collection and shot-by-shot storyboarding remains unsettled. The current shared implementation does not resolve that product question, and this reference does not prescribe a merger or removal.

**Code:** [reference surface](../src/studio/surfaces/referenceStage.ts), [Scene Look UI](../src/studio/sceneLookView.ts), [Scene Look model](../src/studio/sceneLookModel.ts), `renderSceneLookWorkspace` in [workflow desk](../src/workflowDesk.ts), [host look operations](../host/src/workflow/look.rs). **Coverage:** [reference workspace](../tests/reference-images-workspace.test.ts), [model routing](../tests/reference-image-model-route.test.ts), [ordered picker](../tests/ordered-reference-picker.test.ts).

### Cut Sketches: storyboard, timing, shot sources and dialogue audio

**Intent:** keep each shot's creative planning and independently auditionable dialogue together. A rough storyboard preview is not a rendered sequence.

**Current behavior:** shot cards expose sketches, dialogue, ordering and start-frame/source controls. Timing/animatic controls distinguish planned duration, measured audio and source-video limits. Source strategy can refer to a predecessor before its final image exists. Binding an actual frame is a later explicit operation.

For talking cuts using `source_video_lipsync`, Cut Sketches also provides standalone dialogue generation beside audio import, audition, editing and explicit selection. Generate dialogue audio uses the complete current single-speaker dialogue and a chosen saved voice. It is audio-only: no start image or video generation is needed. Delivered speech is a cut-owned, **unselected Draft WAV**. Listen before choosing it for video. Source dialogue review must be resolved before generation; Keep authored dialogue is an explicit reconciliation choice.

**Limits:** this standalone generation path is single-speaker and WAV-oriented. Its presence does not imply equivalent generation controls for every speech mode. An audio preview does not authorize video generation. Some audio controls are also exposed in Production Notes through shared rendering.

**Code:** [shot cards](../src/studio/surfaces/sketchStage.ts), [animatic timing](../src/studio/surfaces/animaticTiming.ts), [start frames and continuity UI](../src/studio/surfaces/referenceStage.ts), [dialogue generation UI](../src/studio/sceneDialogueAudio.ts), [shared audio UI](../src/studio/surfaces/performanceAudio.ts), [speech prepare/settle](../host/src/workflow/dialogue_audio.rs), [continuation](../host/src/workflow_continuation.rs). **Coverage:** [Cut Sketches generation](../tests/cut-sketch-dialogue-generation.test.ts), [audio/video boundary](../tests/dialogue-split-boundary.test.ts), [native audio split](../host/tests/dialogue_audio_split.rs), [settlement race](../host/tests/dialogue_audio_settlement_race.rs).

### Production Notes, Render and Take Review

**Intent:** turn reviewed planning into an executable shot request, then review generated candidates without confusing submission, delivery and selection.

**Current behavior:** Production Notes builds/edits full video prompts and planning-to-production handoffs. Source strategy, selected images, dialogue/performance audio and reviewed package state affect readiness. For the separated speech/video path, video uses separately reviewed saved audio; audio completion does not submit video or publish video permission.

Production Notes also exposes selected-take trim editing before every later cut is rendered. This allows an earlier shot's edited end to inform a successor. Render prepares the actual operation through existing approval/spending boundaries. Take Review distinguishes candidates and their review status. Jobs tracks work, failures and delivered outputs; a completed provider response is not proof that local registration succeeded.

**Code:** [production notes](../src/studio/surfaces/notesStage.ts), [render](../src/studio/surfaces/renderStage.ts), [take review](../src/studio/surfaces/takeReviewStage.ts), [production host](../host/src/workflow/production.rs), [render lifecycle](../host/src/workflow/render_jobs.rs), [jobs model](../src/studio/jobsModel.ts), [jobs view](../src/studio/jobsView.ts). **Coverage:** [production handoff](../tests/production-handoff-review.test.ts), [request receipts](../tests/sb20-request-receipt.test.ts), [native render lifecycle](../host/tests/s05_render_lifecycle.rs).

### Assemble, sequence review and delivery

**Intent:** assemble selected material, review the actual result and retain edit/version history without claiming to be a full editing suite.

**Current behavior:** scene and project masters are separate operations. Sequence review can play and seek an actual assembled master, identify stale snapshots and request explicit rendered approval. Non-destructive take-bound trim windows, ordering and supported transitions affect assembly; a different selected take can require reconciliation. A timed stills/animatic preview remains distinct from this rendered result.

Post records include cut versions, picture lock, editorial paths/statuses and deliverable checks. Some post fields record workflow status or external work; their existence is not an automatic audio mixer, VFX compositor or mastering engine. Deliverable QC probes actual media and checks supported properties, decode and state/snapshot requirements. Technical validity is not creative approval.

**Code:** [assembly/post UI](../src/studio/surfaces/assembleStage.ts), [sequence review](../src/studio/surfaces/sequenceReview.ts), [assembly execution](../host/src/workflow/assembly_jobs.rs), [post/delivery](../host/src/workflow/post.rs), [deterministic media QC](../host/src/workflow/qc.rs). **Coverage:** [connected production chain](../host/tests/production_chain.rs), [take editing](../tests/v1-video-take-edit.test.ts), [sequence review](../tests/v2-rendered-sequence.test.ts).

## 3. Changes, dependencies and media versions

A revision is not just another model request. It must retain the identity of what changed and expose effects on dependent work. Important distinctions are:

| Change | Relevant behavior and boundary |
|---|---|
| Script/dialogue edit | Reimport/reconciliation preserves authored shot work where supported, records review needs and can invalidate prepared input/approval state. |
| Image crop/edit | Derivative asset/version with source/parent binding and explicit review; not permission to overwrite the original. |
| Audio speed, pitch or trim | Local versioned audio operations, with audition and explicit selection. Changing chosen audio can invalidate video preparation. |
| Selected video trim | Source-ID-bound editorial window checked against measured source duration, not just the storyboard estimate. |
| Predecessor take/frame change | Existing continuity binding can become stale. Extracting/binding a new frame must remain explicit. |
| Input/project change during asynchronous work | Originating ownership and exact inputs are revalidated before registration; late work must not mutate a replacement project/session. |

These are supported boundaries, not a claim that arbitrary edits propagate perfectly through every route. When requesting a change, name which record changes and what should happen to existing media, selection, approval and downstream dependencies.

**Code:** [image editing](../host/src/workflow/image_edit.rs), [audio editing](../host/src/workflow/audio_edit.rs), [performance](../host/src/workflow/performance.rs), [continuity/sources](../host/src/workflow/sources.rs), [edit receipt](../src/studio/components/editReceipt.ts), [video take editor](../src/studio/surfaces/videoTakeEdit.ts). **Coverage:** [image revision binding](../host/tests/image_revision_binding.rs), [edit impacts](../host/tests/x1_edit_impacts.rs), [audio reconciliation](../tests/a3-audio-reconciliation.test.ts).

## 4. Execution, permissions, costs and Stop

- **Known transitions belong to the application.** The renderer emits intents, Electron checks IPC, and the Rust dispatcher validates and executes. Creative drafting/generation uses model/provider adapters where requested. Retired prompt-only Script, Character Creation and Production Helper actions must not be mistaken for available workflows.
- **Approval is scoped.** Direct operations reuse host-prepared, expiring, one-use approval IDs. Provider use has a separate exact-request permission path. A batch action can disclose several bounded requests, but completion is not permission to retry indefinitely or change model/input silently.
- **Money is not inferred.** Existing cost fields use integer minor units and unknown values are not zero. However, project accounting/reconciliation and budget-setting commands are retired. Current per-request consent is **not** a verified price ceiling, strict project budget or complete accounting ledger. Old budget-policy types/calculations do not prove dispatch enforcement.
- **Stop is cooperative, not rollback.** Admission generations/per-request flags and pending-action cancellation protect supported boundaries. Stop does not prove a remote provider canceled work already submitted, reverse charges or undo a completed file operation. A late cancellation must not cancel unrelated newer work.
- **Feedback is part of the workflow.** The initiating surface and project-level feedback expose request progress/terminal outcomes so a collapsed Jobs area is not the only signal. Errors and partial delivery/registration failures must remain distinguishable.

**Code:** [desk approval handling](../src/workflowDesk.ts), [operation approval dialog](../src/studio/operationApproval.ts), [pending approvals](../host/src/pending.rs), [per-request permission](../host/src/workflow/spend.rs), [dispatcher](../host/src/lib.rs), [cancellation context](../host/src/ctx.rs), [request receipts](../src/studio/requestReceipt.ts), [specialist dispatch](../host/src/specialists.rs). **Coverage:** [operation approval](../tests/sb22-operation-approval.test.ts), [cancellation](../host/tests/director_cancellation.rs), [scoped cancellation](../host/tests/sb24_scoped_cancel.rs).

## 5. Files, secrets and runtime boundaries

Electron preload exposes an allowlisted bridge; the main process validates callers/commands and restricts navigation/permissions. The Rust host confines paths to the project or explicit external grants. File/media reads are validated; recoverable project trash is used for supported deletions. These are boundaries in the implementation, not a fresh comprehensive security certification.

Provider credentials use the native secret-store/settings machinery rather than being ordinary project artifacts. The maintained Docker launch distinguishes offline disposable UI fixtures from explicitly opted-in provider-capable sessions with a machine-local encrypted keyring. Do not treat the default offline launch as a persistent provider-enabled session. Do not put operator credentials, runtime profiles or generated evidence in Git.

**Code:** [Electron main](../electron/main.mjs), [preload](../electron/preload.cjs), [command allowlist](../electron/commandAllowlist.cjs), [host session/grants](../host/src/session.rs), [project path resolver](../host/src/project_path.rs), [media validation](../host/src/media_util.rs), [settings](../host/src/settings.rs), [secret store](../host/src/secret_store.rs). **Operation:** [Docker/keyring guide](../../dev-container/README.md).

A frontend hot reload does not rebuild the Rust sidecar. Electron main/preload changes need an app relaunch; native changes need a build on the host running the app. Host-local build volumes are not synchronized source. A source fix is not proof that an already-open Studio window has loaded it.

## 6. Known limits and deliberately unsettled choices

These are not automatic assignments:

- Scene Look versus Reference Images versus shot-storyboard responsibilities still need a product decision; shared records do not settle their intended roles.
- Multi-reference behavior is route- and model-schema-specific. Do not promise uniform coverage across all image editing/model-sheet/wardrobe/video paths.
- Standalone shot speech is single-speaker; live voice/provider quality and supported formats need route-specific evidence, not inference from a local fixture.
- Asset records have source-job/parent links and notes; separate artifact versions and job provider/model fields exist. Dedicated asset duration, structured provenance and rights fields are not a complete implemented metadata system. Adding fields requires coordinated writers/readers/types, not just a schema declaration.
- Project-wide strict spending enforcement and complete accounting are not current guarantees.
- Recipe extensibility and post-status records do not prove every production type, external editorial workflow or final deliverable has been exercised.
- Mobile/remote clients, commercialization, departmental workflows and expanded reference-video analysis are not prerequisites silently inherited from discarded roadmaps.

## Using this reference to discuss a change

Start with the relevant screen/section and describe: **what you are trying to make; what you have now; what should happen; and what must remain unchanged.** For example: “On Characters, I want an alternate outfit without a scene; keep the existing outfit and let me choose the references.” The source map gives implementation context without requiring a code tour before the conversation can start.

Update the affected section when behavior or a design decision changes. Keep history and execution receipts out of this document; retain honest limits rather than adding a new phase checklist. This is the current product/code reference, not a substitute for the user-facing help, operational guide or live project board.
