//! Editable production notes and desk-owned render preparation.
//!
//! The canonical sidecar remains the execution handoff. The desk edits the
//! typed handoff and mirrors the same block into the production master.
use crate::director_actions::{
    format_approval_request, validate_action, ActionKind, DirectorAction,
};
use crate::scene_plan::config::HostConfig;
use crate::scene_plan::extract::{load_handoff_document, replace_cut_block};
use crate::scene_plan::handoff::{
    AssetRole as HandoffAssetRole, ContinuityFrameStrategy, Cue, CutId, DerivedTiming,
    ReferenceAsset, RenderLane, SceneType, Speech, SpeechMode,
};
use crate::scene_plan::render::{canonical_yaml, master_block, sidecar_document};
use crate::scene_plan::resolve::{
    apply_duration, apply_generation, assign_cue_ids, resolve_duration, resolve_generation,
};
use crate::scene_plan::timing::{cue_timing, round2};
use crate::workflow::sources::{resolve_render_source, RenderSourceResolution};
use crate::workflow::state::{
    AssetRole, JobRecord, JobStatus, ProjectState, ShotRole, VisualSourceStrategy,
};
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn resolve(root: &Path, value: &str) -> Result<PathBuf, String> {
    crate::project_path::resolve(
        value,
        Some(root.to_string_lossy().as_ref()),
        crate::project_path::PathIntent::Read,
    )
    .map(|path| path.into_path_buf())
    .map_err(|error| error.to_string())
}

fn production_master(root: &Path, scene_id: u32) -> Option<PathBuf> {
    let state_path = root.join("scene_plan_state.json");
    if let Ok(text) = fs::read_to_string(state_path) {
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            if let Some(path) = value
                .get("sections")
                .and_then(Value::as_array)
                .and_then(|sections| {
                    sections.iter().find(|section| {
                        section.get("source_scene").and_then(Value::as_u64)
                            == Some(u64::from(scene_id))
                    })
                })
                .and_then(|section| section.get("production_path"))
                .and_then(Value::as_str)
            {
                return resolve(root, path).ok();
            }
        }
    }
    let default = root.join("prod.md");
    default.is_file().then_some(default)
}

fn write_atomic(path: &Path, contents: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("invalid output path: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    let temp = path.with_extension(format!(
        "{}.workflow-tmp",
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("txt")
    ));
    fs::write(&temp, contents)
        .map_err(|error| format!("could not write {}: {error}", temp.display()))?;
    fs::rename(&temp, path)
        .map_err(|error| format!("could not replace {}: {error}", path.display()))
}

pub fn save_prompt(
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    cut_number: u32,
    prompt: &str,
    negative_prompt: &str,
) -> Result<String, String> {
    let cut_id = CutId::new(scene_id, cut_number);
    let note_path = state
        .scene(scene_id)
        .and_then(|scene| scene.cuts.iter().find(|cut| cut.id == cut_id))
        .and_then(|cut| cut.production_note_path.clone())
        .ok_or_else(|| {
            format!(
                "Cut {cut_number} has no canonical production note. Create its shot specification first."
            )
        })?;
    let sidecar = resolve(root, &note_path)?;
    let text = fs::read_to_string(&sidecar)
        .map_err(|error| format!("could not read {}: {error}", sidecar.display()))?;
    let mut handoff = load_handoff_document(&text)
        .map_err(|error| format!("production note is not a valid handoff: {error}"))?;
    if handoff.id() != cut_id {
        return Err(format!(
            "{} contains {}, not Scene {scene_id} Cut {cut_number}",
            sidecar.display(),
            handoff.id()
        ));
    }
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("Production prompt cannot be empty.".into());
    }
    handoff.prompt = prompt.to_string();
    handoff.negative_prompt = {
        let negative = negative_prompt.trim();
        (!negative.is_empty()).then(|| negative.to_string())
    };
    let yaml = canonical_yaml(&handoff)?;
    let next_sidecar = sidecar_document(&handoff, &yaml);

    if let Some(master) = production_master(root, scene_id) {
        let current = fs::read_to_string(&master)
            .map_err(|error| format!("could not read {}: {error}", master.display()))?;
        let next = replace_cut_block(&current, cut_id, &master_block(&handoff, &yaml))?;
        write_atomic(&master, &next)?;
    }
    write_atomic(&sidecar, &next_sidecar)?;

    let cut = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .and_then(|scene| scene.cuts.iter_mut().find(|cut| cut.id == cut_id))
        .ok_or_else(|| format!("cut {scene_id}.{cut_number} not found"))?;
    cut.prompt = Some(prompt.to_string());
    cut.negative_prompt = handoff.negative_prompt;
    Ok(note_path)
}

