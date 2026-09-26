# Get Studio to Production

**Status:** Working execution plan  
**Scope:** `Studio/Desktop_App` and the production workflow docs under `Dev_Notes/V2`  
**Primary goal:** Move Studio from a powerful local prototype to a production-safe desktop app with a coherent Studio UI, trustworthy security boundaries, durable production state, and enough fixture coverage to keep improving it without breaking the core workflow.

This document consolidates and orders:

- `Dev_Notes/V2/Production_Studio_UI_Redo_Plan.md`
- `Dev_Notes/V2/Desktop_App_Security_Review.md`
- `Dev_Notes/V2/Studio_Hardening.md`

The source docs remain useful detail references. This document owns the execution order.

---

# 1. Executive Order

Do the work in this order.

```text
0. Move sep_13 branch to main and establish a clean baseline
1. Security containment
2. Security scoping, approval binding, and least privilege
3. Robustness, audits, and release safety
4. Finish the current Studio UI vertical slice
5. Migrate the Studio UI surface by surface
6. Generalize recipes and stages
7. Add cost, jobs, QC, audio, sequence review, and provider capabilities
8. Add production presets, onboarding, reference-video analysis, fixtures, Home, and Assets polish
```

Do not start product expansion while the app still has known code-execution paths, unscoped filesystem access, or exposed secrets.

---

# 2. Product Direction

Studio should not compete primarily on:

- number of supported models;
- autonomous one-prompt video generation;
- social-media automation;
- a node-based workflow builder;
- recreating a nonlinear editor;
- separate mini-studios for every media type.

Studio's direction is:

> A production workspace for creating AI-assisted video without giving up creative control.

The production state, not the Director chat and not any provider, is the source of truth.

The app should help the user:

1. define what they are making;
2. plan how it will be made;
3. generate or acquire assets;
4. compare versions and takes;
5. approve and reject work;
6. track dependencies;
7. assemble a sequence;
8. review and revise it;
9. finish and validate deliverables.

---

# 3. Non-Negotiable Principles

## 3.1 State First

Ask:

```text
What production fact are we representing?
```

before asking:

```text
What button do we need?
```

The UI reads shared workflow state. It must not create parallel durable state.

## 3.2 Host Owns Truth

Readiness, dependencies, approvals, filesystem boundaries, provider access, and validation belong in the host.

Frontend view models derive presentation from host-owned state.

## 3.3 Stable Identity Is Not Display Order

Scenes, shots, assets, jobs, takes, and versions need stable IDs.

Display labels such as `Scene 2` or `Cut 4` are presentation.

Reordering must not change:

- Director scope;
- take ownership;
- dependencies;
- source bindings;
- version history.

## 3.4 Provider Output Is Candidate Work

A successful API request means:

```text
we have a candidate artifact
```

not:

```text
the shot is finished
```

Approval, selected takes, picture lock, QC, and deliverable state remain authoritative.

## 3.5 UI Is A Cross-Cutting Workstream

UI is not one giant separate phase.

For each feature:

```text
state/schema
-> host logic
-> tests
-> selectors/view models
-> UI
-> fixture verification
-> cleanup/docs
```

Known transitions belong to buttons and structured UI. Director can assist, but must not be the only way to operate Studio.

## 3.6 Unknown Is Not Zero

This applies to:

- cost;
- duration;
- progress;
- status;
- readiness.

If Studio does not know something, display unknown.

## 3.7 Imported Content Is Untrusted

Treat these as hostile until validated:

- model output;
- project files;
- handoff YAML;
- copied scripts and research;
- external media;
- preset files;
- provider responses;
- renderer requests.

Every path that did not come from a trusted native dialog or host-derived project state must be checked.

---

# 4. Phase 0 - Move `sep_13` To `main`

## Goal

Make `main` the real baseline before security and product hardening work begins.

## Work

- Confirm the current `sep_13` branch is the intended source of truth.
- Ensure the working tree is clean or intentionally documented.
- Move or merge `sep_13` to `main`.
- Tag or note the pre-security baseline.
- Run the basic app checks after the move.

## Baseline Checks

