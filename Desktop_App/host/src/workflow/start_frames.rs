//! Cut-specific still generation, edits, versions, and approval.
//!
//! Character sheets and approved scene looks are generation inputs. The host
//! owns output paths and the approved cut image that later becomes video input.
use crate::director_actions::{
    format_approval_request, resolve_under_project, validate_action, ActionKind, DirectorAction,
};
use crate::scene_plan::handoff::CutId;
use crate::workflow::import;
use crate::workflow::look;
use crate::workflow::state::{
    AssetRecord, AssetRole, AssetStatus, CutRecord, JobRecord, ProjectState, SceneRecord,
    VisualSourceSpec, VisualSourceStrategy,
};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_SCENE_REFS: usize = 4;
const START_FRAME_INSTRUCTION: &str = "Create one production-ready still image: the frozen opening frame before any dialogue or movement begins. Preserve the named characters' identities from their model sheets and match approved scene references. Any visible mouth must be naturally relaxed and closed, ready for later animation. Do not render speech, lip movement, captions, labels, contact sheets, or model-sheet layouts.";

fn approval_details(
    app: Option<&crate::ctx::HostCtx>,
    action: &DirectorAction,
    validated: &crate::director_actions::ValidatedAction,
    root: &str,
) -> Result<String, String> {
    match app {
        Some(app) => {
            crate::director::format_approval_request_for_app(app, action, validated, Some(root))
        }
        None => Ok(format_approval_request(action, validated)),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationRef {
    pub asset_id: String,
    pub path: String,
    pub label: String,
    pub wardrobe_plate: bool,
}

pub fn prompt_system_prompt() -> &'static str {
    "You draft a single cut start-frame image prompt. Return ONLY JSON:\n\
{\"prompt\":\"one still-image prompt\",\"negative_prompt\":\"optional\"}\n\
Use the cut sketch as the creative source. Describe one frozen opening composition before action or speech begins, not a video sequence. Preserve shot role, subject, on-camera people, location, approved scene look, period, props, and continuity.\n\
STRICT STILL-SOURCE RULES:\n\
- Never include quoted dialogue or describe anyone saying, speaking, delivering, performing, or forming words.\n\
- Never request lip-sync, lip movement, a phoneme, a speaking mouth, or a mouth visibly forming words.\n\
- For a performance cut, keep the face clearly visible but the mouth naturally relaxed and closed in the instant before speech. The later video stage animates it.\n\
- Never describe camera movement, shot progression, timing, audio, sound design, or a sequence of events.\n\
- The negative prompt must not prohibit a closed, relaxed, or neutral mouth.\n\
Do not emit paths, asset ids, models, providers, production notes, or host fields. The host owns references and output paths."
}

pub fn prompt_user_prompt(scene: &SceneRecord, cut: &CutRecord, brief: &str) -> String {
    let mut out = format!(
        "Scene {}: {}\nCut {}\nTitle: {}\n",
        scene.id,
        scene.title,
        cut.id.cut,
        cut.title.as_deref().unwrap_or("(untitled)")
    );
    if let Some(location) = &scene.location {
        out.push_str(&format!("Location: {location}\n"));
    }
    out.push_str(&format!(
        "Required frozen-mouth treatment: {}\n",
        still_mouth_instruction(cut)
    ));
    if let Some(sketch) = &cut.sketch {
        out.push_str(&format!(
            "Shot role: {}\nSubject: {}\nSpeaker: {}\nMouth policy: {}\nOn camera: {}\n",
            sketch
                .shot_role
                .map(|role| role.label())
                .unwrap_or("unspecified"),
            sketch.subject.as_deref().unwrap_or("(unspecified)"),
            sketch.speaker.as_deref().unwrap_or("(none)"),
            sketch
                .mouth_policy
                .map(|policy| policy.label())
                .unwrap_or("unspecified"),
            if sketch.on_camera.is_empty() {
                "(unspecified)".into()
            } else {
                sketch.on_camera.join(", ")
            }
        ));
        if let Some(prompt) = &sketch.visual_prompt {
            out.push_str(&format!("Existing visual prompt:\n{prompt}\n"));
        }
        if !sketch.reference_roles.is_empty() {
            out.push_str(&format!(
                "Requested scene reference roles: {}\n",
                sketch
                    .reference_roles
                    .iter()
                    .map(|role| role.label())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    } else {
        out.push_str("No typed sketch exists; convert the imported cut fields into a still.\n");
        if let Some(subject) = &cut.subject {
            out.push_str(&format!("Subject: {subject}\n"));
        }
        if let Some(prompt) = &cut.prompt {
            out.push_str(&format!(
                "Imported production prompt (extract one opening still):\n{prompt}\n"
            ));
        }
    }
    if let Some(prompt) = &cut.start_frame_prompt {
        out.push_str(&format!("Existing cut start-frame prompt:\n{prompt}\n"));
    }
    if !cut.dialogue.is_empty() {
        out.push_str("Dialogue context:\n");
        for line in &cut.dialogue {
            out.push_str(&format!("- {}: {}\n", line.speaker, line.text));
        }
    }
    out.push_str("\nApproved scene look:\n");
    out.push_str(&look::look_summary(scene));
    if !brief.trim().is_empty() {
        out.push_str("\n\nUser direction:\n");
        out.push_str(brief.trim());
    }
    out
}

pub fn apply_prompt(cut: &mut CutRecord, value: &Value) -> Result<(), String> {
    let (prompt, negative) = prompt_fields(value)?;
    ensure_still_prompt(cut, &prompt, negative.as_deref())?;
    cut.start_frame_prompt = Some(prompt);
    cut.start_frame_negative_prompt = negative;
    Ok(())
}

pub fn save_prompt(cut: &mut CutRecord, prompt: &str, negative: &str) -> Result<(), String> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("start-frame prompt cannot be empty".into());
    }
    let negative = negative.trim();
    cut.start_frame_prompt = Some(prompt.to_string());
    cut.start_frame_negative_prompt = (!negative.is_empty()).then(|| negative.to_string());
    Ok(())
}

pub fn prompt_issues(cut: &CutRecord, value: &Value) -> Result<Vec<String>, String> {
    let (prompt, negative) = prompt_fields(value)?;
    Ok(still_prompt_issues(cut, &prompt, negative.as_deref()))
}

pub fn prompt_correction(issues: &[String]) -> String {
    format!(
        "Workbench host rejected that response as a video/performance prompt rather than a frozen source image:\n- {}\n\
Re-emit ONLY the JSON object. Describe the single instant before speech or movement. Keep visible faces usable for later animation, with naturally relaxed closed mouths. Remove dialogue quotations, word-forming mouths, lip-sync language, audio, timing, camera movement, and event progression. Do not put `closed mouth`, `relaxed mouth`, or `neutral mouth` in the negative prompt.",
        issues.join("\n- ")
    )
}

fn prompt_fields(value: &Value) -> Result<(String, Option<String>), String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "start-frame prompt JSON must be an object".to_string())?;
    let prompt = obj
        .get("prompt")
        .or_else(|| obj.get("visual_prompt"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "start-frame prompt JSON needs `prompt`".to_string())?
        .to_string();
    let negative = obj
        .get("negative_prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Ok((prompt, negative))
}

fn ensure_still_prompt(
    cut: &CutRecord,
    prompt: &str,
    negative: Option<&str>,
) -> Result<(), String> {
    let issues = still_prompt_issues(cut, prompt, negative);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "start-frame prompt is video-like:\n- {}\nRemove the performance-over-time language before generating an image.",
            issues.join("\n- ")
        ))
    }
}