fn handoff_scene_type(role: Option<ShotRole>, has_dialogue: bool) -> SceneType {
    match role {
        Some(ShotRole::Performance) => SceneType::CharacterPerformance,
        Some(ShotRole::Insert | ShotRole::Detail) => SceneType::Insert,
        Some(ShotRole::Establishing) => SceneType::Establishing,
        Some(ShotRole::Reaction | ShotRole::Action) if has_dialogue => SceneType::Dialogue,
        _ => SceneType::BRoll,
    }
}

fn source_reference(state: &ProjectState, id: CutId) -> Result<Vec<ReferenceAsset>, String> {
    let cut = state
        .scene(id.source_scene)
        .and_then(|scene| scene.cuts.iter().find(|cut| cut.id == id))
        .ok_or_else(|| format!("cut {}.{} not found", id.source_scene, id.cut))?;
    let Some(source) = &cut.visual_source else {
        return Ok(cut
            .approved_start_frame
            .iter()
            .map(|path| ReferenceAsset {
                path: path.clone(),
                role: HandoffAssetRole::StartFrame,
            })
            .collect());
    };
    if source.strategy == VisualSourceStrategy::NoStartFrame {
        return Ok(Vec::new());
    }
    let Some(asset_id) = &source.asset_id else {
        return Ok(Vec::new());
    };
    let asset = state
        .asset(asset_id)
        .ok_or_else(|| format!("visual source `{asset_id}` is missing"))?;
    let role = if source.strategy == VisualSourceStrategy::SharedReference {
        HandoffAssetRole::StyleReference
    } else {
        HandoffAssetRole::StartFrame
    };
    Ok(vec![ReferenceAsset {
        path: asset.path.clone(),
        role,
    }])
}

pub fn draft_system_prompt() -> &'static str {
    "You are writing execution-ready VIDEO production prompts from approved production planning. Return ONLY JSON:\n\
{\"cuts\":[{\"cut\":1,\"prompt\":\"full video prompt\",\"negative_prompt\":\"optional\"}]}\n\
Return exactly the requested cuts once each.\n\
\n\
Each prompt must describe the complete generated clip, not a still image and not a new reference image. Synthesize all supplied evidence: the script beat, scene intent, ordered neighboring cut sketches, this cut's role and visual sketch, exact dialogue/performance policy, transition, approved scene-look references, and the approved visual source.\n\
\n\
PROMPT REQUIREMENTS:\n\
- Treat an approved start/continuity image as the literal opening frame supplied to the video model. Continue naturally from it; do not ask the model to recreate a different opening image.\n\
- State the subject action or performance over time, camera behavior, environmental motion, continuity that must be preserved, ending state, and relevant sound.\n\
- Preserve identities, wardrobe, props, location, lighting, composition, and period details established by the supplied visual references.\n\
- Include exact dialogue only when this cut carries dialogue. For source_video_lipsync, name the sole visible speaker and require natural restrained synchronization to the exact line; everyone else remains silent without lip movement.\n\
- For voice_over, make clear that visible mouths do not perform the line. For silent cuts, do not invent dialogue.\n\
- Respect the shot role. Reaction, insert, atmosphere, establishing, transition, and action cuts must not be rewritten as talking-head performances.\n\
- Make the prompt internally complete, concise enough for a video generator, and free of file paths, asset IDs, provider names, YAML, or workflow commentary.\n\
- Negative prompts should prevent likely continuity and generation failures without contradicting the requested action."
}

