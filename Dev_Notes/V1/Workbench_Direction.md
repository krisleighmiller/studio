# Workbench Direction Notes

Date: 2026-07-26  
Status: Product direction (post Black Hole testing insights)

Related:
- `Studio/Dev_Notes/Workbench_Spec.md` — v0.1 workbench spec / build plan (desk + director chat)
- `Studio/Dev_Notes/Project_Explanation.md` — current agents and stack
- `Studio/System_Prompts/` — specialist contracts (the hard part already done)

---

## Product thesis

This is not “press a button, get a YouTube video.”

It sits between:

- Beginner hosted tools (easy UI, weak control, multiple subscriptions for one good shot)
- Pro / node-graph tools (stronger control, steep learning curve, poor docs)

The value is a **stoppable production desk**: script → characters → stills → scene plans → generate → revise → assemble, with human approval at each stage.

The differentiator is not the models. It is the **pipeline contract**: specialist roles, structured handoffs, local project assets, and continuity discipline.

---

## What already works

Specialist system prompts and handoff YAML are good enough for real production (Black Hole as the current proving ground).

OpenCode was the right environment for:

- Designing custom agents
- Enforcing scope boundaries
- Proving the decomposition

OpenCode is **not** the right long-term production UI.

---

## Why the current host fails as a product

Day-to-day friction is not “the agents are wrong.” It is **context-switching tax**.

Examples from Black Hole:

- Mid-pipeline still work (compose Mom into Scene 3’s last frame for Scene 4) was faster in an external image UI than switching agents, re-supplying context, retrying, then switching back.
- Mixed lip-sync / off-camera dialogue requires staying in scene context, not bouncing between specialist chats.
- Scripts and `prod.md` live in Cursor; assets and agents live in OpenCode; work is copy/paste between them.
- OpenCode desktop is unstable (API calls sometimes stop). There is no integrated text editor.

Same class of problem if we only “add Atlas MCP to Cursor”: Cursor is not a custom-agent production desk, and without specialists the user must re-supply full context every time. Multiple OpenCode tabs help a little and still inherit the same UX and stability limits.

**Conclusion:** prompts are the compiler passes; the missing product is the IDE / workbench those passes run inside.

---

## Continuity decisions stay human

Van/Wan audio-driven I2V uses the reference image as the start frame. That is expected.

For Black Hole, reference stills under `Scene_Reference_Image/` were planned after cuts were known. Some choices only appear as footage lands, for example:

- Scene 4: take Scene 3 last frame, keep prop placement, remove Sam, insert Mom
- Scene 5: reuse Scene 3 last frame
- Scene 6: use a frame/still derived from Scene 4

These are **director decisions**, not rules to fully automate. The product must make that mid-pipeline still compose / pick / retry loop easy **inside the workbench**, not invent an autopilot that guesses the plate.

---

## Gaps to close before / while building the app

### 1. Keep testing the current pipeline

Use real shows to find more friction (voice, plates, mixed speech modes, assemble). Do not freeze architecture on one short.

### 2. Voice Profile Studio rewrite

Designed-voice-via-text-description was planned, but Atlas TTS options do not usefully support briefs like “child, higher pitch, faster, funny” for clone or profile creation. Pitch/tempo in Audacity does not scale (e.g. Boxcar Children cast).

Direction:

- Multi-provider voice backend (Atlas plus something with real voice design/cloning, e.g. ElevenLabs)
- Persist provider, voice IDs, and any performance shaping in the profile YAML
- Downstream agents (text-sync / dialogue) must honor that profile so shaping is not a manual post step

### 3. Post-production / assemble

Add an **assemble specialist** (ffmpeg-first): trim handles, cuts/transitions from handoff `editing` fields, dialogue already in picture or VO placement, optional music duck, simple captions later.

This is an agent/action, not a Kdenlive clone. Keep a real NLE as the escape hatch for taste-heavy finishing.

### 4. Provider-agnostic execution layer

Handoffs and project layout stay the product contract. Generative providers sit behind **direct API adapters** (REST/SDK), not behind a single vendor MCP server.

**Why not MCP as the product path:** an MCP tool surface is provider-shaped (Atlas MCP today). Binding specialists to that surface makes the desk an Atlas-only tool and blocks BYOK / multi-provider. OpenCode + Atlas MCP remains a **lab** for proving prompts; the workbench does not ship that coupling.

**Settings-first model choice:** users drop one or more API keys and pick preferred models per capability slot (text-to-image, character-reference image, start-frame video, TTS, chat LLM, etc.). That puts ownership on the user — they must understand what each model does — but the product stays provider-agnostic. Specialists keep the same *functions* (propose pack, approve spend, generate, save, update sheet); executors map those functions onto whichever provider/model the user selected.

**Near-term test provider:** Atlas Cloud, because the OpenCode pipeline was built on it. Defaults and catalogs start there; adapters for ElevenLabs and others come later. Planner / Director prompts stay model-neutral; only the active provider adapter knows endpoint names and param schemas.

---

## Target product: “Cursor for video production”

Meaning:

- The **project** is the source of truth (tree of script, characters, refs, scenes, drafts, final)
- Integrated editors for script / `prod.md` / notes
- A **scene board** with status, assets, last-frame, and scene-local actions
- Specialist runners under the hood (existing system prompts), invoked as actions — not as the primary UI metaphor
- Job history, retry, and draft compare without re-pasting the universe
- Provider layer swappable via direct API adapters; Settings capability defaults pick models (not MCP discovery)

Not meaning:

- “Chat with Atlas inside Cursor/OpenCode”
- One mega-agent that does every stage
- Autopilot from idea to finished video

Conceptual shape:

```text
Video Production Workbench
├── Project tree
├── Editors (markdown / yaml)
├── Scene board (status, assets, actions)
├── Specialist runners (system prompts + tools)
├── Provider adapters (Atlas, ElevenLabs, …)
└── Jobs / drafts / history
```

---

## Hosting and clients (later)

- **Near term:** desktop workbench; OpenCode remains a lab / power-user escape hatch for prompt work.
- **Mobile later:** client/server, not a full on-device studio. Phone/tablet for approvals, voice, kicking jobs, short assemble; backend runs long jobs and stores projects; desktop finishes continuity-heavy work. Same project, multiple clients.
- Heavy generation stays remote. Local ffmpeg is enough for short-form assemble; optional cloud render later.

---

## Near-term order

1. Continue real production testing; log every manual hop (external tools, Audacity, etc.).
2. Finish assemble prompt/contract (ffmpeg + handoff `editing` / asset manifest).
3. Rebuild Voice Profile Studio (multi-provider, design/shaping, profile fields agents honor).
4. Build the workbench app around existing prompts and the current project folder layout.
5. Introduce provider adapters (Atlas-first for testing; Settings-driven model IDs; no MCP product path).
6. Only then: backend + mobile client if still desired.

Research/idea / Production Helper is wired in the workbench as an advisory specialist (doc review, prompt brainstorming, digests) — not a generation pipeline.

---

## Explicit non-goals (for now)

- Replacing Kdenlive with a full timeline NLE
- Fully automatic continuity / start-plate selection
- Shipping OpenCode or “MCP in Cursor” as the end-user product
- Binding the workbench to Atlas MCP (or any single-vendor MCP) as the generative interface
- Building mobile before the desktop workbench exists
- Chasing every generative vendor before the desk UX and Settings/adapter pattern are solid
