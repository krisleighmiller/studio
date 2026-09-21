# Desktop App V2 Workflow Migration Plan

Date: 2026-09-17  
Status: Working migration plan  
Scope: `Desktop_App`, starting with `Lite_Challenge` as the proving project

---

## 1. Why This Migration Exists

The current Desktop App is still too chat-shaped. The Director can call specialists and the host has begun reclaiming mechanical responsibilities, but the user experience still asks one open-ended prompt to solve too many production stages at once.

That is why a request like:

> Scene Strategy cinematic balanced

can still produce mostly `source_video_lipsync` cuts. The model is asked to create production notes before the visual world of the scene exists. The only concrete visual assets it can reliably use are character model-sheet closeups under `Characters/`, so every eligible dialogue line collapses into a talking shot.

The deeper issue is not only prompt wording. It is that the app currently treats generic character references as if they were scene-specific production sources.

This migration changes the product shape from:

```text
chat prompt -> production notes -> render
```

to:

```text
script -> characters -> scene intent -> scene look -> cut sketches -> reference/start images -> production notes -> render -> take review
```

The goal is not full autopilot. The goal is a stoppable production desk where each stage has visible artifacts, user review points, and host-owned mechanics.

---

## 2. Product Principle

Chat remains useful, but it should not be the primary workflow container.

V2 should be workflow-first:

- The UI shows the current stage, scene, cuts, assets, jobs, and approvals.
- Chat operates on the selected stage/cut/asset instead of the whole project at once.
- Specialists become named actions behind buttons and menus.
- The project folder remains the source of truth.
- The host owns paths, IDs, schemas, timing, provider routing, approvals, and state transitions.
- The LLM owns creative judgment inside a bounded stage.

In short:

```text
UI = production desk
Host = workflow/state/mechanics
LLM = bounded creative collaborator
Providers = replaceable execution layer
```

---

## 3. Target Workflow

### Stage 1: Script

Purpose: import or create the source material.

User-facing actions:

- Import script.
- Create script.
- Edit script.
- Split into scenes.
- Extract dialogue/action/sound/transition beats.

Host-owned outputs:

- Scene list.
- Scene text.
- Dialogue list per scene.
- Action beats per scene.
- Sound/transition notes per scene.
- Stable scene IDs.

LLM-owned work:

- Drafting or rewriting script prose when asked.
- Summarizing scene beats.
- Suggesting scene titles or structure.

Important rule:

Production notes should not be generated directly from raw script unless the user explicitly chooses a fast-path opt-out.

---

### Stage 2: Characters

Purpose: define identity, voice, and continuity traits for recurring characters.

User-facing actions:

- Create character profile.
- Generate identity references.
- Attach existing references.
- Create voice profile.
- Mark reference/voice readiness.

Asset roles:

- `identity_reference`
- `body_reference`
- `expression_reference`
- `voice_sample`
- `voice_profile`

Host-owned outputs:

- Typed character metadata.
- Reference image registry.
- Voice registry.
- Readiness state.

LLM-owned work:

- Character description.
- Wardrobe/personality continuity notes.
- Suggested reference prompts.

Important rule:

Character model-sheet images are identity inputs. They are not automatically cut start frames and should not be treated as production sources for every dialogue line.

---

### Stage 3: Scene Intent

Purpose: decide what the scene must accomplish before generating visual references.

This is the missing planning layer between script and image generation.

For each scene, capture:

- Narrative purpose.
- Emotional purpose.
- Character focus.
- Visual contrast.
- Important props.
- Continuity constraints.
- Sound and transition requirements.
- What must be clear to the audience by the end.

Example for `Lite_Challenge` Scene 1:

- The children are vulnerable but purposeful.
- Tom is trying to be steady.
- Pip's hunger and hope soften the scene.
- Nell is alert to danger.
- The brown paper parcel matters.
- The train interior is temporary warmth.
- The rain outside is hostile.
- The scene should end by moving from train safety into exposed platform danger.

