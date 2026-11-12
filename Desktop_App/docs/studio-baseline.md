# Studio Desktop App — Baseline Assessment

> **Historical assessment, not current guidance.** This describes the October 14 source and measurements, not today's behavior or open-work list. Use the [current product and code reference](studio-reference.md). References below to removed Dev Notes are historical provenance, not live instructions.

**Date:** 2026-10-14
**Branch:** main (clean working tree — `git status --short` empty)
**Commit:** last independent main commit (solo project, trunk-based development)

---

## 1. Application Structure

### 1.1 Three-Tier Architecture

Studio is a video-production desktop application with three process tiers:

| Tier | Runtime | Entry Point | Communication |
|------|---------|-------------|---------------|
| **Electron shell** | Node.js (ESM) | `electron/main.mjs` (26,604 chars) | Window management, media access, security policies |
| **Frontend renderer** | Vite + TypeScript | `index.html` → `src/main.ts` | DOM + IPC to Electron; JSON-RPC to Rust host |
| **Rust host** | Native binary | `host/src/main.rs` (4,912 chars) | JSON-RPC over stdio (`stdin`/`stdout` line-delimited) |

The Rust host is a sidecar process. The frontend spawns it and communicates via a typed JSON-RPC bridge (`src/host.ts`, 5,192 chars). The Rust side reads commands from `host/src/lib.rs` (17,333 chars), which registers handlers in `CommandDispatcher` — basically a flat command table mapping verb strings to handler functions.

### 1.2 Root Configuration

```
Desktop_App/
├── package.json          # npm scripts, deps (Vite, Vitest, TypeScript, PixiJS, Electron)
├── tsconfig.json         # strict TS, ESNext target, path aliases "@/*" → "src/*"
├── vite.config.ts        # Vite dev server + build config
├── vitest.config.ts      # Vitest test runner config
├── index.html            # Single-page shell (21,231 chars), includes inline CSS + app mount
├── host/Cargo.toml       # Rust crate "workbench_host", workspace member
├── electron/             # Electron main process (ESM .mjs)
│   ├── main.mjs          # App lifecycle, window creation, security policies
│   ├── hostBinary.test.mjs
│   ├── mediaAccess.test.mjs
│   └── navigationPolicy.test.mjs
├── src/                  # 39+ TypeScript source files (frontend renderer)
├── host/                 # Rust host source + 4 integration test files
│   ├── src/              # 51 Rust source files
│   │   ├── main.rs       # Binary entry: stdin JSON-RPC loop
│   │   ├── lib.rs        # Library root: CommandDispatcher + handler registration
│   │   ├── director.rs   # Director orchestration (80,813 chars — major module)
│   │   ├── director_actions.rs  # Action implementations (77,394 chars)
│   │   ├── scene_plan.rs # Scene planning pipeline root (2,020 chars)
│   │   ├── scene_plan/   # 16 sub-modules (handoff, generation, render, commits, etc.)
│   │   ├── workflow.rs   # Workflow module root (2,023 chars)
│   │   ├── workflow/     # 23 sub-modules (production, recipe, cost, etc.)
│   │   ├── assemble/     # Assembly module
│   │   └── [providers]/  # atlas, elevenlabs, fal, luma, runway, stability, etc. (11 provider modules)
│   └── tests/            # 4 integration test files (19 tests)
├── tests/                # 6 frontend test files (70 tests)
└── docs/                 # (created by this assessment)
```

### 1.3 Key Frameworks and Dependencies

**Frontend (TypeScript):**
- **Vite** — dev server + build bundler
- **Vitest** v5.0.2 — test runner (70 tests, 6 files)
- **PixiJS** — 2D rendering engine (Canvas/WebGL)
- **Electron** — desktop shell

**Rust Host:**
- **serde / serde_json** — JSON serialization
- **reqwest** — HTTP client (provider API calls)
- **rand** — randomization utilities
- No async runtime framework (tokio/smol not in deps); IO appears synchronous or reqwest-blocking
- No database crate — state is in-memory + file-based (JSON project files on disk)

**Missing (verified by absence from `Cargo.toml` deps):**
- No `tokio` — not an async runtime
- No `sqlx` / `rusqlite` / `sled` — no database
- No `clap` — CLI argument parsing is minimal

---

## 2. Video-Production Workflow Trace

### 2.1 Stage Pipeline (the recipe)

