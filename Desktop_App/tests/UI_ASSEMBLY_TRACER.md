# A01 connected copied-media opening-fade tracer

This is a **local technical fixture**, not a live model/generation test or a
creative/listening verdict. It reuses `ui-tracer.mjs`'s owned actual Electron main
inspector, matching renderer CDP, native Open Recent selection, real model-change
handler, form submission, Rust sidecar, loopback HTTP planner and unconditional
cleanup. Only the deterministic planner boundary returns canned actions.

## Preconditions and execution

Follow `UI_TRACER.md` prerequisites. The native-input fixture explicitly pins
Electron to X11 (`--ozone-platform=x11`). Use the exact authorized P01 derivative
at `/home/kris/Content_Creation/Projects/Studio_Recovery_Black_Hole`; its six inputs
and immutable `.p01/fixture-state.json` must match the P01 manifest. Do **not**
re-create/overwrite that project or modify its notes to make an import succeed.
The evidence manifest is at
`/home/kris/.hermes/profiles/studio/kanban-plans/current-roadmap/execution/P01/fixture-manifest.json`.

Each evidence directory must be **new** and each run label must have unused output
and work paths. Existing empty P01 provisioning directories are allowed; populated
runtime/work or existing output paths are refused. All existing ancestors must
resolve canonically inside the derivative; symlinked runtime/work/output ancestors
are refused. No symlinks back to the original project.

```sh
node --test electron/uiTracer.test.mjs electron/uiAssemblyTracer.test.mjs
node tests/ui-assembly-tracer.mjs /absolute/persistent/A01/new-run a01-new-run
```

`first` and `independent` final runs are already preserved and **must not be
reused**. Future authorized runs need new labels. No live provider is allowed.
The isolated runtime lives under `.p01/runs/<label>`; output is
`Final/<label>/black_hole_opening_{baseline,revision}.mp4`; work is under
`.p01/work/<label>/{baseline,revision}`. HOME/cache/data/tmp/userData/settings/recents
and unique keyring IDs are fixture-owned. The preprovisioned disabled P01 settings
remain untouched; the actual run's adopted paths are checked before project open.

## What actually happens

1. Submit one original task through the actual form. The real host inspects script
   and production notes; authoritative observations must contain both cue IDs,
   reference paths and dialogue bindings before staging assembly.
2. Stage explicit ordered `SC1.mp4`/`SC2.mp4`, `trim:estimated`, no captions/music,
   embedded `source_video_lipsync`, `overwrite:false`. The host approval must show
   source `[0,4.8)` and `[1,5.6)` windows and effective `in=true out=false`.
3. Assert no corresponding export/work artifact exists, save the visible card,
   and click its on-screen Approve button with CDP mouse input. No test-side
   application dispatch/approval invocation is used.
4. The actual `approve:assemble` completion must be present **verbatim** in the
   following planner request's assistant history with the original user task.
   There is no injected user `continue`. The specialist's legacy false
   `continueSuggested` is recorded; renderer completion-event routing is what
   demonstrably resumes planning.
5. Open the export through existing Files/Explorer rows and play the actual app
   `workbench-media:` video to `ended`; retain dimensions/frame counts/errors.
6. Submit the opening-only revision through the form. Its only editorial input
   difference is `fade_in:false`; output/work destinations necessarily differ.
   `fade_out` stays omitted and effective **false**, not an invented closing fade.
   Require a distinct fresh approval and repeat render/return/playback.
7. Require exactly two approval-required and two successful execution envelopes,
   six expected planner calls total, consumed approval buttons removed, unchanged
   baseline bytes during revision, and unchanged protected fixture inputs/map.

The owned playback window disables Chromium background throttling, explicitly
recorded in evidence, so compositor occlusion cannot indefinitely starve the
existing pre-request `requestAnimationFrame` wait. This is runtime scheduling
configuration, not a stubbed callback, altered renderer state or replacement
planner path. It does **not** verify default occluded-window scheduling behavior.
Model/form events are DOM automation; approvals are visible CDP mouse clicks;
play uses the existing HTML video's `play()` method, not a claimed native-key test.

## Additional acceptance evidence

A tracer `PASS` alone is insufficient. Independently run:

- `ffprobe -v error -show_format -show_streams -of json <each export>`;
- `ffmpeg -v error -i <each export> -f null -`;
- bounded decoded source-window/order/cut/closing and opening picture/audio
  comparisons (A01's durable `compare_media.py` records exact subprocesses);
- protected-original inventory/hash/size/mtime and copied-input checks;
- separate post-exit PID/port/exact-keyring-ID cleanup audit;
- a new independent runtime/export rerun and the focused host/renderer tests.

Use passthrough frame output when comparing decoded video: FFmpeg's default raw
output synchronization can add a frame for the small initial AAC/container
presentation offset. Do not require exact 9.4-second container duration or
byte-identical lossy renders. A01 measures 32x56 grayscale frames after full decode
and 8 kHz mono audio energy/correlation; these are technical checks, not word,
performance, lip-sync or listening acceptance. Kris still owns listening under
G02/A05.

## Native import boundary

The frozen legacy fixture's root `script.md`/`### Scene N` production notes are
accepted by the inline assembly path. They are **not** the native workflow
importer's `Scripts/` plus `Scene N — Cut M` schema. Readback currently records
`scenes:[]`, import diagnostics and discovered take assets. The tracer does not
invent native scene/cut IDs or claim successful native scene import. Selection is
explicit legacy production shots 1–2, corresponding together to script Scene 1.
Do not repair the importer, rewrite source notes, or infer script-to-production
or full-film acceptance from this copied-media milestone.