fn still_prompt_issues(cut: &CutRecord, prompt: &str, negative: Option<&str>) -> Vec<String> {
    let lower = prompt.to_ascii_lowercase();
    let checks = [
        (
            ["lip-sync", "lip sync", "lipsync"].as_slice(),
            "lip-sync language belongs to video generation",
        ),
        (
            [
                "mouth visibly",
                "mouth forming",
                "forming the words",
                "forming words",
                "mid-speech",
                "mid speech",
                "speaking mouth",
                "visibly speaking",
            ]
            .as_slice(),
            "the prompt depicts speech or a word-forming mouth",
        ),
        (
            [
                "delivering the line",
                "delivers the line",
                "performs the line",
                "performing the line",
                "says the line",
                "speaks the line",
            ]
            .as_slice(),
            "the prompt describes dialogue performance instead of a frozen pre-speech frame",
        ),
        (
            [
                "camera movement",
                "camera moves",
                "camera makes",
                "rack focus",
                "push-in",
                "push in",
                "dolly",
                "tracking shot",
            ]
            .as_slice(),
            "the prompt describes camera movement",
        ),
        (
            ["sound design", "audio:", "voice-over", "voice over"].as_slice(),
            "the prompt contains audio direction",
        ),
    ];
    let mut issues = Vec::new();
    for (patterns, message) in checks {
        if patterns.iter().any(|pattern| lower.contains(pattern))
            && !issues.iter().any(|issue| issue == message)
        {
            issues.push(message.to_string());
        }
    }
    let normalized_prompt = normalized_words(prompt);
    for line in &cut.dialogue {
        let normalized_line = normalized_words(&line.text);
        if normalized_line.len() >= 6 && normalized_prompt.contains(&normalized_line) {
            issues.push("the prompt repeats dialogue text that should only be context".into());
            break;
        }
    }
    if let Some(negative) = negative {
        let negative = negative.to_ascii_lowercase();
        if ["closed mouth", "relaxed mouth", "neutral mouth"]
            .iter()
            .any(|pattern| negative.contains(pattern))
        {
            issues.push(
                "the negative prompt prohibits the neutral closed mouth required for a source still"
                    .into(),
            );
        }
    }
    issues
}

fn normalized_words(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn still_mouth_instruction(cut: &CutRecord) -> &'static str {
    match cut
        .sketch
        .as_ref()
        .and_then(|sketch| sketch.mouth_policy)
    {
        Some(crate::workflow::state::MouthPolicy::NoPeople) => {
            "no person or speaking mouth appears in the frame"
        }
        Some(crate::workflow::state::MouthPolicy::OffCamera) => {
            "the speaker stays off camera; any visible person has a relaxed closed mouth"
        }
        Some(crate::workflow::state::MouthPolicy::ClosedOrNotVisible) => {
            "mouths are naturally closed or not visible"
        }
        Some(crate::workflow::state::MouthPolicy::SpeakingVisible) | None => {
            "the future speaker's face is clearly visible, with a naturally relaxed closed mouth in the instant before speech"
        }
    }
}

pub fn frame_rel(scene_id: u32, cut: u32, version: u32, ext: &str) -> String {
    format!(
        "Preproduction/Scene{scene_id}/start_frames/cut_{cut}_v{version}.{}",
        ext.trim_start_matches('.')
    )
}

fn next_frame_rel(root: &Path, state: &ProjectState, scene_id: u32, cut: u32, ext: &str) -> String {
    for version in 1..=999 {
        let rel = frame_rel(scene_id, cut, version, ext);
        if !stem_exists(root, &rel)
            && !state
                .assets
                .iter()
                .any(|asset| same_stem(&asset.path, &rel))
            && !state.jobs.iter().any(|job| {
                job.output_path
                    .as_deref()
                    .is_some_and(|path| same_stem(path, &rel))
            })
        {
            return rel;
        }
    }
    frame_rel(scene_id, cut, 1000, ext)
}

fn cut_identity_haystack(cut: &CutRecord) -> String {
    if cut.sketch.as_ref().and_then(|sketch| sketch.mouth_policy)
        == Some(crate::workflow::state::MouthPolicy::NoPeople)
    {
        return String::new();
    }
    if let Some(on_camera) = cut
        .sketch
        .as_ref()
        .map(|sketch| &sketch.on_camera)
        .filter(|on_camera| !on_camera.is_empty())
    {
        return on_camera.join("\n");
    }
    if let Some(prompt) = cut
        .start_frame_prompt
        .as_deref()
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
    {
        return prompt.to_string();
    }
    let mut parts = Vec::new();
    if let Some(subject) = &cut.subject {
        parts.push(subject.clone());
    }
    if let Some(sketch) = &cut.sketch {
        for value in [
            sketch.subject.as_ref(),
            sketch.speaker.as_ref(),
            sketch.visual_prompt.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            parts.push(value.clone());
        }
    } else {
        parts.extend(cut.dialogue.iter().map(|line| line.speaker.clone()));
    }
    parts.join("\n")
}

