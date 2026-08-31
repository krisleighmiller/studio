# Electron Fork Decision

Date: 2026-07-30  
Status: Decision executed — workbench shell is Electron + Rust `host/` sidecar (Tauri removed 2026-07-30)  
Related:

- `Studio/Dev_Notes/Tauri_to_Electron_Migration.md` — cutover record / remaining packaging notes
- `Studio/Dev_Notes/Workbench_Spec.md` — product desk / build plan
- `Studio/Dev_Notes/Workbench_Direction.md` — product thesis; mobile later as thin client
- `Studio/Desktop_App/` — Electron + Vite UI + Rust sidecar

---

## Verdict

**Ship the workbench on Electron, not Tauri.**

Linux stability is a hard requirement. The only OS in use for development and daily work is Kubuntu (Wayland). The app’s purpose is video production, so heavy local media playback and future ffmpeg work are not optional. Random WebKit2GTK crash reports already appear **before** the first in-app video-generation request. That is enough signal to change host, not to paper over the host.

---

## Why Tauri fails this project on Linux

### Platform dependency

On Linux, Tauri 2’s webview is **WebKitGTK** (via wry): GTK3 + WebKit2GTK 4.1. There is no supported Chromium webview switch in stable Tauri.

| Platform | Tauri webview |
|----------|----------------|
| Windows | WebView2 (Chromium-based) |
| macOS | WKWebView |
| Linux | WebKit2GTK |

Windows/macOS being “fine” does not help a Linux-only production machine.

### Environment constraints (this machine)

- **Kubuntu / Wayland only.** X11 and XWayland are not acceptable; hardware does not run well on that path (reason for Kubuntu over vanilla Ubuntu / Mint).
- **NVIDIA + WebKitGTK** is a known fragile combo (DMA-BUF / EGL / compositing). Crashes are not “just my config”; shipping workarounds would make every NVIDIA user a support case.
- **Heavy video is the product.** Avoiding `<video>` / scrubbing is not an option.
- **Multi-job load is normal.** Two or more jobs often run at once. App or GPU instability during background work is unacceptable.

### Mitigations considered and rejected as strategy

Common env flags before `tauri::Builder`:

| Flag | Effect | Loss |
|------|--------|------|
| `WEBKIT_DISABLE_DMABUF_RENDERER=1` | Skip newer DMA-BUF GPU buffer path | Efficiency; older buffer sharing; sometimes higher CPU |
| `WEBKIT_DISABLE_COMPOSITING_MODE=1` | Disable GPU compositing for the webview | UI smoothness; more CPU-bound scrolling / layers |
| `GDK_BACKEND=x11` | Force XWayland | Off the table here |

These can help blank-window / EGL / startup failures. They are a weak fix for playback + GPU pressure under real load. They are **not portable product strategy**: other NVIDIA (and some AMD/Intel) stacks can still fail differently. The app already treats WebKit media as fragile (`read_media_bytes` → blob URLs because `asset://` + GStreamer is unreliable).

Upstream GTK4 / WebKitGTK 6 migration exists as work-in-progress for wry/tauri; it is not a shippable replacement today, and it is still WebKit — not a Chromium escape.

---

## Alternatives considered

Anything that still uses **system webview on Linux** keeps the same class of risk:

- Tauri  
- Wails  
- Neutralino  

Real escapes:

| Option | Linux UI engine | Reuse current Vite desk | Notes |
|--------|-----------------|-------------------------|--------|
| **Electron** | Bundled Chromium | High | Best fit; known-good on this hardware; familiar |
| CEF | Chromium | High (HTML) | Same media benefits; much more host engineering |
| Qt (Quick or WebEngine) | Native / Chromium | None | Strong desktop platform; full UI rewrite |
| Flutter | Skia / Impeller | None | Solid cross-platform; Dart rewrite |
| Avalonia | Own renderer | None | .NET rewrite |
| Slint / iced / egui | Native | None | Wrong shape for editor + chat + video desk |
| Tauri + external player (mpv) | Still WebKit for chrome | Full | Band-aid; does not fix general WebKit instability |