Host-owned outputs:

- `scene_intent.yaml` or equivalent typed state.
- Scene-level constraints passed to later stages.

LLM-owned work:

- Drafting the intent summary from script.
- Suggesting emotional/visual priorities.

User-owned work:

- Approving or editing intent before scene look and cut sketches.

---

### Stage 4: Scene Look / Reference Board

Purpose: create the still-image foundation for the scene before production notes.

This stage should generate or attach scene-level references such as:

- Location.
- Wardrobe.
- Props.
- Mood.
- Lighting.
- Composition.
- Group poses.
- Exterior views.
- Interior views.
- Period detail.

Asset roles:

- `location_reference`
- `wardrobe_reference`
- `prop_reference`
- `mood_reference`
- `lighting_reference`
- `composition_reference`
- `group_reference`
- `exterior_reference`
- `interior_reference`

For the train scene, this might include:

- Kids in period clothes sitting in a narrow train carriage.
- Rain-streaked carriage window with warm yellow light.
- Brown paper parcel on Tom's knees.
- Exterior two-carriage night train in rain.
- Willowby Halt platform/red lantern mood reference.
- Swinging corridor lamp detail.

User-facing actions:

- Draft scene reference prompts.
- Generate selected references.
- Attach existing images.
- Send selected image for edit.
- Approve / reject / regenerate.
- Promote image to a scene role.

Host-owned outputs:

- Scene reference registry.
- Image versions.
- Approval status.
- Provider job history.
- Mapping from images to roles.

LLM-owned work:

- Drafting image prompts.
- Suggesting which references are useful.
- Writing edit briefs from user changes.

Important rule:

The host should choose where generated references are saved and how they are registered. The model should not invent output paths.

---

### Stage 5: Cut Sketches

Purpose: plan the scene as editable cut cards before canonical production notes.

Cut sketches are intentionally lighter than production handoffs. They should contain just enough information to generate or choose reference/start images and later produce final notes.

Each cut sketch should include:

- Source scene ID.
- Cut number.
- Title.
- Dialogue covered.
- Action beat covered.
- Location.
- Shot role.
- Subject.
- Speaker, if any.
- On-camera intent.
- Mouth policy.
- Rough duration/pace.
- Suggested visual prompt.
- Suggested negative prompt.
- Suggested reference image roles.
- Transition idea.

Suggested shot roles:

- `performance`
- `reaction`
- `insert`
- `atmosphere`
- `establishing`
- `transition`
- `action`
- `detail`

Suggested mouth policies:

- `speaking_visible`
- `closed_or_not_visible`
- `off_camera`
- `no_people`

Example sketch:

```yaml
source_scene: 1
cut: 2
title: Pip watches the parcel
shot_role: reaction
subject: Pip
location: train_interior
dialogue:
  - speaker: Tom
    text: "We shall be there before supper."
    heard_as: voice_over
on_camera:
  - Pip
mouth_policy: closed_or_not_visible
pace: natural
visual_prompt: >-
  Pip sits close beside Tom in the warm, narrow train carriage, watching the
  brown paper parcel while rain runs down the black window behind him.
references:
  - group_reference
  - prop_reference
  - interior_reference
negative_prompt: "lip movement, modern clothing, extra passengers"
```

Host-owned outputs:

- Typed cut-sketch state.
- Dialogue coverage.
- Cut numbering.
- Mix visibility.
- Validation of strategy goals.

LLM-owned work:

- Proposed cut boundaries.
- Shot role and framing suggestions.
- Prompt language.
- Creative continuity.

User-owned work:

- Edit/reorder/split/merge cuts.
- Change shot role.
- Change prompt.
- Mark whether a cut needs a generated start frame.

Important rule:

The app should be able to show, before production notes, whether a scene is becoming lip-sync heavy, reaction heavy, or atmosphere heavy.

