# Traditional Studio Production Model for the Workbench

Date: 2026-09-23  
Status: Product and architecture direction  
Related:

- `Dev_Notes/V2/Workflow_Migration_Plan.md`
- `Dev_Notes/V1/Workbench_Direction.md`
- `Dev_Notes/V1/Workbench_Spec.md`

---

## 1. Purpose

The current workflow began with a deliberately simple model:

```text
make a script
-> create the characters
-> get reference stills
-> set up the shots
-> shoot the video
```

That is a useful backbone. Its dependency order is broadly correct for a
narrative project, and it gave the Workbench a practical place to start.

The problem is not that this model is wrong. It is that each step compresses
several distinct production responsibilities, artifacts, approvals, and
feedback loops.

A production studio does not move through one uninterrupted waterfall.
Departments work in parallel, cheap planning work happens before expensive
production, footage is reviewed as takes rather than assumed to be final, and
post-production is a major phase rather than a final concatenation step.

The Workbench should adopt those principles without copying the bureaucracy of
a physical studio or creating a separate chat room for every department.

The target remains:

```text
one authoritative project
+ visible production artifacts
+ scoped creative agents
+ host-owned mechanics
+ human approval at meaningful gates
```

---

## 2. Core Recommendation

Use a traditional studio as a model for:

- Responsibilities.
- Artifacts.
- Dependencies.
- Approval gates.
- Versioning.
- Reviews.
- Production status.

Do not use it as a model for:

- A large organization chart.
- Independent agent conversations with separate memories.
- Rigid handoffs that prevent iteration.
- A single mandatory workflow for every kind of video.
- Recreating a full nonlinear editor inside the Workbench.

The Workbench should remain one shared production state. “Departments” are
bounded areas of responsibility operating on that state.

---

## 3. Expanded Production Lifecycle

### Phase 1: Development and Greenlight

Purpose: decide what is being made, why it should exist, and what constraints
define success.

Typical artifacts:

- Creative brief.
- Audience and platform.
- Deliverable list.
- Treatment or concept.
- Source material and research.
- Target runtime.
- Aspect ratios and platform variants.
- Budget or generation-spend limit.
- Schedule or deadline.
- Rights and licensing notes.
- Safety and content constraints.
- Initial feasibility risks.

Approval gate:

> The project has a defined purpose, audience, deliverable, scope, and
> production limit.

The Workbench should not assume that every project begins with a screenplay.

Possible starting sources include:

- Script.
- Story or chapter.
- Product brief.
- Music track.
- Interview or transcript.
- Existing footage.
- Research packet.
- Presentation.
- Visual concept.
- User-recorded performance.

The selected production recipe determines what happens next.

---

### Phase 2: Script and Production Breakdown

Purpose: turn the source material into a production plan.

Writing the script is only the first half of this phase. The script must then
be broken down into everything the production needs to design, acquire,
record, generate, or clear.

Breakdown categories may include:

- Cast and voices.
- Locations and sets.
- Props.
- Wardrobe.
- Hair and appearance continuity.
- Vehicles.
- Creatures.
- Practical effects.
- Visual effects.
- Graphics and titles.
- Screen content.
- Dialogue.
- Voice-over.
- Music.
- Sound effects and ambience.
- Stunts or complex action.
- Sensitive content.
- Source-material rights.
- Expensive or technically risky shots.

Typical artifacts:

- Versioned script.
- Scene, segment, or beat list.
- Production breakdown.
- Requirements registry.
- Initial continuity requirements.
- Risk list.
- Rough runtime estimate.

Approval gate:

> Every production requirement is identified or deliberately deferred.

#### Implication for the Workbench

Character creation should not automatically be the only step after script.
The script breakdown may reveal that the next priority is:

- A location test.
- A prop design.
- A voice recording.
- A music timing pass.
- An archival-footage search.
- A difficult VFX proof.
- A product reference.

Characters are one production requirement among several.

---

### Phase 3: Casting and Production Design

Purpose: establish the canonical world of the production.

This phase includes more than character model sheets.

Responsibilities:

- Character design and casting.
- Voice casting and tests.
- Locations and set design.
- Props.
- Wardrobe.
- Makeup or appearance.
- Material and surface language.
- Color palette.
- Lighting direction.
- Graphics language.
- World and period rules.
- Continuity bible.

Typical artifacts:

- Character bible.
- Casting or voice choices.
- Identity references.
- Body and proportion references.
- Expression and performance references.
- Wardrobe designs.
- Location and set designs.
- Prop designs.
- Style frames.
- Color and lighting bible.
- Approved design variants.

Approval gate:

> Canonical production designs are approved and locked by role.

#### Reference assets need explicit semantics

“Reference still” is too broad. The Workbench should distinguish at least:

- Identity reference.
- Body/proportion reference.
- Expression reference.
- Wardrobe reference.
- Location or set reference.
- Prop reference.
- Style reference.
- Lighting reference.
- Composition reference.
- Group reference.
- Shot-specific plate.
- Start frame.
- End frame.
- Continuity frame.
- Edit source.

An asset becomes canonical because the user approves it for a named role, not
because it happens to exist in a folder.

Approved assets should be versioned and locked against silent replacement.

---

### Phase 4: Storyboards, Previsualization, and Animatic

Purpose: prove that the piece works as an edit before expensive generation.

This is the most important addition to the current simplified workflow.

The production should be edited in rough form before final shots are rendered.

Typical artifacts:

- Storyboards.
- Shot thumbnails.
- Blocking diagrams.
- Staging notes.
- Temporary voice performances.
- Scratch dialogue.
- Temporary music.
- Rough sound effects.
- Timed animatic.
- Sequence timing.
- Coverage warnings.
- Missing transition warnings.

An animatic can be visually rough. Its value is timing and structure, not
finish quality.

It answers questions such as:

- Does the story make sense?
- Is the pacing correct?
- Is every important beat visible?
- Are there too many talking shots?
- Are there enough reactions, inserts, and establishing shots?
- Are transitions motivated?
- Is a shot needlessly expensive?
- Does dialogue need to be shortened?
- Does the edit require footage that has not been planned?
- Can a difficult shot be split into simpler shots?

Approval gate:

> The sequence works in rough editorial form before final production begins.

#### Why this matters more for generative video

Generative footage is expensive in time, money, and review effort. A weak edit
cannot be repaired by generating more attractive individual clips.

The animatic should determine:

- Planned shot order.
- Approximate screen duration.
- Dialogue placement.
- Music and sound timing.
- Required coverage.
- Transition requirements.
- Which shots justify higher generation spend.

---

### Phase 5: Shot Packages and Production Plan

Purpose: convert the approved animatic into executable work.

“Set up the shots” should become a formal shot-packaging stage.

Each shot package may contain:

- Stable shot ID.
- Sequence and display order.
- Narrative or communication purpose.
- Source beat coverage.
- Shot role.
- Subject.
- On-camera participants.
- Blocking and action.
- Performance direction.
- Framing and camera intent.
- Camera movement.
- Lens or perspective intent.
- Approximate duration.
- Dialogue and voice treatment.
- Sound requirements.
- Music relationship.
- VFX and compositing requirements.
- Continuity opening state.
- Continuity closing state.
- Carry-forward requirements.
- Approved source assets.
- Start/end frames when required.
- Negative constraints.
- Execution strategy.
- Required model capabilities.
- Dependencies.
- Expected cost.
- Approval status.

Possible execution strategies include:

- Text-to-video.
- Image-to-video.
- Character-reference video.
- Location/prop-reference video.
- Audio-driven performance.
- Keyframe interpolation.
- Video-to-video.
- Focused video edit.
- Compositing.
- Motion graphics.
- Screen capture.
- Existing-footage edit.
- Local deterministic rendering.

The production plan should also include:

- Shot schedule.
- Dependency graph.
- Provider/model feasibility.
- Spend estimate.
- Priority.
- Retry limit.
- Fallback strategy.
- Expected review owner.

Approval gate:

> Every scheduled shot is creatively approved, technically feasible, and
> assigned an execution strategy.

---

### Phase 6: Production / Shoot

Purpose: create footage and sound as traceable takes.

The core rule is:

> A generated clip is a take, not automatically a completed shot.

