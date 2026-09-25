# Video Workbench — Quick Guide

In-app Help. Specialist sections list required inputs and a basic prompt for each function.

## What this is

A desktop production studio for video projects. **File → Open Project…** opens the folder in **Studio**. Plan, Produce, and Finish cover the recipe stages. Jobs stay under the scene list, and Director sits on the right. Files, Assets, Voices, and Settings stay on their own surfaces.

## First run

1. **Edit → Settings…** (`Ctrl+,`) — add at least one named API (pick a protocol: Atlas, OpenAI-compatible, Anthropic, Gemini, Ollama, ElevenLabs, Kling). Fetch the model library where supported, set **Generation defaults**, and pin chat LLMs if needed. Use **+** to add more APIs.
2. Pick a **chat model** in the dropdown at the bottom of the chat panel.
3. **File → Open Project…** — any folder on disk can be the project root. The project opens in Studio. **View → Studio** (`Ctrl+Alt+P`) returns there.
4. Attach context with **drag-and-drop** onto chat, or explorer / tab **Add to Chat**.

You own model choice. Studio is not locked to one vendor; specialists use your Settings defaults unless you override them in chat or a handoff.

## Director chat

Director is the main chat. It routes intent to wired specialists below.

Chat models only propose small validated actions (`run_specialist`, `inspect`, `ask_user`, `done`). The host validates paths, enforces approval/spend, and runs specialists — models never call providers directly.

Each turn includes project root, selected explorer path, focused editor file (and a text excerpt), plus **chat attachments**. Prefer Attach / drag-and-drop over pasting long paths.

**Approval words:** After a scoped paid/local plan, **Approved** / “go ahead” / “run it” means run that plan — do not re-describe it.

**Stop:** While Director is working, click **Stop** (or press **Esc**) to halt the current turn and cancel auto-continue. In-flight image polls abort; a chat completion that already started may finish that one HTTP call, then the host stops.

**Capture chat:** Copy the current transcript with the clipboard button in the chat header, **Help → Copy Chat Transcript** (`Ctrl+Shift+C`), or right-click the chat panel. Save a markdown file with the save button or **Help → Save Chat Transcript…** (`Ctrl+Shift+E`). The dump includes model, attachments, status bar, and every You / Director / Workbench message so you can paste it into a bug report.

**Judgement override:** If you are in a hurry, want a basic pack, or do not know how to answer a style/character/runtime menu, say **stop asking**, **you decide**, or **use your best judgement**. Director picks remaining brief items from the script, records them in Creative Direction, and writes files. It should not keep offering menus.

**Long scripts:** A full production plan is written in steps (brief, then one script section at a time). After you answer any brief questions, leave Studio running — it continues until `prod.md` and `Scene_Prompts/` are complete.

Say **`adult`** / **`nsfw`** on a generate turn to use **Settings → Adult content** slots (see Settings tips).

## Specialists — required inputs & basic prompts

Use this section function by function. Attach files when noted; give exact project paths for anything that will be written.

---

### 1. Script Assistant

**Job:** Turn notes, chapters, outlines, or a draft into a shootable draft script. Does **not** write Atlas YAML or generate media.

**You provide**
- Source material (attach chapter/notes/script, or point at a path)
- What you want: draft, revise, split/select, runtime estimate, tighten hook, etc.
- Output path when you know it (e.g. `Scripts/Black_Hole_draft.md`); otherwise Director may propose one and proceed for local file work

**Settings:** Chat model only (no generation slots).

**Basic prompt**

> Attach `source_chapter.md`. Draft a shootable short-form script for a ~40s vertical comedy bit and save it to `Scripts/Black_Hole_draft.md`.

**Revise example**

> Revise `Scripts/Black_Hole_draft.md` — tighten the cemetery inventory beat and keep Mom/Sam’s voices consistent.

---

### 2. Character Creation

**Job:** Build or update a character reference sheet (markdown). Interview is optional after extract.

**You provide**
- Source: notes, book/script excerpt, or an existing sheet (attach or path)
- Character name
- Exact output path for the sheet (e.g. `Characters/Mom.md`)
- Optional: “interview me” / “flesh this out” if you want Q&A before the sheet is finalized

**Settings:** Chat model only.

**Basic prompt**

> Attach the chapter. Pull a character sheet for Mom into `Characters/Mom.md`.

**Update example**

> Update `Characters/Mom.md` — add that she wears a plain black short-sleeve shirt and keep the rest.

