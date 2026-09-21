# Tauri → Electron Migration Outline

Date: 2026-07-30  
Status: Complete (shell cutover) — Electron + Rust `host/` sidecar; Tauri removed. Packaging (`electron-builder`) still optional.  
Related:

- `Studio/Dev_Notes/Electron_Fork_Decision.md` — why Electron; mobile/remote deferred
- `Studio/Dev_Notes/Workbench_Spec.md` — desk features to preserve
- `Studio/Desktop_App/` — Electron shell (`electron/`), Vite UI (`src/`), Rust sidecar (`host/`)

---

## Goal

Replace the Tauri 2 shell with Electron while keeping:

- The Vite desk UI (tree, editor, player, chat, settings, voice)
- Project-folder-as-source-of-truth
- Existing Rust job logic (Atlas, director, ElevenLabs, voice, settings, FS) with minimal rewrite at first
- A process boundary so jobs are not trapped in the renderer (door for later thin remote client)

Out of scope for the cutover: mobile UI, LAN HTTP API, new ffmpeg features, provider expansion.

---

## Current inventory

### Frontend (`Desktop_App/src/`)

| Area | Files | Host coupling |
|------|-------|----------------|
| Desk shell | `main.ts`, `styles.css`, `index.html` | Heavy: `invoke`, dialogs, menu events, `convertFileSrc`, window |
| Settings | `settingsPanel.ts`, `settingsTypes.ts` | `invoke` (load/save/catalog) |
| Voice | `voicePanel.ts` | `invoke` + dialog `open` |
| Media helpers | `media.ts` | Mostly pure; playback wired from `main.ts` |
| Markdown | `markdown.ts` | None |

Stack: Vite 6, TypeScript, vanilla TS (not React). UI talks to the host only through `src/host.ts` → Electron preload.

### Rust host (was `src-tauri/`; now `Desktop_App/host/src/`)

| Module | Role | Approx LOC |
|--------|------|------------|
| `lib.rs` | FS commands, classify, `read_media_bytes`, menu setup, WebKit env | ~400 |
| `atlas.rs` | Atlas direct API, polling, uploads | ~1.2k |
| `director.rs` | Director turns, tools, progress | ~1.1k |
| `voice.rs` | `pw-record` / `arecord` / `wpctl`, WAV finalize | ~650 |
| `elevenlabs.rs` | Voices, TTS preview, design voice | ~400 |
| `settings.rs` | Settings file + Atlas catalog fetch | ~200 |
| `prompts.rs` | Embedded prompt text | ~270 |

Commands registered today (IPC surface to re-expose):

**FS / media:** `list_dir`, `read_text_file`, `write_text_file`, `create_file`, `create_dir`, `rename_path`, `delete_path`, `classify_file`, `can_open_in_editor`, `read_media_bytes`

**Settings:** `load_settings`, `save_settings`, `fetch_atlas_catalog`

**Director:** `director_turn` (async)

**ElevenLabs:** `elevenlabs_list_voices`, `elevenlabs_tts_preview`, `elevenlabs_design_voice`, `elevenlabs_create_designed_voice`, `elevenlabs_download_preview_url`

**Voice:** `voice_list_recording_sources`, `voice_start_recording`, `voice_stop_recording`, `voice_recording_level`, `voice_save_recording`, `voice_save_library_voice`, `voice_save_designed_voice`

**Events / OS:** custom application menu → frontend `listen("menu", …)`; dialogs via plugin; Linux edit-menu quirks already owned by the frontend.

### Linux-specific behavior to preserve or improve

- Media: prefer reliable local playback under Chromium (`file://` / custom protocol / path APIs). Blob-via-`read_media_bytes` can remain as fallback but should not be required for routine video.
- Voice: keep shelling to PipeWire/`arecord` from the host process (works from Electron main or Rust sidecar via `child_process` / existing Rust).
- Do **not** carry forward WebKit env hacks (`WEBKIT_GST_*`, DMA-BUF / compositing disables) as product requirements.

