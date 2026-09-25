# Video Workbench

Desktop production studio. Shell is **Electron** (Chromium); job/host logic runs in a **Rust sidecar** (`host/`).

Opening a project lands in **Studio**. The rail is Home, Studio, Files, Assets, and Voices. Studio is the production workspace: Plan, Produce, and Finish over the recipe stages, with Jobs under the scene list and Director on the right. Files, media viewing, Settings, and Voices stay on their own surfaces. **View → Studio** (`Ctrl+Alt+P`) returns there.

## Director and specialists

- Native menu bar: File / Edit / View / Help
- Files keeps the explorer, editor tabs (text, markdown preview, image, audio, video), and media viewer
- File → Open Project… opens any folder as the project root and lands in Studio
- Settings: user-named APIs with protocol kinds (Atlas, OpenAI-compatible, Anthropic, Gemini, Ollama, ElevenLabs, Kling, Replicate, fal.ai, Stability, Runway, Luma, Sync Labs), model catalogs, **Generation defaults** + optional **Adult content** (spicy) slots, chat model picker. Say **adult** / **nsfw** in Director chat to use Adult slots for that turn.
- Generative routing by model → Settings API: Atlas, Kling (JWT), OpenAI Images, Gemini/Imagen, Replicate, fal.ai, Stability, Runway, Luma, Sync Labs (lip sync)
- Director action protocol: models propose validated actions; host enforces paths, approval, and provider calls
- Director chat: thin planner emits validated actions; host runs specialists. Text runners (**Script**, **Character Creation**, **Scene Plan**, **Production Helper**) load `System_Prompts/` via the selected chat model. Media runners (**Model Sheet**, **Still Image**, **Scene Generation**, **Text-Sync**, **Lip sync**, **Assemble**) use Settings / Adult generation slots
- **Character Voice** tab (View menu / explorer): Record, ElevenLabs library, or Design-from-traits — UI-only, not Director

## Dev (Electron + Rust host)

Requires: Node 20+, Rust toolchain, system PipeWire tools for voice recording.

```bash
cd Studio/Desktop_App
npm install
npm run dev:app
```

This builds `workbench-host`, starts Vite on `http://127.0.0.1:1420`, then launches Electron.

Settings live under Electron `userData/config/settings.json`. On first launch, settings are copied once from the old Tauri config dir if present (`~/.config/com.kris.videoworkbench/`).

## Host only / UI only

```bash
npm run dev:host          # cargo build host/
npm run build:host        # release host binary
npm run build             # tsc + vite build
npm run dev               # Vite only (no desktop shell)
npm start                 # Electron loads dist/index.html (not the dev server)
```

Background: why Electron — `Studio/Dev_Notes/Electron_Fork_Decision.md`. Cutover notes — `Studio/Dev_Notes/Tauri_to_Electron_Migration.md`.