**Interview example**

> Help me flesh out Mom from `Characters/Mom.md` — ask a few questions, then rewrite the sheet.

---

### 3. Scene Plan

**Job:** Black Hole–style plan only — **not** prose beat sheets.

**Deliverables**
1. Master production file (default `prod.md`): Creative Direction + every scene’s narration beat + full Atlas handoff YAML
2. Per-scene prompt sidecars (default `Scene_Prompts/SceneN.txt`): headers `Character_Reference` / `Voice` / `Output` / `Filenames` + the **same** YAML under `Prompt:`

**You provide**
- Script / narration / shot notes (attach)
- Character sheets or reference paths when known
- Master path + sidecar folder (or accept defaults `prod.md` + `Scene_Prompts/`)
- Mode: **create**, **revise** scene(s), or batch **Update** (refs/voices/outputs)
- Optional: **you decide** / **stop asking** if you want Director to pick style, character look, and runtime fill from the script

**Settings:** Chat model only (planning; no media spend).

**Basic prompt (create)**

> Attach `Scripts/Black_Hole_draft.md` and `Characters/Mom.md` / `Characters/Sam.md`. Build `prod.md` and `Scene_Prompts/` for this project (9:16 clay-animation comedy).

**Judgement-override example**

> Attach the script. Build `prod.md` and `Scene_Prompts/`. Leave character refs and voices blank. Stop asking and use your best judgement.

New creates are written **script-section by script-section**. Each script scene becomes as many generation clips as it needs (usually one talking shot per speaker turn). Asking for “scene 1 only” still expands that scene into every remaining cut. Sidecars go in `Scene_Prompts/SceneN.txt` next to pack-root `prod.md` — not under `Scene_Prompts/Scene1/`. Delete a stale `scene_plan_state.json` before retrying a broken pack.

**Update example**

> Update Mom’s character reference on all her scenes in `prod.md` / `Scene_Prompts/` to `/path/to/mom.png`.

**Revise example**

> Revise Scene 6 in `prod.md` and `Scene_Prompts/Scene6.txt` — shorten the line and keep lipsync mode.

You may rename files (`FlightPt1.md`, `Scene1.md`) but keep the handoff shape. Each scene YAML needs `editing` / `speech` / `prompt` / etc. (Assemble later reads `editing` + `speech` — see Assemble).

---

### 4. Character Model Sheet

**Job:** Reusable identity still pack (turnarounds / expressions, usually green-screen style) for continuity in later video/stills. Paid image generation; pack approval required.

**You provide**
- Identity: character sheet path and/or reference image(s) and/or a clear text brief
- Exact **output directory** under the project (e.g. `Characters/Mom/model_sheet`)
- Style note (e.g. hyperrealistic, match sheet art)
- Optional: resume from existing `model_sheet_manifest.yaml`

**Settings:** Image slots — `image.text_to_image`, `image.edit` / character-reference capable still model. Do **not** point character-reference at a model that requires video.

**Basic prompt**

> Attach `Characters/Mom.md` and her face ref. Build a hyperrealistic model sheet for Mom into `Characters/Mom/model_sheet`.

Approve each described paid batch when asked. A plain-text request is enough: the host generates a single `closeup_front` plate, you approve that face, then the remaining 11 shots run one at a time. Do not expect a collage or contact sheet in one file.

---

### 5. Still Image

**Job:** Ordinary production stills — locations, props, mood, start frames, character-in-scene — or a **focused edit** of an existing still. Exact save path required.

**You provide**

| Mode | Required |
|------|----------|
| New still | Brief / prompt + exact `output_path` (`.png` / `.jpg`) |
| Character-in-scene | Character ref image(s) + scene brief + output path |
| Focused edit | Existing source still (attach/select) + what to change + new output path |

**Settings:** `image.text_to_image`, `image.image_to_image`, `image.character_reference`, **Image → Focused edit** (`image.edit`, e.g. `google/nano-banana-2/edit`).

**Approval:** Ordinary stills / focused edits generate when path + brief are enough (no pre-approval pause). Overwrite still needs permission.

**Basic prompt (new still)**

> Make a hyperrealistic RV interior workstation still at `Locations/rv_interior.png`.

**Basic prompt (character-in-scene)**

> Attach Mom’s ref. Mom in the beige car driver seat, 9:16, save to `Locations/mom_driving_start.png`.

**Basic prompt (focused edit)**