---

## Target shape

```text
Desktop_App/   (or renamed workbench package)
├── electron/
│   ├── main.ts          BrowserWindow, Menu, dialog, IPC facade
│   ├── preload.ts       contextBridge → window.workbench API
│   └── paths / protocol helpers
├── host/                Rust sidecar (moved from src-tauri) OR in-process later
│   └── same modules: atlas, director, voice, …
├── src/                 existing Vite UI (bridge imports swapped)
├── index.html
├── package.json         electron + vite + electron-builder (or forge)
└── …
```

**Phase-A host (recommended):** Rust binary as **sidecar**. Electron main spawns it, talks JSON-RPC or line-delimited JSON over stdio (or localhost loopback with auth token). UI → `ipcRenderer.invoke` → main → sidecar.

**Phase-B host (optional later):** Port hot paths to TypeScript in Electron main / utilityProcess. Only if maintaining two runtimes hurts more than a port.

---

## API mapping (Tauri → Electron)

| Tauri today | Electron target |
|-------------|-----------------|
| `invoke("cmd", args)` | `window.workbench.invoke("cmd", args)` via preload → `ipcMain.handle` → sidecar/main |
| `listen("menu", …)` | main `Menu` click → `webContents.send("menu", id)` → renderer listener |
| `open` / `save` / `ask` (plugin-dialog) | `dialog.showOpenDialog` / `showSaveDialog` / `dialog.showMessageBox` in main, exposed through preload |
| `convertFileSrc(path)` | `pathToFileURL` or `app://` / custom privileged protocol registered in main |
| `getCurrentWindow()` | drop or thin wrapper; use DOM / Electron where needed |
| `tauri-plugin-opener` | `shell.openPath` / `shell.openExternal` |
| Rust `app.emit` progress | sidecar → main → `webContents.send("job-progress", …)` |

Keep the **command names and JSON shapes** stable where possible so the UI churn is mostly import/wrapper replacement, not every call site.

Suggested thin frontend module:

```ts
// src/host.ts — single seam
export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
export function onMenu(handler: (id: string) => void): () => void;
export function toMediaUrl(path: string): string;
export async function openDialog(opts: …): Promise<…>;
// etc.
```

During migration, `host.ts` can temporarily wrap `@tauri-apps/*` then switch to Electron preload without rewriting every panel twice.

---

## Migration phases

### Phase 0 — Freeze Tauri feature work on the shell

- Stop adding Tauri-only APIs except critical bugfixes.
- Introduce `src/host.ts` seam in the current app (still backed by Tauri) so panels stop importing `@tauri-apps/*` directly.
- Document command list (this file) as the contract.

### Phase 1 — Electron shell boots the existing UI

- Add Electron main + preload; Vite `loadURL` in dev / `loadFile` in prod.
- Window, basic menu (Open Project, Save, Edit actions already frontend-owned), no sidecar yet if needed for a smoke test with mocked invoke.
- Confirm Wayland + NVIDIA: window shows, no WebKit stack, scrub a local sample video in Chromium.

**Exit criteria:** Empty or mocked desk runs stably on Kubuntu Wayland with real `<video>` playback.

### Phase 2 — Sidecar IPC parity

- Package Rust as a standalone binary (same code, thin CLI/stdio host instead of `tauri::command`).
- Implement Electron main router for every command in the inventory.
- Wire dialogs, menu events, settings path (Electron `userData` vs Tauri app config dir — migrate or dual-read once).
- Replace `convertFileSrc` / blob path with Chromium-friendly media URLs; keep `read_media_bytes` only if still useful.

**Exit criteria:** Open project, edit/save files, settings, director turn, voice list/record smoke, media scrub — all through Electron + sidecar.

### Phase 3 — Cleanup (done) / Packaging (optional later)