The production workflow is defined by **recipes** — JSON files that declare a linear DAG of stages with dependencies. There are currently two recipes in `host/src/workflow/recipes.json`:

**Recipe: `narrative_dialogue`** (10 stages)
```
script → characters → scene_intent → scene_look → cut_sketches →
reference_images → production_notes → render → take_review → assemble
```

**Recipe: `montage`** (9 stages)
```
script → scene_intent → scene_look → cut_sketches → reference_images →
production_notes → render → take_review → assemble
```

Each stage has a `work_area` classification: `plan`, `produce`, or `finish`.

### 2.2 End-to-End Flow

The pipeline starts with user input and ends with assembled video:

```
User Input (script text)
    │
    ▼
┌─────────────────────────────────────────────────────┐
│ 1. SCRIPT + CHARACTERS                              │
│    Frontend: surfaceModels.ts builds the UI surface  │
│    Rust: director.rs dispatches to LLM prompts       │
│    Output: SceneRecord with SceneIntent              │
├─────────────────────────────────────────────────────┤
│ 2. SCENE LOOK                                       │
│    User configures visual treatment per scene        │
│    Frontend: sceneLookModel.ts + surfaceSelectors.ts │
│    Rust: director_actions.rs apply look to scene     │
│    Output: SceneLook attached to SceneRecord         │
├─────────────────────────────────────────────────────┤
│ 3. CUT SKETCHES → REFERENCE IMAGES → PROD NOTES     │
│    Pre-production: generates cut breakdown,          │
│    reference images, production notes per cut        │
│    Rust: scene_plan/handoff.rs (typed SceneHandoff)  │
│          scene_plan/generation.rs                    │
│    Output: Cuts with typed host-owned vs model-owned │
│            field separation                          │
├─────────────────────────────────────────────────────┤
│ 4. RENDER                                           │
│    Each cut is rendered via provider APIs            │
│    Rust: workflow/production.rs orchestrates         │
│          provider modules (atlas, luma, kling, etc.)  │
│    Cost: workflow/cost.rs tracks spend in i64 cents  │
│    Output: Rendered takes (video/image files)        │
├─────────────────────────────────────────────────────┤
│ 5. TAKE REVIEW                                      │
│    User reviews generated takes, selects/retakes     │
│    Frontend: takeReviewStage.ts                      │
│    Output: Approved takes per cut                    │
├─────────────────────────────────────────────────────┤
│ 6. ASSEMBLE                                         │
│    Takes are assembled into final video              │
│    Rust: assemble/ module                            │
│    Media: ffmpeg_path.rs for FFmpeg integration      │
│    Output: Final video file                          │
└─────────────────────────────────────────────────────┘
```

### 2.3 Scene Handoff Protocol (Critical Interface)

The **SceneHandoff** type in `host/src/scene_plan/handoff.rs` (14,919 chars) is the typed boundary between the LLM (model-owned fields) and the application (host-owned fields). Each cut has:

- **Host-owned:** `CutId`, file paths, rendering state, cost tracking
- **Model-owned:** prompt text, scene descriptions, character references, timing

The separation ensures the AI model can only write to fields it controls; the host enforces consistency for state transitions, file paths, and cost accounting.

### 2.4 Cost/Budget Tracking

`host/src/workflow/cost.rs` (19,896 chars total):

- Uses **integer minor units (i64 cents)** with saturating arithmetic for all authoritative totals
- Never uses floating-point for money
- Unknown costs are explicitly represented as `None`/`Unknown`, never as zero
- Provider pricing lives in `host/src/workflow/providers.json` (1,233 chars) — currently with placeholder prices tagged `"2026-09-27-placeholder"`
- Provider identity is API-agnostic: the pricing registry keys on data strings (`ApiEntry.kind` + capability defaults), with no enum of providers — adding a provider is a data edit, not a code change

### 2.5 Frontend Surface Architecture

The frontend organizes UI into **surfaces** — each stage of production gets its own surface with typed models and selectors:

| Surface File | Purpose |
|---|---|
| `surfaceModels.ts` (18,237 chars) | Builder functions that construct surface data from workflow state |
| `surfaceIntents.ts` (4,187 chars) | Intent definitions — what actions a surface can dispatch |
| `surfaceSelectors.ts` (8,291 chars) | Visual/configuration selectors (rendering config, source filtering) |
| `productionSurfaces.ts` (1,890 chars) | Production pipeline surface configs |
| `workspace.ts` (5,678 chars) | Workspace model builder |