Every provider generation or local render should create a recorded attempt.

Take metadata should include:

- Take ID.
- Shot ID.
- Attempt number.
- Exact input artifacts.
- Prompt and negative prompt.
- Model.
- Provider.
- Model version when available.
- Normalized parameters.
- Seed when available.
- Duration.
- Aspect ratio and resolution.
- Audio inputs.
- Start/end frames.
- Cost.
- Start and finish timestamps.
- Provider job ID.
- Output checksums.
- Error or warning information.
- Parent attempt when retried or edited.

Equivalent traditional-production records:

- Slate.
- Camera report.
- Sound report.
- Continuity report.
- Daily production report.

The Workbench should support:

- Multiple takes per shot.
- Alternate takes.
- Rejected takes.
- Focused edits.
- Rerenders.
- Pickups.
- Extracted continuity frames.
- Version comparison.
- Notes tied to exact versions.

---

### Phase 7: Dailies and Take Review

Purpose: decide what footage survives.

Traditional productions review dailies rather than waiting until the entire
shoot is finished. Generative production needs the same loop.

Take statuses may include:

- Unreviewed.
- Selected.
- Alternate.
- Rejected.
- Needs edit.
- Needs rerender.
- Needs pickup.
- Continuity problem.
- Technical failure.
- Used in current cut.

Review questions include:

- Does the performance work?
- Is identity stable?
- Is the action correct?
- Does the shot start and end in usable states?
- Does it cut with neighboring shots?
- Is dialogue synchronized?
- Are props, wardrobe, location, and lighting continuous?
- Are there unwanted artifacts?
- Does the clip provide the planned coverage?
- Is the best response an edit, rerender, pickup, or editorial workaround?

Approval gate:

> A selected take or an explicit replacement plan exists for every required
> shot.

---

### Phase 8: Editorial and Post-Production

Purpose: turn selected takes into the finished piece.

Assembly is only the beginning.

Editorial stages:

- Selects.
- String-out.
- Assembly cut.
- Rough cut.
- Fine cut.
- Picture lock.

Post-production responsibilities:

- Visual effects.
- Compositing.
- Cleanup.
- Reframing.
- Stabilization.
- Speed changes.
- Color correction.
- Color grade.
- Dialogue editing.
- ADR or replacement dialogue.
- Foley.
- Sound effects.
- Ambience.
- Music edit.
- Music licensing.
- Sound mix.
- Loudness normalization.
- Titles.
- Lower thirds.
- Credits.
- Captions and subtitles.

Approval gate:

> Picture and sound are approved for mastering.

The Workbench does not need to become a complete NLE. It should:

- Manage versions and selected takes.
- Build deterministic edit plans.
- Produce review exports.
- Handle straightforward ffmpeg work.
- Keep exact editorial decisions in project state.
- Open an external NLE when the work becomes taste-heavy or frame-specific.

---

### Phase 9: Quality Control, Delivery, and Archive

Purpose: ensure the production is technically correct, legally usable, and
recoverable.

Creative QC:

- Story and message clarity.
- Continuity.
- Performance.
- Identity drift.
- Visual artifacts.
- Pacing.
- Music and sound balance.

Technical QC:

- Resolution.
- Aspect ratio.
- Frame rate.
- Codec.
- Color space.
- Audio channels.
- Loudness.
- Lip sync.
- Caption timing.
- Safe areas.
- Text legibility.
- Black frames.
- Missing or duplicated shots.

Delivery artifacts:

- Master.
- Platform-specific variants.
- Caption files.
- Audio-only or clean versions when needed.
- Thumbnails and poster frames.
- Metadata.
- Approval report.

Archive artifacts:

- Final project manifest.
- Source and selected assets.
- Job and model history.
- Prompts and normalized parameters.
- Licenses and consent records.
- Checksums.
- Fonts and graphics.
- Final scripts and production notes.
- External-NLE project files when applicable.

Approval gate:

> Every named deliverable passes creative and technical QC, and the project
> can be reconstructed from its archive.

---

## 4. Departments as Workbench Responsibilities

### Production

Owns:

- Scope.
- Schedule.
- Dependencies.
- Budget and provider spend.
- Approvals.
- Rights.
- Risk.
- Production status.
- Delivery requirements.

