# Studio Product Hardening and Differentiation Plan

**Status:** Proposed next-stage implementation roadmap
**Target:** After the current Production Studio UI Redo reaches functional parity
**Primary goal:** Turn Studio from a working AI-assisted video production prototype into a coherent production environment that is meaningfully differentiated from generator-first AI video applications.

---

# 1. Product Direction

Studio should not compete primarily on:

* number of supported models;
* fully autonomous one-prompt video generation;
* social-media automation;
* a node-based workflow builder;
* recreating a nonlinear editor;
* providing a separate "studio" for every media operation.

Studio's strongest direction is:

> **A production workspace for creating AI-assisted video without giving up creative control.**

The user remains responsible for creative decisions.

AI and media providers perform production work inside a persistent, inspectable project state.

The application should help the user:

1. define what they are making;
2. plan how it will be made;
3. generate or acquire assets;
4. compare versions and takes;
5. approve and reject work;
6. track dependencies;
7. assemble a sequence;
8. review and revise it;
9. finish and validate deliverables.

The production state, not the Director chat and not any individual provider, remains the source of truth.

---

# 2. Current Strengths to Preserve

The following existing Studio behaviors are core product principles and should not be weakened during future work.

## 2.1 Provider output is not automatically approved work

A provider response creates an artifact, take, or candidate.

It does not become canonical merely because generation succeeded.

Existing concepts such as:

* `selected_take`;
* approval state;
* artifact versions;
* take review;
* dependency state;
* picture lock;
* deliverable QC;

must remain authoritative.

---

## 2.2 Stable identity is separate from display order

Scenes, shots, assets, jobs, takes, and versions should continue using stable IDs.

Display labels such as:

```text
Scene 2
Cut 4
```

are presentation.

They must not become relational identity.

Reordering a shot must not change:

* Director scope;
* take ownership;
* dependencies;
* source bindings;
* version history.

---

## 2.3 Known transitions belong to the application

Buttons and structured UI own deterministic actions such as:

* approve;
* reject;
* bind source;
* generate;
* render;
* select take;
* assemble;
* run QC.

Director remains optional assistance.

Do not turn structured production operations back into chat commands.

---

## 2.4 Shared production state remains central

Do not create separate agent memories for:

* directing;
* editorial;
* sound;
* VFX;
* production design;
* continuity.

These are responsibilities and views over one production.

---

# 3. Roadmap Overview

Implement the following in order.

```text
Phase A — Finish the existing vertical slice
Phase B — Remove recipe and stage hard-coding
Phase C — Production cost and capability governance
Phase D — Deterministic technical QC
Phase E — Audio as first-class production state
Phase F — Sequence Review
Phase G — Provider capability schemas
Phase H — Reusable Production Presets
Phase I — Better project creation / source-first onboarding
Phase J — Reference-video analysis
Phase K — Fixture productions and product hardening
```

Do not begin later phases merely because they are more interesting.

Each phase should leave Studio in a releasable state.

---

# 4. Phase A — Finish the Existing Vertical Slice

## Goal

Prove one project can move through the current production architecture from planning to reviewable output without bypassing workflow state.

This phase should complete the work already described in:

```text
Dev_Notes/V2/Production_Studio_UI_Redo_Plan.md
```

Do not add major new product capabilities during this phase.

---

## Required work

Complete:

* assembly reconciliation;
* review-export registration;
* assembly job completion;
* review-export versioning;
* continuity-frame review;
* continuity-frame rebinding;
* stable Director scope;
* truthful job timing;
* job scope metadata;
* job cost placeholders where available;
* Finish readiness;
* opening a review export from Studio.

---

## Completion test

The `Lite_Challenge` fixture must support:

```text
approved plan
→ approved source strategies
→ render
→ takes
→ take review
→ selected takes
→ continuity binding
→ dependent render
→ assemble
→ review export
→ Finish status
```

without manually editing workflow state.

---

# 5. Phase B — Remove Recipe and Stage Hard-Coding

## Problem

Current frontend types hard-code the production vocabulary:

```ts
export type StageId =
  | "script"
  | "characters"
  | "scene_intent"
  | ...
  | "assemble";

export type ProductionRecipe =
  | "narrative_dialogue"
  | "montage";
```

This is acceptable for the current prototype but conflicts with the intended recipe architecture.

Future recipes may include:

```text
documentary
product advertisement
presenter explainer
music video
existing-footage edit
podcast repurpose
tutorial
```

These must not require editing core TypeScript unions every time.

---

## Goal

Make stages and recipes data-driven.

The application should know that something is a stage without knowing every possible stage ID at compile time.