Stage-specific surfaces exist for: script, characters, sketch, reference, readiness, render, take review, assemble, sequence review, profile, and notes.

---

## 3. Component Status Classification

### 3.1 Implemented (verified working)

| Component | Evidence |
|---|---|
| Command dispatch (frontend + Rust) | `workflowDesk.ts` (78KB) + `host/src/lib.rs` (17KB) — full registry |
| Scene handoff protocol | `host/src/scene_plan/handoff.rs` — typed schema with host/model-owned field separation |
| Recipe pipeline | `host/src/workflow/recipes.json` — 2 recipes, `recipe.rs` (45,802 chars) — full pipeline engine |
| Cost/budget tracking | `host/src/workflow/cost.rs` — saturating i64 cents, provider pricing registry |
| Provider integrations | 11 provider modules in `host/src/` (atlas, elevenlabs, fal, kling, luma, runway, etc.) |
| Frontend surfaces | 15 surface files, all stage surfaces present |
| Director orchestration | `director.rs` (80KB) + `director_actions.rs` (77KB) |
| Scene planning | 16 sub-modules under `scene_plan/` (generation, handoff, rendering, commits, validation, etc.) |
| Assembly/FFmpeg | `assemble/` module + `ffmpeg_path.rs` |
| **Test suites (all passing)** | 348 Rust tests, 70 frontend tests, 16 Electron security tests |
| Electron security policies | Navigation, media, CSP, window management (16 tests, all passing) |

### 3.2 Incomplete

| Item | Details |
|---|---|
| **Provider pricing** | `providers.json` has `"note": "2026-09-27-placeholder"` — all prices are notional, not real |
| **Only 2 recipes** | `recipes.json` has `narrative_dialogue` and `montage` — no documentary, music video, or other formats |
| **No e2e/integration tests across tiers** | No test exercises the full Electron→Frontend→Rust chain |
| **Production planner document** | `Studio/Dev_Notes/V2/Get_Studio_To_Production.md` (45KB) lists phases 0–16 — the security review is a gate criterion; many phases are planned but not yet executed |

### 3.3 Disconnected / Potentially Broken

| Item | Details |
|---|---|
| **No audio workflow surfaces** | No surface file for audio management exists, yet audio modules exist in Rust (`workflow/audio.rs`) and there's an `audio-selector.test.ts` |
| **No character consistency across scenes** | No `character_continuity` module — characters defined per scene but no cross-scene validation (matches production planner notes on pending phases) |
| **No export format options** | The `assemble/` module appears to produce a single output format — no format selection surface |
| **No multi-project support** | No project-switching surface or workspace management beyond single-project state |

### 3.4 Not Yet Built (from production planner)

The production plan (`Studio/Dev_Notes/V2/Get_Studio_To_Production.md`) lists 16+ phases. Git history confirms the following status (commits cited below):

**Executive Order items 0–3 ✅ Done (security gate satisfied):**
- **Phase 0** — clean baseline (`9a81e4f` "Stand up a host-owned production desk", `4d7c5f5`)
- **Phase 1** — security containment (`03b3a1d`): `project_path.rs` (path sandbox), `session.rs` (host-owned root, TTL grants), `commandAllowlist.cjs` (89 allowed commands)
- **Phase 2** — security scoping + approval binding (`13c9f18`): `text_writes.rs` (specialist allowlist, stage→approve→commit), `pending.rs` (fingerprinted approval cards), `secrets.rs` (API keys in memory only)
- **Phase 3** — robustness + release safety (`13c9f18`): `http_util.rs` (512MB download cap, retry), `navigationPolicy.cjs` (strict CSP, navigation lock, window.open→deny)

**Executive Order items 4–8 (product work):**
- **Phase 4** — Studio UI vertical slice: **Partial** — surfaces exist; gate criteria unmet
- **Phase 5–6** — Shell + surface migration: **Not started** — foundation code exists only
- **Phase 7** — Recipe generalization: **Deferred** — hardcoded unions remain
- **Phase 8** — Cost governance: **Skeleton** — cost logic done; pricing placeholder (`bbf7762`)
- **Phases 9–16+** — QC, audio, sequence review, capabilities, presets, onboarding, reference analysis, fixtures, Home, Assets, Director evolution, foundations: **Not started**