Run, where applicable:

```bash
cd Studio/Desktop_App
npm run build
cd host
cargo test
```

Also smoke test:

- app opens;
- project opens;
- `Lite_Challenge` loads;
- existing Studio/Production Desk still works;
- Settings still load.

## Done When

- `main` contains the intended current app.
- The pre-security baseline is recoverable.
- Security work can happen without branch ambiguity.

---

# 5. Phase 1 - Security Containment

This phase comes before product hardening.

It closes the most direct code-execution and credential-leak paths.

## 5.1 ProjectPath Resolver

Build one host-owned path resolver and route risky paths through it.

Rules:

- Reject empty input and NUL bytes.
- Require a project root for project reads and writes.
- Join relative paths to the project root.
- Allow absolute paths only when they resolve under the project root or were granted by a trusted dialog flow.
- Lexically reject `..` that escapes the root.
- Canonicalize the root.
- Canonicalize the deepest existing ancestor of the candidate.
- Use component-based `Path::starts_with`, not string prefix checks.
- Reject sibling-prefix tricks like `/project-evil`.
- Reject dangling or escaping symlinks for writes.
- For writes, reject paths with newline characters and other provider/tool-dangerous forms where relevant.

Replace or wrap:

- `director_actions.rs` `resolve_under_project`
- `atlas.rs` destination and existing-file resolvers
- `media_util.rs` path and write helpers
- image/video adapter input resolvers
- workflow delete and state-file path resolvers
- scene-plan and asset resolvers where they accept raw strings

## 5.2 Text Specialist Write Controls

The model must not choose arbitrary write paths.

Implement:

- open-project required for all writes;
- fixed resolver required for all writes;
- per-specialist output directory and extension allowlists:
  - script: `Scripts/**/*.md` or `.txt`
  - character creation: `Characters/**/*.md`
  - production helper digest: approved research/digest locations only
- overwrite handling:
  - host-derived default paths can write normally;
  - overwrites or unusual paths are staged for approval;
  - approval card shows a diff or concise summary.
- cap file count and bytes per model reply.

Preferred long-term pattern:

```text
model emits content
host derives destination
host writes after validation
```

Scene Plan already follows this spirit. Copy it.

## 5.3 Electron Navigation Lockdown

Prevent remote pages from inheriting the preload bridge.

Implement:

- deny main-window navigation except the app origin;
- deny frame navigation and redirects except app origin;
- deny `window.open`;
- intercept preview/docs links and open external links in the system browser after confirmation;
- validate allowed external schemes as `http:` and `https:` only;
- block or explicitly control remote preview images;
- expose the preload bridge only on the expected app origin.

## 5.4 Settings File Permissions

Immediately:

- write `settings.json` with mode `0600`;
- create config directory with mode `0700`;
- chmod existing settings on load or startup;
- migrate old settings without preserving unsafe permissions.

Manual one-time local fix:

```bash
chmod 600 ~/.config/video-workbench/config/settings.json
```

## 5.5 Phase 1 Tests

Host tests:

- traversal rejects `newdir/../../x`;
- sibling prefix rejects `<root>-evil/x`;
- absolute outside path rejects;
- dangling symlink rejects;
- symlink directory pointing outside rejects;
- no-project absolute path rejects;
- text specialist cannot write outside allowed directory;
- text specialist cannot overwrite protected state files;
- production helper digest follows the same rules.

Electron smoke tests:

- preview links open externally after confirm;
- Electron window stays on the app;
- `window.open("https://example.com")` is denied;
- remote preview images are blocked or explicitly controlled.

## Done When

The app no longer has the known direct paths from model or markdown content to arbitrary local filesystem writes or bridge access.

---

# 6. Phase 2 - Security Scoping And Approval Binding

## Goal

Bind sensitive actions to host-controlled authority instead of chat text, renderer trust, or unscoped paths.

## 6.1 Host-Side Pending Actions

Replace history markers as authority.

Implement:

- pending actions stored host-side;
- random action ID;
- expiry, roughly 10 to 15 minutes;
- approval button calls `director_approve({ id })`;
- any unrelated user message cancels the pending action;
- `continue` resumes planner work only, never approves spend;
- strip pending markers from model-visible and user-visible output.