---

### Stage 6: Reference Images / Start Frames

Purpose: create scene-specific visual sources for cuts.

This is where the current manual workflow should become first-class.

Each cut can have:

- Generated start frame.
- Edited start frame.
- Attached still.
- Continuity frame extracted from prior video.
- Approved performance source.
- Approved reaction plate.

Asset roles:

- `cut_start_frame`
- `performance_source`
- `reaction_plate`
- `insert_plate`
- `continuity_frame`
- `last_frame`
- `edit_source`

User-facing actions:

- Generate start frame from cut sketch.
- Attach image.
- Send image for edit.
- Extract last frame from prior render.
- Compare versions.
- Approve image for production notes.

Host-owned outputs:

- Cut image registry.
- Version history.
- Approval status.
- Link from cut sketch to approved visual source.

LLM-owned work:

- Image prompt drafting.
- Edit brief drafting.

Important rule:

Generic character identity refs can seed image generation, but approved cut-specific images should become the visual source for video generation.

---

### Stage 7: Production Notes

Purpose: create canonical generation handoffs only after the scene/cut visual foundation exists.

Production notes should be generated from:

- Script scene.
- Scene intent.
- Scene look board.
- Cut sketches.
- Approved cut start frames / references.
- Character and voice registries.
- Host config and provider capabilities.

The final production handoff should not be where the model invents:

- Paths.
- Provider.
- Stage.
- Aspect ratio.
- Cue IDs.
- Timing buckets.
- Source mode.
- Generation path.
- Required capabilities.
- Reference image paths.

Host-owned outputs:

- `prod.md`.
- Sidecars.
- Canonical YAML.
- Derived timing.
- Render lane.
- Asset bindings.

LLM-owned work:

- Polished final prompt.
- Continuity narration.
- Delivery notes.
- Negative prompt.
- Creative shot language.

Important rule:

`source_video_lipsync` should be reserved for deliberate performance cuts. A character being lip-sync ready should never be enough by itself.

---

### Stage 8: Render

Purpose: execute approved production notes.

User-facing actions:

- Render selected cut.
- Render ready scene.
- Render video-only.
- Render audio/text-sync.
- Retry failed job.
- Reveal output.

Host-owned outputs:

- Provider job records.
- Output files.
- Render metadata.
- Cost/approval log.
- Failure details.

LLM-owned work:

- None during provider execution.
- Optional diagnosis or edit suggestions after failure.

Important rule:

Provider calls are jobs with approval and history. They should not be hidden behind a single chat reply.

---

### Stage 9: Take Review

Purpose: choose what footage survives.

This stage is necessary because generative video produces takes, not guaranteed final shots.

Statuses:

- `selected`
- `alternate`
- `rejected`
- `needs_edit`
- `needs_rerender`
- `used_in_assembly`

User-facing actions:

- Play take.
- Compare takes.
- Mark selected.
- Send frame for edit.
- Extract last frame.
- Request rerender.
- Promote to assembly.

Host-owned outputs:

- Take registry.
- Selected clip per cut.
- Alternate clips.
- Rejection notes.
- Continuity frames.

LLM-owned work:

- Optional review summary.
- Suggested edit/rerender brief.

---

### Stage 10: Assemble

Purpose: assemble selected takes into a scene or full piece.

Host-owned work:

- Discover selected takes.
- Read production state.
- Read transitions.
- Use ffprobe/ffmpeg.
- Build assemble plan.
- Apply handles/trims.
- Export drafts.

LLM-owned work:

- Optional music/caption/finish suggestions.
- Optional natural-language change request parsing.

Important rule:

Assemble should not depend on the model reconstructing a scene list the host already has.

---

## 4. Opt-Out Modes

The workflow should support different project sizes and user patience levels.

### Full Pre-Production

Best for serious shorts, recurring series, multipart stories, or any scene where continuity matters.

Flow:

```text
script -> characters -> scene intent -> scene look -> cut sketches -> start images -> production notes -> render
```

### Fast Draft

Best for quick exploration.

Flow:

```text
script -> cut sketches -> production notes
```

The app should warn when visual references are missing and make it clear that output may drift.

### Direct Production Notes

Best for compatibility with the current workflow.

Flow:

```text
script -> production notes
```

This should remain available, but it should be labeled as a shortcut and should not be the default for projects that need continuity.

### Manual Assets

Best when the user already has references from outside tools.

Flow:

```text
script -> characters -> attach scene/cut refs -> production notes
```

---

## 5. Proposed Project Layout

Keep compatibility with existing project folders, but introduce a logical V2 layout the app can migrate toward.

```text
project/
├── Scripts/
│   └── story.md
├── Characters/
│   └── Tom Vale/
│       ├── character.yaml
│       ├── model_sheet/
│       └── voice/
├── Preproduction/
│   ├── workflow_state.json
│   └── Scene1/
│       ├── scene_intent.yaml
│       ├── scene_look.yaml
│       ├── cut_sketches.yaml
│       ├── references/
│       └── start_frames/
├── Prod_Notes/
│   ├── prod.md
│   └── Scene1/
│       └── Cut1.txt
├── Renders/
│   └── Scene1/
│       └── Cut1/
│           ├── takes/
│           └── selected.yaml
└── Final/
```

Existing paths like `Scene_Prompts/` can remain supported during migration.

The app should not require users to rename old projects immediately. It should detect what exists, map it into workflow state, and offer normalization later.

---

## 6. Core State Model

The new UI needs explicit workflow state. This can start as a single JSON file and become typed Rust structs.

Top-level concepts:

```text
ProjectState
├── scripts
├── characters
├── scenes
├── assets
├── jobs
└── settings snapshot / capability slots
```

Scene state:

```text
SceneState
├── id
├── title
├── source_path
├── source_excerpt
├── intent
├── look_board
├── cut_sketches
├── production_notes
├── renders
└── stage statuses
```

Cut state:

```text
CutState
├── id
├── title
├── dialogue coverage
├── shot_role
├── subject
├── mouth_policy
├── pace
├── sketch_prompt
├── approved_start_frame
├── production_handoff
├── render_lane
├── takes
└── selected_take
```

Asset state:

```text
AssetState
├── path
├── role
├── scene_id
├── cut_id
├── character_id
├── source_job_id
├── parent_asset_id
├── status
└── notes
```

Job state:

```text
JobState
├── id
├── kind
├── provider
├── model
├── input assets
├── output assets
├── cost/approval status
├── started/finished timestamps
├── raw provider id
└── error/retry info
```

---

## 7. UI Migration

The UI should move from chat-first to stage-first.

### Minimum V2 UI

Start with:

- Left project tree.
- Stage rail.
- Scene list.
- Main panel for selected stage.
- Right inspector for selected scene/cut/asset.
- Jobs drawer.
- Chat drawer scoped to current selection.

Stage rail:

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

The first UI does not need to be beautiful. It needs to make state visible.

### Chat's New Role

Chat should answer and assist, but action buttons should drive the workflow.

Examples:

- In Scene Intent: "Tighten the emotional purpose."
- In Scene Look: "Make the train feel poorer and older."
- In Cut Sketches: "Turn Cut 2 into a reaction shot."
- In Reference Images: "Send this image for edit; Pip should look colder."
- In Production Notes: "Regenerate prompt from the approved start frame."

The Director should receive the current selected stage/cut/asset as context and should not have to infer the whole universe from free text.

---

## 8. Migration From Current Projects

Existing projects should be importable into V2 state.

For `Lite_Challenge`, the importer should read:

- Script files under `Scripts/`.
- Character folders under `Characters/`.
- Existing `prod.md`.
- Existing `Scene_Prompts/SceneN/CutM.txt`.
- Any generated images/renders if present.