> Attach `Locations/rv_interior.png`. Edit this — remove the coffee cup, keep everything else — save to `Locations/rv_interior_v2.png`.

---

### 6. Scene Generation

**Job:** (A) Run a pasted Atlas handoff → video (+ optional separate TTS), or (B) focused generative **edit of an existing clip**. Not talking-shot text-sync; not Sync Labs; not assemble.

**You provide**

| Mode | Required |
|------|----------|
| Handoff | One scene handoff (attach/paste/`handoff_path`) + exact video output path (+ audio path if TTS) + audio mode: `no audio` / clone WAV / voice-profile YAML |
| Continuity without start still | `continuity_input.previous_video_path` so host can extract `.out-frame.png` via ffmpeg |
| Freeform T2V | Prompt + video `output_path` (+ optional duration) |
| Start-frame I2V | Existing still + prompt + video output path |
| Focused video edit | Source clip (attach/select) + edit brief + exact output `.mp4` |

**Settings:** Video slots (`video.start_frame`, `video.character_reference`, `video.location_prop_other_reference`, `video.text_only`, **Video → Focused edit**). Audio TTS slots when handoff needs speech files. System `ffmpeg` on PATH for out-frame extract.

**Approval:** Handoff batches need **Approved** before first upload/generate. Ordinary focused video edits generate when source + path + brief are enough.

**Basic prompt (handoff)**

> Attach `Scene_Prompts/Scene3.txt`. Generate this scene — video to `Scenes/Scene3/SC3.mp4`, no audio.

**Basic prompt (handoff + clone)**

> Attach Scene 2 handoff and Mom’s clone WAV. Video `Scenes/Scene2/SC2.mp4`, audio `Scenes/Scene2/SC2.wav`.

**Basic prompt (focused video edit)**

> Attach `Scenes/Scene01.mp4`. Edit this — remove the coffee cup, keep everything else — save to `Scenes/Scene01_v2.mp4`.

**Basic prompt (freeform)**

> Generate a 5s misty pine forest drone shot at dawn to `Videos/forest_dawn.mp4`.

---

### 7. Text-Sync (talking shot)

**Job:** Audio-driven talking shot: handoff → TTS (`voice_clone` / `voice_profile`) → ffmpeg pad handles → Van/Wan **image-to-video + audio**. Not Sync Labs post re-sync; not silent Scene Generation.

**You provide**
- Talking-shot handoff (`speech.mode: source_video_lipsync` or `audio_driven_i2v`) with `speech.cues`
- Character still (or handoff `Character_Reference`)
- Voice: clone WAV **or** voice-profile YAML **or** reuse an existing performance WAV
- Exact `output_video` / `output_audio` if not already in handoff Filenames

**Settings:** **Video → Text-sync (talking shot)** (Van/Wan I2V+audio); **Audio → TTS (voice clone)** / profile. System `ffmpeg` / `ffprobe` on PATH.

**Approval:** Paid batch plan → **Approved** → host runs the stack.

**Basic prompt**

> Attach `Scene_Prompts/Scene1.txt`, `mom_vert.png`, and Mom’s clone WAV. Text-sync talking shot to `Scenes/Scene1/SC1.mp4` and `Scenes/Scene1/SC1.wav`.

---

### 8. Lip sync (Sync Labs)

**Job:** Re-sync an **existing** base video to a dialogue WAV via Sync Labs. Not Text-Sync generation.

**You provide**
- Existing video path
- Dialogue / performance audio path (WAV/MP3)
- Exact output video path

**Settings:** Add a **Sync Labs** API (`https://api.sync.so`). Set **Video → Lip sync (Sync Labs)** to a Sync model (`sync-3`, `lipsync-2`, …).

**Basic prompt**

> Lip-sync `Scenes/Scene4/SC4.mp4` with `Scenes/Scene4/SC4.wav` and save to `Scenes/Scene4/SC4_lipsync.mp4`.

---

### 9. Assemble / post-production

**Job:** Host-owned **ffmpeg** master: trim takes, join, optional music/captions. Not an NLE (escape hatch: Kdenlive on work dir / master). No Atlas / TTS / lip-sync.

**You provide**
1. **Production notes** — usually `prod.md` (scene headings + handoff YAML; Assemble reads `editing` + `speech`)
2. **Assemble map** — usually `Final/final_prompt.txt` (takes, output, trim, captions/music)

**Settings:** System `ffmpeg` / `ffprobe` on PATH. No generation slots.

**Approval:** Assemble Plan → **Approved** → local render.