Approval cards must show:

- provider;
- model;
- operation;
- cost estimate when known;
- output path;
- every file that will be read or uploaded;
- every artifact that will be modified.

## 6.2 Provider Adapter Scoping

All provider adapters must accept scoped project paths or explicit grants, not raw strings.

Implement:

- media-type allowlists by role;
- magic-byte sniffing for image/audio/video;
- size caps;
- project-root confinement;
- upload path disclosure in approval cards.

Applies to:

- start frames;
- reference images;
- source videos;
- voice clones;
- handoff-internal paths;
- external imported media.

## 6.3 Director Reads And Attachments

Require a project for Director inspect, dossiers, and attachments unless the path came from an explicit one-time grant.

Add a hard denylist regardless of project root:

- settings directories;
- `.ssh`;
- `.gnupg`;
- credential files;
- `.env`;
- known private key names.

## 6.4 Workflow Deletes

Deletes must use `ProjectPath` plus expected-subdirectory checks.

Do this for:

- delete take;
- wardrobe plate delete;
- start-frame delete;
- scratch audio clear;
- any future asset cleanup.

Prefer trash over permanent unlink where possible.

Validate state-file paths on project load and reject absolute paths or `..`.

## 6.5 Secret Handling

Move toward:

```text
renderer sees hasKey
host owns actual key
provider calls use apiId
```

Short term:

- redact known key values from host responses;
- use `error.without_url()` or equivalent before formatting reqwest errors;
- for Gemini, use `x-goog-api-key` header instead of `?key=...`.

Long term:

- Electron `safeStorage` or Secret Service/libsecret;
- settings JSON stores metadata and `hasKey`, not plaintext secrets.

## Done When

- Model-authored pending markers cannot execute.
- `ok` cannot approve an old paid job.
- Provider uploads cannot read outside the project or grant.
- Sensitive local files cannot enter model context through inspect or attachment paths.
- Host responses do not contain key material.

---

# 7. Phase 3 - Renderer Least Privilege And Release Safety

## Goal

The renderer should not be a trusted filesystem and credential broker.

## 7.1 Host Project Session

Add host-side project session state:

- main process opens project with native dialog;
- host stores the current project root;
- renderer references project state but does not define root authority;
- every filesystem command is scoped through `ProjectPath`;
- outside-project access requires a one-time grant.

## 7.2 Renderer Command Allowlist

In Electron main:

- allowlist command names;
- validate `event.senderFrame.url`;
- remove dead `shell:openPath` or scope it tightly;
- dialogs become the only trusted source of outside paths.

## 7.3 Renderer And Browser Hardening

Implement:

- CSP;
- `sandbox: true`;
- deny-all permission handlers;
- media protocol scoped to project and grants;
- remove `bypassCSP`;
- DevTools dev-only;
- explicit dev mode flag such as `WORKBENCH_DEV=1`;
- packaged/non-dev launches load built files, not `localhost`;
- dev server restricted to `127.0.0.1`.

## 7.4 SSRF And Resource Limits

Implement:

- allowlisted ElevenLabs preview hosts;
- `https`-only downloads where possible;
- max download size;
- streaming downloads to disk;
- bounded worker pool or semaphore;
- caps on text reads and media reads.

## 7.5 ffmpeg Safety

Implement:

- canonical local file paths;
- reject newlines in paths;
- prefix local paths with `file:` where appropriate;
- avoid `-safe 0` unless inputs are fully generated and escaped by the host.

## 7.6 Audit Tooling

Add or run:

- `npm audit fix` where safe;
- `cargo-audit`;
- dependency audit checks before release.

When packaging exists:

- Electron fuses;
- host binary signing;
- disable dev-only host binary overrides in non-dev builds.

## Done When

The renderer can no longer read, write, delete, fetch secrets, or upload files unless the host has explicitly scoped and authorized the operation.

---

# 8. Phase 4 - Studio UI Vertical Slice

This phase finishes the current `Production_Studio_UI_Redo_Plan.md` work after the security foundation is safe enough to build on.