pub fn generation_refs(
    root: &Path,
    state: &ProjectState,
    scene_id: u32,
    cut_number: u32,
) -> Result<Vec<GenerationRef>, String> {
    let scene = state
        .scene(scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    let cut = scene
        .cuts
        .iter()
        .find(|cut| cut.id.cut == cut_number)
        .ok_or_else(|| format!("cut {scene_id}.{cut_number} not found"))?;
    let hay = cut_identity_haystack(cut);
    let identity = look::keep_existing_identity_refs(
        root,
        look::collect_look_identity_refs(state, scene_id, AssetRole::MoodReference, &hay, ""),
    );
    let mut refs: Vec<GenerationRef> = identity
        .into_iter()
        .map(|reference| {
            let wardrobe_plate = reference.role == AssetRole::WardrobePlate;
            GenerationRef {
                asset_id: reference.asset_id,
                path: reference.path,
                label: if wardrobe_plate {
                    format!("{} wardrobe plate", reference.character_name)
                } else {
                    format!("{} {}", reference.character_name, reference.role.label())
                },
                wardrobe_plate,
            }
        })
        .collect();
    let has_character_wardrobe_plate = refs.iter().any(|reference| reference.wardrobe_plate);

    let requested: Vec<AssetRole> = cut
        .sketch
        .as_ref()
        .map(|sketch| sketch.reference_roles.clone())
        .unwrap_or_default();
    let fallback_to_approved = requested.is_empty();
    if let Some(board) = &scene.look_board {
        let mut scene_refs = Vec::new();
        for slot in &board.slots {
            if slot.role == AssetRole::WardrobeReference && has_character_wardrobe_plate {
                continue;
            }
            let include = slot.role == AssetRole::WardrobeReference
                || requested.contains(&slot.role)
                || fallback_to_approved;
            if !include {
                continue;
            }
            let Some(asset_id) = &slot.approved_asset_id else {
                continue;
            };
            let Some(asset) = state.asset(asset_id) else {
                continue;
            };
            if asset.status != AssetStatus::Approved || !file_exists(root, &asset.path) {
                continue;
            }
            scene_refs.push(GenerationRef {
                asset_id: asset.id.clone(),
                path: asset.path.clone(),
                label: format!("{} {}", slot.title, slot.role.label()),
                wardrobe_plate: false,
            });
        }
        scene_refs.sort_by_key(|reference| {
            if state
                .asset(&reference.asset_id)
                .is_some_and(|asset| asset.role == AssetRole::WardrobeReference)
            {
                0
            } else {
                1
            }
        });
        for reference in scene_refs.into_iter().take(MAX_SCENE_REFS) {
            if !refs.iter().any(|item| item.path == reference.path) {
                refs.push(reference);
            }
        }
    }
    Ok(refs)
}

pub fn prepare_generate(
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    cut_number: u32,
) -> Result<(String, String), String> {
    prepare_generate_inner(root, state, scene_id, cut_number, None)
}

pub fn prepare_generate_with_host(
    app: &crate::ctx::HostCtx,
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    cut_number: u32,
) -> Result<(String, String), String> {
    prepare_generate_inner(root, state, scene_id, cut_number, Some(app))
}

fn prepare_generate_inner(
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    cut_number: u32,
    approval_app: Option<&crate::ctx::HostCtx>,
) -> Result<(String, String), String> {
    let cut = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .and_then(|scene| scene.cuts.iter_mut().find(|cut| cut.id.cut == cut_number))
        .ok_or_else(|| format!("cut {scene_id}.{cut_number} not found"))?;
    if cut
        .visual_source
        .as_ref()
        .is_none_or(|source| source.strategy != VisualSourceStrategy::PreproductionStartFrame)
    {
        cut.visual_source = Some(VisualSourceSpec::new(
            VisualSourceStrategy::PreproductionStartFrame,
        ));
    }
    let (title, prompt, negative) = {
        let scene = state
            .scene(scene_id)
            .ok_or_else(|| format!("scene {scene_id} not found"))?;
        let cut = scene
            .cuts
            .iter()
            .find(|cut| cut.id.cut == cut_number)
            .ok_or_else(|| format!("cut {scene_id}.{cut_number} not found"))?;
        let prompt = cut
            .start_frame_prompt
            .as_deref()
            .or_else(|| {
                cut.sketch
                    .as_ref()
                    .and_then(|sketch| sketch.visual_prompt.as_deref())
            })
            .or(cut.prompt.as_deref())
            .map(str::trim)
            .filter(|prompt| !prompt.is_empty())
            .ok_or_else(|| {
                "draft or enter a cut visual prompt before generating a start frame".to_string()
            })?
            .to_string();
        let negative = cut
            .start_frame_negative_prompt
            .clone()
            .or_else(|| {
                cut.sketch
                    .as_ref()
                    .and_then(|sketch| sketch.negative_prompt.clone())
            })
            .or_else(|| cut.negative_prompt.clone());
        ensure_still_prompt(cut, &prompt, negative.as_deref())?;
        (
            cut.title
                .clone()
                .unwrap_or_else(|| format!("Cut {cut_number}")),
            prompt,
            negative,
        )
    };
    let refs = generation_refs(root, state, scene_id, cut_number)?;
    let rel = next_frame_rel(root, state, scene_id, cut_number, "png");
    create_parent(root, &rel)?;
    let mut generation_prompt = format!("{prompt}\n\n{START_FRAME_INSTRUCTION}");
    if refs.iter().any(|reference| reference.wardrobe_plate) {
        generation_prompt.push_str(
            "\n\nWhere a wardrobe plate is attached, that plate locks both identity and costume. Copy its garments, colors, layers, and condition. Do not revert to the original model-sheet clothes.",
        );
    }
    if let Some(negative) = negative.filter(|value| !value.trim().is_empty()) {
        generation_prompt.push_str("\n\nAvoid: ");
        generation_prompt.push_str(negative.trim());
    }
    let mut inputs = Map::new();
    inputs.insert("prompt".into(), Value::String(generation_prompt));
    inputs.insert("output_path".into(), Value::String(rel.clone()));
    if let Some(aspect) = &state.settings.aspect_ratio {
        inputs.insert("aspect_ratio".into(), Value::String(aspect.clone()));
    }
    if !refs.is_empty() {
        inputs.insert(
            "reference_images".into(),
            json!(refs
                .iter()
                .map(|reference| reference.path.clone())
                .collect::<Vec<_>>()),
        );
    }
    let action = DirectorAction {
        action: ActionKind::RunSpecialist,
        specialist: Some("still_image".into()),
        inputs,
        message: None,
        op: None,
        path: None,
    };
    let root_s = root.to_string_lossy().to_string();
    let validated = validate_action(action.clone(), Some(&root_s)).map_err(|e| e.to_string())?;
    let mut card = format!(
        "Cut start frame · Scene {scene_id} Cut {cut_number} · {title}\nHost will save to `{rel}`.\nThis cut image—not a model-sheet closeup—becomes the video source after approval."
    );
    append_refs(&mut card, &refs);
    card.push_str("\n\n");
    card.push_str(&approval_details(
        approval_app,
        &action,
        &validated,
        &root_s,
    )?);
    push_job(
        state,
        "cut_start_frame",
        scene_id,
        cut_number,
        rel,
        refs.iter()
            .map(|reference| reference.asset_id.clone())
            .collect(),
        approval_app,
        &action,
        &root_s,
    );
    let note = if approval_app.is_some() {
        format!("Start-frame generate ready for Scene {scene_id} Cut {cut_number}. Use the one-time approval button; chat text cannot approve it.")
    } else {
        format!("Start-frame plan prepared for Scene {scene_id} Cut {cut_number}; no provider operation is authorized here.")
    };
    Ok((card, note))
}

pub fn prepare_edit(
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    cut_number: u32,
    asset_id: &str,
    brief: &str,
) -> Result<(String, String), String> {
    prepare_edit_inner(root, state, scene_id, cut_number, asset_id, brief, None)
}

pub fn prepare_edit_with_host(
    app: &crate::ctx::HostCtx,
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    cut_number: u32,
    asset_id: &str,
    brief: &str,
) -> Result<(String, String), String> {
    prepare_edit_inner(
        root,
        state,
        scene_id,
        cut_number,
        asset_id,
        brief,
        Some(app),
    )
}

fn prepare_edit_inner(
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    cut_number: u32,
    asset_id: &str,
    brief: &str,
    approval_app: Option<&crate::ctx::HostCtx>,
) -> Result<(String, String), String> {
    let brief = brief.trim();
    if brief.is_empty() {
        return Err("enter an edit brief first".into());
    }
    let cut_id = CutId::new(scene_id, cut_number);
    let source = state
        .asset(asset_id)
        .filter(|asset| asset.cut_id == Some(cut_id) && asset.role.is_cut_image())
        .ok_or_else(|| format!("cut image `{asset_id}` not found"))?
        .clone();
    if !file_exists(root, &source.path) {
        return Err(format!("source image not found: {}", source.path));
    }
    let refs = generation_refs(root, state, scene_id, cut_number)?;
    let ext = Path::new(&source.path)
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| is_image_ext(value))
        .unwrap_or("png");
    let rel = next_frame_rel(root, state, scene_id, cut_number, ext);
    create_parent(root, &rel)?;
    let mut inputs = Map::new();
    inputs.insert("source_frame".into(), Value::String(source.path.clone()));
    inputs.insert("edit_brief".into(), Value::String(brief.to_string()));
    inputs.insert("preserve_props".into(), Value::Bool(true));
    inputs.insert("output_path".into(), Value::String(rel.clone()));
    if let Some(aspect) = &state.settings.aspect_ratio {
        inputs.insert("aspect_ratio".into(), Value::String(aspect.clone()));
    }
    let extra_refs: Vec<&GenerationRef> = refs
        .iter()
        .filter(|reference| reference.path != source.path)
        .collect();
    if !extra_refs.is_empty() {
        inputs.insert(
            "reference_images".into(),
            json!(extra_refs
                .iter()
                .map(|reference| reference.path.clone())
                .collect::<Vec<_>>()),
        );
    }
    let action = DirectorAction {
        action: ActionKind::RunSpecialist,
        specialist: Some("still_image".into()),
        inputs,
        message: None,
        op: None,
        path: None,
    };
    let root_s = root.to_string_lossy().to_string();
    let validated = validate_action(action.clone(), Some(&root_s)).map_err(|e| e.to_string())?;
    let mut card = format!(
        "Cut start-frame edit · Scene {scene_id} Cut {cut_number}\nSource version: `{}`\nHost will save the edit to `{rel}`.",
        source.path
    );
    append_refs(&mut card, &refs);
    card.push_str("\n\n");
    card.push_str(&approval_details(
        approval_app,
        &action,
        &validated,
        &root_s,
    )?);
    let mut input_asset_ids = vec![source.id];
    for reference in refs {
        if !input_asset_ids.contains(&reference.asset_id) {
            input_asset_ids.push(reference.asset_id);
        }
    }
    push_job(
        state,
        "cut_start_frame_edit",
        scene_id,
        cut_number,
        rel,
        input_asset_ids,
        approval_app,
        &action,
        &root_s,
    );
    let note = if approval_app.is_some() {
        format!("Start-frame edit ready for Scene {scene_id} Cut {cut_number}. Use the one-time approval button; chat text cannot approve it.")
    } else {
        format!("Start-frame edit plan prepared for Scene {scene_id} Cut {cut_number}; no provider operation is authorized here.")
    };
    Ok((card, note))
}