---

## Proposed model

Replace:

```ts
type StageId = ...
```

with something conceptually equivalent to:

```ts
type StageId = string;
type RecipeId = string;
```

and introduce structured metadata.

Example:

```ts
type StageDefinition = {
  id: string;
  label: string;
  work_area: "plan" | "produce" | "finish";
  depends_on: string[];
  required_roles: string[];
  optional?: boolean;
  ui_surface?: string | null;
};
```

Recipe:

```ts
type RecipeDefinition = {
  id: string;
  label: string;
  description?: string;
  stages: StageDefinition[];
};
```

The Rust host remains authoritative for readiness and validation.

---

## Files likely affected

Review at minimum:

```text
Desktop_App/src/workflowTypes.ts
Desktop_App/src/studio/surfaces.ts
Desktop_App/src/studio/surfaceNav.ts
Desktop_App/host/src/workflow/recipe.rs
Desktop_App/host/src/workflow/state.rs
Desktop_App/host/src/workflow.rs
```

Do not duplicate recipe logic into the frontend.

---

## Migration requirement

Existing workflow files must continue loading.

If the current schema stores known recipe strings, provide migration/default handling.

---

## Tests

Add tests proving:

* an unknown recipe ID can load;
* a recipe can introduce a new stage without changing `workflowTypes.ts`;
* a recipe can omit characters;
* a recipe can omit lip sync;
* stage order follows recipe data;
* work-area grouping follows recipe metadata;
* unknown stages do not crash Studio;
* unavailable stages do not leave dead navigation.

---

# 6. Phase C — Production Cost and Capability Governance

## Goal

Make generation spending visible before it becomes surprising.

Studio already has much of the required structure:

```ts
JobRecord {
  provider
  model
  cost
  currency
  created_at
  started_at
  completed_at
}
```

and the production profile already contains a budget field.

The next step is to make those meaningful.

---

## 6.1 Structured project budget

Replace or supplement free-text budget information with:

```ts
type ProjectBudget = {
  currency: string;
  limit?: number | null;
  warning_at?: number | null;
  hard_stop?: boolean;
};
```

Track:

```text
estimated committed spend
completed spend
failed/retried spend where billable
remaining budget
unknown-cost jobs
```

Never treat unknown cost as zero.

---

## 6.2 Pre-generation estimate

Where provider pricing is known, show before execution:

```text
Model: Kling ...
Duration: 10 seconds
Resolution: 1080p
Estimated cost: $0.82
Remaining project budget: $18.41
```

The estimate belongs to the job or planned operation.

---

## 6.3 Actual cost

When provider responses expose real billing information, store it separately from the estimate.

Suggested structure:

```ts
estimated_cost?: number;
actual_cost?: number;
currency?: string;
```

---

## 6.4 Job presentation

Extend Jobs so each job can show:

```text
operation
provider
model
scope
status
elapsed time
estimated cost
actual cost
retry lineage
output
```

---

## 6.5 Budget policy

Initially support:

```text
No limit
Warn only
Require confirmation above threshold
Hard stop
```

Do not allow the AI Director to bypass a hard budget policy.

---

# 7. Phase D — Deterministic Technical QC

## Goal

Turn Finish into an actual delivery-validation surface.

QC should primarily be deterministic.

Do not ask an LLM whether a video technically works when FFmpeg or ffprobe can answer it.

---

## Initial checks

Implement checks incrementally.

### File integrity

Check:

* file exists;
* file decodes;
* expected streams exist;
* duration is valid;
* duration is within expected tolerance.

### Video

Check:

* width;
* height;
* aspect ratio;
* frame rate;
* expected codec where applicable;
* no zero-frame output.

### Audio

Check:

* audio exists when required;
* audio duration roughly matches video;
* catastrophic silence;
* obvious clipping;
* sample rate where required.

### Assembly

Check:

* every required shot is represented;
* no missing selected take;
* assembly matches current selected-take state;
* assembly is not stale relative to current dependencies.

### Captions

Check:

* caption file exists when required;
* caption duration does not exceed video;
* caption track can be parsed;
* caption state corresponds to the current picture-lock version.

---

## Later checks

Potential later checks:

```text
unexpected black frames
frozen segments
duplicate frames over suspicious durations
audio loudness targets
missing dialogue coverage
safe-area checks
platform-specific delivery constraints
```

These are not required for the first QC implementation.

---

## QC result model

Prefer structured findings.

Example:

```ts
type QCFinding = {
  id: string;
  severity: "info" | "warning" | "error";
  check: string;
  message: string;
  artifact_id?: string;
};
```