## Goal

Prove one project can move through the current production architecture from planning to reviewable output without bypassing workflow state.

Use `Lite_Challenge` Scene 1 as the primary fixture.

## 8.1 Complete Assembly Lifecycle

Implement:

- reconcile a successful assemble output on import;
- mark assembly job and current attempt completed;
- register output as assembly or review-export artifact;
- associate output with scene and cut version;
- mark selected takes as used without losing selected identity;
- derive Assemble readiness from real output or cut version;
- add Open Review Export action;
- preserve failed and previous assembly attempts.

Done when:

- a prepared local assembly becomes a completed job after refresh;
- Assemble stage becomes present or approved from state;
- review export opens from Studio;
- reassembly creates a new version.

## 8.2 Reviewable Continuity Frame Choice

Implement:

- preview selected predecessor take;
- default candidate time near the end;
- expose frame-time control;
- extract frame at chosen time;
- persist frame time, source take ID, parent asset ID, and artifact version;
- require explicit binding or confirmation after predecessor selection changes;
- show dependent shots that become stale before replacing binding.

Done when:

- user can choose the frame;
- successor remains blocked until the frame is bound;
- replacement does not silently approve a new continuity frame.

## 8.3 Stable Director Scope

Add optional stable IDs to UI-to-host context:

- `workflowSceneStableId`
- `workflowShotStableId`
- `workflowAssetId`
- `workflowLookSlotId`
- `workflowRequirementId`
- `workflowWorkArea`

The host resolves IDs against current project state.

Done when:

- Director panel and host prompt report the same context;
- reordering cuts does not change the shot attached to Director.

## 8.4 Minimum Job Metadata

Add optional fields:

- created timestamp;
- started timestamp;
- completed timestamp;
- operation scope;
- progress summary;
- estimated cost;
- actual cost;
- currency;
- retry lineage.

Unknown values display as unknown.

Done when:

- jobs truthfully show status and elapsed time;
- render job links to shot and take;
- assembly job links to review export.

## 8.5 Alternate Angle Decision

Do not enable Add Alternate Angle until the host transition exists.

Required transition:

- allocate stable shot ID;
- insert display label without renumbering identity;
- create editable cut sketch;
- add animatic timing;
- leave package and source strategy incomplete;
- do not invent production-note paths.

If not ready, render the control unavailable with a clear reason.

## 8.6 Presentation Layer

Introduce a pure TypeScript presentation layer between `ProjectState` and DOM rendering.

Boundaries:

- controller invokes host commands and refreshes state;
- selection stores stable IDs and ephemeral UI state;
- selectors derive view models;
- components render view models and emit intents;
- components do not call `invoke` directly.

Suggested modules:

```text
src/studio/
  controller.ts
  selection.ts
  viewModel.ts
  selectors/
  components/
```

Required view models:

- project summary;
- scene summary;
- workspace;
- Director context;
- job summary;
- finish summary.

## Done When

`Lite_Challenge` supports:

```text
approved plan
-> approved source strategies
-> render
-> takes
-> take review
-> selected takes
-> continuity binding
-> dependent render
-> assemble
-> review export
-> Finish status
```

without manually editing workflow state.

---

# 9. Phase 5 - Studio Shell And First Workspace

## Goal

Open into a production-studio shell rather than an editor with a production tab embedded inside it.

## Global Shell

Implement:

- navigation rail for Home, Studio, Files, Assets, Voices;
- top bar for project identity, current work area, Jobs, Settings;
- Studio as primary production workspace;
- Files and Voices preserved;
- Assets begins as a filtered project-media view, not a second asset registry.

## Studio Workspace

Implement:

- left scene list with thumbnail, title, readiness, warning state;
- center selected scene and selected production artifact;
- right scoped Director and Finish/readiness;
- Jobs visible without taking over the workspace;
- compact work-area/detail navigation instead of the old stage rail.

## Scene Look First Slice

Implement:

- approved look image prominently displayed;
- alternates beside it;
- approve/reject/attach/generate/open actions;
- role, title, narrative purpose, approval state, dependencies;
- scene summary from state.