pub fn attach_image(
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    cut_number: u32,
    source: &str,
) -> Result<String, String> {
    require_cut(state, scene_id, cut_number)?;
    let source = resolve_under_project(source, Some(root.to_string_lossy().as_ref()))
        .or_else(|_| {
            let path = PathBuf::from(source);
            if path.is_absolute() {
                Ok(path)
            } else {
                Err(crate::director_actions::ActionError::Validate(
                    "image path could not be resolved".into(),
                ))
            }
        })
        .map_err(|error| error.to_string())?;
    if !source.is_file() {
        return Err(format!("image not found: {}", source.display()));
    }
    let ext = source
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| is_image_ext(value))
        .unwrap_or("png");
    let rel = next_frame_rel(root, state, scene_id, cut_number, ext);
    let root_str = root.to_string_lossy().into_owned();
    // Confine destination BEFORE create_dir_all: resolve against the
    // project root and enforce the exact Preproduction/SceneN/start_frames bucket.
    let dest = crate::project_path::resolve(&rel, Some(&root_str), crate::project_path::PathIntent::Write)
        .map(|resolved| resolved.into_path_buf())
        .map_err(|error| format!("start-frame destination outside project: {error}"))?;
    crate::project_path::under_scene_bucket(root, &dest, "start_frames")
        .map_err(|error| format!("start-frame destination outside start_frames bucket: {error}"))?;
    if crate::project_path::relative_to_root(root, &dest)?.parent()
        != Path::new(&rel).parent()
    {
        return Err("start-frame destination is outside this scene start_frames bucket".into());
    }
    if source != dest.as_path() {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("could not create start-frame folder: {error}"))?;
        }
    }
    // Re-confine AFTER create_dir_all — fs::copy follows parent chain,
    // so the final destination must still be inside the exact start_frames
    // bucket of this scene. This blocks directory or existing-file symlinks.
    let dest_again = crate::project_path::resolve(&rel, Some(&root_str), crate::project_path::PathIntent::Write)
        .map(|resolved| resolved.into_path_buf())
        .map_err(|error| format!("start-frame destination outside project: {error}"))?;
    crate::project_path::under_scene_bucket(root, &dest_again, "start_frames")
        .map_err(|error| format!("start-frame destination outside start_frames bucket: {error}"))?;
    if crate::project_path::relative_to_root(root, &dest_again)?.parent()
        != Path::new(&rel).parent()
    {
        return Err("start-frame destination is outside this scene start_frames bucket".into());
    }
    if source != dest_again.as_path() {
        fs::copy(&source, &dest_again).map_err(|error| format!("could not copy image: {error}"))?;
    }
    let parent_asset_id =
        latest_cut_asset(state, scene_id, cut_number).map(|asset| asset.id.clone());
    let asset_id = state.push_asset(AssetRecord {
        id: String::new(),
        path: rel,
        role: AssetRole::CutStartFrame,
        status: AssetStatus::Draft,
        scene_id: Some(scene_id),
        cut_id: Some(CutId::new(scene_id, cut_number)),
        character_id: None,
        source_job_id: None,
        parent_asset_id,
        notes: Some("attached cut start frame".into()),
    });
    let cut = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .and_then(|scene| scene.cuts.iter_mut().find(|cut| cut.id.cut == cut_number))
        .ok_or_else(|| format!("cut {scene_id}.{cut_number} not found"))?;
    cut.visual_source = Some(VisualSourceSpec::new(
        VisualSourceStrategy::PreproductionStartFrame,
    ));
    Ok(asset_id)
}

pub fn set_status(
    state: &mut ProjectState,
    scene_id: u32,
    cut_number: u32,
    asset_id: &str,
    status: AssetStatus,
) -> Result<String, String> {
    let cut_id = CutId::new(scene_id, cut_number);
    let path = state
        .asset(asset_id)
        .filter(|asset| asset.cut_id == Some(cut_id) && asset.role.is_cut_image())
        .ok_or_else(|| format!("cut image `{asset_id}` not found"))?
        .path
        .clone();
    if status == AssetStatus::Approved {
        let previous: Vec<String> = state
            .assets
            .iter()
            .filter(|asset| {
                asset.id != asset_id
                    && asset.cut_id == Some(cut_id)
                    && asset.role.is_cut_image()
                    && asset.status == AssetStatus::Approved
            })
            .map(|asset| asset.id.clone())
            .collect();
        for id in &previous {
            if let Some(asset) = state.asset_mut(id) {
                asset.status = AssetStatus::Superseded;
            }
            crate::workflow::identity::record_version(state, id, AssetStatus::Superseded);
        }
        crate::workflow::identity::record_version(state, asset_id, AssetStatus::Approved);
        if !previous.is_empty() {
            crate::workflow::identity::mark_upstream_changed(
                state, scene_id, cut_number, &previous,
            );
        }
    }
    state
        .asset_mut(asset_id)
        .ok_or_else(|| format!("cut image `{asset_id}` not found"))?
        .status = status;
    let cut = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .and_then(|scene| scene.cuts.iter_mut().find(|cut| cut.id.cut == cut_number))
        .ok_or_else(|| format!("cut {scene_id}.{cut_number} not found"))?;
    match status {
        AssetStatus::Approved => {
            cut.approved_start_frame = Some(path);
            let mut source = VisualSourceSpec::new(VisualSourceStrategy::PreproductionStartFrame);
            source.asset_id = Some(asset_id.to_string());
            cut.visual_source = Some(source);
        }
        AssetStatus::Rejected if cut.approved_start_frame.as_deref() == Some(path.as_str()) => {
            cut.approved_start_frame = None;
            if let Some(source) = &mut cut.visual_source {
                if source.strategy == VisualSourceStrategy::PreproductionStartFrame
                    && source.asset_id.as_deref() == Some(asset_id)
                {
                    source.asset_id = None;
                }
            }
        }
        _ => {}
    }
    Ok(asset_id.to_string())
}