The Production desk should answer:

- What is blocked?
- What is ready?
- What is waiting for approval?
- What has already cost money?
- What must be completed for the next review?

### Direction

Owns:

- Creative intent.
- Performance.
- Blocking.
- Shot purpose.
- Continuity decisions.
- Dailies notes.
- Selection and pickup decisions.

Direction is where human judgment remains most important.

### Art and Design

Owns:

- Characters.
- Casting appearance.
- Sets and locations.
- Props.
- Wardrobe.
- Materials.
- Style frames.
- Color and lighting bible.
- Approved visual canon.

### Camera and Animation

Owns:

- Framing.
- Camera movement.
- Perspective.
- Motion and timing.
- Shot execution strategy.
- Model capability requirements.
- Start/end frame requirements.

### Sound

Owns:

- Voice casting.
- Dialogue recording.
- Scratch tracks.
- Voice-over.
- ADR.
- Music.
- Ambience.
- Sound effects.
- Foley.
- Mix.
- Loudness.

Sound should not be postponed until final assembly. Dialogue and music often
determine shot timing before production.

### Editorial and Post

Owns:

- Coverage.
- Takes and selects.
- Cut versions.
- VFX.
- Compositing.
- Color.
- Titles.
- Captions.
- Masters.

### Archive and Delivery

Owns:

- Provenance.
- Licenses.
- Consent and likeness records.
- Model and parameter history.
- Checksums.
- Export variants.
- Final project package.

---

## 5. Animation and Clay-Style Production

An animated project should not be forced through a live-action ordering.

A useful animation flow is:

```text
brief / treatment
-> script
-> voice or scratch track
-> storyboard
-> timed animatic
-> character and world design
-> shot layout
-> animation / generation
-> lighting and render
-> compositing
-> editorial and sound
-> QC and delivery
```

Several stages can run in parallel. For example, character exploration can
begin while the script is developing, but canonical character approval should
occur before expensive identity-dependent shots.

### Clay or stop-motion-inspired production bible

For a clay-style project, the Production Profile should be able to record:

- Puppet scale.
- Character proportions.
- Clay or material texture.
- Fingerprints and tactile surface treatment.
- Replacement-face or expression language.
- Eye and mouth construction.
- Hair construction.
- Wardrobe materials.
- Joint and rig visibility.
- Range-of-motion limitations.
- Animation cadence.
- Pose and settle behavior.
- Set scale.
- Prop construction.
- Lighting scale and softness.
- Depth-of-field language.
- Camera movement limits.
- Frame-rate or stepped-motion treatment.
- Compositing rules.
- Continuity rules.

These are project-level invariants, not phrases that should be rediscovered in
every prompt.

Possible animation-specific artifacts:

- Character turnarounds.
- Scale chart.
- Expression sheet.
- Pose sheet.
- Mouth/phoneme set.
- Hand or replacement-part sheet.
- Set elevations.
- Prop scale references.
- Lighting tests.
- Motion tests.
- Animation tests.
- Shot layout.
- Exposure or timing sheet.

Even when a generative model simulates clay rather than photographing physical
puppets, these artifacts improve consistency and make the production
directable.

---

## 6. Agentic Production Requirements

A traditional studio model does not by itself solve the risks introduced by
agents and generative providers.

### 6.1 Agents propose; the host commits

Agents may:

- Draft creative options.
- Recommend shots.
- Write prompts.
- Suggest edits.
- Diagnose failed takes.

The host should own:

- IDs.
- Paths.
- Schemas.
- State transitions.
- Approvals.
- Model routing.
- Provider parameters.
- Spend policy.
- File writes.
- Version creation.
- Lineage.

### 6.2 Approved artifacts are immutable versions

An approval points to an exact asset version.

Later changes create a new version. They do not silently alter the approved
asset in place.

### 6.3 Reproducibility is a production record

Every generated or transformed artifact should retain:

- Input artifact IDs.
- Tool or provider.
- Model.
- Parameters.
- Prompt.
- Timestamp.
- Cost.
- Parent attempt.
- Output checksum.

