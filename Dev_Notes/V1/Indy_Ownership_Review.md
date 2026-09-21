I finished the read-through. Before the map, one thing that overrides everything else:

## The frontend doesn't compile right now

Commit `f1532b9` deleted `src/settingsPanel.ts` and `src/settingsTypes.ts`, but `main.ts` still imports from them in ~20 places:

```
src/main.ts(33,8): error TS2307: Cannot find module './settingsPanel'
```

`index.html:129` still has the `<div id="settings-view">` mount point, `main.ts` still has a `"settings"` tab kind, a dirty-tracking path, a close-confirm path, and an `open-settings` menu handler. So the settings panel wasn't "folded into the workbench" — it was removed and nothing replaced it. The Rust host is clean (`cargo check --all-targets` passes with 4 warnings). Everything below assumes you're going to restore that file, because `capabilityDefaults` is the spine of host-side model selection and there is currently no UI that writes it.

---

# Part 1 — The ownership map as written

## Layer 0: Frontend → host context packet

`main.ts` decides what the model is allowed to see. Nothing else does.

```2942:2944:Desktop_App/src/main.ts
          capabilityDefaults: settings.capabilityDefaults ?? {},
          adultContentEnabled: Boolean(settings.adultContentEnabled),
          adultCapabilityDefaults: settings.adultCapabilityDefaults ?? {},
```

Plus `projectRoot`, `selectedPath`, `focusedEditorPath`, `focusedEditorExcerpt`, `attachedPaths`. **Frontend owns** context selection. **LLM owns** nothing here. The host mutates one field of this packet itself — `adult_routing_requested` — from keyword scanning of the last 8 user turns (`director.rs:184-191`), deliberately not trusting the model with that decision.

## Layer 1: The Director turn — the LLM is a router

The live system prompt is `planner_system_prompt()` (`director_actions.rs:1048-1215`), ~165 lines. It opens with the intended split:

```1053:1055:Desktop_App/host/src/director_actions.rs
You are NOT allowed to call provider APIs, invent tool results, or access the filesystem yourself.
You ONLY emit one or more validated director actions. The host validates, enforces approval/spending, resolves paths, and runs specialists.
For script / character_creation / scene_plan / production_helper, the host loads the specialist contract and calls the selected chat model — you only route with a clear brief and paths.
```

**Host owns** here: action parsing, input-key allowlisting, path resolution under project root (`resolve_under_project`), the 12-round action loop, a 110-second wall clock (`TURN_WALL_CLOCK`), pending-action encode/decode across approval turns, adult-routing detection, and answering "which model will this use?" from Settings without a model call at all (`looks_like_pending_plan_question` → `answer_pending_plan_question`, `director.rs:413-445`).

**LLM owns** here: which specialist runs, and every value inside `inputs` — output paths, prompts, briefs, source paths, `duration`, `aspect_ratio`, `trim`, `caption_mode`, `discover`, `sync_ms`, `voice_clone`, `style`, `reference_images`, and `model`.

### Two real leaks in this layer

**Approval is partly LLM-controlled.** The host gate:

```857:867:Desktop_App/host/src/director.rs
                    let host_must_approve = matches!(
                        specialist.as_str(),
                        "scene_generation" | "model_sheet" | "text_sync" | "lip_sync" | "assemble"
                    );
                    // assemble is local ffmpeg (no provider spend) but still plan-gated.
                    let needs_approval = if specialist == "assemble" {
                        true
                    } else {
                        specialist_spends(&specialist)
                            && (validated.action.requires_approval || host_must_approve)
                    };
```

`specialist_spends` includes `still_image` (`specialists.rs:3517-3522`), but `host_must_approve` does not. So when the model emits `requires_approval:false` on a still, real money is spent with no approval card — and the planner prompt *teaches* it to do exactly that:

```1160:1162:Desktop_App/host/src/director_actions.rs
<director_action>
{{"action":"run_specialist","specialist":"still_image","inputs":{{"source_frame":"Locations/frame.png","edit_brief":"remove coffee cup","output_path":"Locations/frame_v2.png","preserve_props":true}},"requires_approval":false}}
</director_action>
```

**Model IDs are LLM-overridable** despite the prompt saying "You do not pick model IDs yourself." `model` is in the input allowlist, and every media runner honors it ahead of Settings:

```1571:1576:Desktop_App/host/src/specialists.rs
    let model = if let Some(m) = input_str(inputs, &["model"]) {
        ModelRef {
            api_id: String::new(),
            model_id: m.to_string(),
        }
    } else {
```