## Scoped Director

Implement:

- visible attached context;
- add/remove context entries without deleting project state;
- model picker, stop behavior, approval cards, specialist execution preserved;
- scope by scene, shot, asset, requirement, or work area;
- refresh Studio after successful Director actions without losing selection.

## Jobs And Finish

Implement:

- compact Jobs panel from shared ledger;
- filters for active, failed, awaiting approval, completed;
- job output links;
- Finish from selected takes, cut versions, assembly, post, QC, deliverables;
- stale dependency and pickup/rerender indicators.

## Done When

- all existing Scene Look actions work from the new screen;
- selection uses stable identity;
- Finish never reports complete while required selected takes are missing;
- latest review export opens from Studio;
- Files, media, Settings, and Voices still work.

---

# 10. Phase 6 - Move Remaining Workflow Surfaces

Move one surface at a time.

Suggested order:

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

- build selector and view model;
- reuse existing host command;
- reach action parity;
- add empty, loading, error, and blocked states;
- verify security scoping;
- remove old renderer only after parity.

Do not copy the old ten-stage rail into global navigation.

## Done When

- no normal production action requires the legacy desk;
- obsolete `workflowDesk.ts` rendering paths can be removed;
- duplicated CSS can be deleted without changing host behavior.

---

# 11. Phase 7 - Recipe And Stage Generalization

## Goal

Make recipes and stages data-driven without turning the app into stringly typed chaos.

## Requirements

Do not simply replace every union with unconstrained strings.

Implement:

- host-owned recipe registry;
- schema versioning;
- migrations for old workflow files;
- runtime validation;
- branded IDs or generated frontend metadata where useful;
- unknown-stage fallback behavior;
- recipe-provided work-area grouping;
- stage dependency metadata;
- stage availability and disabled reasons.

The frontend may treat stage IDs as data, but it should not duplicate recipe truth.

## Tests

Add tests proving:

- unknown recipe ID loads safely;
- new stage can appear without editing core TypeScript unions;
- recipe can omit characters;
- recipe can omit lip sync;
- stage order follows recipe data;
- work-area grouping follows recipe metadata;
- unavailable stages do not leave dead navigation;
- old workflow files load.

## Done When

A new recipe can be added by data and host validation logic, not by editing every UI switch statement.

---

# 12. Phase 8 - Cost, Jobs, And Production Governance

## Goal

Make generation spending visible before it becomes surprising.

## Foundation

Pull forward a minimal provider registry:

- operation support;
- duration buckets;
- input limits;
- known pricing;
- pricing version or timestamp;
- unknown-cost marker.

## Budget Model

Add structured budget:

```ts
type ProjectBudget = {
  currency: string;
  limit?: number | null;
  warning_at?: number | null;
  hard_stop?: boolean;
};
```

Track:

- estimated committed spend;
- actual completed spend;
- billable failed/retried spend;
- remaining budget;
- unknown-cost jobs.

Use decimal or integer minor units for money. Avoid floating point accumulation for authoritative totals.

## Job Model

Jobs should show:

- operation;
- provider;
- model;
- scope;
- status;
- elapsed time;
- estimate;
- actual cost;
- retry lineage;
- output;
- provider prediction ID;
- cancellation/resume state where supported.

## Policy

Support:

- no limit;
- warn only;
- require confirmation above threshold;
- hard stop.

Director cannot bypass hard budget policy.

## Done When

Before paid work runs, Studio can tell the user what is known, what is unknown, and whether the project budget allows the operation.

---

# 13. Phase 9 - Deterministic Technical QC

## Goal

Turn Finish into an actual delivery-validation surface.

Do not ask an LLM whether a video technically works when FFmpeg or ffprobe can answer it.

## Initial Checks

File integrity:

- exists;
- decodes;
- expected streams exist;
- valid duration;
- duration within tolerance.

Video:

- width;
- height;
- aspect ratio;
- frame rate;
- expected codec where applicable;
- nonzero frames.

Audio:

- exists when required;
- duration roughly matches video;
- catastrophic silence;
- obvious clipping;
- sample rate where required.