Then it should create draft V2 state:

- Scene records from the script and/or production notes.
- Cut records from existing handoffs.
- Current speech-mode mix.
- Existing prompts.
- Existing reference paths.
- Missing start-frame warnings.
- Missing scene-look-board warnings.

The migrated UI should be able to say:

```text
Scene 1:
- 9 cuts found
- 6 lip-sync / 1 voice-over / 2 silent
- 0 approved cut-specific start frames
- production notes exist
- scene look board missing
- cut sketches can be backfilled from production notes
```

That gives the user a way to recover the current work without pretending it was produced by the new workflow.

---

## 9. Strategy Handling

Strategy should stop being only prompt advice.

The host should track scene strategy in state and validate the cut sketch mix before production notes.

Suggested strategy policies:

### Performance Heavy

- Most character dialogue may be `performance`.
- Lip-sync is expected for eligible pivotal lines.
- Silent/insert cuts are still allowed for action beats.

### Cinematic Balanced

- Must include a mix of performance, reaction, insert/atmosphere/action cuts when the scene has enough material.
- Should not allow every eligible character line to become a performance/lip-sync cut.
- Should require at least some dialogue to be carried as voice-over or off-camera when there are multiple lines.
- Should surface a warning or rejection if the sketch is effectively performance-heavy.

### Atmosphere Heavy

- Most dialogue should play over reaction, insert, atmosphere, or action shots.
- Lip-sync should be reserved for one or two pivotal lines.

The exact ratios can start simple and evolve with use.

Example initial rule:

```text
For cinematic-balanced scenes with 4+ dialogue lines and 3+ cut sketches:
- reject if every reference-ready dialogue line is marked performance/speaking_visible
- warn if lip-sync/performance cuts exceed 60% of dialogue cuts
- require at least one non-performance dialogue cut unless explicitly overridden
```

This belongs in host validation, not only in the prompt.

---

## 10. Asset Semantics

The single word "reference" is too vague. V2 must make roles explicit.

Identity assets:

- `identity_reference`
- `body_reference`
- `expression_reference`

Scene look assets:

- `location_reference`
- `wardrobe_reference`
- `prop_reference`
- `mood_reference`
- `lighting_reference`
- `composition_reference`
- `group_reference`

Cut assets:

- `cut_start_frame`
- `performance_source`
- `reaction_plate`
- `insert_plate`
- `continuity_frame`
- `last_frame`
- `edit_source`

Render outputs:

- `video_take`
- `audio_take`
- `selected_take`
- `assembly_draft`
- `final_export`

Key rule:

`Characters/` assets establish identity. `Preproduction/SceneN/start_frames/` assets establish the actual cut image. Production notes should prefer cut-specific approved images over generic character closeups.

---

## 11. Specialist Boundary Changes

V2 should split the current scene-plan behavior into smaller stage-specific runners.

### Scene Intent Runner

Input:

- Script scene.
- Project style.
- User direction.

Output:

- Typed scene intent.

### Scene Look Runner

Input:

- Scene intent.
- Script scene.
- Character summaries.

Output:

- Reference-board prompt suggestions.
- Suggested asset roles.

### Cut Sketch Runner

Input:

- Script scene.
- Dialogue coverage.
- Scene intent.
- Scene look summary.
- Strategy.

Output:

- Typed cut sketches.

### Reference Prompt Runner

Input:

- Scene look or cut sketch.
- Selected asset roles.
- Character identity refs.

Output:

- Image prompt/edit brief.

### Production Notes Runner

Input:

- Approved cut sketch.
- Approved start frame / scene references.
- Character/voice registries.
- Host config.

Output:

- Canonical handoff creative fields only.

Host derives:

- File paths.
- IDs.
- Aspect ratio.
- Provider/model.
- Render lane.
- Timing.
- Reference paths.
- Approval requirements.

---

## 12. Implementation Phases