---

## 4. Test Inventory

### 4.1 Frontend Tests (Vitest) — 70 tests, 6 files, all passing

| File | Size | Tests | Focus |
|------|------|-------|-------|
| `tests/studio-acceptance.test.ts` | 52,733 chars | ~50 | Core acceptance: surfaces, selectors, budget, workflow state |
| `tests/surface-selectors.test.ts` | 8,827 chars | ~9 | Surface selector correctness |
| `tests/budget-selector.test.ts` | 3,911 chars | ~5 | Budget calculation and cost tracking |
| `tests/assets-selector.test.ts` | — | ~3 | Asset selection logic |
| `tests/audio-selector.test.ts` | — | ~2 | Audio selector logic |
| `tests/visual-hierarchy.test.ts` | — | ~1 | Visual hierarchy validation |

### 4.2 Rust Tests — 329 unit + 19 integration, all passing

| File | Tests | Focus |
|------|-------|-------|
| `host/src/*` (inline `#[cfg(test)]`) | 329 | Unit tests across all modules |
| `host/tests/workflow_import.rs` | 7 | Workflow import validation (lite challenge shaped scenes) |
| `host/tests/lite_challenge.rs` | 10 | Lite challenge end-to-end |
| `host/tests/scene_plan_commit_flow.rs` | 1 | Scene plan commit transaction flow |
| `host/tests/vertical_slice.rs` | 1 | Vertical slice integration |
| Doc-tests | 0 | No doc-tests (zero) |

### 4.3 Electron Security Tests — 16 tests, all passing

| File | Focus |
|------|-------|
| `electron/hostBinary.test.mjs` | Host binary path resolution |
| `electron/mediaAccess.test.mjs` | File access grants, symlink restrictions, settings isolation |
| `electron/navigationPolicy.test.mjs` | Navigation policies, CSP, window.open denial, preview link confirmations |

### 4.4 Commands Executed

```
# Test results (all exit 0)
cargo test           # 329 unit + 19 integration passed (host/)
npm test             # 6 files, 70 tests passed (Desktop_App/)
npx tsc -p tsconfig.json  # TypeScript compilation clean (exit 0)
```

---

## 5. Summary: Three Highest-Priority Issues

### Priority 1: Provider Pricing is Placeholder

`host/src/workflow/providers.json` contains notional prices tagged `"2026-09-27-placeholder"`. The budget system (`cost.rs`, `budget-selector.test.ts`) is implemented and tested, but it operates on fake numbers. No production run can produce an accurate cost estimate until real provider pricing is populated.

### Priority 2: No End-to-End Integration Tests

All 434 tests pass, but none exercise the full Electron→Frontend→Rust pipe. The frontend tests mock the Rust host; the Rust integration tests run headless. A real E2E test (spawn the app, create a project, run a minimal production pipeline, verify output) does not exist. This means the integration points — JSON-RPC message formats, error propagation, workflow state synchronization — are untested.

### Priority 3: Audio Module Has No UI Surface

The Rust codebase has a `workflow/audio.rs` module and there's an `audio-selector.test.ts`, but no corresponding `audioStage.ts` surface file exists in `src/studio/surfaces/`. Audio is a dependency in the recipe pipeline (characters have voice roles), so this is a missing bridge between the implemented backend and the user interface.

---

## 6. Recommended Bounded Implementation Task

**Task:** Replace placeholder provider pricing in `host/src/workflow/providers.json` with real pricing data from each provider's public API pricing page, and add validation in `cost.rs` that rejects configurations with placeholder markers at startup.

**Scope:**
1. Research current per-unit pricing for each provider listed in `providers.json` (atlas-cloud, elevenlabs, and any others in the file)
2. Replace the placeholder prices with real values and remove the `"2026-09-27-placeholder"` note
3. Add a startup validation in `cost.rs` or `lib.rs` that checks for the string `"placeholder"` in any price field and refuses to start (panics or returns an error) if found
4. Verify: `cargo test` still passes, and the budget selector tests still produce correct calculations with real numbers
5. No UI changes — this is purely host-side data + validation

**Estimated scope:** Small (~2 hours). Touches 2 files (`providers.json` + `cost.rs` or `lib.rs`). No frontend changes.

---

*Assessment complete. Awaiting approval before implementing any task.*