pub fn delete_image(
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    cut_number: u32,
    asset_id: &str,
) -> Result<String, String> {
    let cut_id = CutId::new(scene_id, cut_number);
    let asset = state
        .asset(asset_id)
        .filter(|asset| asset.cut_id == Some(cut_id) && asset.role.is_cut_image())
        .ok_or_else(|| format!("cut image `{asset_id}` not found"))?
        .clone();
    let root_s = root.to_string_lossy().to_string();
    let path =
        resolve_under_project(&asset.path, Some(&root_s)).map_err(|error| error.to_string())?;
    crate::project_path::under_scene_bucket(root, &path, "start_frames")?;
    if path.is_file() {
        crate::project_path::move_to_trash(root, &path)?;
    }
    state.assets.retain(|item| item.id != asset_id);
    for item in &mut state.assets {
        if item.parent_asset_id.as_deref() == Some(asset_id) {
            item.parent_asset_id = None;
        }
    }
    for job in &mut state.jobs {
        job.input_asset_ids.retain(|id| id != asset_id);
        job.output_asset_ids.retain(|id| id != asset_id);
    }
    if let Some(cut) = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .and_then(|scene| scene.cuts.iter_mut().find(|cut| cut.id.cut == cut_number))
    {
        if cut.approved_start_frame.as_deref() == Some(asset.path.as_str()) {
            cut.approved_start_frame = None;
        }
        if let Some(source) = &mut cut.visual_source {
            if source.asset_id.as_deref() == Some(asset_id) {
                source.asset_id = None;
                source.source_take_id = None;
                source.frame_time_seconds = None;
            }
        }
    }
    Ok(asset.path)
}

pub fn on_still_saved(project_root: Option<&str>, requested_path: &str) -> Option<String> {
    let root = Path::new(project_root?.trim());
    if !root.join(crate::workflow::state::STATE_REL).is_file() {
        return None;
    }
    let actual = actual_path(root, requested_path)?;
    let rel = relativize(root, &actual)?;
    if !is_start_frame_rel(&rel) {
        return None;
    }
    let scene_id = scene_from_rel(&rel)?;
    let cut_number = cut_from_rel(&rel)?;
    let cut_id = CutId::new(scene_id, cut_number);
    let mut state = import::load_state(root).ok()?;
    if let Some(existing_id) = state
        .assets
        .iter()
        .find(|asset| asset.path == rel && asset.cut_id == Some(cut_id))
        .map(|asset| asset.id.clone())
    {
        complete_job(&mut state, &rel, &existing_id);
        let _ = import::save_project(root, state);
        return Some(format!("Cut start frame already registered: `{rel}`."));
    }
    require_cut(&state, scene_id, cut_number).ok()?;
    let job_id = matching_job(&state, &rel).map(|job| job.id.clone());
    let parent_asset_id =
        latest_cut_asset(&state, scene_id, cut_number).map(|asset| asset.id.clone());
    let asset_id = state.push_asset(AssetRecord {
        id: String::new(),
        path: rel.clone(),
        role: AssetRole::CutStartFrame,
        status: AssetStatus::Draft,
        scene_id: Some(scene_id),
        cut_id: Some(cut_id),
        character_id: None,
        source_job_id: job_id,
        parent_asset_id,
        notes: Some("generated cut start frame".into()),
    });
    complete_job(&mut state, &rel, &asset_id);
    let _ = import::save_project(root, state);
    Some(format!(
        "Registered cut start-frame draft `{asset_id}` at `{rel}`. Approve it on the Reference Images stage."
    ))
}

fn append_refs(card: &mut String, refs: &[GenerationRef]) {
    if refs.is_empty() {
        return;
    }
    card.push_str("\nGeneration references:");
    for reference in refs {
        card.push_str(&format!("\n- {} · `{}`", reference.label, reference.path));
    }
}

fn push_job(
    state: &mut ProjectState,
    kind: &str,
    scene_id: u32,
    cut_number: u32,
    rel: String,
    input_asset_ids: Vec<String>,
    approval_app: Option<&crate::ctx::HostCtx>,
    action: &DirectorAction,
    root_s: &str,
) {
    let job_id = state.next_job_id();
    let mut job = JobRecord {
        input_asset_ids,
        output_path: Some(rel),
        raw_provider_id: Some(format!("s{scene_id}_c{cut_number}")),
        ..JobRecord::open(job_id, kind)
    };
    if let Some(app) = approval_app {
        if let Ok(Some(estimate)) = crate::director::estimate_for_action(app, action, Some(root_s))
        {
            job.provider = Some(estimate.kind);
            job.model = estimate.model;
            job.currency = Some(estimate.currency);
            job.estimated_cost_minor = Some(estimate.minor);
        }
    }
    state.jobs.push(job);
}

fn require_cut(state: &ProjectState, scene_id: u32, cut_number: u32) -> Result<(), String> {
    state
        .scene(scene_id)
        .and_then(|scene| scene.cuts.iter().find(|cut| cut.id.cut == cut_number))
        .map(|_| ())
        .ok_or_else(|| format!("cut {scene_id}.{cut_number} not found"))
}

fn latest_cut_asset(state: &ProjectState, scene_id: u32, cut_number: u32) -> Option<&AssetRecord> {
    let cut_id = CutId::new(scene_id, cut_number);
    state
        .assets
        .iter()
        .rev()
        .find(|asset| asset.cut_id == Some(cut_id) && asset.role.is_cut_image())
}

fn matching_job<'a>(state: &'a ProjectState, rel: &str) -> Option<&'a JobRecord> {
    state.jobs.iter().find(|job| {
        matches!(
            job.kind.as_str(),
            "cut_start_frame" | "cut_start_frame_edit"
        ) && job
            .output_path
            .as_deref()
            .is_some_and(|path| same_stem(path, rel))
    })
}

fn complete_job(state: &mut ProjectState, rel: &str, asset_id: &str) {
    if let Some(job) = state.jobs.iter_mut().find(|job| {
        matches!(
            job.kind.as_str(),
            "cut_start_frame" | "cut_start_frame_edit"
        ) && job
            .output_path
            .as_deref()
            .is_some_and(|path| same_stem(path, rel))
    }) {
        job.finish(Some(rel), Some(asset_id));
    }
}