### Phase 0: Stabilize Current Branch

Goal: make sure the current ownership work stays green.

Tasks:

- Keep restored settings files.
- Keep Tier 1/Tier 2 ownership changes passing.
- Confirm host tests.
- Avoid starting Tier 3 assemble work until V2 workflow direction is captured.

Done when:

- Current host tests pass.
- This migration plan exists under `Dev_Notes/V2`.

---

### Phase 1: Define V2 State and Importer

Goal: represent the new workflow without UI complexity.

Tasks:

- Add typed state structs for project/scene/cut/assets/jobs.
- Add migration/import code for existing `Scripts/`, `Characters/`, `prod.md`, and `Scene_Prompts`.
- Add tests using `Lite_Challenge`-shaped fixtures.
- Surface current scene mix and missing-reference warnings.

Done when:

- Existing production notes can be imported into V2 state.
- The app can report scene/cut status without generating anything.

---

### Phase 2: Minimal Workflow UI

Goal: make stages visible.

Tasks:

- Add stage rail.
- Add scene list.
- Add selected-scene inspector.
- Add cut list/cards.
- Add job/status drawer placeholder.
- Keep existing chat available as a drawer/panel.

Done when:

- User can open `Lite_Challenge`.
- User can see Scene 1, its cuts, current mix, and missing preproduction artifacts.

---

### Phase 3: Scene Intent + Cut Sketches

Goal: prove the planning layer fixes the lip-sync monoculture before adding image generation.

Tasks:

- Implement scene intent generation/editing.
- Implement cut sketch generation/editing.
- Add strategy validation at cut-sketch level.
- Add UI controls for shot role and mouth policy.
- Backfill sketches from existing production notes.

Done when:

- `Scene Strategy cinematic balanced` produces a mixed cut sketch plan.
- The UI warns/rejects performance-heavy output when balanced is requested.
- User can manually change a cut from performance to reaction/insert/atmosphere.

---

### Phase 4: Scene Look Board

Goal: generate and manage scene-level reference prompts/assets.

Tasks:

- Add scene look state.
- Add reference prompt generation.
- Add attach-existing-image flow.
- Add generated-reference job records.
- Add approve/reject/version states.

Done when:

- Scene 1 can have approved train interior, exterior, wardrobe, prop, and mood references.
- These assets are available to cut sketches and reference-image generation.

---

### Phase 5: Cut Start Frames / Image Edits

Goal: replace manual start-frame creation and reference swapping.

Tasks:

- Generate cut-specific start frame prompts from cut sketches.
- Use character identity refs plus scene look refs.
- Save outputs under host-derived paths.
- Add send-for-edit workflow.
- Add image compare/version approval UI.
- Mark approved start frame per cut.

Done when:

- A cut can move from sketch to approved start frame without hand-editing paths.
- The approved start frame, not the model-sheet closeup, becomes the video source.

---

### Phase 6: Production Notes From Approved Sketches

Goal: make production notes downstream of approved planning artifacts.

Tasks:

- Generate canonical handoff from cut sketch + approved visual source.
- Preserve host-owned derivations from Tier 2.
- Keep `source_video_lipsync` limited to performance cuts.
- Prefer `voice_over`/I2V for reaction and insert cuts.
- Write `prod.md` and sidecars from typed state.

Done when:

- Scene 1 can be regenerated from sketches/images into production notes.
- The production-note mix matches the approved cut-sketch mix.
- The model cannot turn every ready character into lip-sync during this stage.

---

### Phase 7: Render + Take Review

Goal: make generation outputs reviewable.

Tasks:

- Render from selected production notes.
- Track provider jobs.
- Store takes per cut.
- Add selected/alternate/rejected statuses.
- Add rerender/edit-from-take actions.
- Support last-frame extraction into continuity assets.

Done when:

- User can render one approved cut, review takes, select one, and feed its last frame into a later cut.

---

