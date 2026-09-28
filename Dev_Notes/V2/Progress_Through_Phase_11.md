# Progress Note — Stopped After Phase 11

**Status:** Phases 0–11 of `Get_Studio_To_Production.md` are complete, committed, and
pushed to `origin/main` (`github.com/krisleighmiller/studio`). Work stopped at the start of
Phase 12.

## Where we stopped

Phase 11 (Sequence Review) is fully done, including the final playback slice. The very last
commit, `00db07e`, extracted the animatic/sequence-review UI into its own module
(`Desktop_App/src/studio/surfaces/sequenceReview.ts`) and added the timeline scrub bar,
click-to-seek, and current-shot highlight.

## Next: Phase 12 — Provider Capability Schemas

Resume by reading the Phase 12 checklist in `Get_Studio_To_Production.md` (starts at
**line 1087**, under `# 16. Phase 12 - Provider Capability Schemas`). No in-progress
code carries over — the working tree is clean.

## Remaining after Phase 11

- Phase 12 — Provider Capability Schemas
- Phase 13 — Production Presets
- Phase 14 — Better Project Creation
- Phase 15 — Reference-Video Analysis
- Phase 16 — Fixture Productions And Product Hardening

## Test baseline (as of the stop point)

- `cargo test` — **348 passed, 0 failed**
- `npm test` — **16 electron + 70 vitest, all passing**
- `tsc -p tsconfig.json` — clean
- `cargo build` — 0 warnings

## Standing constraints to carry forward

- **API-agnostic identity:** the pricing registry keys on *data strings*
  (`ApiEntry.kind` + a `capabilityDefaults` key) — never an enum of providers or
  capabilities. Adding a provider is a `providers.json` data edit, not a code change.
- **Money is integer minor units (i64 cents), never floats.** Unknown cost is an explicit
  `None`/`Unknown`, never zero.
- **Deterministic QC:** never ask an LLM what ffprobe/ffmpeg can answer; media validation
  goes through `qc.rs` structured findings.
- **Solo git hygiene:** checkpoints go straight onto `main`, committed and pushed.
- **Module-boundary gate:** every `*Stage.ts` surface module must stay under 600 lines
  (enforced by `tests/surface-selectors.test.ts`); split cohesive features into their own
  modules when a stage grows.

## Deferred items (unrelated to the stop reason)

- Four `AssetRecord` metadata fields (`duration_seconds`, `version`, `provenance`, structured
  `rights`) are deferred — see `Deferred_Audio_Asset_Metadata.md` for the reason and the
  revisit checklist.
- Six `price_minor` values in `providers.json` are placeholders
  (`pricing_version: "2026-09-27-placeholder"`).