**Choice: Electron** — not because it is elegant, but because it preserves the desk UI, puts the app on Chromium (what portable Linux video-ish apps actually ship), and is already validated on this hardware by daily Electron use.

---

## What an Electron fork means (not a 1:1 UI port)

Current split (~2026-07-30):

| Layer | Approx size | Fate |
|-------|-------------|------|
| Vite frontend (`src/*.ts`, CSS) | ~5k LOC | Keep conceptually; rewrite host bridge only |
| Rust backend (`src-tauri/src/*.rs`) | ~4k LOC | Rehost: sidecar preferred short-term, or port to Node later |
| Tauri shell (commands, menu, dialogs, `convertFileSrc`) | Glue | Replace with Electron main / IPC / `dialog` / `Menu` |

Frontend panels (tree, editor, player, chat, settings, voice) stay. Tauri APIs do not.

Recommended architecture:

```text
Electron shell (Chromium UI)
  └── IPC
        └── Job / host service  (prefer: keep Rust as sidecar initially)
              ├── FS + project I/O
              ├── Atlas / director / ElevenLabs
              ├── Voice capture (pw-record / arecord)
              └── ffmpeg jobs (extract, assemble) when added
```

Jobs must not live only inside the renderer. A UI crash must not silently kill mid-flight generation. That boundary also leaves a door for a later remote/thin client (see below).

Tradeoffs accepted with Electron: larger install and RAM; better local video, Wayland, and crash behavior for this workload.

---

## Mobile / tablet / remote — thought experiment (do not bake now)

### Personal pain

Laptop held up by a resource-heavy test run; inability to keep directing video work from another device. Wish-list, not a deal-breaker.

### Likelihood

| Mode | Chance | Reality |
|------|--------|---------|
| Full studio on phone/tablet (ffmpeg, heavy scrub, full desk) | Low | Bad fit; local post is a resource hog |
| Thin remote director (approve, kick job, edit script/prompts, light media) | Medium–high for personal use | Matches the pain; heavy gen already API-side |
| Public multi-user hosted SaaS | Separate product | Auth, sync, workers — not a side quest |

Heavy generation via external APIs helps a thin client. Local ffmpeg (last frame, assemble, pad audio) stays on a machine with the project disk — not on the phone.

### Ideas kicked around

1. **Self-hosted server** (FastAPI-like): Electron can run Express/Fastify/Hono in main, or a separate daemon. Phone hits LAN URL. Solves thumb-twiddling only if the **host** runs the jobs; ffmpeg does not move to the phone.
2. **Smaller mobile build** (scripts/prompts/simple media, no ffmpeg): sensible later product slice; wrong focus before the desktop desk works.

### Decision

**Do not** build mobile UI, LAN server, or a second slim app now.

**Do** leave a cheap architectural door during the Electron move:

1. Project folder remains source of truth (already the model).
2. Jobs / API / ffmpeg live outside the renderer (sidecar or main-process service).
3. Do not hard-wire “only one local window may talk to jobs” — IPC now; HTTP/WebSocket later as a thin adapter on the same service.

This matches existing product notes: mobile later as client/server for approve / kick / short assemble; not a full on-device studio; do not build mobile before the desktop desk exists (`Workbench_Direction.md`, `Workbench_Spec.md` Phase 5 / non-goals).

Cheaper relief for laptop thrashing in the meantime: process niceness, avoid heavy local scrub during assemble, or run the host on a second machine.

---

## Timing

Switch **before** the first video-generation path and more ffmpeg surface area land on Tauri.

Every new Tauri command, menu hook, and media path makes the move more expensive. WebKit flags would only delay a host decision that the constraints already force.

---

## Non-goals of this decision

- Waiting for Tauri GTK4 / WebKit6 as the stability fix  
- Shipping WebKit env-flag workarounds as the Linux support story  
- Building mobile or a public hosted backend in the Electron cutover  
- Rewriting specialist prompts or the project-folder contract as part of the shell change  

The shell changes. The production desk contract does not.