An empty `api_id` also means the provider router has to guess the adapter.

## Layer 2a: Text specialists (`script`, `character_creation`)

Host loads a bundled contract, builds a dossier from the referenced paths, calls the chat model, parses three tags, writes files. **LLM owns the entire file content *and the destination path*** (it names the path in the `<workbench_file path="...">` attribute; the host only checks it resolves under the project root). This is the least host-owned pipeline in the app.

## Layer 2b: `scene_plan` — the one pipeline built the way you want

This is the interesting one, and it's the template for everything else. The module header states the intent, and it mostly delivers:

```1:4:Desktop_App/host/src/scene_plan.rs
//! Scene-plan handoff: typed parsing, validation, timing, rendering and a
//! transactional commit for LLM-planned cuts. The LLM owns the creative
//! content; this module owns the YAML contract, the arithmetic, asset
//! discovery, and the files.
```

**Host owns:** scene section + dialogue extraction from the script, character/voice asset discovery (`assets.rs`), strategy selection (`strategy.rs` — explicit parse, else inferred from the brief), render-bucket policy per lane derived from the Settings models (`config.rs`), cut numbering and chunking (3 cuts per reply), word-count timing at 165 WPM with idempotent `round2` rounding (`timing.rs`), verbatim dialogue coverage tracking (`coverage.rs:consume_prefix`), 920 lines of cross-field validation, canonical YAML/master/sidecar rendering, atomic stage-then-rename commit, filesystem/state reconciliation (`migrate.rs`), a 2-attempt correction loop, and redo.

**LLM owns:** how many cuts, where the cut boundaries fall, which dialogue goes in which cut, shot type and framing, the visual prompt, negative prompt, continuity narration, delivery notes, transitions, and the speech mode (lip-sync vs. voice-over vs. silent).

## Layer 2c: Media specialists — host owns execution, LLM owns the dials

**Host owns:** lane→slot model resolution, uploads, provider param construction, polling, timeout-resume with a re-encoded pending action, download verification, all ffmpeg. Model-sheet prompts are a fixed Rust format string — genuinely host-owned:

```3061:3070:Desktop_App/host/src/specialists.rs
fn build_shot_prompt(identity: &str, style: &str, wardrobe: &str, framing: &str) -> String {
    format!(
        "{identity}\n\nStyle: {style}.\nWardrobe: {wardrobe}.\n\
Shot: {framing}.\n\
Seamless chroma-green backdrop (#00B140), even neutral studio lighting, no scenic background, \
```

**LLM owns:** `duration` and `aspect_ratio` on the freeform video lane, where the host's only policy is a bare default:

```1334:1337:Desktop_App/host/src/specialists.rs
    let duration = input_str(inputs, &["duration"])
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(5);
    let aspect = input_str(inputs, &["aspect_ratio"]).unwrap_or("16:9");
```

...plus `sync_ms`, `trim`, `caption_mode`, `discover`, and every output path.

## Dead weight I found

Of the nine bundled contracts in `prompts.rs`, **only three are ever sent to a model.** `SCRIPT_ASSISTANT_PROMPT`, `CHARACTER_CREATION_PROMPT`, and `PRODUCTION_HELPER_PROMPT` are live. `MODEL_SHEET`, `STILL_IMAGE`, `SCENE_GENERATION`, `TEXT_SYNC`, and `ASSEMBLE` are never referenced outside their own declarations.

`SCENE_PLAN_PROMPT` is subtler and worth calling out: it's reachable through `text_specialist_prompt("scene_plan")` at `specialists.rs:644`, but `scene_plan` never routes to `run_text_specialist` —

```580:586:Desktop_App/host/src/specialists.rs
        "scene_plan" => {
            let llm = llm.ok_or_else(|| {
                "`scene_plan` needs the Director chat model (pick one in the chat dropdown)."
                    .to_string()
            })?;
            crate::scene_plan::run(app, context, validated, project_root, llm)
        }
```

`scene_plan::run` builds its own contract inline at `scene_plan.rs:841`. So `vid-prompt-system-prompt.md` (611 lines) is also dead, and that match arm is a trap for the next person who reads it. Total: **~2,174 of 2,932 prompt lines compiled into the binary and never sent.** Separately, `scene_plan/fixtures.rs` is a plain `pub mod`, not `#[cfg(test)]`, so 312 lines of test fixtures ship in release builds.

---

# Part 2 — What would have to change