**Basic prompt**

> Assemble using `Final/final_prompt.txt` and `prod.md`. Overwrite the existing master if present.

**Without a map file**

> Assemble `prod.md` to `Final/Show_final.mp4` using Scenes/ (discover), trim estimated, no captions.

#### Production notes shape (`prod.md`)

Scene Plan writes this. Assemble reads `### Scene N — …` and each scene’s YAML.

| Field | Role |
|-------|------|
| `editing.estimated_screen_duration_seconds` (`S`) | On-screen length when `trim: estimated` (start at `H`, keep `S` seconds) |
| `editing.head_handle_seconds` (`H`) | Pre-roll / settle to skip |
| `editing.tail_handle_seconds` (`T`) | Post-roll; with `trim: handles` → `duration = D − H − T` |
| `editing.transition_in` / `transition_out` | `cut`, `fade` (edge fade on first in / last out; fade/dissolve between scenes when asked), or seamless/frame-continuation → hard cut |
| `speech.mode` | `source_video_lipsync` → baked video audio; `voiceover_only` → lay scene WAV; `none` → B-roll |
| `speech.cues[].text` | Exact caption text when captions enabled |
| `speech.cues[].estimated_duration_seconds` | Caption timing aid |

```yaml
editing:
  estimated_screen_duration_seconds: 5.7
  head_handle_seconds: 1.0
  tail_handle_seconds: 1.0
  transition_in: "cut"
  transition_out: "cut"
  continuity_frame_strategy: "none"
  cut_guidance: null
speech:
  mode: "source_video_lipsync"
  language: "en"
  cues:
    - id: S06_a
      speaker: Mom
      text: "You don't dig your own grave. You get a guy with a backhoe to do that."
      delivery: "practical, corrective, warmly self-assured"
      estimated_duration_seconds: 5.1
```

**Handles vs estimated (talking shots):** Keep `H`/`T` as real pads (often `1.0`). Do not inflate `T` to eat silent model tails. When the take is a long bucket and speech ends early, set `S` to speech + short settle and use **`trim: estimated`**. If trim is omitted and overshoot is obvious, the host may auto-pick `estimated`.

#### Assemble map shape (`Final/final_prompt.txt`)

```yaml
assemble:
  production: "/absolute/or/project-relative/prod.md"
  output: "/absolute/or/project-relative/Final/Show_final.mp4"
  trim: estimated              # estimated | handles | full | custom
  captions:
    mode: none                 # none | soft | burned | both
    language: en
  scenes:
    - id: 1
      video: "/…/Scenes/Scene1/SC1.mp4"
    - id: 2
      video: "/…/Scenes/Scene2/SC2.mp4"
    - id: 4
      video: "/…/Scenes/Scene4/SC4_retry.mp4"
      # audio: "/…/SC4.wav"   # only if VO must be laid under picture
```

Optional: `music_path`, `music_volume` (~0.18), `duck_under_dialogue`, `discover: true`, `prefer_retry`, `overwrite`, `work_dir`, per-scene `trim` / `start` / `duration`.

| Trim | Math (`D` = take length) | When |
|------|--------------------------|------|
| **`estimated`** | start=`H`, duration=`S` | Talking shots / model overshoot |
| **`handles`** | start=`H`, duration=`D−H−T` | Performance fills the take |
| **`full`** | entire file | No editorial trim |
| **`custom`** | per-scene start/duration | Hand-tuned |

**Layout**

```text
Project/
  prod.md
  Scenes/SceneN/SCn.mp4   (+ optional SCn.wav)
  Final/final_prompt.txt
  Final/Show_final.mp4
  Final/work/             # intermediates
```

**After approval:** trim (1080×1920 @ 24fps) → edge fades if first/last request fade → concat → optional music/captions → master + report.

---

### 10. Production Helper

**Job:** Advisory only — read project docs and say whether they look correct; brainstorm why image/video prompts are failing; turn research notes into a short idea digest. Does **not** generate media or assemble.

**You provide**
- What you want: review, brainstorm, or digest
- Docs to check (attach, select in explorer, focus in editor, or name paths)
- Optional output path when you want a digest file written

**Settings:** Chat model only (uses the same chat LLM as Director; no generation slots).

**Basic prompt (review)**

> Attach `prod.md` and `Final/final_prompt.txt`. Are these ready for Assemble? Flag anything Text-Sync or Assemble will choke on.

**Brainstorm example**

