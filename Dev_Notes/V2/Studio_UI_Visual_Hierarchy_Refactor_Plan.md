# Studio UI Visual Hierarchy Refactor Plan

**Status:** Proposed visual-only UI refactor  
**Target:** Current V2 Studio shell on `pre-sep13`  
**Primary goal:** Make Studio feel like a focused creative production workspace rather than a direct rendering of all available workflow state.

---

## 1. Problem Summary

The current Studio UI is functionally rich but visually noisy.

The main problem is **not the color palette** and it is **not the overall information architecture**. The problem is that the interface gives too many pieces of information equal visual weight.

The current screen attempts to show, at the same time:

- global navigation;
- scene navigation;
- job history;
- production stage navigation;
- current scene state;
- blockers;
- requirement actions;
- current artifact;
- alternate artifacts;
- Finish state;
- Director state;
- prompt metadata;
- approvals;
- generation actions.

As a result, the actual creative task is visually buried.

For Scene Look, the user is primarily trying to answer:

> What does this scene look like, and do I approve this version?

The approved/current image should therefore dominate the workspace.

Instead, the current implementation places many layers of status and controls before the primary creative artifact.

The UI is rendering the production model more directly than it is presenting the user's current task.

---

## 2. Core Design Principle

For every Studio surface, identify the **primary production artifact or decision**.

That artifact should receive most of the visual space and attention.

Secondary information should support the current task without competing with it.

A useful rule:

> The production artifact is the protagonist. Everything else is supporting UI.

Examples:

- Scene Look → current look image
- Cut Sketches → selected cut / storyboard card
- Reference Images → selected source/reference
- Render → current shot and render status
- Take Review → selected take/video
- Finish → current assembly/deliverable

---

## 3. Visual Hierarchy Problems

### 3.1 Too much information appears before the current artifact

The current Scene Look surface places the following before the main image:

- stage/status pills;
- Profile;
- blocker summary;
- requirement attachment buttons;
- scene breadcrumb;
- scene title;
- state counts;
- repeated blocker banners;
- Draft Look Board;
- look-slot buttons.

This pushes the image too far down the workspace.

### Required change

The Scene Look surface should begin approximately as:

```text
Scene 1 — The Last Train
Scene Look · Interior · Approved

[compact blocker summary if needed]

[large current image] [alternate versions]

Approve   Reject   Generate alternate

Prompt & metadata ▸
Warnings (8) ▸
Dependencies (6) ▸
```

The main image should appear immediately after the scene header and any truly relevant current-task warning.

---

## 4. Reduce Permanent Columns

The current desktop layout effectively becomes:

```text
Navigation | Scenes + Jobs | Main Workspace | Finish | Director
```

This gives too much permanent space to supporting UI and too little to the creative workspace.

### 4.1 Combine Finish and Director

Replace the permanent Finish and Director columns with one shared right-side inspector.

Suggested layout:

```text
┌──────────────────────────────┐
│ Director     Finish          │
├──────────────────────────────┤
│                              │
│ active tab content           │
│                              │
└──────────────────────────────┘
```

Target width:

```text
~300–340px
```

Requirements:

- Director and Finish become tabs within the same inspector.
- Preserve current Director functionality.
- Preserve current Finish functionality.
- Remember the last selected inspector tab if practical.
- Allow the entire inspector to collapse.
- Collapsing the inspector should return space to the primary workspace.

### Why

The user does not need Finish and Director at full width simultaneously.

Both are contextual support surfaces rather than the primary creative artifact.

---

## 5. Move Jobs Out of the Scene Column

Jobs currently share the left-side vertical space with Scenes.

This creates two independent scrolling lists fighting for the same narrow column.

Studio already has a Jobs control in the top bar.

### Required change

Move Jobs into a collapsible bottom drawer.

Closed state:

```text
Jobs  • 3 active
```

Open state:

```text
──────────────────────────────────────────────
Jobs

Render Cut 4       Kling       00:42
Look generation    Flux        Awaiting approval
Assembly           FFmpeg      Complete
──────────────────────────────────────────────
```

Requirements:

- closed by default;
- show active/failed count in the top-bar Jobs control;
- drawer may span the production workspace width;
- maintain existing filters;
- maintain links from jobs to their affected artifact;
- failures should be visible without requiring the drawer to stay open.