Some providers are nondeterministic even with complete records. The goal is
traceability and best-effort reproduction, not a false promise of identical
pixels.

### 6.4 Branches are normal

Creative production often needs:

- Alternate designs.
- Alternate edits.
- Alternate performances.
- Alternate aspect ratios.
- Client versions.
- Platform versions.

The state model should support branches and promotion rather than forcing one
mutable “current” file.

### 6.5 Capability tests precede dependency

Before a production plan depends on a model, run a small test for the required
capability:

- Character consistency.
- Multi-character composition.
- Lip sync.
- Camera motion.
- Prop preservation.
- Start/end frame control.
- Duration.
- Video editing.
- Text rendering.

A model listing or marketing label is not proof that it can execute the shot.

### 6.6 Rights and provenance are first-class

Track:

- Source-material rights.
- Music licenses.
- Stock licenses.
- Voice consent.
- Likeness consent.
- Model and provider terms.
- Fonts.
- Uploaded private assets.
- Required attribution.

### 6.7 Quality control must include generative failures

In addition to normal technical QC, check:

- Identity drift.
- Wardrobe drift.
- Prop drift.
- Background mutation.
- Unwanted lip movement.
- Extra limbs or people.
- Temporal instability.
- Camera-direction errors.
- Incorrect text.
- Audio artifacts.
- Inconsistent frame geometry.

---

## 7. Workflow Shape: Graph, Not Waterfall

The default narrative path may look linear:

```text
brief
-> script
-> breakdown
-> design
-> storyboard
-> animatic
-> shot packages
-> production
-> dailies
-> post
-> delivery
```

The actual workflow is a dependency graph.

Examples:

- A voice test may cause a character rewrite.
- An animatic may require a shorter scene.
- A model capability test may change the shot design.
- Dailies may reveal that a continuity plate is missing.
- An edit may remove an ungenerated shot.
- A failed hero shot may be replaced by two simpler shots.
- Music timing may alter shot durations.
- A legal issue may replace an asset late in post.

The Workbench should permit backward movement while making the consequences
visible.

Changing an approved upstream artifact should mark dependent artifacts as:

- Potentially stale.
- Needs review.
- Invalid.
- Safe to keep.

It should not automatically destroy downstream work.

---

## 8. Multiple Entry Points and Production Recipes

No universal workflow should require every project to begin with script and
characters.

Example recipes:

### Narrative Dialogue

```text
script -> breakdown -> cast/design -> boards -> animatic -> shots -> takes -> edit
```

### Montage or Music Video

```text
track -> beat/timing map -> visual treatment -> boards -> shots -> takes -> edit
```

### Presenter or Explainer

```text
brief -> outline -> script -> voice/presenter -> graphics/B-roll plan -> production -> edit
```

### Product Film or Advertisement

```text
product brief -> claims/constraints -> treatment -> product refs -> boards -> hero shots -> edit
```

### Documentary or Archival Piece

```text
research/assets -> transcript -> story edit -> rights -> gap list -> pickups/VO -> edit
```

### Abstract Animation or Motion Graphics

```text
concept/music/data -> visual system -> timing map -> style frames -> motion tests -> render -> composite
```

The recipe defines:

- Stages.
- Dependencies.
- Required artifact roles.
- Optional stages.
- Forms.
- Validators.
- Readiness rules.
- Suggested folder layout.
- Fast paths.

The stable Workbench kernel should not need new enums and UI switches for each
recipe.

---

## 9. Implications for the Studio Architecture

### 9.1 Add a Production Profile

Project-wide state should include:

- Production type or recipe.
- Deliverables.
- Audience.
- Platforms.
- Runtime.
- Aspect ratios.
- Resolution.
- Frame rate.
- Language.
- Medium.
- Style bible.
- Audio plan.
- Continuity rules.
- Safety/content constraints.
- Budget.
- Schedule.
- Rights notes.

Clay animation belongs in this profile. It should not be a hard-coded
`is_claymation` branch.

### 9.2 Use immutable identity

Do not use display order as identity.

Use stable IDs for:

- Sources.
- Sequences.
- Segments.
- Scenes.
- Shots.
- Characters.
- Production entities.
- Assets.
- Artifact versions.
- Jobs.
- Attempts.
- Takes.
- Edits.
- Deliverables.