Assembly:

- every required shot represented;
- no missing selected take;
- assembly matches current selected-take state;
- assembly is not stale relative to dependencies.

Captions:

- exists when required;
- duration does not exceed video;
- parses;
- state corresponds to picture-lock version.

## Result Model

Use structured findings:

```ts
type QCFinding = {
  id: string;
  severity: "info" | "warning" | "error";
  check: string;
  message: string;
  artifact_id?: string;
};
```

Version QC against:

- deliverable;
- cut version;
- picture-lock version.

If picture changes afterward, QC becomes stale.

## Done When

Finish can say why a deliverable is ready, stale, blocked, or technically invalid using deterministic checks.

---

# 14. Phase 10 - Audio As First-Class Production State

## Goal

Represent sound requirements and sound assets using the same discipline already applied to visual assets.

## Audio Roles

Support:

- dialogue;
- voice over;
- music;
- ambience;
- room tone;
- sound effect;
- foley;
- source audio;
- scratch audio;
- mix.

Do not make each role a separate app surface.

## Audio Metadata

Audio assets should support:

- scene association;
- shot association;
- character association where relevant;
- versioning;
- approval;
- duration;
- source provider;
- generation job;
- rights/license notes;
- provenance.

## UI

Scene:

- Dialogue
- Music
- Ambience
- Effects

Finish:

- audio assets complete;
- mix pending;
- mix approved;
- loudness QC.

Assets:

- filter by audio role.

## Done When

A scene can answer what audio is required, what is missing, what is approved, and what blocks Finish.

---

# 15. Phase 11 - Sequence Review

## Goal

Give the user enough editorial visibility to understand the assembled sequence without recreating Premiere, Resolve, or Final Cut.

## Durable Model

Add or clarify a non-destructive sequence/timeline model:

- selected take references;
- trim ranges;
- ordering;
- transitions;
- audio bindings;
- picture-lock version;
- stale dependency state.

Do not destructively edit assembly files.

## UI

Represent selected scene or sequence as ordered cards:

- thumbnail;
- cut label;
- selected take;
- duration;
- audio indicator;
- dependency state;
- warning state.

Playback:

- play;
- pause;
- scrub;
- click cut to seek;
- current cut highlighting.

Light controls:

- trim in;
- trim out;
- replace selected take;
- reorder where recipe permits;
- transition choice.

## Done When

The user can review timing and selected takes in context, make simple non-destructive changes, and understand what becomes stale.

---

# 16. Phase 12 - Provider Capability Schemas

## Goal

Stop leaking provider-specific assumptions throughout the app.

## Schema

Capabilities should describe:

- operation;
- required and optional inputs;
- prompt support;
- negative prompt support;
- start/end frame support;
- reference image min/max;
- audio support;
- supported durations;
- aspect ratios;
- resolutions;
- seed support;
- safety/adult-slot behavior where applicable;
- known pricing hooks.

## Architecture Rule

Keep:

```text
provider-neutral shot specification
-> provider adapter
-> concrete request
```

The shot specification must not become a provider API payload.

## UI

Generation controls are derived from capabilities.

If a model does not support an end frame, do not render the end-frame control.

If a model supports exactly three references, allow up to three.

Do not show unavailable controls disabled everywhere.

## Done When

Changing a provider capability changes the available Studio controls and validation without scattered UI conditionals.

---

# 17. Phase 13 - Production Presets

## Goal

Allow production language to be reused without copying prompt text.

User-facing names:

- Production Preset
- Style Preset
- Production Bible

## Contents

Visual:

- medium;
- style notes;
- palette;
- lighting;
- composition;
- camera preferences;
- negative constraints.

Typography:

- title treatment;
- font preferences;
- caption treatment;
- safe-area preferences.

Audio:

- voice preferences;
- music direction;
- mix target.

Delivery:

- aspect ratio;
- resolution;
- frame rate;
- platform;
- captions.

Generation:

- preferred image model;
- preferred video model;
- preferred voice model;
- fallback model.

## Security And State Rules

- Presets do not contain secrets.
- Imported presets are untrusted until validated.
- Applying a preset creates visible project defaults.
- Presets do not silently override project decisions.