### Result

The left panel becomes dedicated to scene navigation.

---

## 6. Replace Stage Pills with Real Navigation

The current production-stage row uses rounded status pills such as:

```text
● Script READY
● Characters READY
● Scene Intent APPROVED
● Scene Look APPROVED
```

This creates several problems:

- the row looks like a set of filters rather than navigation;
- status text makes every item visually busy;
- Profile wraps onto another line;
- the active stage is not visually dominant enough.

### Required change

Use a single-line secondary navigation bar.

Example:

```text
PLAN

Script   Characters   Intent   Look   Shots   Sources   Notes   Profile
                              ━━━━
```

Status should be represented primarily by a small colored dot.

Example:

```text
● Script
● Characters
● Intent
● Look
● Shots
```

Full status labels such as `Approved`, `Draft`, or `Blocked` may appear:

- on hover;
- in a tooltip;
- in the selected surface header;
- when the status is exceptional.

### Important distinction

`Plan`, `Produce`, and `Finish` are **work areas**.

Individual recipe stages are **places inside those work areas**.

The visual design should reinforce that hierarchy.

---

## 7. Consolidate Blockers

The current Scene Look screen repeats the same blocker information multiple times:

1. a combined blocker sentence;
2. multiple attachment buttons;
3. one large banner for each blocker.

Example:

```text
Blocked on Guard, Brown paper parcel, Hanging corridor lamp...

Attach Guard
Attach Brown paper parcel
...

Blocked on Guard

Blocked on Brown paper parcel

Blocked on Hanging corridor lamp
```

This is both redundant and visually dominant.

### Required change

Replace repeated blocker UI with one compact summary.

Example:

```text
⚠ 6 requirements blocking production             Review requirements
```

Expanded state:

```text
Guard                                  Attach
Brown paper parcel                     Attach
Hanging corridor lamp                  Attach
Pip's coat                             Attach
Red lantern at Willowby Hall           Attach
Tom's purse                            Attach
```

### Context rule

Only allow blockers to dominate the workspace if they block the **current operation**.

If a missing requirement blocks rendering but does not prevent reviewing Scene Look, it should remain available as contextual warning information rather than taking over the entire Scene Look workspace.

---

## 8. Reduce “Rounded Rectangle Disease”

The current UI applies a similar treatment to many different kinds of things:

```css
border: 1px solid var(--border);
border-radius: 8px;
background: var(--bg-elevated);
```

This makes:

- navigation;
- objects;
- actions;
- warnings;
- status;
- filters;
- metadata;

all look like the same kind of component.

The result is visually noisy despite a restrained palette.

### New visual grammar

#### Navigation

Use:

- text;
- small status indicator;
- underline or subtle active background.

Avoid boxing every navigation item.

#### Status

Use:

- small dot;
- short muted label.

Example:

```text
● Approved
● Draft
● Needs review
```

#### Cards

Reserve card surfaces for actual production objects:

- scene;
- take;
- asset;
- job;
- deliverable.

#### Warnings

Use one consistent warning surface.

#### Buttons

Use button styling only for actual actions.

### Rule of thumb

Before adding:

```css
border: 1px solid var(--border);
border-radius: 8px;
background: var(--bg-elevated);
```

ask:

> Is this actually an object?

If not, avoid boxing it.

---

## 9. Quiet the Scene Rail

The current scene rail uses a strong red warning stripe for most blocked scenes.

When many scenes are blocked, the entire panel becomes visually red and the warning loses meaning.

### Required change

Use quieter scene-state indicators.

Suggested meaning:

```text
gray    = not started / neutral
green   = ready / approved
amber   = blocked / needs attention
red     = actual failure / invalid state
```

A scene row might appear as:

```text
[thumbnail] Scene 1 — The Last Train
            Look approved · 8 warnings        ●
```

### Important rule

Use red sparingly.

`Blocked` is usually amber.

Red should mean something has failed, is invalid, or requires urgent correction.

---

## 10. Improve Typography Hierarchy

The UI currently relies heavily on:

```text
11px
12px
13px
```

This is reasonable for an IDE but makes a visual production tool feel dense and overly technical.

### Suggested scale

```text
Application body       14px
Secondary/meta         12px
Section heading        15–16px / 600
Scene/page title       22–24px / 650
Top work-area title    18px
```

