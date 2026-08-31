# Video Production Workbench — Spec / Plan (v0.1)

Date: 2026-07-26  
Status: Draft — rearrange freely once machine load allows real implementation  
Related:

- `Studio/Dev_Notes/Workbench_Direction.md` — product thesis and why OpenCode is not the host
- `Studio/Dev_Notes/Project_Explanation.md` — current specialists and finished pieces
- `Studio/System_Prompts/` — specialist contracts (do not grow these into a mega-prompt)

---

## 1. One-line product

A **stoppable desktop production desk**: open a project folder, direct work in chat, edit and preview locally, run existing specialist prompts as jobs — human approval at every spendy step.

Not autopilot. Not OpenCode. Not “chat with Atlas in Cursor.”

---

## 2. Absolute requirements (MVP)

| # | Need | Notes |
|---|------|--------|
| 1 | **User-space filesystem** | Open any project root. Read/write is not locked to one app-owned directory. Import notes, chapters, research dirs, refs from anywhere the OS allows. |
| 2 | **Built-in text editor** | Edit script, character sheets, `prod.md`, handoff YAML, notes without leaving the workbench. |
| 3 | **Multimedia player** | Play video/audio; scrub; variable playback rate (at least ~0.25x–1x) for sync checks; show current time. Frame extract for continuity plates (ffmpeg). |
| 4 | **Chat interface** | Director channel: stage changes, specialist nudges, mid-pipeline asks (“last frame + props + Mom”). |

**Provider (near term):** Atlas Cloud as the **test** direct API (OpenCode pipeline was built on it).  
**Product rule:** BYOK / multi-provider via adapters. Users add API key(s) in Settings, fetch that provider’s model library, and pick preferred models per **capability slot** (chat LLM, image text-to-image, image character-reference, video start-frame, TTS, etc.). Specialists do not marketplace-browse via MCP; they use Settings defaults unless the user or handoff overrides. Handoffs and project layout stay the product contract; providers are swappable. The desk must not become an Atlas-only tool.

---

## 3. UX model: chat is the director, UI is the desk

### Hybrid (chosen)

- **Chat** for intent, stage hops, messy continuity asks, “make stubs and come back.”
- **Panels / actions** for open project, tree, editor, player, scene board, retry, compare drafts, extract last frame, kick known jobs.

### Rejected extremes

- **Chat-only** — too slow for play / open / retry; agents invent workflow.
- **Menu-only** — too stiff for director-style continuity and non-linear stage order.

### Design rule: selection is context

Whatever is focused or selected (file in tree, scene on board, clip in player, text in editor) is attached to the next director turn. If nothing useful is selected, ask — do not guess the universe.

Examples:

- Clip selected → “pull the last frame” needs no path typing.
- Character md focused → “send that off for reference photos” routes to Model Sheet with that file.
- Nothing selected → router asks which scene/file.

---

## 4. Conceptual architecture

```text
Video Production Workbench (desktop)
├── Project I/O          open root, import assets, watch tree
├── Project tree         scripts, characters, refs, scenes, drafts, final
├── Editors              markdown / yaml (script, prod notes, handoffs)
├── Scene board          status, assets, last-frame, scene-local actions
├── Media player         play, rate, scrub, extract frame
├── Director (thin)      chat → route intent + selection → specialist
├── Specialist runners   existing system prompts + tools (not the primary UI)
├── Settings             API keys, catalog cache, capabilityDefaults, chat LLM
├── Provider adapters    direct API (Atlas test first; ElevenLabs / others later)
└── Jobs / drafts        history, retry, compare, cost/approval gates
```

### Director vs specialists

| Layer | Job | Prompt size |
|-------|-----|-------------|
| **Director** | Parse intent, pick stage/specialist, attach selection + project paths, ask for missing confirmations | Thin — stages, handoff rules, UI context. Not the production bible. |
| **Specialists** | Do one job well (script, character, stills, scene plan, gen, lip-sync, assemble, voice) | Keep current prompts. Do not merge or “improve” into one mega-agent. |

OpenCode remains a **lab / power-user escape hatch** for prompt surgery. It is not the end-user product.

---

## 5. Stage map (specialists ↔ chat intents)

Non-linear by design. User may script before characters settle, or jump mid-pipeline for a plate.

| Stage / focus | Specialist (existing prompt) | Typical chat nudges | Typical UI actions |
|---------------|------------------------------|---------------------|--------------------|
| Import / setup | (none — app I/O) | “Import this chapter / research folder” | Open project, import files/dirs |
| Script | Script Assistant | “Let’s start on the script”; “blank profiles for those characters” | Edit script md |
| Character | Character Reference Assistant | “Let’s work on the character”; “show me that profile” | Edit character md |
| Voice | Voice Profile Studio (app / later in-workbench) | “Let’s work on the voice profile” | Profile editor, preview WAV |
| Model sheet / stills | Model Sheet / Still Image | “Send that character for reference photos”; “I need a reference for the next scene” | Approve stills, set output paths |
| Scene plan | Scene Prompt Assistant | “Break the script into scenes” | Review handoff YAML list |
| Generate | Video/Audio Generator | “Generate this scene”; “video only / with VO” | Approval gate, job status |
| Talking / lip-sync | Text-Sync | “Do the talking shot for this cue” | Player sync check |
| Continuity plate | Still Image (+ ffmpeg extract) | “Last frame of that scene; keep props; put Mom in” | Extract frame, edit brief, regenerate |
| Assemble | Post Production / assemble | “Assemble what we have” | Asset order, optional music/captions |
| Escape hatch | External NLE | — | Reveal files in folder / open in Kdenlive |