The honest summary: `scene_plan` is roughly 80% of the way to the split you want, and nothing else is past 30%. But even inside `scene_plan`, the LLM is still doing arithmetic and mechanical derivation. Here's what I'd change, in the order I'd do it.

## Tier 0: Unbreak the build

Restore `settingsPanel.ts` / `settingsTypes.ts` from `7c9ddf4`, or finish the removal (drop the `"settings"` tab kind, the `#settings-view` div, the `open-settings` menu item) and ship an alternative editor for `capabilityDefaults`. Right now there is no way for a user to set the slots the entire host depends on.

## Tier 1: Close the three ownership leaks

1. **Take `requires_approval` away from the model entirely.** Delete it from the action schema. Replace `needs_approval` with a host-side decision: does this specialist, with these resolved inputs and this resolved slot, make a billable provider call? `still_image` always spends, so it always gates. If you want a cheap-edit fast path, gate it on the *resolved model's* cost tier, which the host knows and the model doesn't.
2. **Drop `model` from the input allowlist.** Settings slots become the only source of model IDs, matching what the prompt already claims. If you need an override for testing, put it behind a dev flag, not the LLM's JSON.
3. **Make `output_path` host-derived.** `scene_plan` already proves you can do this — `sidecar_root_from` + cut IDs generate every sidecar path without asking. Extend the same convention engine to stills, videos, sheets, and masters. Let `output_path` stay as an override when the user names a file explicitly, but stop requiring the model to invent one.

## Tier 2: Strip the derivable fields out of the scene-plan YAML

This is the highest-leverage change in the codebase. Look at what the contract currently demands:

```882:895:Desktop_App/host/src/scene_plan.rs
generation_requirements:
  source_mode: reference_to_video
  generation_path: audio_driven_i2v
  aspect_ratio: "9:16"
  required_capabilities:
    - character_reference
  priorities:
    - prompt_adherence
prompt: |-
  Creative visual prompt.
reference_assets:
  - path: "<exact path from the host's asset list>"
    role: character_reference
negative_prompt: "bad framing, distorted faces"
```

Every field in `generation_requirements` is mechanically derivable, and `validate.rs` **already encodes the derivation** — as a rejection matrix instead of a resolver:

```905:905:Desktop_App/host/src/scene_plan.rs
- Rejected combinations: `source_video_lipsync` without a usable reference/start frame, with zero or several on-camera speakers, or on `text_to_video`; `audio_driven_i2v` without lip-sync; invented or foreign reference paths.
```

If the host can tell you a combination is illegal, it can tell you which combination is legal. Given `speech.mode`, whether there's an on-camera speaker, and whether that speaker has a discovered reference, `source_mode`, `generation_path`, and `required_capabilities` are determined. Invert the matrix into a resolver and delete all four fields from the contract.

`aspect_ratio` is worse: the host stores it in `PlanState`, passes it to the model in the user block, and then validates that the model echoed it back correctly. That's a round-trip that can only fail.