### Phase 8: Assemble From Selected Takes

Goal: make assemble host-planned.

Tasks:

- Discover selected takes.
- Build assemble plan from state.
- Apply transition/trim policy.
- Export draft.
- Record assembly outputs.

Done when:

- User can assemble a scene without asking the LLM to reconstruct scene order.

---

## 13. First Vertical Slice

Use `Lite_Challenge` Scene 1.

The first useful slice should be:

```text
Import current project -> show Scene 1 status -> create/edit scene intent -> create/edit cut sketches -> validate cinematic-balanced mix
```

Do not start with provider image generation. First prove that the workflow can create a better plan.

Success criteria:

- The app imports existing `prod.md` / `Scene_Prompts`.
- Scene 1 shows `6 lip-sync / 1 voice-over / 2 silent`.
- The app flags that as effectively performance-heavy for a cinematic-balanced request.
- A new cut-sketch pass can propose reaction/insert/atmosphere dialogue coverage.
- The user can edit those sketches before production notes exist.

Only after this works should the app generate scene references or start frames.

---

## 14. Testing Strategy

Tests should be stage-specific.

Importer tests:

- Reads existing `prod.md`.
- Reads sidecars.
- Reconstructs cuts.
- Preserves current prompts.
- Reports missing V2 artifacts.

Strategy tests:

- Cinematic-balanced rejects all-performance dialogue scenes.
- Atmosphere-heavy allows only limited performance cuts.
- Performance-heavy allows mostly lip-sync.
- Explicit opt-out bypasses strategy mix checks with a warning.

State tests:

- Cut IDs remain stable.
- Asset roles round-trip.
- Approved start frame is selected over identity reference.
- Generated output paths are host-derived.

Production-note tests:

- Handoff generation uses approved start frame.
- Character model sheet is not used as video source unless explicitly approved as a cut source.
- Reaction/insert cut does not become `source_video_lipsync`.
- Host derives timing, lane, paths, refs, provider, and IDs.

UI smoke tests:

- Project opens.
- Scene list renders.
- Cut cards render.
- Stage status updates after action.
- Job drawer records local/generated jobs.

---

## 15. Risks

### Risk: Overbuilding Before the Workflow Is Proven

Mitigation:

Start with `Lite_Challenge` Scene 1 and no provider generation. Validate state, UI, and cut sketches first.

### Risk: Too Many New File Formats

Mitigation:

Start with one workflow state file and typed structs. Split into per-scene files later only if useful.

### Risk: The UI Becomes a Heavy NLE

Mitigation:

Keep it as a production desk, not a timeline editor. Use external NLEs as the escape hatch.

### Risk: Chat and UI Fight Each Other

Mitigation:

Chat must always be scoped to current selection and stage. Stage buttons own known workflow transitions.

### Risk: Existing Projects Break

Mitigation:

Import and map existing layouts. Do not force folder normalization on first open.

### Risk: Strategy Rules Become Too Rigid

Mitigation:

Warn first, reject only the most obvious violations, and include explicit opt-outs.

---

## 16. Open Decisions

- Should V2 workflow state live in one `Preproduction/workflow_state.json` file, or in per-scene YAML files?
- Should scene intent and cut sketches be editable as forms, YAML, or both?
- How strict should cinematic-balanced validation be?
- Should generated scene references require approval before cut sketches, or only before production notes?
- Should start-frame generation be per cut by default, or only for cuts marked as needing one?
- How should the UI expose image versions without becoming a full asset manager?
- What is the minimum viable job-history schema needed before provider calls move into this workflow?

---

## 17. Near-Term Recommendation

Pause further Tier 3+ ownership work until the V2 workflow foundation is started.

Next implementation target:

```text
V2 state + Lite_Challenge importer + minimal stage UI + scene intent + cut sketches + strategy mix validation
```

That is the smallest change that attacks the real problem: the app currently creates final production notes before the visual planning layer exists.