pub fn draft_user_prompt(
    state: &ProjectState,
    scene_id: u32,
    requested: &[u32],
    brief: &str,
) -> Result<String, String> {
    let scene = state
        .scene(scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    let mut out = format!(
        "SCENE\nTitle: {}\nScript excerpt:\n{}\n\n",
        scene.title,
        scene.source_excerpt.as_deref().unwrap_or("(not available)")
    );
    if let Some(intent) = &scene.intent {
        out.push_str("Approved scene intent:\n");
        out.push_str(
            &serde_json::to_string_pretty(intent)
                .map_err(|error| format!("could not serialize scene intent: {error}"))?,
        );
        out.push_str("\n\n");
    }
    out.push_str("Approved scene-look references:\n");
    if let Some(board) = &scene.look_board {
        for slot in &board.slots {
            let Some(asset_id) = &slot.approved_asset_id else {
                continue;
            };
            let path = state
                .asset(asset_id)
                .map(|asset| asset.path.as_str())
                .unwrap_or("(missing)");
            out.push_str(&format!(
                "- {} / {}: {}. Planning description: {}\n",
                slot.role.display_label(),
                slot.title,
                path,
                slot.prompt.as_deref().unwrap_or("(none)")
            ));
        }
    }
    out.push_str("\nORDERED CUT CONTEXT\n");
    for cut in &scene.cuts {
        let sketch = cut
            .sketch
            .as_ref()
            .ok_or_else(|| format!("Cut {} has no cut sketch.", cut.id.cut))?;
        let source = match resolve_render_source(state, cut.id) {
            Ok(RenderSourceResolution::Bound(path)) => format!("approved visual source: {path}"),
            Ok(RenderSourceResolution::NoStartFrame) => "approved strategy: no start frame".into(),
            Ok(RenderSourceResolution::Unplanned) => "visual source not planned".into(),
            Err(error) => error,
        };
        out.push_str(&format!(
            "\nCut {}{}:\n{}\nVisual source: {}\nStart-frame description: {}\nCurrent production prompt (stale planning may be ignored): {}\n",
            cut.id.cut,
            if requested.contains(&cut.id.cut) {
                " [WRITE THIS CUT]"
            } else {
                ""
            },
            serde_json::to_string_pretty(sketch)
                .map_err(|error| format!("could not serialize cut sketch: {error}"))?,
            source,
            cut.start_frame_prompt.as_deref().unwrap_or("(none)"),
            cut.prompt.as_deref().unwrap_or("(none)")
        ));
    }
    out.push_str(&format!(
        "\nReturn full video prompts for cuts: {}.",
        requested
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    ));
    if !brief.trim().is_empty() {
        out.push_str("\nAdditional user direction:\n");
        out.push_str(brief.trim());
    }
    Ok(out)
}

pub fn parse_drafts(
    value: &Value,
    requested: &[u32],
) -> Result<Vec<(u32, String, String)>, String> {
    let cuts = value
        .get("cuts")
        .and_then(Value::as_array)
        .ok_or_else(|| "production-prompt JSON needs a `cuts` array".to_string())?;
    let mut parsed = Vec::new();
    for item in cuts {
        let cut = item
            .get("cut")
            .and_then(Value::as_u64)
            .map(|number| number as u32)
            .ok_or_else(|| "each production prompt needs a numeric `cut`".to_string())?;
        let prompt = item
            .get("prompt")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .ok_or_else(|| format!("Cut {cut} has no production video prompt"))?
            .to_string();
        let negative = item
            .get("negative_prompt")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string();
        if !requested.contains(&cut) {
            return Err(format!("model returned unrequested Cut {cut}"));
        }
        if parsed.iter().any(|(number, _, _)| *number == cut) {
            return Err(format!("model returned Cut {cut} more than once"));
        }
        parsed.push((cut, prompt, negative));
    }
    if requested
        .iter()
        .any(|cut| !parsed.iter().any(|(number, _, _)| number == cut))
    {
        return Err("model did not return every requested cut".into());
    }
    parsed.sort_by_key(|(cut, _, _)| *cut);
    Ok(parsed)
}

pub fn sync_from_planning(
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    cut_number: u32,
) -> Result<String, String> {
    let id = CutId::new(scene_id, cut_number);
    let cut = state
        .scene(scene_id)
        .and_then(|scene| scene.cuts.iter().find(|cut| cut.id == id))
        .ok_or_else(|| format!("cut {scene_id}.{cut_number} not found"))?;
    let sketch = cut
        .sketch
        .as_ref()
        .ok_or_else(|| format!("Cut {cut_number} has no cut sketch to synchronize."))?;
    let note_path = cut
        .production_note_path
        .clone()
        .ok_or_else(|| format!("Cut {cut_number} has no canonical production note to update."))?;
    let sidecar = resolve(root, &note_path)?;
    let text = fs::read_to_string(&sidecar)
        .map_err(|error| format!("could not read {}: {error}", sidecar.display()))?;
    let mut handoff = load_handoff_document(&text)
        .map_err(|error| format!("production note is not a valid handoff: {error}"))?;
    if handoff.id() != id {
        return Err(format!("{} contains the wrong cut", sidecar.display()));
    }

    handoff.title = sketch.title.clone().or_else(|| cut.title.clone());
    handoff.scene_type = handoff_scene_type(sketch.shot_role, !cut.dialogue.is_empty());
    handoff.editing.pace = sketch.pace.unwrap_or(handoff.editing.pace);
    handoff.editing.continuity_frame_strategy =
        match cut.visual_source.as_ref().map(|source| source.strategy) {
            Some(VisualSourceStrategy::PreviousSelectedTakeExitFrame) => {
                ContinuityFrameStrategy::LastFrameAsStart
            }
            Some(VisualSourceStrategy::SharedReference) => ContinuityFrameStrategy::SharedReference,
            _ => ContinuityFrameStrategy::None,
        };
    let mode = cut.speech_mode.unwrap_or(SpeechMode::Silent);
    let visible_speaker = cut.dialogue.first().map(|line| line.speaker.as_str());
    handoff.speech = Speech {
        mode: Some(mode),
        language: (!cut.dialogue.is_empty()).then(|| {
            handoff
                .speech
                .language
                .clone()
                .unwrap_or_else(|| "en".into())
        }),
        cues: cut
            .dialogue
            .iter()
            .map(|line| Cue {
                id: String::new(),
                speaker: line.speaker.clone(),
                text: line.text.clone(),
                on_camera: mode == SpeechMode::SourceVideoLipsync
                    && visible_speaker == Some(line.speaker.as_str()),
                delivery: None,
                pause_before_seconds: 0.0,
                pause_after_seconds: 0.0,
                estimated_duration_seconds: None,
            })
            .collect(),
    };
    assign_cue_ids(id, &mut handoff.speech.cues);
    handoff.reference_assets = source_reference(state, id)?;
    let concrete_source = handoff
        .reference_assets
        .iter()
        .find(|asset| {
            matches!(
                asset.role,
                HandoffAssetRole::StartFrame | HandoffAssetRole::StyleReference
            )
        })
        .map(|asset| asset.path.clone());
    // The workflow source is authoritative for execution. Keeping an imported
    // model-sheet path here would contradict both reference_assets and the
    // image that the render adapter actually uploads.
    for binding in &mut handoff.cast {
        binding.reference_image = concrete_source.clone();
    }

    let on_camera: BTreeSet<String> = handoff
        .speech
        .cues
        .iter()
        .filter(|cue| cue.on_camera)
        .map(|cue| cue.speaker.clone())
        .collect();
    let generation = resolve_generation(
        handoff.speech.mode,
        &on_camera,
        &handoff.cast,
        None,
        &handoff.reference_assets,
    );
    let lane = RenderLane::for_request(generation.generation_path, generation.source_mode);
    apply_generation(&mut handoff, generation);
    let config = HostConfig::default();
    let cue_timings = handoff
        .speech
        .cues
        .iter()
        .map(|cue| cue_timing(cue, config.speaking_rate_wpm))
        .collect::<Vec<_>>();
    for (cue, timing) in handoff.speech.cues.iter_mut().zip(&cue_timings) {
        cue.estimated_duration_seconds = Some(timing.speech_seconds);
    }
    let speech_seconds = round2(cue_timings.iter().map(|timing| timing.total_seconds).sum());
    let timing = resolve_duration(speech_seconds, handoff.editing.pace, lane, &config)?;
    apply_duration(&mut handoff, &timing);
    handoff.derived = Some(DerivedTiming {
        speaking_rate_wpm: config.speaking_rate_wpm,
        speech_duration_seconds: speech_seconds,
        minimum_source_duration_seconds: timing.minimum_source_seconds,
        render_lane: lane,
        render_duration_seconds: timing.render_duration_seconds,
    });

    let yaml = canonical_yaml(&handoff)?;
    let next_sidecar = sidecar_document(&handoff, &yaml);
    if let Some(master) = production_master(root, scene_id) {
        let current = fs::read_to_string(&master)
            .map_err(|error| format!("could not read {}: {error}", master.display()))?;
        let next = replace_cut_block(&current, id, &master_block(&handoff, &yaml))?;
        write_atomic(&master, &next)?;
    }
    write_atomic(&sidecar, &next_sidecar)?;

    let cut = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .and_then(|scene| scene.cuts.iter_mut().find(|cut| cut.id == id))
        .ok_or_else(|| format!("cut {scene_id}.{cut_number} not found"))?;
    cut.title = handoff.title;
    cut.scene_type = Some(handoff.scene_type);
    cut.render_lane = handoff.derived.as_ref().map(|timing| timing.render_lane);
    cut.prompt = Some(handoff.prompt);
    cut.negative_prompt = handoff.negative_prompt;
    Ok(note_path)
}

fn next_take_path(root: &Path, state: &ProjectState, id: CutId) -> String {
    let dir = format!("Scenes/Scene{}/Cut{}", id.source_scene, id.cut);
    let stem = id.code();
    let first = format!("{dir}/{stem}.mp4");
    let reserved = |path: &str| {
        state
            .jobs
            .iter()
            .any(|job| job.kind == "render" && job.output_path.as_deref() == Some(path))
    };
    if !root.join(&first).exists() && !reserved(&first) {
        return first;
    }
    for version in 2..=999 {
        let rel = format!("{dir}/{stem}_v{version}.mp4");
        if !root.join(&rel).exists() && !reserved(&rel) {
            return rel;
        }
    }
    format!("{dir}/{stem}_v1000.mp4")
}

fn names_match(left: &str, right: &str) -> bool {
    let normalize = |value: &str| {
        value
            .to_ascii_lowercase()
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    let left = normalize(left);
    let right = normalize(right);
    left == right
        || left
            .first()
            .zip(right.first())
            .is_some_and(|(left, right)| left == right)
}

fn text_sync_voice(
    root: &Path,
    state: &ProjectState,
    id: CutId,
) -> Result<Option<(String, String, &'static str)>, String> {
    let cut = state
        .scene(id.source_scene)
        .and_then(|scene| scene.cuts.iter().find(|cut| cut.id == id))
        .ok_or_else(|| format!("cut {}.{} not found", id.source_scene, id.cut))?;
    if cut.speech_mode != Some(SpeechMode::SourceVideoLipsync) {
        return Ok(None);
    }
    let speaker = cut
        .dialogue
        .first()
        .map(|line| line.speaker.as_str())
        .ok_or_else(|| format!("Cut {} is lip-sync but has no dialogue", id.cut))?;
    let character = state
        .characters
        .iter()
        .find(|character| names_match(&character.name, speaker))
        .ok_or_else(|| format!("No character record matches lip-sync speaker `{speaker}`"))?;
    let find = |role| {
        character.asset_ids.iter().find_map(|asset_id| {
            state
                .asset(asset_id)
                .filter(|asset| asset.role == role)
                .map(|asset| (asset.id.clone(), asset.path.clone()))
        })
    };
    if let Some((asset_id, path)) = find(AssetRole::VoiceSample) {
        if root.join(&path).is_file() {
            return Ok(Some((asset_id, path, "voice_clone")));
        }
    }
    if let Some((asset_id, path)) = find(AssetRole::VoiceProfile) {
        if root.join(&path).is_file() {
            return Ok(Some((asset_id, path, "voice_profile")));
        }
    }
    Err(format!(
        "Lip-sync speaker `{}` has no registered voice sample or profile.",
        character.name
    ))
}

pub fn prepare_render(
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    cut_number: u32,
) -> Result<(String, String), String> {
    let id = CutId::new(scene_id, cut_number);
    // Render preflight refreshes host-owned fields and concrete source
    // bindings without replacing the user-approved production prompt.
    sync_from_planning(root, state, scene_id, cut_number)?;
    let cut = state
        .scene(scene_id)
        .and_then(|scene| scene.cuts.iter().find(|cut| cut.id == id))
        .ok_or_else(|| format!("cut {scene_id}.{cut_number} not found"))?;
    let handoff_path = cut.production_note_path.clone().ok_or_else(|| {
        format!(
            "Cut {cut_number} has no production note. Save a canonical shot specification first."
        )
    })?;
    if cut
        .prompt
        .as_deref()
        .is_none_or(|prompt| prompt.trim().is_empty())
    {
        return Err("Save a production prompt before rendering.".into());
    }
    let source = resolve_render_source(state, id)?;
    let text_sync_voice = text_sync_voice(root, state, id)?;
    let is_text_sync = text_sync_voice.is_some();
    let source_label = match &source {
        RenderSourceResolution::Bound(path) => path.clone(),
        RenderSourceResolution::NoStartFrame => "text-only / no start frame".into(),
        RenderSourceResolution::Unplanned => {
            return Err(format!(
                "Cut {cut_number} has no approved visual-source strategy."
            ))
        }
    };
    let reusable_job = state.jobs.iter().rposition(|job| {
        job.kind == "render"
            && job.status == JobStatus::Pending
            && job.raw_provider_id.as_deref() == Some(id.code().as_str())
            && job
                .output_path
                .as_deref()
                .is_some_and(|path| !root.join(path).exists())
    });
    let output_path = reusable_job
        .and_then(|index| state.jobs[index].output_path.clone())
        .unwrap_or_else(|| next_take_path(root, state, id));
    let mut inputs = Map::new();
    inputs.insert("handoff_path".into(), Value::String(handoff_path.clone()));
    inputs.insert("output_path".into(), Value::String(output_path.clone()));
    if !is_text_sync {
        inputs.insert("resolution".into(), Value::String("1080p".into()));
    }
    if let RenderSourceResolution::Bound(path) = &source {
        if cut
            .visual_source
            .as_ref()
            .is_some_and(|source| source.strategy == VisualSourceStrategy::SharedReference)
        {
            inputs.insert(
                "reference_images".into(),
                Value::Array(vec![Value::String(path.clone())]),
            );
        } else {
            inputs.insert("source_frame".into(), Value::String(path.clone()));
        }
    }
    if let Some((_, path, input_key)) = &text_sync_voice {
        inputs.insert((*input_key).into(), Value::String(path.clone()));
    }
    let action = DirectorAction {
        action: ActionKind::RunSpecialist,
        specialist: Some(
            if is_text_sync {
                "text_sync"
            } else {
                "scene_generation"
            }
            .into(),
        ),
        inputs,
        message: None,
        op: None,
        path: None,
    };
    let root_s = root.to_string_lossy().to_string();
    let validated = validate_action(action.clone(), Some(&root_s)).map_err(|e| e.to_string())?;
    let mut card = format!(
        "Render Scene {scene_id} Cut {cut_number}\nProduction note: `{handoff_path}`\nVisual source: `{source_label}`\nNew take: `{output_path}`\n\n"
    );
    card.push_str(&format_approval_request(&action, &validated));

    let mut input_asset_ids: Vec<String> = cut
        .visual_source
        .as_ref()
        .and_then(|source| source.asset_id.clone())
        .into_iter()
        .collect();
    if let Some((asset_id, _, _)) = &text_sync_voice {
        if !input_asset_ids.contains(asset_id) {
            input_asset_ids.push(asset_id.clone());
        }
    }
    crate::workflow::identity::assign(state);
    let shot_id = state
        .scene(scene_id)
        .and_then(|scene| scene.cuts.iter().find(|cut| cut.id == id))
        .map(|cut| cut.stable_id.clone())
        .unwrap_or_default();
    let scope = (!shot_id.is_empty()).then(|| crate::workflow::state::JobScope {
        kind: "shot".into(),
        id: shot_id,
    });
    if let Some(index) = reusable_job {
        state.jobs[index].input_asset_ids = input_asset_ids;
        state.jobs[index].output_path = Some(output_path);
        state.jobs[index].error = None;
        if state.jobs[index].scope.is_none() {
            state.jobs[index].scope = scope;
        }
    } else {
        let job_id = state.next_job_id();
        state.jobs.push(JobRecord {
            provider: Some("atlascloud".into()),
            input_asset_ids,
            output_path: Some(output_path),
            raw_provider_id: Some(id.code()),
            scope,
            ..JobRecord::open(job_id, "render")
        });
    }
    Ok((
        card,
        format!("Render for Cut {cut_number} is ready. Approve the provider plan in chat."),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_plan::config::HostConfig;
    use crate::scene_plan::extract::parse_handoff_yaml;
    use crate::scene_plan::fixtures::{lipsync_scene_yaml, scene_markdown};
    use crate::scene_plan::render::{canonical_yaml, sidecar_document};
    use crate::scene_plan::validate::validate_scene;
    use crate::workflow::state::{
        AssetRecord, AssetStatus, CharacterProfile, CharacterRecord, CutRecord, CutSketch,
        MouthPolicy, VisualSourceSpec, VisualSourceStrategy,
    };

    #[test]
    fn prompt_save_updates_sidecar_and_master() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let handoff = parse_handoff_yaml(&lipsync_scene_yaml(1, "Hello there.")).unwrap();
        let validated = validate_scene(handoff, &HostConfig::default(), None, None).unwrap();
        let yaml = canonical_yaml(&validated.handoff).unwrap();
        fs::create_dir_all(root.join("Scene_Prompts/Scene1")).unwrap();
        fs::write(
            root.join("Scene_Prompts/Scene1/Cut1.txt"),
            sidecar_document(&validated.handoff, &yaml),
        )
        .unwrap();
        fs::write(root.join("prod.md"), scene_markdown(1, &yaml)).unwrap();
        let mut state = ProjectState::new();
        let cut = state.scene_mut(1, "Scene 1").cut_mut(1);
        cut.production_note_path = Some("Scene_Prompts/Scene1/Cut1.txt".into());

        save_prompt(
            root,
            &mut state,
            1,
            1,
            "A revised moving-image prompt.",
            "text",
        )
        .unwrap();

        let sidecar = fs::read_to_string(root.join("Scene_Prompts/Scene1/Cut1.txt")).unwrap();
        let master = fs::read_to_string(root.join("prod.md")).unwrap();
        assert!(sidecar.contains("A revised moving-image prompt."));
        assert!(master.contains("A revised moving-image prompt."));
        assert_eq!(
            state.scenes[0].cuts[0].negative_prompt.as_deref(),
            Some("text")
        );
    }

    #[test]
    fn render_requires_a_ready_source_and_registers_job() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let handoff = parse_handoff_yaml(&lipsync_scene_yaml(1, "Hello there.")).unwrap();
        let validated = validate_scene(handoff, &HostConfig::default(), None, None).unwrap();
        let yaml = canonical_yaml(&validated.handoff).unwrap();
        fs::create_dir_all(root.join("Scene_Prompts/Scene1")).unwrap();
        fs::write(
            root.join("Scene_Prompts/Scene1/Cut1.txt"),
            sidecar_document(&validated.handoff, &yaml),
        )
        .unwrap();
        fs::write(root.join("prod.md"), scene_markdown(1, &yaml)).unwrap();
        let mut state = ProjectState::new();
        let mut cut = CutRecord::new(CutId::new(1, 1));
        cut.prompt = Some("Move gently.".into());
        cut.production_note_path = Some("Scene_Prompts/Scene1/Cut1.txt".into());
        cut.visual_source = Some(VisualSourceSpec::new(VisualSourceStrategy::NoStartFrame));
        cut.sketch = Some(CutSketch {
            shot_role: Some(ShotRole::Atmosphere),
            visual_prompt: Some("Move gently.".into()),
            ..CutSketch::default()
        });
        state.scene_mut(1, "Scene 1").cuts.push(cut);

        let (card, _) = prepare_render(root, &mut state, 1, 1).unwrap();

        assert!(card.contains("Scenes/Scene1/Cut1/S01C01.mp4"));
        assert!(card.contains("\"resolution\": \"1080p\""));
        assert_eq!(state.jobs[0].kind, "render");
        assert_eq!(state.jobs[0].status, JobStatus::Pending);
    }

    #[test]
    fn sync_rebuilds_note_from_sketch_and_approved_source() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let handoff = parse_handoff_yaml(&lipsync_scene_yaml(1, "Hello there.")).unwrap();
        let validated = validate_scene(handoff, &HostConfig::default(), None, None).unwrap();
        let yaml = canonical_yaml(&validated.handoff).unwrap();
        fs::create_dir_all(root.join("Scene_Prompts/Scene1")).unwrap();
        fs::write(
            root.join("Scene_Prompts/Scene1/Cut1.txt"),
            sidecar_document(&validated.handoff, &yaml),
        )
        .unwrap();
        fs::write(root.join("prod.md"), scene_markdown(1, &yaml)).unwrap();
        fs::create_dir_all(root.join("Preproduction/Scene1/start_frames")).unwrap();
        fs::write(
            root.join("Preproduction/Scene1/start_frames/cut_1_v2.png"),
            b"test",
        )
        .unwrap();
        fs::create_dir_all(root.join("Characters/Sam/voice")).unwrap();
        fs::write(
            root.join("Characters/Sam/voice/Sam_voice_sample_01.wav"),
            b"test",
        )
        .unwrap();

        let mut state = ProjectState::new();
        state.assets.push(AssetRecord {
            id: "asset_1".into(),
            path: "Preproduction/Scene1/start_frames/cut_1_v2.png".into(),
            role: crate::workflow::state::AssetRole::CutStartFrame,
            status: AssetStatus::Approved,
            scene_id: Some(1),
            cut_id: Some(CutId::new(1, 1)),
            character_id: None,
            source_job_id: None,
            parent_asset_id: None,
            notes: None,
        });
        state.assets.push(AssetRecord {
            id: "asset_voice".into(),
            path: "Characters/Sam/voice/Sam_voice_sample_01.wav".into(),
            role: AssetRole::VoiceSample,
            status: AssetStatus::Approved,
            scene_id: None,
            cut_id: None,
            character_id: Some("character_sam".into()),
            source_job_id: None,
            parent_asset_id: None,
            notes: None,
        });
        state.characters.push(CharacterRecord {
            id: "character_sam".into(),
            name: "Sam".into(),
            dir: "Characters/Sam".into(),
            identity_ready: true,
            voice_ready: true,
            profile_path: None,
            model_sheet_manifest_path: None,
            profile: CharacterProfile::default(),
            asset_ids: vec!["asset_voice".into()],
            notes: Vec::new(),
        });
        let cut = state.scene_mut(1, "Scene 1").cut_mut(1);
        cut.production_note_path = Some("Scene_Prompts/Scene1/Cut1.txt".into());
        cut.speech_mode = Some(SpeechMode::SourceVideoLipsync);
        cut.dialogue = vec![crate::script::DialogueLine {
            speaker: "Sam".into(),
            text: "Hello there.".into(),
        }];
        cut.sketch = Some(CutSketch {
            title: Some("Sam listens".into()),
            shot_role: Some(ShotRole::Reaction),
            mouth_policy: Some(MouthPolicy::ClosedOrNotVisible),
            visual_prompt: Some("Sam listens without speaking as rain crosses the window.".into()),
            negative_prompt: Some("open mouth".into()),
            ..CutSketch::default()
        });
        cut.visual_source = Some(VisualSourceSpec {
            strategy: VisualSourceStrategy::PreproductionStartFrame,
            predecessor: None,
            asset_id: Some("asset_1".into()),
            source_take_id: None,
            frame_time_seconds: None,
            predecessor_shot_id: None,
        });

        sync_from_planning(root, &mut state, 1, 1).unwrap();

        let text = fs::read_to_string(root.join("Scene_Prompts/Scene1/Cut1.txt")).unwrap();
        let updated = load_handoff_document(&text).unwrap();
        assert_eq!(updated.title.as_deref(), Some("Sam listens"));
        assert_eq!(updated.scene_type, SceneType::Dialogue);
        assert_eq!(updated.speech.mode, Some(SpeechMode::SourceVideoLipsync));
        assert!(updated.speech.cues[0].on_camera);
        assert_eq!(
            updated.cast[0].reference_image.as_deref(),
            Some("Preproduction/Scene1/start_frames/cut_1_v2.png")
        );
        assert_eq!(
            updated.reference_assets,
            vec![ReferenceAsset {
                path: "Preproduction/Scene1/start_frames/cut_1_v2.png".into(),
                role: HandoffAssetRole::StartFrame,
            }]
        );
        assert_eq!(
            updated.derived.unwrap().render_lane,
            RenderLane::AudioDrivenI2v
        );
        let (approval, _) = prepare_render(root, &mut state, 1, 1).unwrap();
        assert!(approval.contains("Operation: `text_sync`"));
        assert!(approval.contains("\"source_frame\""));
        assert!(approval.contains("Preproduction/Scene1/start_frames/cut_1_v2.png"));
        assert!(approval.contains("\"voice_clone\""));
        assert!(approval.contains("Characters/Sam/voice/Sam_voice_sample_01.wav"));
        assert!(state.jobs[0]
            .input_asset_ids
            .contains(&"asset_voice".to_string()));
    }

    #[test]
    fn production_prompt_response_requires_every_requested_cut_once() {
        let parsed = parse_drafts(
            &serde_json::json!({
                "cuts": [
                    {"cut": 2, "prompt": "Pip moves through the full shot.", "negative_prompt": "text"},
                    {"cut": 1, "prompt": "Tom performs the full line.", "negative_prompt": ""}
                ]
            }),
            &[1, 2],
        )
        .unwrap();
        assert_eq!(parsed[0].0, 1);
        assert_eq!(parsed[1].0, 2);
        assert!(parse_drafts(
            &serde_json::json!({"cuts": [{"cut": 1, "prompt": "Only one."}]}),
            &[1, 2]
        )
        .is_err());
    }
}