A QC run should be versioned against:

```text
deliverable
cut version
picture-lock version
```

If the picture changes afterward, previous QC becomes stale.

---

# 8. Phase E — Audio as First-Class Production State

## Problem

Studio currently handles character voices better than general sound production.

Video production needs more than TTS.

---

## Goal

Represent sound requirements and sound assets using the same discipline already applied to visual assets.

---

## Audio roles

Introduce explicit roles such as:

```text
dialogue
voice_over
music
ambience
room_tone
sound_effect
foley
source_audio
scratch_audio
mix
```

Do not make each role a separate application surface.

---

## Audio asset metadata

Example:

```ts
type AudioAssetRole =
  | "dialogue"
  | "voice_over"
  | "music"
  | "ambience"
  | "room_tone"
  | "sound_effect"
  | "foley"
  | "scratch"
  | "mix";
```

Audio assets should support:

* scene association;
* shot association where relevant;
* character association where relevant;
* versioning;
* approval;
* duration;
* source provider;
* generation job;
* rights/license notes.

---

## Scene sound plan

A scene should be able to answer:

```text
What dialogue is required?
What music is required?
What ambience is required?
What effects are required?
What is missing?
What is approved?
```

---

## UI

Do not create a giant DAW.

Expose sound contextually:

### Scene

Show:

```text
Dialogue
Music
Ambience
Effects
```

### Finish

Show:

```text
Audio assets complete
Mix pending
Mix approved
Loudness QC
```

### Assets

Allow filtering by audio roles.

---

# 9. Phase F — Sequence Review

## Goal

Give the user enough editorial visibility to understand the assembled sequence without attempting to recreate Premiere, Resolve, or Final Cut.

---

## Non-goal

Do **not** build:

* multi-track keyframe editing;
* advanced transitions;
* effects compositing;
* color-grading tools;
* professional audio mixing;
* a general NLE.

The existing external-NLE escape hatch stays.

---

## Initial Sequence Review

Represent the selected scene as ordered cards:

```text
[Cut 1][Cut 2][Cut 3][Cut 4]
```

Each card displays:

```text
thumbnail
cut label
selected take
duration
audio indicator
dependency state
warning state
```

---

## Playback

Provide:

* play;
* pause;
* scrub;
* click cut to seek;
* current cut highlighting.

---

## Lightweight edit controls

Potential first controls:

```text
trim in
trim out
replace selected take
reorder where recipe permits
transition choice
```

Every change must update production state.

Do not edit an assembly file destructively.

---

## Versioning

A sequence modification should create or update a cut version.

Example progression:

```text
Selects v1
Assembly v1
Fine Cut v1
Picture Lock v1
```

---

# 10. Phase G — Provider Capability Schemas

## Problem

Different models support different combinations of:

```text
text
start image
end image
multiple references
duration
resolution
aspect ratio
audio
seed
negative prompt
camera controls
```

Provider-specific assumptions should not leak throughout the application.

---

## Goal

Expose capabilities through provider/model metadata.

---

## Proposed capability schema

Example only:

```ts
type ModelCapabilities = {
  operation:
    | "text_to_image"
    | "image_to_image"
    | "text_to_video"
    | "image_to_video"
    | "video_to_video"
    | "lip_sync"
    | "tts"
    | "music";

  inputs: {
    prompt?: boolean;
    negative_prompt?: boolean;
    start_frame?: boolean;
    end_frame?: boolean;
    reference_images?: {
      min: number;
      max: number;
    };
    audio?: boolean;
  };

  options: {
    durations?: number[];
    aspect_ratios?: string[];
    resolutions?: string[];
    seed?: boolean;
  };
};
```

---

## UI behavior

Generation controls should be derived from capability metadata.

Example:

If model does not support an end frame:

```text
Do not render the end-frame control.
```

If model supports exactly three references:

```text
Allow up to three.
```

Do not show unavailable controls disabled everywhere.

---

## Architecture rule

Keep:

```text
provider-neutral shot specification
→ provider adapter
→ concrete request
```

The shot specification must not become a provider API payload.

---

# 11. Phase H — Reusable Production Presets

## Goal

Allow users to reuse production language across projects without manually copying prompt text.

Avoid the narrower term `Brand Kit`.

Suggested user-facing names:

```text
Production Preset
Style Preset
Production Bible
```

---

## Preset contents

A preset may contain:

### Visual

```text
medium
style notes
palette
lighting direction
composition preferences
lens/camera preferences
negative visual constraints
```

### Typography

```text
title treatment
font preferences
caption treatment
safe-area preferences
```