fn create_parent(root: &Path, rel: &str) -> Result<(), String> {
    if let Some(parent) = root.join(rel).parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create start-frame folder: {error}"))?;
    }
    Ok(())
}

fn stem_exists(root: &Path, rel: &str) -> bool {
    let requested = root.join(rel);
    let Some(parent) = requested.parent() else {
        return false;
    };
    let Some(stem) = requested.file_stem() else {
        return false;
    };
    fs::read_dir(parent).ok().is_some_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            let path = entry.path();
            path.is_file() && !path.is_symlink() && path.file_stem() == Some(stem) && is_image_path(&path)
        })
    })
}

fn same_stem(a: &str, b: &str) -> bool {
    let normalize = |value: &str| {
        Path::new(value)
            .with_extension("")
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase()
    };
    normalize(a) == normalize(b)
}

fn actual_path(root: &Path, requested: &str) -> Option<PathBuf> {
    let root_string = root.to_string_lossy();
    let requested = crate::project_path::resolve(
        requested,
        Some(root_string.as_ref()),
        crate::project_path::PathIntent::Read,
    )
    .ok()?
    .into_path_buf();
    if requested.is_file() {
        return Some(requested);
    }
    let parent = requested.parent()?;
    let stem = requested.file_stem()?;
    fs::read_dir(parent)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find_map(|path| {
            if path.file_stem() != Some(stem) || !is_image_path(&path) {
                return None;
            }
            let confined = crate::project_path::resolve(
                &path.to_string_lossy(),
                Some(root_string.as_ref()),
                crate::project_path::PathIntent::Read,
            )
            .ok()?
            .into_path_buf();
            confined.is_file().then_some(confined)
        })
}

fn file_exists(root: &Path, value: &str) -> bool {
    let root_string = root.to_string_lossy();
    crate::project_path::resolve(
        value,
        Some(root_string.as_ref()),
        crate::project_path::PathIntent::Read,
    )
    .is_ok_and(|path| path.as_path().is_file())
}

fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(is_image_ext)
}

fn is_image_ext(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "gif"
    )
}

fn is_start_frame_rel(value: &str) -> bool {
    let normalized = value.replace('\\', "/");
    let parts: Vec<_> = normalized.split('/').collect();
    parts.len() == 4
        && parts[0] == "Preproduction"
        && parts[1].strip_prefix("Scene").is_some_and(|number| {
            !number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit())
        })
        && parts[2] == "start_frames"
        && is_image_ext(
            Path::new(parts[3])
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or(""),
        )
}

fn scene_from_rel(value: &str) -> Option<u32> {
    let normalized = value.replace('\\', "/");
    let rest = normalized.split("Preproduction/Scene").nth(1)?;
    let number: String = rest.chars().take_while(char::is_ascii_digit).collect();
    number.parse().ok()
}

fn cut_from_rel(value: &str) -> Option<u32> {
    let stem = Path::new(value).file_stem()?.to_str()?;
    let rest = stem.strip_prefix("cut_")?;
    rest.split("_v").next()?.parse().ok()
}