Scene and shot numbers remain editable display labels.

### 9.3 Make artifacts the center of the system

An artifact record should include:

- Stable ID.
- Type.
- Role.
- Scope.
- Version.
- Path or external reference.
- Status.
- Approval.
- Parent.
- Dependencies.
- Source job.
- Metadata.
- Checksum.
- Created time.

### 9.4 Use one authoritative job ledger

Every provider call and local operation should use the same:

- Job record.
- Attempt record.
- Approval policy.
- Progress model.
- Error model.
- Retry lineage.
- Output registration.

### 9.5 Separate creative specifications from provider requests

Store provider-neutral artifacts such as:

- Creative Shot Spec.
- Dialogue Performance Spec.
- Still Composition Spec.
- Edit Spec.
- Assemble Spec.

Provider adapters compile those specs into API-specific requests as late as
possible.

### 9.6 Make the UI stage-first and review-first

The production desk should expose:

- Current stage.
- Readiness.
- Blockers.
- Dependencies.
- Approvals.
- Cost.
- Jobs.
- Versions.
- Dailies.
- Takes.
- Selected takes.
- Stale downstream artifacts.

Chat remains a scoped creative collaborator, not the workflow container.

---

## 10. Recommended Implementation Order

### 1. Finish the existing narrative vertical slice

Complete:

```text
approved intent/look/sketch/start frame
-> canonical shot specification
-> render job
-> take registration
-> take review
-> selected take
-> assembly
-> review export
```

This proves the Workbench can complete a production, not only plan one.

### 2. Add the Production Profile and script breakdown

Make project-wide creative and technical constraints explicit.

Add a typed requirements registry so characters are no longer the only
first-class production element.

### 3. Add storyboard and animatic state

Support rough images, temporary audio, ordered timing, and coverage validation
before final production.

### 4. Stabilize IDs, versions, artifacts, and jobs

Do this before adding many more stages. Otherwise every stage will deepen the
current coupling.

### 5. Introduce recipe-defined stage graphs

Keep Narrative Dialogue as the first built-in recipe. Prove at least one
non-dialogue recipe before treating the abstraction as complete.

### 6. Add post-production and delivery state

Represent:

- Cut versions.
- Picture lock.
- Audio mix.
- VFX status.
- Captions.
- Mastering.
- QC.
- Deliverables.

### 7. Prove breadth with fixture productions

Test:

- Clay dialogue comedy.
- No-dialogue atmospheric montage.
- Presenter explainer.
- Product advertisement with no character registry.
- Documentary using existing footage and voice-over.
- Abstract animation driven by music.

---

## 11. Anti-Patterns to Avoid

### Do not add style-specific workflow branches

Avoid:

```text
is_claymation
is_anime
is_live_action
```

These are Production Profile values and recipe inputs, not core workflow
branches.

### Do not try to fix architecture with larger prompts

Prompts cannot repair:

- Mutable IDs.
- Closed stage enums.
- Fixed folder assumptions.
- Missing lineage.
- Missing take review.
- Missing post state.
- Provider leakage.

### Do not make every department an autonomous agent

That recreates context-switching and split-brain state.

Use scoped tools and specialists over one project record.

### Do not treat generation as completion

Provider success means a take exists. It does not mean:

- The take is good.
- The take matches continuity.
- The take is selected.
- The shot is complete.
- The edit works.

### Do not force every production through screenplay concepts

Scenes, dialogue, characters, and mouths are important for one recipe, not
universal primitives for all video.

---

## 12. Product Principle

The Workbench should feel like a production studio because it provides:

- A shared project.
- Clear responsibilities.
- Visible artifacts.
- Production history.
- Reviews and approvals.
- Traceable takes.
- Editorial decisions.
- Delivery confidence.

It should not feel like a production studio because it imitates administrative
overhead.

The concise target is:

```text
Development decides what to make.
Preproduction proves how it will work.
Production creates traceable takes.
Dailies decide what survives.
Post turns selections into a finished piece.
QC and delivery make the result usable.

Agents assist each responsibility.
The host owns state and mechanics.
The user owns approval and taste.
```