### Audio

```text
voice preferences
music direction
mix target
```

### Delivery

```text
default aspect ratio
resolution
frame rate
platform
caption requirements
```

### Generation

```text
preferred image model
preferred video model
preferred voice model
fallback model
```

---

## Important distinction

A preset provides defaults.

It does not silently override project-specific decisions.

Once applied to a project, values should be visible and editable.

---

# 12. Phase I — Better Project Creation

## Problem

Studio currently exposes production machinery more clearly than it exposes the question:

> What are you trying to make?

Users should not need to understand the recipe architecture before creating a project.

---

## Goal

Create a useful front door without creating a restrictive wizard.

---

## New Production flow

Start with:

```text
What are you starting with?
```

Options might include:

```text
An idea
A script
Existing footage
A reference video
Audio or music
A product
Research/source material
Blank production
```

---

## Then ask:

```text
What are you making?
```

Examples:

```text
Narrative scene
Montage
Explainer
Documentary
Product video
Presenter video
Music-driven video
Custom
```

Studio uses this to recommend a recipe.

The recommendation is editable.

---

## Principle

The onboarding flow sets initial production state.

It must not become a permanent wizard.

After creation, the user lands in normal Studio.

---

# 13. Phase J — Reference-Video Analysis

## Goal

Allow an existing video to inform a new production without converting the entire source into one giant prompt.

---

## Import

User may attach:

```text
local video
downloaded reference
supported external URL later
```

URL acquisition can be implemented separately.

---

## Analysis output

Extract deterministic metadata first:

```text
duration
resolution
aspect ratio
frame rate
audio presence
shot boundaries
approximate shot durations
```

Optional analysis may infer:

```text
pacing
structure
camera-motion tendencies
dialogue/narration ratio
caption style
lighting/style notes
music tempo
```

---

## Store as structured reference data

Example:

```ts
type ReferenceVideoAnalysis = {
  duration: number;
  shots: ReferenceShot[];
  pacing?: PacingProfile;
  visual_notes?: string[];
  audio_notes?: string[];
};
```

Do not automatically convert analysis into approved production decisions.

---

## Usage

A reference could inform:

```text
production profile
animatic pacing
shot-duration suggestions
caption treatment
camera language
music direction
```

The user decides what to carry over.

---

# 14. Phase K — Fixture Productions

Before expanding the provider list substantially, validate Studio against multiple real production shapes.

At minimum build complete fixture projects for:

---

## Fixture 1 — Narrative dialogue

Tests:

```text
characters
voices
identity references
continuity
lip sync
reaction shots
take review
picture lock
```

---

## Fixture 2 — Atmospheric montage

Tests:

```text
no character requirement
music
ambience
non-dialogue pacing
recipe stage omission
```

---

## Fixture 3 — Existing-footage documentary

Tests:

```text
imported footage
source metadata
voice-over
rights notes
clip selection
sequence editing
captions
```

---

## Fixture 4 — Product advertisement

Tests:

```text
product identity
reference imagery
image-to-video
graphics
short runtime
multiple delivery aspect ratios
```

---

## Fixture 5 — Presenter/explainer

Tests:

```text
voice
presentation graphics
B-roll
captions
mixed generated/imported media
```

---

## Fixture 6 — Music-driven project

Tests:

```text
music as the source structure
beat-driven timing
no mandatory dialogue
audio-first planning
```

---

# 15. Home / Project Library

Once multiple fixtures work, improve Home.

Home should answer:

```text
What projects do I have?
What was I working on?
What needs attention?
```

Project cards can show:

```text
thumbnail
project name
recipe
last modified
current work area
blocked/pending state
latest review export
```

Possible states:

```text
Planning
Ready to Produce
Rendering
Awaiting Review
Needs Pickup
Finishing
Complete
```

These must be derived from production state rather than manually assigned.

---

# 16. Asset Library Improvements

Assets should become easier to reuse without introducing a second asset system.

The project asset registry remains authoritative.

---

## Filters

Support filtering by:

```text
image
video
audio
character
location
prop
look
reference
take
continuity
approved
unapproved
stale
```

---

## Reuse

An existing asset should be selectable wherever a compatible asset role is required.

Examples:

```text
Use approved location image as shared reference
Use existing prop image as a cut source
Attach existing ambience track
```

---

## Multi-reference selection

Provider capability metadata controls:

```text
minimum references
maximum references
reference ordering
```

---

# 17. Director Evolution

Director should become more useful through context, not autonomy.

---

## Keep

```text
scoped context
stable IDs
specialists
host validation
approval cards
model selection
stop behavior
```