### Simplify duplicated labels

Avoid headings like:

```text
Scene 1 — Scene 1 — The Last Train
```

Prefer:

```text
Scene 1 — The Last Train
```

Then use a smaller context line:

```text
Interior Look · Approved
```

---

## 11. Keep the Existing Palette, but Use It More Deliberately

The current palette is workable.

Current approximate direction:

```css
--bg: #ece8e2;
--bg-panel: #f7f4ef;
--bg-elevated: #fffdf9;
--bg-muted: #e4dfd6;
--accent: #2f5d50;
```

The problem is not the colors themselves.

The problem is that several similar beige values are used broadly enough that the interface becomes visually flat.

### Suggested token simplification

Example:

```css
--canvas: #f3f1ec;
--sidebar: #ebe8e1;
--surface: #fffdfa;
--surface-subtle: #f8f6f1;

--border: #ddd8ce;

--text: #24211d;
--muted: #777067;

--accent: #356b5d;
--accent-soft: #e5efeb;

--warning: #a46920;
--warning-soft: #faf1df;

--danger: #9d4336;
--danger-soft: #fae9e5;
```

### Intended use

```text
Canvas          background behind work
Sidebar         navigation / utility panels
Surface         actual objects and artifact views
Surface subtle  secondary content
Accent          selection and primary action
Warning         attention required
Danger          failure / invalid state
```

Do not use borders on every rectangular element.

---

## 12. Scene Look Target Layout

Desktop target:

```text
┌─────┬────────────────┬───────────────────────────────────────────┬───────────────────┐
│     │ Scenes         │ Script Characters Intent Look Shots ... │ Director | Finish │
│     │                ├───────────────────────────────────────────┤                   │
│     │ Scene 1        │ Scene 1 — The Last Train                 │ Attached context  │
│ NAV │ Scene 2        │ Interior Look                 Approved    │                   │
│     │ Scene 3        │                                           │ Scene 1           │
│     │ Scene 4        │ ⚠ 6 blockers                     Review   │ Cut 1             │
│     │                │                                           │ Interior Look     │
│     │                │ ┌────────────────────────────┐ ┌────────┐ │                   │
│     │                │ │                            │ │ alt 1  │ │                   │
│     │                │ │                            │ ├────────┤ │                   │
│     │                │ │       HERO IMAGE           │ │ alt 2  │ │                   │
│     │                │ │                            │ ├────────┤ │                   │
│     │                │ │                            │ │   +    │ │                   │
│     │                │ └────────────────────────────┘ └────────┘ │                   │
│     │                │                                           │                   │
│     │                │ Approve   Reject   Generate alternate     │ Ask Director…     │
│     │                │                                           │                   │
│     │                │ ▸ Prompt & metadata                       │                   │
└─────┴────────────────┴───────────────────────────────────────────┴───────────────────┘
```

Jobs are not shown here because they belong in the collapsible bottom drawer.

---

## 13. Progressive Disclosure

Studio contains substantial production state.

The user should not have to look at all of it simultaneously.

Use collapsible secondary sections.

Examples:

```text
Prompt & generation settings                      ▸
Dependencies (6)                                  ▸
Warnings (8)                                      ▸
Generation history (4)                            ▸
Technical details                                 ▸
```

### Principle

Complexity may exist in the system without being permanently visible in the interface.

Only information required for the current decision should be visually prominent.

---

## 14. Scene Look DOM / Component Reordering

The current Scene Look rendering order is approximately:

```text
header
blockers
failed jobs
Draft look board
look-slot tabs
image
warnings
notes
purpose
prompt fields
actions
plates
```

The target order should become closer to:

```text
scene header
current slot/context
current-task blocker summary if applicable
primary image + alternates
primary review actions
slot navigation if needed
collapsed prompt/details
collapsed warnings/dependencies
advanced actions
plates / secondary assets
```

The exact implementation may vary, but the primary image must move significantly upward.

This requires changes to `sceneLookView.ts`, not only CSS.

---

## 15. Recommended Refactor Scope

Create a dedicated branch:

```text
ui/visual-hierarchy
```

This should be a **visual and composition refactor only**.

### Do not change

During this pass, do not change:

- Rust workflow semantics;
- approval rules;
- asset versioning;
- recipe behavior;
- job semantics;
- dependency logic;
- Director execution;
- provider behavior.

