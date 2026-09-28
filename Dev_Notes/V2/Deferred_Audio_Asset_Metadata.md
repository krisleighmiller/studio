# Deferred: Audio Asset Metadata Fields

**Status:** explicitly deferred (not a bug, not lost work — a scoped gap).

## What

Phase 10 ("Audio As First-Class Production State") lists eleven audio-metadata
requirements under "Audio Metadata":

- scene association — **done** (`AssetRecord.scene_id`)
- shot association — **done** (`AssetRecord.cut_id`)
- character association — **done** (`AssetRecord.character_id`)
- versioning — **deferred**
- approval — **done** (`AssetRecord.status`)
- duration — **deferred**
- source provider — **deferred**
- generation job — **done** (`AssetRecord.source_job_id`)
- rights/license notes — **deferred** (currently only free-text `AssetRecord.notes`)
- provenance — **deferred** (partially covered by `AssetRecord.parent_asset_id`)

## Why deferred

An attempt to add four fields (`duration_secs`, `version`, `provenance`, `rights`)
directly to `AssetRecord` broke ~25 Rust struct literals across the host test suite
(`host/src/workflow/state.rs` consumers), because the struct is constructed
positionally-by-field in many places and Rust rejects literals that omit a field
without a `#[serde(default)]`-style default. Those edits were reverted.

Separately, there is **no generation path that would populate most of these fields
yet** — no workflow step currently produces a value for `version`, `provenance`, or
`source provider` on an audio asset at creation time. Adding the fields now is
schema churn with no writer and no reader.

## When to revisit

Re-add these fields the next time a workflow step actually produces them, and do it
as a single coordinated change:

1. add the field with `#[serde(default)]` (or an explicit `Default` impl) so existing
   literals keep compiling;
2. sync the frontend type in `workflowTypes.ts` in the same commit;
3. populate it at the generation site (not in a post-hoc backfill).

Until then, treat `version` / `provenance` / `duration` / structured `rights` as
**absent-by-design**, not missing features. Free-text hangs on `notes` when needed.