---

## Add

Director should understand:

```text
selected project
selected scene
selected shot
selected asset
selected requirement
current work area
blocked dependencies
current recipe
budget state
```

---

## Useful Director requests

Examples:

```text
Why is this shot blocked?
Suggest two cheaper ways to produce this cut.
Compare these three takes.
Draft an alternate cut sketch.
What still needs approval in this scene?
Explain why Finish says stale.
```

The Director may propose actions.

The host still executes known transitions.

---

# 18. Explicit Non-Goals

Do not implement these during this roadmap unless fixture testing proves them necessary.

## No full NLE

Studio should provide sequence review and basic editorial operations.

Advanced editorial work goes to an external NLE.

---

## No node-based workflow builder

Recipes already provide workflow abstraction.

Do not add a second workflow system merely because other AI tools have graph editors.

---

## No provider-count competition

Do not measure progress by:

```text
20 models
50 models
400 models
```

A smaller set of well-described models is more useful than hundreds of opaque choices.

---

## No separate "Image Studio", "Video Studio", etc.

Media generation should happen inside production context.

The user should normally generate an image because:

```text
Scene 3 / Cut 4 requires a start frame
```

not because they navigated to an unrelated Image Generator page.

---

## No autonomous production manager requirement

Studio may eventually support more automation.

Human approval must remain a first-class workflow.

---

## No campaign/social publishing yet

Publishing, content calendars, analytics, and social account integration belong to a later product layer.

First prove production.

---

# 19. Suggested Branch / Commit Strategy

Avoid implementing this as one enormous migration.

Suggested branches:

```text
product/recipe-generalization
product/cost-governance
product/qc
product/audio-state
product/sequence-review
product/provider-capabilities
product/production-presets
product/project-onboarding
product/reference-analysis
```

Within each branch, prefer commits in the order:

```text
1. state/schema
2. host logic
3. tests
4. selectors/view models
5. UI
6. fixture verification
7. cleanup/docs
```

Do not start with CSS.

---

# 20. Required Engineering Rules

For every new capability:

### State first

Ask:

```text
What production fact are we representing?
```

before asking:

```text
What button do we need?
```

---

### Host owns truth

Readiness, dependencies, approvals, and validation belong in the host.

Frontend view models derive presentation.

---

### Preserve history

Changing an approved upstream artifact should:

```text
create a new version
mark dependents stale or needing review
preserve previous output
```

not destroy history.

---

### Unknown is not zero

This applies particularly to:

```text
cost
duration
status
progress
```

If Studio does not know something, display unknown.

---

### Provider completion is not production completion

A successful API request means:

```text
we have a candidate artifact
```

not:

```text
the shot is finished
```

---

# 21. Testing Strategy

Each phase needs three layers.

## Host tests

Verify:

```text
state transitions
versioning
dependency invalidation
recipe behavior
job lifecycle
cost logic
QC logic
```

---

## Selector/model tests

Verify:

```text
visible statuses
warnings
available actions
disabled reasons
summary counts
```

---

## Fixture acceptance tests

Run actual fixture projects through the affected workflow.

Do not rely solely on synthetic unit state.

---

# 22. Milestone Definition

The roadmap has succeeded when Studio can credibly support the following statement:

> A user can begin with an idea, script, existing footage, or reference material; plan a production appropriate to that source; generate or import required assets; review and select takes; preserve continuity and versions; assemble and review a sequence; track cost and production state; finish audio and picture; validate a deliverable; and export the project without the AI Director becoming the source of truth.

At that point, Studio is no longer primarily an AI-generation interface.

It is a production environment.

---

# 23. Immediate Next Actions

Do not begin the full roadmap at once.

The immediate work queue is:

### Now

Finish:

```text
Production_Studio_UI_Redo_Plan.md
```

and prove the existing narrative vertical slice.

### Immediately afterward

Create:

```text
Phase B — Recipe and Stage Generalization
```

before adding more recipes.

### Then

Implement:

```text
Cost / Jobs
Technical QC
Audio production state
Sequence Review
```

These four provide the largest improvement to Studio as a production tool.

### After those stabilize

Implement:

```text
Provider capability schemas
Production presets
New Production onboarding
Reference-video analysis
```

### Throughout

Run multiple fixture productions.

If a feature does not improve one of those productions, reconsider whether Studio needs it.

---

# 24. Product Rule of Thumb

When evaluating a future feature, ask:

> Does this help the user make, understand, review, control, or finish the production?

If yes, it probably belongs in Studio.

If it mainly answers:

> Can we expose another AI thing?

it probably does not.