### Change only

Focus on:

- shell composition;
- DOM ordering;
- CSS;
- collapsible presentation;
- tab structure;
- typography;
- spacing;
- visual state treatment.

---

## 16. Implementation Order

### Phase 1 — Shell simplification

1. Combine Finish and Director into one right-side inspector.
2. Add `Director` / `Finish` tabs.
3. Allow inspector collapse.
4. Move Jobs to bottom drawer.
5. Remove Jobs from the scene column.

### Phase 2 — Stage navigation

1. Replace rounded stage pills with single-line stage navigation.
2. Represent state using small status dots.
3. Preserve work-area hierarchy.
4. Prevent navigation wrapping.

### Phase 3 — Scene Look hierarchy

1. Move primary image immediately below scene header.
2. Keep alternate versions beside the image.
3. Put approval/rejection/generation actions directly under the image.
4. Move prompt metadata below as collapsed details.
5. Consolidate blockers.
6. Consolidate warnings.

### Phase 4 — Scene navigation cleanup

1. Reduce warning-strip usage.
2. Introduce neutral/green/amber/red state rules.
3. Simplify scene metadata.
4. Keep thumbnails.

### Phase 5 — Visual system cleanup

1. Define a spacing scale.
2. Define typography scale.
3. Simplify background/surface tokens.
4. Remove unnecessary borders.
5. Normalize button styles.
6. Normalize warning styles.
7. Normalize card styles.

---

## 17. Suggested Spacing System

Use a small spacing scale rather than arbitrary margins.

Example:

```css
--space-1: 4px;
--space-2: 8px;
--space-3: 12px;
--space-4: 16px;
--space-5: 24px;
--space-6: 32px;
```

Suggested use:

```text
4px   icon/text micro gaps
8px   controls inside compact rows
12px  card/internal component spacing
16px  standard section spacing
24px  major workspace sections
32px  page-level separation
```

Consistency matters more than the exact numbers.

---

## 18. Suggested Button Hierarchy

Avoid rendering every action with equal emphasis.

### Primary action

One per current decision where practical.

Example:

```text
Approve
```

### Secondary action

Example:

```text
Generate alternate
Attach image
```

### Quiet / tertiary action

Example:

```text
Open
Delete
Advanced
```

Destructive actions should not visually compete with primary creative actions.

---

## 19. Suggested State Language

Use concise status labels.

Prefer:

```text
Approved
Draft
Needs review
Blocked
Failed
Ready
```

Avoid displaying internal implementation terminology unless useful to the user.

Long state explanations should appear in:

- tooltips;
- detail panels;
- expandable warnings.

---

## 20. Acceptance Criteria

The refactor is successful when:

- the primary production artifact dominates the central workspace;
- Director and Finish no longer consume separate permanent columns;
- Jobs no longer compete with Scenes for left-column height;
- stage navigation stays on one line at normal desktop widths;
- repeated blocker information appears only once in the primary view;
- the Scene Look image is visible without substantial scrolling;
- prompt metadata is available without permanently occupying prime screen space;
- red is reserved for failure/invalid states;
- the interface contains substantially fewer decorative boxes;
- all existing production actions still work;
- no host workflow behavior changes.

---

## 21. Manual Visual Test

Use `Lite_Challenge` Scene 1 as the reference fixture.

At normal desktop size, opening Scene Look should allow the user to understand within several seconds:

1. which scene is selected;
2. which look slot is selected;
3. which image is currently approved/current;
4. what alternate versions exist;
5. whether the current image can be approved/rejected;
6. whether anything blocks the current task;
7. where to ask Director for help;
8. where to inspect Finish state.

The user should **not** need to read the entire screen to determine what they are supposed to do next.

---

## 22. Design Rule of Thumb

For every new UI element, ask:

> Does this need to be visible for the decision the user is making right now?

If no:

- collapse it;
- move it to the inspector;
- move it to Jobs;
- put it behind a detail disclosure;
- or remove it from the current surface.

For every box, ask:

> Is this actually an object?

If no, do not automatically give it a border, rounded corners, and its own background.

---

## 23. Final Product Principle

Studio should feel like:

> a calm creative workspace with deep production state available when needed

not:

> a dashboard proving how much state the application knows.

The application can remain structurally sophisticated while presenting one clear production decision at a time.