## Done When

A project can start from a reusable production language while keeping every applied value inspectable and editable.

---

# 18. Phase 14 - Better Project Creation

## Goal

Expose the question:

```text
What are you trying to make?
```

without forcing the user to understand recipe architecture.

## New Production Flow

Ask:

```text
What are you starting with?
```

Options:

- idea;
- script;
- existing footage;
- reference video;
- audio or music;
- product;
- research/source material;
- blank production.

Then ask:

```text
What are you making?
```

Options:

- narrative scene;
- montage;
- explainer;
- documentary;
- product video;
- presenter video;
- music-driven video;
- custom.

Studio recommends a recipe. The recommendation is editable.

## Security Rule

Imported source material enters through trusted import or grant flows and is copied/registered into project state where appropriate.

## Done When

Onboarding sets initial production state, then lands the user in normal Studio.

---

# 19. Phase 15 - Reference-Video Analysis

## Goal

Allow an existing video to inform a production without turning the whole source into one giant prompt.

## Import

User may attach:

- local video;
- downloaded reference;
- supported external URL later.

URL acquisition is separate work and must follow security restrictions.

## Deterministic Metadata

Extract first:

- duration;
- resolution;
- aspect ratio;
- frame rate;
- audio presence;
- shot boundaries;
- approximate shot durations.

Optional analysis may infer:

- pacing;
- structure;
- camera motion tendencies;
- dialogue/narration ratio;
- caption style;
- lighting/style notes;
- music tempo.

## Storage

Store as structured reference data.

Do not automatically convert analysis into approved production decisions.

## Done When

Reference analysis can inform pacing, profile, animatic timing, captions, camera language, or music direction, and the user chooses what to carry over.

---

# 20. Phase 16 - Fixture Productions And Product Hardening

Before substantially expanding providers, validate Studio against multiple production shapes.

## Fixtures

Narrative dialogue:

- characters;
- voices;
- identity references;
- continuity;
- lip sync;
- reaction shots;
- take review;
- picture lock.

Atmospheric montage:

- no character requirement;
- music;
- ambience;
- non-dialogue pacing;
- recipe stage omission.

Existing-footage documentary:

- imported footage;
- source metadata;
- voice-over;
- rights notes;
- clip selection;
- sequence editing;
- captions.

Product advertisement:

- product identity;
- reference imagery;
- image-to-video;
- graphics;
- short runtime;
- multiple delivery aspect ratios.

Presenter/explainer:

- voice;
- presentation graphics;
- B-roll;
- captions;
- mixed generated/imported media.

Music-driven project:

- music as source structure;
- beat-driven timing;
- no mandatory dialogue;
- audio-first planning.

## Fixture Rule

If a feature does not improve one of these productions, reconsider whether Studio needs it.

---

# 21. Home, Assets, And Director Evolution

These can progress after fixtures start proving the broader product shape.

## Home / Project Library

Home should answer:

- What projects do I have?
- What was I working on?
- What needs attention?

Project cards can show:

- thumbnail;
- project name;
- recipe;
- last modified;
- current work area;
- blocked/pending state;
- latest review export.

States are derived from production state, not manually assigned.

## Asset Library

The project asset registry remains authoritative.

Support filters for:

- image;
- video;
- audio;
- character;
- location;
- prop;
- look;
- reference;
- take;
- continuity;
- approved;
- unapproved;
- stale.

Existing assets should be selectable wherever a compatible role is required.

Provider capability metadata controls multi-reference min/max and ordering.

## Director Evolution

Director should become more useful through context, not autonomy.

Keep:

- scoped context;
- stable IDs;
- specialists;
- host validation;
- approval cards;
- model selection;
- stop behavior.

Add awareness of:

- selected project;
- selected scene;
- selected shot;
- selected asset;
- selected requirement;
- current work area;
- blocked dependencies;
- current recipe;
- budget state.

Useful requests:

- Why is this shot blocked?
- Suggest two cheaper ways to produce this cut.
- Compare these three takes.
- Draft an alternate cut sketch.
- What still needs approval in this scene?
- Explain why Finish says stale.