- ~~Remove `@tauri-apps/*`, `src-tauri`, Tauri fallbacks in `host.ts` / UI~~ — done.
- ~~Update README / decision docs; Workbench_Spec shell line is Electron~~ — done.
- `electron-builder` (or Forge) for Linux AppImage/deb — **not done yet**; keep ffmpeg as system dependency or bundle later deliberately.

**Exit criteria (cleanup):** Electron-only tree; `npm run dev:app` is the only desktop entry.  
**Exit criteria (packaging):** local Linux package install that survives multi-job use on this machine.

### Phase 4 — Resume product work

- Next feature work (video generation request, ffmpeg last-frame, etc.) lands **only** on Electron.
- Still no mobile/LAN server unless pain returns; if it does, add HTTP adapter on the **same** sidecar protocol (see decision doc).

---

## Sidecar protocol sketch

Keep it boring:

- Main spawns `workbench-host` with env `WORKBENCH_PROJECT` optional, `WORKBENCH_SETTINGS_DIR`, etc.
- Request: `{ "id": "1", "cmd": "list_dir", "args": { "path": "…" } }`
- Response: `{ "id": "1", "ok": true, "result": … }` or `{ "id": "1", "ok": false, "error": "…" }`
- Events (no id): `{ "event": "job-progress", "payload": { … } }`

`director_turn` and Atlas polling stay in the sidecar so the UI thread/renderer never blocks on provider waits. Electron main should not re-implement retry/poll logic in Phase 2.

---

## Settings / data path migration

Tauri and Electron use different default config directories. During Phase 2:

1. Detect existing Tauri settings file if present; copy or read-through once.
2. Standardize on Electron `app.getPath("userData")` for the Electron build.
3. Document the path in README so API keys are not “lost” after the switch.

Project content stays in user-chosen folders — no migration needed.

---

## Testing checklist (Linux / Wayland / NVIDIA)

- [ ] App starts on Plasma Wayland without X11 flags  
- [ ] Open large project tree; scroll / expand without crash  
- [ ] Open and scrub local mp4 / webm / wav at 0.25×–1×  
- [ ] Save markdown; create/rename/delete paths  
- [ ] Settings load/save; Atlas catalog fetch  
- [ ] Director turn with tool calls writing files  
- [ ] Voice: list sources, short record, save profile  
- [ ] Two overlapping jobs (e.g. director + TTS preview) while scrubbing media  
- [ ] Kill renderer (or reload) mid-job: sidecar job behavior is defined (continue vs cancel) — pick continue for generation  

---

## Risk register

| Risk | Mitigation |
|------|------------|
| Sidecar protocol becomes a second framework | Keep command names identical to today’s `invoke` surface; one router table |
| Settings/API keys “disappear” after switch | Explicit one-time path migrate |
| Media still awkward under `file://` | Custom privileged protocol; never depend on WebKit GST hacks |
| Scope creep into mobile/HTTP | Decision doc: defer; only preserve process boundary |
| Dual maintenance Tauri + Electron | No dual ship: Phase 0 freeze, then cut over; do not feature-parity both |

---

## Effort intuition (order-of-magnitude)

| Work | Relative |
|------|----------|
| `host.ts` seam + stop direct Tauri imports | Small |
| Electron main/preload/menu/dialogs boot | Small–medium |
| Rust sidecar wrapper + IPC parity | Medium (largest chunk) |
| Media URL strategy + packaging | Medium |
| Full Rust → Node port | Large — **not** required for cutover |

The desk UI is not the expensive part. The host boundary is.

---

## Immediate next actions

1. ~~Add `src/host.ts` seam~~ — done.  
2. ~~Scaffold Electron main/preload~~ — done.  
3. ~~Extract Rust into a stdio sidecar~~ — done (`host/`, `workbench-host`).  
4. ~~Delete Tauri shell / deps / fallbacks~~ — done.  
5. Resume Workbench_Spec feature work (video gen, ffmpeg) on Electron only.  
6. Optional later: `electron-builder` AppImage/deb packaging.

### Dev entry

```bash
cd Studio/Desktop_App
npm install
npm run dev:app
```