fn relativize(root: &Path, path: &Path) -> Option<String> {
    crate::project_path::relative_to_root(root, path)
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::state::{
        CharacterRecord, CutSketch, JobStatus, LookBoard, LookSlot, MouthPolicy, ShotRole,
    };

    fn write(root: &Path, rel: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"image").unwrap();
    }

    fn fixture(root: &Path) -> ProjectState {
        let mut state = ProjectState::new();
        state.settings.aspect_ratio = Some("9:16".into());
        for (id, path, role, character_id) in [
            (
                "tom_identity",
                "Characters/Tom Vale/model_sheet/tom_closeup_front.png",
                AssetRole::IdentityReference,
                Some("tom_vale"),
            ),
            (
                "tom_body",
                "Characters/Tom Vale/model_sheet/tom_standing_front.png",
                AssetRole::BodyReference,
                Some("tom_vale"),
            ),
            (
                "wardrobe",
                "Preproduction/Scene1/references/wardrobe_1.png",
                AssetRole::WardrobeReference,
                None,
            ),
            (
                "interior",
                "Preproduction/Scene1/references/interior_1.png",
                AssetRole::InteriorReference,
                None,
            ),
        ] {
            write(root, path);
            state.assets.push(AssetRecord {
                id: id.into(),
                path: path.into(),
                role,
                status: AssetStatus::Approved,
                scene_id: role.is_scene_look().then_some(1),
                cut_id: None,
                character_id: character_id.map(str::to_string),
                source_job_id: None,
                parent_asset_id: None,
                notes: (role == AssetRole::BodyReference).then(|| "standing_front".into()),
            });
        }
        state.characters.push(CharacterRecord {
            id: "tom_vale".into(),
            name: "Tom Vale".into(),
            dir: "Characters/Tom Vale".into(),
            identity_ready: true,
            voice_ready: false,
            profile_path: None,
            model_sheet_manifest_path: None,
            profile: Default::default(),
            asset_ids: vec!["tom_identity".into(), "tom_body".into()],
            notes: Vec::new(),
        });
        let scene = state.scene_mut(1, "The last train");
        scene.location = Some("night train".into());
        let mut board = LookBoard::default();
        let mut wardrobe =
            LookSlot::new("s1_look_03", AssetRole::WardrobeReference, "Travel clothes");
        wardrobe.asset_ids.push("wardrobe".into());
        wardrobe.approved_asset_id = Some("wardrobe".into());
        let mut interior =
            LookSlot::new("s1_look_01", AssetRole::InteriorReference, "Train interior");
        interior.asset_ids.push("interior".into());
        interior.approved_asset_id = Some("interior".into());
        board.slots.extend([interior, wardrobe]);
        board.sync_asset_ids();
        scene.look_board = Some(board);
        let cut = scene.cut_mut(1);
        cut.title = Some("Tom guards the parcel".into());
        cut.subject = Some("Tom Vale".into());
        cut.sketch = Some(CutSketch {
            title: Some("Tom guards the parcel".into()),
            shot_role: Some(ShotRole::Performance),
            subject: Some("Tom Vale".into()),
            speaker: Some("Tom Vale".into()),
            mouth_policy: Some(MouthPolicy::SpeakingVisible),
            visual_prompt: Some("Tom Vale in the narrow train, brown parcel on his knees".into()),
            negative_prompt: Some("modern clothes, text".into()),
            reference_roles: vec![AssetRole::InteriorReference],
            on_camera: vec!["Tom Vale".into()],
            ..CutSketch::default()
        });
        state
    }

    #[test]
    fn llm_prompt_only_updates_creative_fields() {
        let mut cut = CutRecord::new(CutId::new(1, 1));
        apply_prompt(
            &mut cut,
            &json!({
                "prompt": "Tom at the rainy window",
                "negative_prompt": "modern train",
                "output_path": "/tmp/invented.png",
                "reference_images": ["bad.png"]
            }),
        )
        .unwrap();
        assert_eq!(
            cut.start_frame_prompt.as_deref(),
            Some("Tom at the rainy window")
        );
        assert_eq!(
            cut.start_frame_negative_prompt.as_deref(),
            Some("modern train")
        );
        assert!(cut.sketch.is_none());
        assert!(cut.approved_start_frame.is_none());
    }

    #[test]
    fn llm_video_prompt_is_rejected_but_manual_draft_can_be_saved() {
        let mut cut = CutRecord::new(CutId::new(1, 1));
        cut.dialogue.push(crate::script::DialogueLine {
            speaker: "Tom Vale".into(),
            text: "We shall be there before supper.".into(),
        });
        cut.sketch = Some(CutSketch {
            mouth_policy: Some(MouthPolicy::SpeakingVisible),
            ..CutSketch::default()
        });
        let first = json!({
            "prompt": "Tom in the carriage, natural lip-sync framing"
        });
        let first_issues = prompt_issues(&cut, &first).unwrap();
        assert!(first_issues.iter().any(|issue| issue.contains("lip-sync")));

        let second = json!({
            "prompt": "Cinematic still of Tom, mouth visibly forming the words, “We shall be there before supper.”",
            "negative_prompt": "closed mouth, camera movement"
        });
        let second_issues = prompt_issues(&cut, &second).unwrap();
        assert!(second_issues
            .iter()
            .any(|issue| issue.contains("word-forming mouth")));
        assert!(second_issues
            .iter()
            .any(|issue| issue.contains("repeats dialogue")));
        assert!(second_issues
            .iter()
            .any(|issue| issue.contains("prohibits the neutral closed mouth")));
        assert!(apply_prompt(&mut cut, &second).is_err());
        assert!(cut.start_frame_prompt.is_none());
        save_prompt(
            &mut cut,
            "Tom in the carriage, natural lip-sync framing",
            "modern objects",
        )
        .unwrap();
        assert_eq!(
            cut.start_frame_prompt.as_deref(),
            Some("Tom in the carriage, natural lip-sync framing")
        );
    }

    #[test]
    fn performance_start_frame_accepts_visible_face_before_speech() {
        let mut cut = CutRecord::new(CutId::new(1, 1));
        cut.sketch = Some(CutSketch {
            mouth_policy: Some(MouthPolicy::SpeakingVisible),
            ..CutSketch::default()
        });
        apply_prompt(
            &mut cut,
            &json!({
                "prompt": "Tom Vale faces the camera in the rainy carriage, his mouth naturally relaxed and closed in the instant before speech",
                "negative_prompt": "open mouth, visible speech, lip movement, text"
            }),
        )
        .unwrap();
        assert!(cut.start_frame_prompt.is_some());
    }

    #[test]
    fn generation_uses_identity_and_approved_requested_scene_refs() {
        let dir = tempfile::tempdir().unwrap();
        let state = fixture(dir.path());
        let refs = generation_refs(dir.path(), &state, 1, 1).unwrap();
        let ids: Vec<&str> = refs
            .iter()
            .map(|reference| reference.asset_id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec!["tom_identity", "tom_body", "wardrobe", "interior"]
        );
    }

    #[test]
    fn explicit_on_camera_character_uses_only_their_wardrobe_plates() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = fixture(root);
        for (id, path, notes) in [
            (
                "pip_plate_close",
                "Preproduction/Scene1/wardrobe_plates/pip_vale_closeup_front.png",
                "closeup_front",
            ),
            (
                "pip_plate_standing",
                "Preproduction/Scene1/wardrobe_plates/pip_vale_standing_front.png",
                "standing_front",
            ),
        ] {
            write(root, path);
            state.assets.push(AssetRecord {
                id: id.into(),
                path: path.into(),
                role: AssetRole::WardrobePlate,
                status: AssetStatus::Approved,
                scene_id: Some(1),
                cut_id: None,
                character_id: Some("pip_vale".into()),
                source_job_id: None,
                parent_asset_id: None,
                notes: Some(notes.into()),
            });
        }
        state.characters.push(CharacterRecord {
            id: "pip_vale".into(),
            name: "Pip Vale".into(),
            dir: "Characters/Pip Vale".into(),
            identity_ready: true,
            voice_ready: false,
            profile_path: None,
            model_sheet_manifest_path: None,
            profile: Default::default(),
            asset_ids: vec!["pip_plate_close".into(), "pip_plate_standing".into()],
            notes: Vec::new(),
        });
        let cut = &mut state.scenes[0].cuts[0];
        cut.prompt = Some("Pip speaks while Tom and Nell sit beside him in the carriage.".into());
        cut.start_frame_prompt =
            Some("Medium close-up of Pip Vale alone in warm lamplight.".into());
        cut.sketch.as_mut().unwrap().on_camera = vec!["Pip Vale".into()];
        cut.sketch.as_mut().unwrap().reference_roles =
            vec![AssetRole::WardrobeReference, AssetRole::InteriorReference];

        let refs = generation_refs(root, &state, 1, 1).unwrap();
        let paths: Vec<&str> = refs
            .iter()
            .map(|reference| reference.path.as_str())
            .collect();
        assert_eq!(
            paths,
            vec![
                "Preproduction/Scene1/wardrobe_plates/pip_vale_closeup_front.png",
                "Preproduction/Scene1/wardrobe_plates/pip_vale_standing_front.png",
                "Preproduction/Scene1/references/interior_1.png",
            ]
        );
        assert!(!paths.iter().any(|path| path.contains("Tom Vale")));
        assert!(!paths
            .iter()
            .any(|path| path.ends_with("references/wardrobe_1.png")));
    }

    #[test]
    fn prepare_generate_creates_host_path_and_pending_job() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = fixture(root);
        let (card, _) = prepare_generate(root, &mut state, 1, 1).unwrap();
        assert!(card.contains("Preproduction/Scene1/start_frames/cut_1_v1.png"));
        assert!(card.contains("Characters/Tom Vale/model_sheet/tom_closeup_front.png"));
        assert!(card.contains("Preproduction/Scene1/references/wardrobe_1.png"));
        assert!(card.contains("### Files read or uploaded"));
        assert!(card.contains("### Artifacts modified"));
        assert!(!card.contains("```json"));
        assert!(!card.contains("Avoid: modern clothes, text"));
        let reads = card.split("### Artifacts modified").next().unwrap_or(&card);
        assert!(reads.contains("tom_closeup_front.png"));
        assert!(!reads.contains("source_frame"));
        let job = state.jobs.last().unwrap();
        assert_eq!(job.kind, "cut_start_frame");
        assert_eq!(job.status, JobStatus::Pending);
        assert_eq!(job.input_asset_ids.len(), 4);
    }

    #[test]
    fn prepare_generate_blocks_a_preexisting_video_like_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = fixture(root);
        state.scenes[0].cuts[0].start_frame_prompt =
            Some("Tom speaks with natural lip-sync framing".into());
        let error = prepare_generate(root, &mut state, 1, 1).unwrap_err();
        assert!(error.contains("video-like"));
        assert!(state.jobs.is_empty());
    }

    #[test]
    fn versions_skip_other_extensions_and_pending_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = fixture(root);
        write(root, "Preproduction/Scene1/start_frames/cut_1_v1.jpg");
        state.jobs.push(JobRecord {
            id: "job_9".into(),
            kind: "cut_start_frame".into(),
            provider: None,
            model: None,
            input_asset_ids: Vec::new(),
            output_asset_ids: Vec::new(),
            status: JobStatus::Pending,
            output_path: Some("Preproduction/Scene1/start_frames/cut_1_v2.png".into()),
            raw_provider_id: Some("s1_c1".into()),
            error: None,
            attempts: Vec::new(),
            created_at: None,
            started_at: None,
            completed_at: None,
            scope: None,
            progress: None,
            cost_minor: None,
            estimated_cost_minor: None,
            currency: None,
            retry_of: None,
            spend_category: None,
            reconciliation: None,
            assembly_snapshot: None,
            reserved_max_minor: None,
            actual_verified: false,
            unbounded_exposure: false,
            request_metadata: None,
        });
        assert_eq!(
            next_frame_rel(root, &state, 1, 1, "png"),
            "Preproduction/Scene1/start_frames/cut_1_v3.png"
        );
    }

    #[test]
    fn saved_frame_registers_then_approve_and_edit_use_it_as_source() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = fixture(root);
        prepare_generate(root, &mut state, 1, 1).unwrap();
        let requested = state.jobs[0].output_path.clone().unwrap();
        import::save_project(root, state).unwrap();
        write(root, &requested);
        let note = on_still_saved(Some(root.to_str().unwrap()), &requested).unwrap();
        assert!(note.contains("Registered cut start-frame draft"));
        let mut state = import::load_state(root).unwrap();
        let frame = state
            .assets
            .iter()
            .find(|asset| asset.role == AssetRole::CutStartFrame)
            .unwrap()
            .clone();
        set_status(&mut state, 1, 1, &frame.id, AssetStatus::Approved).unwrap();
        assert_eq!(
            state.scenes[0].cuts[0].approved_start_frame.as_deref(),
            Some(requested.as_str())
        );
        let (card, _) =
            prepare_edit(root, &mut state, 1, 1, &frame.id, "Make Tom's coat wetter").unwrap();
        assert!(card.contains("### Files read or uploaded"));
        assert!(card.contains("### Artifacts modified"));
        assert!(!card.contains("```json"));
        assert!(!card.contains("\"source_frame\""));
        assert!(card.contains(&requested));
        assert!(card.contains("cut_1_v2.png"));
        assert_eq!(state.jobs.last().unwrap().kind, "cut_start_frame_edit");
        assert_eq!(
            state.jobs.last().unwrap().input_asset_ids.first(),
            Some(&frame.id)
        );
    }

    #[test]
    fn attach_approve_delete_manages_exact_cut_version() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = fixture(root);
        let source = root.join("outside.jpg");
        fs::write(&source, b"jpg").unwrap();
        let id = attach_image(root, &mut state, 1, 1, source.to_str().unwrap()).unwrap();
        let path = state.asset(&id).unwrap().path.clone();
        assert!(root.join(&path).is_file());
        set_status(&mut state, 1, 1, &id, AssetStatus::Approved).unwrap();
        assert_eq!(
            state.scenes[0].cuts[0].approved_start_frame.as_deref(),
            Some(path.as_str())
        );
        delete_image(root, &mut state, 1, 1, &id).unwrap();
        assert!(!root.join(path).exists());
        assert!(state.asset(&id).is_none());
        assert!(state.scenes[0].cuts[0].approved_start_frame.is_none());
    }

    #[test]
    fn manually_deleted_approved_frame_is_pruned_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = fixture(root);
        let source = root.join("attached.png");
        fs::write(&source, b"png").unwrap();
        let id = attach_image(root, &mut state, 1, 1, source.to_str().unwrap()).unwrap();
        set_status(&mut state, 1, 1, &id, AssetStatus::Approved).unwrap();
        let path = state.asset(&id).unwrap().path.clone();
        import::save_project(root, state).unwrap();
        fs::remove_file(root.join(&path)).unwrap();

        let loaded = import::load_state(root).unwrap();
        assert!(loaded.asset(&id).is_none());
        assert!(loaded.scenes[0].cuts[0].approved_start_frame.is_none());
    }

    #[test]
    fn approving_a_new_start_frame_keeps_the_previous_version() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("Preproduction/Scene1/start_frames")).unwrap();
        let old_rel = "Preproduction/Scene1/start_frames/cut_1_v1.png";
        let new_rel = "Preproduction/Scene1/start_frames/cut_1_v2.png";
        let old_bytes = b"version-one-bytes";
        fs::write(root.join(old_rel), old_bytes).unwrap();
        fs::write(root.join(new_rel), b"version-two-bytes").unwrap();
        let mut state = ProjectState::new();
        state
            .scene_mut(1, "Scene")
            .cuts
            .push(CutRecord::new(CutId::new(1, 1)));
        let old_id = state.push_asset(AssetRecord {
            id: String::new(),
            path: old_rel.into(),
            role: AssetRole::CutStartFrame,
            status: AssetStatus::Draft,
            scene_id: Some(1),
            cut_id: Some(CutId::new(1, 1)),
            character_id: None,
            source_job_id: None,
            parent_asset_id: None,
            notes: None,
        });
        let new_id = state.push_asset(AssetRecord {
            id: String::new(),
            path: new_rel.into(),
            role: AssetRole::CutStartFrame,
            status: AssetStatus::Draft,
            scene_id: Some(1),
            cut_id: Some(CutId::new(1, 1)),
            character_id: None,
            source_job_id: None,
            parent_asset_id: None,
            notes: None,
        });
        set_status(&mut state, 1, 1, &old_id, AssetStatus::Approved).unwrap();
        set_status(&mut state, 1, 1, &new_id, AssetStatus::Approved).unwrap();

        let previous = state.asset(&old_id).unwrap();
        assert_eq!(previous.id, old_id);
        assert_eq!(previous.path, old_rel);
        assert_eq!(previous.status, AssetStatus::Superseded);
        assert_eq!(fs::read(root.join(old_rel)).unwrap(), old_bytes);
        assert_eq!(state.asset(&new_id).unwrap().status, AssetStatus::Approved);
        assert_eq!(
            state.scenes[0].cuts[0].approved_start_frame.as_deref(),
            Some(new_rel)
        );
        let version = state
            .versions
            .iter()
            .find(|version| version.asset_id == old_id)
            .unwrap();
        assert_eq!(version.path, old_rel);
        assert_eq!(version.status, AssetStatus::Superseded);
    }

    #[test]
    fn path_helpers_reject_files_outside_the_project_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        let outside = dir
            .path()
            .join("Preproduction/Scene1/start_frames/cut_1.png");
        fs::create_dir_all(root.join("Preproduction/Scene1/start_frames")).unwrap();
        fs::create_dir_all(outside.parent().unwrap()).unwrap();
        fs::write(&outside, b"outside").unwrap();
        fs::write(
            root.join("Preproduction/Scene1/start_frames/cut_1.png"),
            b"inside",
        )
        .unwrap();

        assert!(actual_path(&root, outside.to_str().unwrap()).is_none());
        assert!(!file_exists(&root, outside.to_str().unwrap()));
        assert!(actual_path(&root, "../Preproduction/Scene1/start_frames/cut_1.png").is_none());
        assert!(file_exists(
            &root,
            "Preproduction/Scene1/start_frames/cut_1.png"
        ));
    }
}