The host still executes known transitions.

---

# 22. Additional Production Foundations

These should be threaded through earlier phases rather than saved for the end.

## 22.1 Provenance And Reproducibility

For every generated artifact, store:

- prompt;
- negative prompt;
- model;
- provider;
- provider job or prediction ID;
- request params;
- seed if available;
- source references;
- cost estimate;
- actual cost;
- timestamps;
- content hash;
- producing job ID.

## 22.2 Project Export, Import, And Backup

Add explicit support for:

- portable project bundles;
- missing-asset checks;
- redacted settings;
- no secrets in exports;
- project manifest;
- content hashes;
- recovery from partial writes.

## 22.3 Project Locking And Transactional Writes

Prevent corruption from concurrent writers.

Implement:

- project lock file or host session guard;
- atomic writes;
- temp-file then rename;
- rollback or backup for critical state updates;
- clear recovery path after crash.

---

# 23. Branch And Commit Strategy

Avoid one enormous branch.

Suggested order:

```text
mainline/sep-13-promotion
security/project-path
security/text-writes-and-navigation
security/approval-and-scoping
security/renderer-least-privilege
studio/ui-vertical-slice
studio/shell-and-scene-look
studio/surface-migration
product/recipe-generalization
product/cost-governance
product/qc
product/audio-state
product/sequence-review
product/provider-capabilities
product/production-presets
product/project-onboarding
product/reference-analysis
product/fixture-hardening
```

Within each branch:

1. state/schema;
2. host logic;
3. tests;
4. selectors/view models;
5. UI;
6. fixture verification;
7. cleanup/docs.

Do not start with CSS.

---

# 24. Testing Strategy

Each phase needs three layers.

## Host Tests

Verify:

- path confinement;
- state transitions;
- versioning;
- dependency invalidation;
- recipe behavior;
- job lifecycle;
- cost logic;
- QC logic;
- approval binding;
- provider adapter scoping.

## Selector/View Model Tests

Verify:

- visible statuses;
- warnings;
- available actions;
- disabled reasons;
- summary counts;
- stable selection;
- recipe work-area grouping;
- Finish readiness.

## UI Tests

Use deterministic fixture state and local media.

Verify:

- Studio opens selected scene;
- selection works;
- approval updates visible primary;
- Director context follows selection;
- Jobs open correct output;
- Finish opens review export;
- keyboard navigation and focus remain visible;
- narrow layouts collapse without losing actions.

## Fixture Acceptance

Run actual fixture projects through affected workflows.

Do not rely only on synthetic unit state.

## Security Regression

Keep tests for:

- traversal;
- sibling prefix;
- no-project reads/writes;
- symlink escapes;
- model-authored pending markers;
- loose text approval;
- provider upload outside project;
- settings key exposure;
- remote navigation;
- media protocol outside project.

---

# 25. Release Definition

Studio is production-ready for local use when:

- security containment and renderer least privilege are complete;
- secrets are not exposed to the renderer or chat transcripts;
- the app opens from packaged/built files outside dev mode;
- `Lite_Challenge` completes the vertical slice;
- the new Studio shell replaces the legacy desk for normal work;
- jobs, takes, selected takes, assembly, review export, and Finish are truthful;
- cost and unknown-cost behavior are visible;
- deterministic QC exists for deliverables;
- audio, sequence review, and assets are represented in production state;
- at least three fixture types complete realistic workflows;
- project export/import or backup path exists;
- host tests, selector tests, UI smoke tests, and security regressions pass.

At that point, Studio can credibly say:

> A user can begin with an idea, script, existing footage, or reference material; plan a production appropriate to that source; generate or import required assets; review and select takes; preserve continuity and versions; assemble and review a sequence; track cost and production state; finish audio and picture; validate a deliverable; and export the project without the AI Director becoming the source of truth.

---

# 26. Rule Of Thumb

When evaluating a future feature, ask:

> Does this help the user make, understand, review, control, or finish the production?

If yes, it probably belongs in Studio.

If it mainly answers:

> Can we expose another AI thing?

it probably does not.