> Here’s the still prompt I used for Scene 4’s start frame (attached). It’s drifting Mom’s face — what should I change?

**Digest example**

> Digest `notes/chapter_03.md` into an idea packet at `notes/chapter_03_digest.md` for Script Assistant.

---

### 11. Character Voice (UI, not Director)

**Job:** Record, pick from ElevenLabs library, or design-from-traits. Saves voice assets for later clone/profile use.

**You provide:** Character name / directory via the **Character Voice** tab (View menu or explorer → Open Character Voice…).

**Basic use:** Open the tab → Record or Library → Approve / Choose to save under the character folder. Then point Text-Sync / Scene Generation at that WAV or profile.

## Studio

| Area | Notes |
|------|--------|
| **Studio** | Default production workspace after a project opens. Plan, Produce, and Finish. View → Studio |
| **Jobs** | Ledger under the scene list. Open a finished or failed job to reach its image, video, audio, or review export |
| **Finish** | Takes, review export, picture lock, post, and QC for the selected scene |
| **Director** | Right-hand panel. Attached context is what the next turn sends |
| **Files** | Explorer, editor tabs, and the media player. Rename (`F2`), delete (`Del`, confirms), copy path, Add to Chat |
| **Assets** | Project media filtered from the same files. Opens in the media viewer |
| **Settings** | Edit → Settings… |
| **Voices** | View → Character Voice (or a character’s Create voice / Edit voice). Record / ElevenLabs library / Design traits |

## Settings tips

- API keys live under Electron `userData/config/settings.json`, not in the project. Each named API has its own key (and Secret Key for Kling).
- Protocols are app-defined adapters; you name instances however you like. Chat: Atlas / OpenAI-compatible / Anthropic / Gemini / Ollama. Generation follows the model in Generation defaults: Atlas, Kling (JWT), OpenAI Images, Gemini/Imagen, Replicate, fal.ai, Stability, Runway, Luma, Sync Labs (lip sync).
- Large Atlas catalogs make `settings.json` large; that is expected for now.
- Chat model choice is separate from video/image/audio **Generation defaults**.
- Model Sheet / Still Image read Image generation defaults: `image.edit`, `image.text_to_image`, `image.image_to_image`, `image.character_reference`. Point **Focused edit** at a still edit model (e.g. `*/edit`). Do **not** set character-reference to a `reference-to-image` model that requires video.
- Scene Generation reads Video slots (`video.edit`, `video.start_frame`, `video.character_reference`, `video.location_prop_other_reference`, `video.text_only`) and Audio (`audio.tts_clone`, `audio.tts_profile`). Point **Focused edit** at a video-to-video / edit model. System `ffmpeg` must be on PATH for continuity out-frame extract.
- Text-Sync reads **Video → Text-sync (talking shot)** (`video.lip_sync_text_narration`, fallback Start frame) plus Audio TTS slots. Needs system `ffmpeg` / `ffprobe` for handle pads and optional 9:16 talking stills.

### Adult / NSFW generation defaults

1. **Settings → Adult content** — enable the section and set spicy Atlas models on the slots you need (same shape as Generation defaults). Spicy models stay out of the main Generation defaults list.
2. **Director routing words** — on a generate turn, say **`adult`** or **`nsfw`** (or phrases like “using the adult models” / “spicy models”). The host then prefers Adult slots for that turn; any unset adult slot falls back to the main default.
3. Approval / **continue** after an adult ask still keeps adult routing (recent chat history), so you do not need to repeat the keyword on every “Approved”.
4. Without those words, specialists use **main** Generation defaults even if Adult content is enabled in Settings.

Example: “Please generate this scene using the adult models.” → Scene Generation + Adult video slots.

## Roadmap crumbs

Still coming: more specialists, scene board, richer job UI. Per-function prompts live under **Specialists** above.

## Shortcuts (partial)

| Shortcut | Action |
|----------|--------|
| `Ctrl+O` | Open project |
| `Ctrl+S` / `Ctrl+Shift+S` | Save / Save As |
| `Ctrl+,` | Settings |
| `Ctrl+F` / `Ctrl+H` | Find / Replace |
| `F2` | Rename selected / active file |
| `Del` | Delete selected (when not typing in an editor field) |
| `Ctrl+Shift+P` | Toggle markdown preview |
| `Ctrl+Shift+C` | Copy chat transcript |
| `Ctrl+Shift+E` | Save chat transcript… |
| `Ctrl+Alt+V` | Character Voice |