`reference_assets[].path` — the host discovers assets, prints them to the model, asks the model to copy an exact string, and then rejects it if the copy is wrong (`validate.rs:245`). Meanwhile `validate.rs:186` already computes a speaker→character `CastBinding`. Let the LLM say `speaker: Tom` (a creative choice: who's on camera) and let the host attach the path.

And the one I'd prioritize — duration:

```906:906:Desktop_App/host/src/scene_plan.rs
- Durations: the host recomputes cue durations at 165 WPM. `estimated_screen_duration_seconds + head + tail` must fit a render bucket of the lane you use (listed by the host). Do not emit `target_duration_seconds`.
```

You are asking a language model to add three floats and check membership in a bucket set, then spending one of two `MAX_CORRECTION_ATTEMPTS` when it gets it wrong. The host already computes speech seconds from word count (`timing.rs:seconds_for_words`) and already has `HostConfig::choose_render_duration`. Let the model express *pace* — a rough seconds hint, or an enum like `tight | natural | held` — and have the host snap to the smallest legal bucket at or above the computed speech floor. Head/tail handles are policy constants, not creative decisions. Same for `provider: atlascloud`, `speech.language`, `stage`, and the cue `id` format (`S{source:02}C{cut:02}_suffix` is pure mechanics).

Do all of that and the YAML the model writes collapses to roughly: cut boundaries, title, continuity narration, speech mode per cue, speaker, dialogue text, on-camera flag, delivery, transitions, prompt, negative prompt. Every one of those is a creative choice, and rejections become meaningful.

## Tier 3: Make assemble host-planned

Assemble currently asks the LLM for `trim`, `caption_mode`, `discover`, and a scene list. The host has `prod.md`, has `scene_plan_state.json` with every committed cut, and has `ffprobe`. It also has `editing.transition_in` / `transition_out` already recorded per cut in the validated handoffs. So the host can build the entire assemble plan from committed state and ask the user to approve it, with zero LLM involvement. The only genuinely creative assemble inputs are music selection and any override of the recorded transitions. `discover:true` should become the only mode; the inline `scenes[]` array is a second code path that exists so the model can reconstruct information the host never lost.

## Tier 4: Bring `script` and `character_creation` under the same discipline

These are still "model writes a whole markdown file, host saves it." The tell that this hurts is in `update_character_sheet_refs`, where the host does regex surgery on prose it didn't generate:

```3104:3106:Desktop_App/host/src/specialists.rs
    for line in &mut lines {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("character image reference:") {
```

The host is string-matching a line prefix in a document the model wrote freehand. If character sheets had a typed front-matter block the way cuts have typed YAML — with the host rendering the prose around the model's creative fields — `update_character_sheet_refs` becomes a struct field assignment and `model_sheet` stops depending on the LLM's formatting discipline.

## Tier 5: Shrink the router's job

The planner prompt spends most of its 165 lines teaching the model input *schemas* — which keys `text_sync` takes, which keys `assemble` takes, when not to set `source_frame`. That's API documentation, and the host already owns those schemas (`director_actions.rs` has the allowlists). Most routing is deterministic from input shape anyway: a handoff path → `scene_generation`; prompt + `.png` output → `still_image`; prompt + `.mp4` → freeform video.

The cleaner shape is to have the model emit intent plus a creative brief — `{intent, subject, creative_brief}` — and let the host construct the typed action. That also lets you split the two jobs the model is currently doing in one turn with one model: routing (cheap, deterministic, fast) and creative writing (expensive, needs the strong model). Right now both share the 110-second wall clock and the 12-round loop.

## Tier 6: Measure before you argue

Everything above is my read of the code, not evidence. `error.rs` already classifies every rejection by `IssueKind`. Log a counter per kind per turn. After a week of real use you'll know empirically whether the model is failing on bucket arithmetic and path-copying (mechanical — move to host) or on lip-sync/voice-over logic (creative — keep, but improve the contract). That converts this refactor from a judgment call into a prioritized list.

---

## Two structural risks worth naming

**`specialists.rs` is 3,840 lines** and is the main obstacle to all of the above. It mixes routing, param aliasing, provider quirks, prompt building, approval notes, and resume logic. The param shotgun in `run_scene_generation` is the clearest symptom — the host sets six aliases to the same URL because it doesn't know which key the provider wants:

```1612:1619:Desktop_App/host/src/specialists.rs
    if !uploaded_urls.is_empty() {
        params["image_urls"] = json!(uploaded_urls.clone());
        params["images"] = json!(uploaded_urls.clone());
        params["media_files"] = json!(uploaded_urls.clone());
        params["image"] = json!(uploaded_urls[0]);
        params["image_url"] = json!(uploaded_urls[0]);
        params["reference_image"] = json!(uploaded_urls[0]);
    }
```

That belongs in the provider adapters (`atlas.rs` already remaps and drops unsupported keys). Split `specialists.rs` by lane the way you split `scene_plan` — that refactor is what makes Tiers 1–3 tractable.

**There's a coupling nobody documented.** `run_scene_generation`'s handoff lane now hard-requires a scene-plan-produced document:

```1558:1559:Desktop_App/host/src/specialists.rs
    let request = RenderRequest::from_document(&handoff_text)
        .map_err(|e| format!("Handoff is not a validated scene-plan cut: {e}"))?;
```

But the planner prompt still tells the model it can pass any attached scene file as `handoff_path`, and the fallback path at `specialists.rs:1529-1546` still sniffs attachments for the string `"stage:"`. Any legacy or hand-written handoff will pass the sniff test and then fail parsing with a message the user can't act on. Either migrate old handoffs or make that error explain what to do.

Also minor cleanup while you're in there: delete the five unreferenced prompt constants and the dead `"scene_plan"` arm in `text_specialist_prompt`, gate `fixtures.rs` behind `#[cfg(test)]`, and collapse the three identical `let mut last = Value::Null;` polling loops in `luma.rs` / `replicate.rs` / `runway.rs` into one shared helper — they're generating the same warning three times because they're the same copy-pasted code.