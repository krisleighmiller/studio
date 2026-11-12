# Video Workbench

Desktop production studio. Shell is **Electron** (Chromium); job/host logic runs in a **Rust sidecar** (`host/`).

Opening a project lands in **Studio**. The rail is Home, Studio, Files, Assets, and Voices. Studio is the production workspace: Plan, Produce, and Finish over the recipe stages, with Jobs under the scene list and an inspector (Finish, Saved conversations, Project state) on the right. Files, media viewing, Settings, and Voices stay on their own surfaces. **View → Studio** (`Ctrl+Alt+P`) returns there.

## Surfaces and generation

- Native menu bar: File / Edit / View / Help
- Files keeps the explorer, editor tabs (text, markdown preview, image, audio, video), and media viewer
- File → Open Project… opens any folder as the project root and lands in Studio
- Settings: user-named APIs with wire-protocol kinds (Atlas Cloud, OpenAI chat, Anthropic messages, Gemini, Ollama, ElevenLabs, Kling, Replicate, fal.ai, Stability, Runway, Luma, Sync Labs), model catalogs, **Generation defaults** + optional **Adult content** slots
- Generation runs through host-owned specialists with per-request approval and provider/cost consent. The prompt-only chat specialists (Script, Character Creation, Production Helper) are retired.
- **Character Voice** (View → Character Voice): Record, ElevenLabs library, or Design-from-traits

## Dev (Docker workbench)

Development uses the portable Docker workbench in `Studio/dev-container/`: one synchronized source tree mounted into containers on both the server and the laptop, with Rust builds targeting the host-local `studio-dev_runtime` volume.

```bash
cd Studio/dev-container
./build              # build the studio-dev:portable image (bounded legacy builder)
./prepare            # npm ci + Electron download + frontend build + Rust host build
./studio exec -T workbench cargo build --locked --manifest-path host/Cargo.toml   # rebuild just the host
./native-display --interactive-providers    # interactive launch, from the laptop graphical terminal
```

See `dev-container/README.md` for the full workflow, offline UI smoke, and interactive-provider/keyring setup.

Background: why Electron — `Studio/Dev_Notes/Electron_Fork_Decision.md`. Cutover notes — `Studio/Dev_Notes/Tauri_to_Electron_Migration.md`.