Director may create **stubs** (blank character files, empty voice slots) without running expensive generation.

---

## 6. Project as source of truth

- Opening a directory **is** opening the project.
- App does not own a single global working directory.
- Conventional layout can be suggested on first open / import; do not force a rename of an existing proving-ground tree (e.g. Black Hole) on day one — prefer detect + map, then optional “normalize.”

Suggested logical areas (names flexible):

```text
project/
├── notes/ or research/     imported dumps, chapters
├── script/
├── characters/
├── voices/                 wav + profile yaml
├── references/             model sheets, stills, plates
├── scenes/                 handoffs, per-scene media
├── drafts/                 job outputs, compares
├── final/                  assemble outputs
└── prod.md                 running production notes
```

Exact folder names can follow whatever Black Hole already uses until a normalize pass.

---

## 7. Chat behavior (director contract)

### In scope

- Stage switch (“move on to the script”)
- Specialist nudge with current selection
- Stub creation
- Continuity compose briefs grounded in selected media
- Ask before paid upload/generate
- Summarize what will run (plan) then execute on confirm

### Out of scope for the director

- Rewriting specialist system prompts at runtime into one giant instruction set
- Autopilot full pipeline without stage approvals
- Guessing start plates / continuity when the human has not chosen
- Silent creative changes to handoff meaning

### Context packet (every turn)

Attach silently (or show in a collapsible “context” strip):

1. Project root  
2. Active stage (if any)  
3. Selected path(s) / scene id  
4. Player: media path, timestamp, optional in/out  
5. Editor: focused file  
6. Recent job ids for “retry that”

---

## 8. Jobs, approval, history

- Generation = job with status: queued → running → needs review → accepted / rejected.
- Retry and draft compare without re-pasting script/characters/universe.
- Approval gate before provider spend (same spirit as current agent prompts).
- Local ffmpeg work (extract frame, pad audio, assemble) can be cheaper/faster path; still log as jobs.

---

## 9. Tech sketch (near term; movable)

| Piece | Lean toward | Notes |
|-------|-------------|--------|
| Shell | **Electron** (+ Rust host sidecar) | Chromium for Linux/Wayland video; see `Electron_Fork_Decision.md` |
| UI | Web frontend in shell | Editor + player + chat + tree |
| Editor | Embedded code/markdown editor | Script / yaml / prod notes |
| Player | HTML5 / native video element + rate control | ffmpeg sidecar for frame extract / assemble |
| Director LLM | Chat model from Settings | Thin router only; model ID from `selectedChatModel` |
| Specialists | Prompt files + tool runners | Same *functions* as `System_Prompts/`; host adapts tool calls |
| Generative | Direct API adapters | Settings `capabilityDefaults` pick models; **not** MCP discovery |
| Voice | Rebuild Voice Profile Studio; multi-provider | Profile YAML must be honored by text-sync / dialogue |

Do not block workbench scaffolding on perfect voice/assemble; those remain parallel open items.

---

## 10. Build phases (suggested order)

Reorder when unblocked; this is the parking-lot plan.

### Phase 0 — Keep proving the pipeline (now)

- Finish / test voice rewrite and post-prod assemble under current tools when GPU/CPU job ends.
- Log every manual hop (external image UI, Audacity, path paste, agent switch).

### Phase 1 — Shell + desk (no fancy routing yet)

- Open arbitrary project folder
- Tree + markdown/yaml editor
- Media player with rate + scrub + extract last/current frame
- Import files/folders into project
- Job log stub (local)

### Phase 2 — Specialist runners (actions first)

- Wire each existing system prompt as a named runner with file in / file out
- Scene board: status + “run …” on a scene
- Approval + history
- Atlas **direct API** adapter only (test provider); honor Settings capability defaults

### Phase 3 — Director chat

- Thin router over runners + selection context + Settings model packet
- Stage map intents from §5
- Stub creation and mid-pipeline still compose flows

### Phase 4 — Provider / voice hardening

- Additional API-key providers and adapters (same capability slots)
- Voice profiles with shaping fields agents honor
- Optional: in-app voice studio vs external AppImage

### Phase 5 — Later clients (optional)

- Backend for long jobs + shared project store
- Mobile for approve / kick / short assemble — not a full on-device studio

---

## 11. Explicit non-goals (v0.1)

- Full timeline NLE (Kdenlive remains escape hatch)
- Fully automatic continuity / start-plate selection
- Shipping OpenCode or “MCP in Cursor” as the product
- Binding generative work to Atlas MCP (or any single-vendor MCP) inside the workbench
- One mega-agent that owns every stage
- Mobile before desktop desk exists
- Chasing every generative vendor before desk UX and the Settings/adapter pattern are solid
- Growing system prompts until they are unusable with a human in the loop
- Hiding model choice so hard that the app becomes a single-provider appliance

---

## 12. Open questions (decide when implementing)

1. First-open project layout: detect existing trees vs force scaffold.
2. Director model vs specialist models (same Atlas model or cheaper router).
3. Whether Voice Profile Studio is absorbed into the workbench in Phase 1–2 or stays sidecar until Phase 4.
4. How much scene-board UI is required before chat (recommend: board before chat).
5. Persistence format for jobs/history (sidecar JSON in project vs app DB).

---

## 13. Success check (first useful day)

You can:

1. Open a real project directory (e.g. Black Hole).
2. Edit `prod.md` / script without leaving the app.
3. Play a scene at half speed and extract a last frame.
4. Run one specialist (e.g. still compose or scene gen) as a job with approval.
5. Say in chat “use that frame; keep props; add Mom” and get a routed still job with paths filled from selection.

If those five work, the product thesis is validated. Everything else is iteration.
