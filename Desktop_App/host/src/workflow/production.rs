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
    AssetRole as HandoffAssetRole, Continuity, ContinuityFrameStrategy, Cue, CutId, DerivedTiming,
    Editing, GenerationRequirements, ReferenceAsset, RelationToPrevious, RenderLane, SceneHandoff,
    SceneType, Speech, SpeechMode, Stage, Transition,
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
Each prompt must describe the complete generated clip, not a still image and not a new reference image. Synthesize the script beat, scene intent, ordered neighboring cut sketches, this cut's role and visual sketch, exact dialogue/performance policy, and transition. Use approved scene-look references and a visual source only when supplied; never demand separate prop images just to write a prompt.\n\
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
    out.push_str("Selected approved scene-look references for these cuts:\n");
    let selected_roles: Vec<AssetRole> = scene.cuts.iter()
        .filter(|cut| requested.contains(&cut.id.cut))
        .flat_map(|cut| cut.sketch.iter().flat_map(|sketch| sketch.reference_roles.iter().copied()))
        .collect();
    if let Some(board) = &scene.look_board {
        for slot in &board.slots {
            if !selected_roles.contains(&slot.role) {
                continue;
            }
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
    sync_from_planning_with_prompt(root, state, scene_id, cut_number, None)
}

/// A first editable prompt can establish the canonical sidecar without an image.
/// Subsequent updates retain the existing typed handoff and its identity.
pub fn sync_from_planning_with_prompt(
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    cut_number: u32,
    initial_prompt: Option<&str>,
) -> Result<String, String> {
    let (path, _) = prepare_from_planning(root, state, scene_id, cut_number, initial_prompt, false)?;
    Ok(path)
}

/// Compute the handoff without writing the canonical sidecar or master.
pub fn preview_from_planning(root: &Path, state: &ProjectState, scene_id: u32, cut_number: u32) -> Result<Value, String> {
    let mut staged = state.clone();
    let (_, preview) = prepare_from_planning(root, &mut staged, scene_id, cut_number, None, true)?;
    Ok(preview)
}

fn prepare_from_planning(
    root: &Path, state: &mut ProjectState, scene_id: u32, cut_number: u32,
    initial_prompt: Option<&str>, dry_run: bool,
) -> Result<(String, Value), String> {
    let id = CutId::new(scene_id, cut_number);
    let cut = state
        .scene(scene_id)
        .and_then(|scene| scene.cuts.iter().find(|cut| cut.id == id))
        .ok_or_else(|| format!("cut {scene_id}.{cut_number} not found"))?;
    if dry_run && cut.script_dialogue_review.is_some() {
        return Err("Script dialogue changed. Resolve this cut in Sketches before refreshing its production note; existing authored cues remain untouched.".into());
    }
    let sketch = cut
        .sketch
        .as_ref()
        .ok_or_else(|| format!("Cut {cut_number} has no cut sketch to synchronize."))?;
    let new_note = cut.production_note_path.is_none();
    let note_path = cut.production_note_path.clone().or_else(|| initial_prompt.map(|_| {
        format!("Scene_Prompts/Scene{scene_id}/Cut{cut_number}.txt")
    })).ok_or_else(|| format!("Cut {cut_number} has no canonical production note to update."))?;
    let sidecar = if new_note { root.join(&note_path) } else { resolve(root, &note_path)? };
    let mut handoff = if new_note {
        if sidecar.exists() {
            return Err(format!("Existing production note `{note_path}` is not registered; refresh/import it rather than overwriting."));
        }
        SceneHandoff {
            source_scene: scene_id,
            cut: cut_number,
            title: sketch.title.clone().or_else(|| cut.title.clone()),
            provider: "atlascloud".into(),
            stage: Stage::BaseVideo,
            scene_type: handoff_scene_type(sketch.shot_role, !cut.dialogue.is_empty()),
            continuity: Continuity {
                group: format!("scene_{scene_id}"),
                relation_to_previous: RelationToPrevious::NewSetup,
                opening_state: None, closing_state: None, carry_forward: Vec::new(),
            },
            editing: Editing {
                estimated_screen_duration_seconds: 0.0,
                head_handle_seconds: 0.0, tail_handle_seconds: 0.0,
                pace: sketch.pace.unwrap_or_default(),
                transition_in: Transition::Cut, transition_out: Transition::Cut,
                continuity_frame_strategy: ContinuityFrameStrategy::None,
                cut_guidance: None,
            },
            speech: Speech::default(),
            generation_requirements: GenerationRequirements::default(),
            prompt: initial_prompt.unwrap_or_default().trim().to_string(),
            reference_assets: Vec::new(), negative_prompt: None,
            generation_notes: None, cast: Vec::new(), derived: None,
        }
    } else {
        let text = fs::read_to_string(&sidecar)
            .map_err(|error| format!("could not read {}: {error}", sidecar.display()))?;
        load_handoff_document(&text)
            .map_err(|error| format!("production note is not a valid handoff: {error}"))?
    };
    if handoff.id() != id {
        return Err(format!("{} contains the wrong cut", sidecar.display()));
    }
    let previous = handoff.clone();

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
    // Replace only the selected execution source. Other reference assets are
    // authored context, not derived from the visual-source picker.
    let mut selected = source_reference(state, id)?;
    selected.extend(handoff.reference_assets.iter().filter(|asset| {
        !matches!(asset.role, HandoffAssetRole::StartFrame | HandoffAssetRole::StyleReference)
    }).cloned());
    handoff.reference_assets = selected;
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
    let old_sidecar = if new_note { String::new() } else {
        fs::read_to_string(&sidecar).map_err(|e| format!("could not read {}: {e}", sidecar.display()))?
    };
    let master = production_master(root, scene_id);
    let old_master = master.as_ref().map(fs::read_to_string).transpose()
        .map_err(|e| format!("could not read production master: {e}"))?;
    let next_master = old_master.as_ref().map(|current| replace_cut_block(current, id, &master_block(&handoff, &yaml))).transpose()?;
    let mut signature = sha2::Sha256::new();
    use sha2::Digest;
    signature.update(serde_json::to_vec(state).map_err(|e| e.to_string())?);
    signature.update(old_sidecar.as_bytes());
    signature.update(old_master.as_deref().unwrap_or("").as_bytes());
    let preview = serde_json::json!({
        "path": note_path,
        "oldCues": previous.speech.cues,
        "newCues": handoff.speech.cues,
        "oldTiming": previous.derived,
        "newTiming": handoff.derived,
        "oldScreenSeconds": previous.editing.estimated_screen_duration_seconds,
        "newScreenSeconds": handoff.editing.estimated_screen_duration_seconds,
        "oldReferences": previous.reference_assets,
        "newReferences": handoff.reference_assets,
        "prompt": handoff.prompt,
        "negativePrompt": handoff.negative_prompt,
        "changed": next_sidecar != old_sidecar || next_master != old_master,
        "oldDocument": old_sidecar,
        "newDocument": next_sidecar,
        "oldMaster": old_master,
        "newMaster": next_master,
        "revision": signature.finalize().iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
    });
    if dry_run { return Ok((note_path, preview)); }
    if let Some(master) = master {
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
    cut.production_note_path = Some(note_path.clone());
    cut.scene_type = Some(handoff.scene_type);
    cut.render_lane = handoff.derived.as_ref().map(|timing| timing.render_lane);
    cut.prompt = Some(handoff.prompt);
    cut.negative_prompt = handoff.negative_prompt;
    Ok((note_path, preview))
}

pub(super) fn next_take_path(root: &Path, state: &ProjectState, id: CutId) -> String {
    let dir = format!("Scenes/Scene{}/Cut{}", id.source_scene, id.cut);
    let stem = id.code();
    let first = format!("{dir}/{stem}.mp4");
    let reserved = |path: &str| {
        state
            .jobs
            .iter()
            .any(|job| job.kind == "render" && job.output_path.as_deref() == Some(path))
    };
    if !root.join(&first).exists() && !root.join(&first).with_extension("wav").exists() && !reserved(&first) {
        return first;
    }
    for version in 2..=999 {
        let rel = format!("{dir}/{stem}_v{version}.mp4");
        if !root.join(&rel).exists() && !root.join(&rel).with_extension("wav").exists() && !reserved(&rel) {
            return rel;
        }
    }
    format!("{dir}/{stem}_v1000.mp4")
}

pub(crate) fn verified_pre_submit_failure(state: &ProjectState, prior: &JobRecord) -> bool {
    if prior.kind != "render" || prior.status != JobStatus::Failed
        || !prior.error.as_deref().is_some_and(|error| error.starts_with("HOST PRE-SUBMIT VALIDATION:")
            && error.contains("No generation submitted.")) {
        return false;
    }
    let child: Vec<_> = state.jobs.iter().filter(|job| job.kind == "atlas_generate_video"
        && job.error == prior.error).collect();
    if child.len() != 1 { return false; }
    let video = child[0];
    video.status == JobStatus::Failed
        && video.output_path.is_none()
        && video.request_metadata.as_ref().and_then(|metadata| metadata.prediction_id.as_ref()).is_none()
        && prior.attempts.len() == 1 && video.attempts.len() == 1
        && prior.attempts[0].started_at.is_some() && prior.attempts[0].completed_at.is_some()
        && prior.attempts[0].started_at == video.attempts[0].started_at
        && prior.attempts[0].completed_at == video.attempts[0].completed_at
        && prior.attempts[0].error == video.attempts[0].error
}

/// Only a failed *pre-submit* video can be retried from saved audio. Preserve
/// the failed job and reserve a new output rather than rewriting its history.
pub fn ensure_failed_pre_submit_audio_retry(root: &Path, state: &mut ProjectState, scene_id: u32, cut_number: u32) -> Result<(), String> {
    let cut = state.scene(scene_id).and_then(|scene| scene.cuts.iter().find(|cut| cut.id.cut == cut_number))
        .ok_or("Saved-audio recovery shot not found")?;
    if cut.speech_mode != Some(SpeechMode::SourceVideoLipsync) || cut.stable_id.is_empty() {
        return Err("Saved-audio retry needs a stable talking shot".into());
    }
    let shot_id = cut.stable_id.clone();
    let id = cut.id;
    let source_asset = cut.visual_source.as_ref().and_then(|source| source.asset_id.clone());
    let pending = state.jobs.iter().filter(|job| job.kind == "render" && job.status == JobStatus::Pending
        && job.scope.as_ref().is_some_and(|scope| scope.kind == "shot" && scope.id == shot_id)).count();
    if state.jobs.iter().any(|job| job.kind.starts_with("atlas_generate_")
        && matches!(job.status, JobStatus::Pending | JobStatus::Running)) {
        return Err("An operation is pending or ambiguous; collect it before saved-audio retry.".into());
    }
    if pending == 1 {
        let job = state.jobs.iter().find(|job| job.kind == "render" && job.status == JobStatus::Pending
            && job.scope.as_ref().is_some_and(|scope| scope.kind == "shot" && scope.id == shot_id))
            .expect("counted pending render");
        let failed_for_shot = state.jobs.iter().any(|old| old.kind == "render" && old.status == JobStatus::Failed
            && old.scope.as_ref().is_some_and(|scope| scope.kind == "shot" && scope.id == shot_id)
            && old.error.as_deref().is_some_and(|error| error.starts_with("HOST PRE-SUBMIT VALIDATION:")));
        if failed_for_shot && !job.retry_of.as_ref().is_some_and(|id| state.jobs.iter().any(|old|
            old.id == *id && old.scope == job.scope && verified_pre_submit_failure(state, old))) {
            return Err("Pending render is not bound to the verified failed attempt; no saved-audio retry.".into());
        }
        return Ok(());
    }
    if pending != 0 {
        return Err("An operation is pending or ambiguous; collect it before saved-audio retry.".into());
    }
    let failed: Vec<_> = state.jobs.iter().filter(|job| job.kind == "render" && job.status == JobStatus::Failed
        && job.scope.as_ref().is_some_and(|scope| scope.kind == "shot" && scope.id == shot_id)
        && job.error.as_deref().is_some_and(|error| error.starts_with("HOST PRE-SUBMIT VALIDATION:"))).collect();
    if failed.len() != 1 { return Err("Saved-audio retry requires one verified pre-submit failed render for this shot.".into()); }
    let prior = failed[0];
    if !verified_pre_submit_failure(state, prior) {
        return Err("Video submission outcome is not proven pre-submit; do not retry.".into());
    }
    let previous_output = prior.output_path.as_deref().ok_or("Failed render has no output path")?;
    let old_video = crate::project_path::resolve(previous_output, Some(root.to_str().ok_or("invalid root")?), crate::project_path::PathIntent::Write)
        .map_err(|error| error.to_string())?.into_path_buf();
    if old_video.exists() { return Err("Failed render output exists; inspect or collect it before retry.".into()); }
    let wav = old_video.with_extension("wav");
    if !wav.is_file() { return Err("Saved performance WAV is missing; no speech will be regenerated.".into()); }
    validate_saved_performance(root, state, scene_id, cut_number, &wav)?;
    if !matches!(resolve_render_source(state, id)?, RenderSourceResolution::Bound(_)) {
        return Err("Select an approved visual source before saved-audio retry.".into());
    }
    let new_output = next_take_path(root, state, id);
    if new_output == previous_output || root.join(&new_output).exists()
        || root.join(&new_output).with_extension("wav").exists() {
        return Err("Could not reserve a fresh output without overwriting existing media.".into());
    }
    let mut job = JobRecord::open(state.next_job_id(), "render");
    job.output_path = Some(new_output);
    job.raw_provider_id = Some(id.code());
    job.scope = Some(crate::workflow::state::JobScope { kind: "shot".into(), id: shot_id });
    job.retry_of = Some(prior.id.clone());
    job.input_asset_ids = prior.input_asset_ids.iter().filter(|asset_id| {
        state.asset(asset_id).is_some_and(|asset| !asset.role.is_cut_image())
    }).cloned().collect();
    if let Some(source_asset) = source_asset { job.input_asset_ids.push(source_asset); }
    state.jobs.push(job);
    Ok(())
}

/// Reconcile only the uniquely attributable local guard failure. Never adopt
/// existing audio as new output or turn a failed attempt back into Pending.
pub fn reconcile_stranded_audio_retry(root: &Path, state: &mut ProjectState, scene_id: u32, cut_number: u32) -> Result<bool, String> {
    const GUARD: &str = "saved-audio retry is no longer proven pre-submit; no video submitted";
    let cut = state.scene(scene_id).and_then(|s| s.cuts.iter().find(|c| c.id.cut == cut_number)).ok_or("Saved-audio recovery shot not found")?;
    let pending: Vec<_> = state.jobs.iter().enumerate().filter(|(_, j)| j.kind == "render" && j.status == JobStatus::Pending
        && j.scope.as_ref().is_some_and(|s| s.kind == "shot" && s.id == cut.stable_id)).collect();
    if pending.len() != 1 { return Ok(false); }
    let (index, retry) = pending[0];
    let Some(raw) = retry.output_path.as_deref() else { return Ok(false); };
    let resolve_output = |raw: &str| crate::project_path::resolve(raw, Some(root.to_str().ok_or("invalid root")?), crate::project_path::PathIntent::Write)
        .map(|p| p.into_path_buf()).map_err(|e| e.to_string());
    let video = resolve_output(raw)?;
    let copied = video.with_extension("wav");
    if !copied.exists() { return Ok(false); }
    super::render_jobs::pending_binding(root, raw)?.ok_or("Missing pending saved-audio binding")?;
    if resolve_output(copied.to_str().ok_or("invalid WAV path")?)? != copied {
        return Err("Saved-audio retry WAV was redirected; no media changed.".into());
    }
    let refuse = "Occupied saved-audio retry is ambiguous; reconcile its failed child and complete WAV before retrying. No media changed.";
    let prior = retry.retry_of.as_ref().and_then(|id| state.jobs.iter().find(|j| j.id == *id && j.scope == retry.scope))
        .filter(|j| verified_pre_submit_failure(state, j)).ok_or(refuse)?;
    if state.jobs.iter().filter(|j| j.kind == "render" && matches!(j.status, JobStatus::Pending | JobStatus::Running)).count() != 1
        || state.jobs.iter().any(|j| j.kind.starts_with("atlas_generate_") && matches!(j.status, JobStatus::Pending | JobStatus::Running))
        || retry.attempts.len() != 1 || retry.attempts[0].status != JobStatus::Pending
        || retry.attempts[0].started_at.is_some() || retry.attempts[0].completed_at.is_some()
        || retry.started_at.is_some() || retry.completed_at.is_some() || retry.error.is_some()
        || retry.request_metadata.as_ref().and_then(|m| m.prediction_id.as_ref()).is_some() || video.exists() {
        return Err(refuse.into());
    }
    let created = retry.created_at.as_deref().ok_or(refuse)?;
    let children: Vec<_> = state.jobs.iter().filter(|j| j.kind == "atlas_generate_video"
        && j.created_at.as_deref().is_some_and(|t| t >= created) && j.error.as_deref() == Some(GUARD)).collect();
    if children.len() != 1 { return Err(refuse.into()); }
    let child = children[0];
    if child.status != JobStatus::Failed || child.output_path.is_some() || child.attempts.len() != 1
        || !child.request_metadata.as_ref().is_some_and(|m| m.prediction_id.is_none() && !m.request_hash.is_empty())
        || child.attempts[0].status != JobStatus::Failed || child.attempts[0].error.as_deref() != Some(GUARD)
        || !child.attempts[0].started_at.as_deref().is_some_and(|t| t >= created)
        || !child.attempts[0].completed_at.as_ref().zip(child.attempts[0].started_at.as_ref()).is_some_and(|(end,start)| end >= start)
        || state.jobs.iter().any(|j| j.kind == "render" && j.id != retry.id
            && j.created_at.as_ref().zip(child.attempts[0].completed_at.as_ref()).is_none_or(|(t,end)| t.as_str() >= created && t <= end)) {
        return Err(refuse.into());
    }
    let old_video = resolve_output(prior.output_path.as_deref().ok_or(refuse)?)?;
    let original = old_video.with_extension("wav");
    if resolve_output(original.to_str().ok_or("invalid WAV path")?)? != original {
        return Err("Protected saved WAV was redirected; no media changed.".into());
    }
    if old_video.exists() || original == copied || original.parent() != copied.parent()
        || !original.is_file() || !copied.is_file()
        || fs::metadata(&original).map_err(|e|e.to_string())?.len() > 512 * 1024 * 1024
        || fs::metadata(&original).map_err(|e|e.to_string())?.len() != fs::metadata(&copied).map_err(|e|e.to_string())?.len()
        || fs::read(&original).map_err(|e|e.to_string())? != fs::read(&copied).map_err(|e|e.to_string())? {
        return Err(refuse.into());
    }
    let child_id = child.id.clone();
    let started = child.attempts[0].started_at.clone();
    let completed = child.attempts[0].completed_at.clone();
    let retry = &mut state.jobs[index];
    retry.fail(None, GUARD);
    retry.started_at = started.clone(); retry.completed_at = completed.clone();
    retry.attempts[0].started_at = started; retry.attempts[0].completed_at = completed;
    retry.note_progress(&format!("Reconciled local pre-submit failure from {child_id}; identical saved WAV retained, no video submitted. This is history, not permission to retry."));
    Ok(true)
}

/// Legacy WAVs lack a transcript receipt. Duration/activity cannot certify that
/// a multi-cue file is not the old first-cue-only TTS result.
fn validate_saved_performance(root: &Path, state: &ProjectState, scene_id: u32, cut_number: u32, audio: &Path) -> Result<(), String> {
    let cut = state.scene(scene_id).and_then(|s| s.cuts.iter().find(|c| c.id.cut == cut_number)).ok_or("Saved-audio recovery shot not found")?;
    let handoff_multi = if let Some(note) = cut.production_note_path.as_deref() {
        let path = resolve(root, note)?;
        let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
        crate::scene_plan::extract::load_handoff_document(&text)?.speech.cues.len() > 1
    } else { false };
    if handoff_multi || cut.dialogue.len() > 1 || cut.sketch.as_ref().is_some_and(|s| s.dialogue.len() > 1) {
        return Err("Saved WAV has no proof it covers the complete multi-line dialogue (legacy TTS may contain only the first line). Audition and select a complete performance before preparing video; do not resume this WAV or regenerate speech implicitly.".into());
    }
    let inspection = crate::media_inspect::inspect_media(audio, &Map::new())?;
    if inspection.get("has_audio").and_then(Value::as_bool) != Some(true)
        || !inspection.get("video_stream").is_some_and(Value::is_null)
        || super::qc::check_audio_decode(audio).iter().any(|f| f.severity == super::qc::QCSeverity::Error) {
        return Err("Saved performance must be complete, decodable audio-only media; select a complete WAV. No speech will be regenerated.".into());
    }
    Ok(())
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
    prepare_render_inner(root, state, scene_id, cut_number, None)
}

pub fn prepare_render_with_host(
    app: &crate::ctx::HostCtx,
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    cut_number: u32,
) -> Result<(String, String), String> {
    prepare_render_inner(root, state, scene_id, cut_number, Some(app))
}

fn prepare_render_inner(
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    cut_number: u32,
    approval_app: Option<&crate::ctx::HostCtx>,
) -> Result<(String, String), String> {
    let id = CutId::new(scene_id, cut_number);
    if state.scene(scene_id).and_then(|s|s.cuts.iter().find(|c|c.id==id)).is_some_and(|c|c.speech_mode==Some(SpeechMode::SourceVideoLipsync)) {
        return Err("Generate or import dialogue audio in Scene Look, audition and select a complete WAV, then use Prepare video only with selected WAV. Video never generates speech.".into());
    }
    // Render new take is a distinct operation, never a fallback into an old
    // retry or an implicit replacement of paid audio. The card discloses TTS.
    if state.scene(scene_id).and_then(|s| s.cuts.iter().find(|c| c.id == id)).is_some_and(|c| c.selected_performance_asset_id.is_some()) {
        return Err("An explicit WAV is selected. Use Prepare video only with selected WAV; no speech will be regenerated.".into());
    }
    if state.jobs.iter().any(|job| job.kind.starts_with("atlas_generate_")
        && matches!(job.status, JobStatus::Pending | JobStatus::Running)) {
        return Err("A provider operation is unsettled; reconcile it before preparing a new take.".into());
    }
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
    let output_path = next_take_path(root, state, id);
    if root.join(&output_path).exists() || root.join(&output_path).with_extension("wav").exists()
        || state.jobs.iter().any(|job| job.kind == "render" && job.output_path.as_deref() == Some(&output_path)) {
        return Err("No unoccupied new-take output is available; existing media was preserved.".into());
    }
    let mut inputs = Map::new();
    inputs.insert("handoff_path".into(), Value::String(handoff_path.clone()));
    inputs.insert("output_path".into(), Value::String(output_path.clone()));
    if is_text_sync {
        inputs.insert("output_audio".into(), Value::String(
            std::path::Path::new(&output_path).with_extension("wav").to_string_lossy().into_owned()));
    }
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
    let approval = match approval_app {
        Some(app) => crate::director::format_approval_request_for_app(
            app,
            &action,
            &validated,
            Some(&root_s),
        )?,
        None => format_approval_request(&action, &validated),
    };
    card.push_str(&approval);

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
    {
        let job_id = state.next_job_id();
        let mut job = JobRecord {
            provider: None,
            input_asset_ids,
            output_path: Some(output_path),
            raw_provider_id: Some(id.code()),
            scope,
            ..JobRecord::open(job_id, "render")
        };
        if let Some(app) = approval_app {
            if let Ok(Some(estimate)) =
                crate::director::estimate_for_action(app, &action, Some(&root_s))
            {
                job.provider = Some(estimate.kind);
                job.model = estimate.model;
                job.currency = Some(estimate.currency);
                job.estimated_cost_minor = Some(estimate.minor);
            }
        }
        state.jobs.push(job);
    }
    let note = if approval_app.is_some() {
        format!("Render for Cut {cut_number} is ready. Use the one-time approval button; chat text cannot approve it.")
    } else {
        format!(
            "Render plan for Cut {cut_number} prepared; no provider operation is authorized here."
        )
    };
    Ok((card, note))
}


/// Explicit recovery of a pending shot. Never allocate a take or infer old consent.
pub fn prepare_saved_audio_with_host(app: &crate::ctx::HostCtx, root: &Path, state: &ProjectState, scene_id: u32, cut_number: u32) -> Result<(String,String),String> {
    let epoch=crate::pending::generation();
    let cut=state.scene(scene_id).and_then(|s|s.cuts.iter().find(|c|c.id.cut==cut_number)).ok_or("Saved-audio recovery shot not found")?;
    if cut.speech_mode!=Some(SpeechMode::SourceVideoLipsync) { return Err("Saved-audio recovery is only for a talking shot".into()); }
    if state.jobs.iter().any(|j|j.kind.starts_with("atlas_generate_") && matches!(j.status,JobStatus::Running|JobStatus::Pending)) {
        return Err("A provider operation is unsettled. Collect/reconcile the existing prediction before preparing recovery; do not resubmit.".into());
    }
    let jobs:Vec<_>=state.jobs.iter().filter(|j|j.kind=="render" && j.status==JobStatus::Pending && j.scope.as_ref().is_some_and(|s|s.kind=="shot" && s.id==cut.stable_id)).collect();
    if jobs.len()!=1 { return Err("Saved-audio recovery needs one unambiguous existing pending render for this shot".into()); }
    let output=jobs[0].output_path.as_deref().ok_or("Pending render has no output")?;
    let _binding=crate::workflow::render_jobs::pending_binding(root,output)?.ok_or("No prepared render binding")?;
    let video=crate::project_path::resolve(output,Some(root.to_str().ok_or("invalid root")?),crate::project_path::PathIntent::Write).map_err(|e|e.to_string())?.into_path_buf();
    let expected_audio=video.with_extension("wav");
    let saved_video = if let Some(prior_id) = jobs[0].retry_of.as_deref() {
        let prior=state.jobs.iter().find(|job| job.id==prior_id && job.kind=="render"
            && job.status==JobStatus::Failed
            && job.error.as_deref().is_some_and(|error|error.starts_with("HOST PRE-SUBMIT VALIDATION:")))
            .ok_or("Saved-audio retry lost its verified pre-submit predecessor")?;
        let previous=prior.output_path.as_deref().ok_or("Failed render has no output path")?;
        crate::project_path::resolve(previous,Some(root.to_str().ok_or("invalid root")?),crate::project_path::PathIntent::Write)
            .map_err(|e|e.to_string())?.into_path_buf()
    } else { video.clone() };
    let audio=resolve(root,saved_video.with_extension("wav").to_str().ok_or("invalid audio path")?)?;
    if audio.parent()!=video.parent() || !audio.is_file() || video.is_file()
        || (audio!=expected_audio && expected_audio.exists()) {
        return Err("Saved performance WAV is missing, redirected, or the retry output is occupied; no speech will be regenerated".into());
    }
    validate_saved_performance(root, state, scene_id, cut_number, &audio)?;
    let source=match resolve_render_source(state,cut.id)? { RenderSourceResolution::Bound(p)=>p,_=>return Err("Saved-audio recovery requires the existing approved image binding".into()) };
    let mut inputs=Map::new();
    inputs.insert("handoff_path".into(),Value::String(cut.production_note_path.clone().ok_or("Shot has no handoff")?));
    inputs.insert("source_frame".into(),Value::String(source));
    inputs.insert("performance_wav".into(),Value::String(audio.to_string_lossy().into_owned()));
    inputs.insert("output_audio".into(),Value::String(expected_audio.to_string_lossy().into_owned()));
    inputs.insert("output_path".into(),Value::String(output.into()));
    let action=DirectorAction{action:ActionKind::RunSpecialist,specialist:Some("text_sync".into()),inputs,message:None,op:None,path:None};
    let context=crate::director::current_workflow_context(app,&root.to_path_buf())?;
    let raw=crate::director::prepare_workflow_approval(app,&context,&action)?;
    let card=crate::pending::publish_guarded(&raw,epoch,None,||!app.director_cancelled() && crate::session::root().as_deref()==Some(root))?;
    let retry_label = if jobs[0].retry_of.is_some() { "fresh retry of a proven pre-submit failure" } else { "existing pending shot" };
    Ok((format!("Resume {retry_label} using saved audio. Historical approval context is not inferred: review the current image, prompt, model and exact WAV below. No speech synthesis is included.\n\n{card}"),"Saved-audio recovery prepared; use its fresh native approval, not the original voice-clone card.".into()))
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
    fn failed_pre_submit_render_reserves_new_job_and_reuses_saved_audio() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("Scenes/Scene1/Cut1")).unwrap();
        let wav = root.join("Scenes/Scene1/Cut1/S01C01.wav");
        let output = std::process::Command::new("ffmpeg")
            .args(["-v","error","-f","lavfi","-i","anullsrc=r=16000:cl=mono","-t","1","-c:a","pcm_s16le","-y"])
            .arg(&wav).output().unwrap();
        assert!(output.status.success());
        fs::create_dir_all(root.join("Preproduction/Scene1/references")).unwrap();
        fs::write(root.join("Preproduction/Scene1/references/interior.png"), b"fixture image").unwrap();
        let mut state = ProjectState::new();
        let scene = state.scene_mut(1, "Scene");
        let cut = scene.cut_mut(1);
        cut.stable_id = "shot_1".into();
        cut.speech_mode = Some(SpeechMode::SourceVideoLipsync);
        let mut source = VisualSourceSpec::new(VisualSourceStrategy::SharedReference);
        source.asset_id = Some("asset_18".into());
        cut.visual_source = Some(source);
        state.assets.push(AssetRecord { id:"asset_18".into(), path:"Preproduction/Scene1/references/interior.png".into(),
            role:AssetRole::InteriorReference, status:AssetStatus::Approved, scene_id:Some(1),cut_id:None,
            character_id:None,source_job_id:None,parent_asset_id:None,notes:None });
        let error = "HOST PRE-SUBMIT VALIDATION: incompatible image field. No generation submitted.";
        let mut old = JobRecord::open("job_3", "render");
        old.scope = Some(crate::workflow::state::JobScope { kind:"shot".into(), id:"shot_1".into() });
        old.raw_provider_id = Some("S01C01".into());
        old.output_path = Some("Scenes/Scene1/Cut1/S01C01.mp4".into());
        old.input_asset_ids = vec!["asset_20".into(),"voice_1".into()];
        old.mark_running("fixture pre-submit");
        old.fail(None,error);
        let mut child = JobRecord::open("spend_1", "atlas_generate_video");
        child.mark_running("fixture pre-submit");
        child.fail(None,error);
        for job in [&mut old, &mut child] {
            job.attempts[0].started_at = Some("2026-11-01T23:07:34Z".into());
            job.attempts[0].completed_at = Some("2026-11-01T23:07:36Z".into());
        }
        state.jobs.extend([old.clone(),child.clone()]);
        let before = fs::read(&wav).unwrap();
        // This fixture has no production note/voice. It cannot prepare an
        // ordinary take, but must not rewrite the old failed job.
        let mut ordinary = state.clone();
        assert!(prepare_render(root,&mut ordinary,1,1).is_err());
        assert_eq!(ordinary.jobs,state.jobs);
        ensure_failed_pre_submit_audio_retry(root,&mut state,1,1).unwrap();
        assert_eq!(state.jobs[0],old);
        assert_eq!(state.jobs[1],child);
        let retry = &state.jobs[2];
        assert_eq!(retry.retry_of.as_deref(),Some("job_3"));
        assert_eq!(retry.output_path.as_deref(),Some("Scenes/Scene1/Cut1/S01C01_v2.mp4"));
        assert_eq!(retry.status,JobStatus::Pending);
        // This fixture has no production note/voice. It cannot prepare an
        // ordinary take, but must not rewrite the old failed job.
        let mut ordinary = state.clone();
        assert!(prepare_render(root,&mut ordinary,1,1).is_err());
        assert_eq!(ordinary.jobs,state.jobs);
        assert_eq!(fs::read(&wav).unwrap(),before);
        let count = state.jobs.len();
        ensure_failed_pre_submit_audio_retry(root,&mut state,1,1).unwrap();
        assert_eq!(state.jobs.len(),count);
        let mut uncertain = state.clone(); uncertain.jobs.pop();
        uncertain.jobs[1].request_metadata = Some(crate::workflow::state::SpendRequestMetadata {
            request_hash:"hash".into(),disclosure:None,duration_seconds:None,width:None,height:None,prediction_id:Some("remote_1".into()) });
        assert!(ensure_failed_pre_submit_audio_retry(root,&mut uncertain,1,1).is_err());
        assert_eq!(uncertain.jobs.len(),2);
    }

    #[test]
    fn production_prompt_uses_only_cut_selected_scene_look_roles() {
        let mut state = ProjectState::new();
        let scene = state.scene_mut(1, "One shot");
        let mut board = crate::workflow::state::LookBoard::default();
        for (id, role) in [("interior", AssetRole::InteriorReference), ("wardrobe", AssetRole::WardrobeReference)] {
            let mut slot = crate::workflow::state::LookSlot::new(id, role, id);
            slot.approved_asset_id = Some(id.into());
            board.slots.push(slot);
        }
        scene.look_board = Some(board);
        let cut = scene.cut_mut(1);
        cut.sketch = Some(CutSketch { reference_roles: vec![AssetRole::InteriorReference],
            visual_prompt: Some("One room".into()), ..CutSketch::default() });
        for (id, role) in [("interior", AssetRole::InteriorReference), ("wardrobe", AssetRole::WardrobeReference)] {
            state.assets.push(AssetRecord { id: id.into(), path: format!("Preproduction/Scene1/references/{id}.png"),
                role, status: AssetStatus::Approved, scene_id: Some(1), cut_id: None,
                character_id: None, source_job_id: None, parent_asset_id: None, notes: None });
        }
        let prompt = draft_user_prompt(&state, 1, &[1], "").unwrap();
        assert!(prompt.contains("interior.png"));
        assert!(!prompt.contains("wardrobe.png"));
    }

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
        assert!(card.contains("### Files read or uploaded"));
        assert!(card.contains("### Artifacts modified"));
        assert!(!card.contains("```json"));
        assert!(!card.contains("\"resolution\": \"1080p\""));
        let writes = card.split("### Artifacts modified").nth(1).unwrap_or("");
        assert!(writes.contains("S01C01.mp4"));
        assert_eq!(state.jobs[0].kind, "render");
        assert_eq!(state.jobs[0].status, JobStatus::Pending);
        // Minimum job metadata: prepared-only jobs record creation, keep an
        // honest unknown start, and carry the shot scope for the Jobs links.
        let shot_stable = state.scene(1).unwrap().cuts[0].stable_id.clone();
        assert!(state.jobs[0].created_at.is_some());
        assert!(state.jobs[0].started_at.is_none());
        let scope = state.jobs[0]
            .scope
            .as_ref()
            .expect("render job carries a shot scope");
        assert_eq!(scope.kind, "shot");
        assert_eq!(scope.id, shot_stable);
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
            reference_asset_ids: Vec::new(),
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
        assert!(approval.contains("### Files read or uploaded"));
        assert!(approval.contains("### Artifacts modified"));
        assert!(!approval.contains("```json"));
        assert!(!approval.contains("\"source_frame\""));
        assert!(!approval.contains("\"voice_clone\""));
        assert!(approval.contains("Preproduction/Scene1/start_frames/cut_1_v2.png"));
        assert!(approval.contains("Characters/Sam/voice/Sam_voice_sample_01.wav"));
        assert!(state.jobs[0]
            .input_asset_ids
            .contains(&"asset_voice".to_string()));
        // A new take never reuses the first pending job or either saved WAV.
        fs::create_dir_all(root.join("Scenes/Scene1/Cut1")).unwrap();
        fs::write(root.join("Scenes/Scene1/Cut1/S01C01.wav"), b"old paid audio").unwrap();
        let first_job = state.jobs[0].clone();
        let (second, _) = prepare_render(root, &mut state, 1, 1).unwrap();
        assert!(second.contains("S01C01_v2.mp4"));
        assert!(second.contains("S01C01_v2.wav"));
        assert_eq!(state.jobs[0], first_job);
        assert_eq!(state.jobs[1].retry_of, None);
        fs::write(root.join("Scenes/Scene1/Cut1/S01C01_v2.wav"), b"stranded copy").unwrap();
        let retry_job = state.jobs[1].clone();
        let (third, _) = prepare_render(root, &mut state, 1, 1).unwrap();
        assert!(third.contains("S01C01_v3.mp4"));
        assert!(third.contains("S01C01_v3.wav"));
        assert_eq!(state.jobs[1], retry_job);
        assert_eq!(state.jobs[2].output_path.as_deref(), Some("Scenes/Scene1/Cut1/S01C01_v3.mp4"));
        assert_eq!(fs::read(root.join("Scenes/Scene1/Cut1/S01C01.wav")).unwrap(), b"old paid audio");
        assert_eq!(fs::read(root.join("Scenes/Scene1/Cut1/S01C01_v2.wav")).unwrap(), b"stranded copy");
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
