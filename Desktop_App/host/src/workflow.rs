//! V2 workflow: typed project state and import from existing folders.
pub mod animatic;
pub mod breakdown;
pub mod characters;
pub mod identity;
pub mod import;
pub mod intent;
pub mod llm;
pub mod look;
pub mod post;
pub mod production;
pub mod recipe;
pub mod report;
pub mod route;
pub mod scope;
pub mod sketches;
pub mod sources;
pub mod start_frames;
pub mod state;
pub mod validate;

pub use import::{import_project, load_or_import, ImportOptions, ImportResult};
pub use report::{project_report, MixCounts, ProjectReport, SceneReport};
pub use route::{classify_planning, PlanningKind};
pub use state::{
    AssetRecord, AssetRole, CharacterProfile, CharacterRecord, CutRecord, ProjectState,
    SceneRecord, VisualSourceSpec, VisualSourceStrategy, STATE_REL,
};

use crate::ctx::HostCtx;
use crate::director_actions::{
    format_approval_request, validate_action, ActionKind, DirectorAction,
};
use crate::scene_plan::handoff::CutId;
use crate::scene_plan::strategy::SceneStrategy;
use crate::workflow::state::{AssetStatus, MouthPolicy, ShotRole, TakeStatus};
use serde_json::{json, Map, Value};
use std::path::Path;

fn arg_path(args: &Value, key: &str) -> Result<String, String> {
    let supplied = args
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    #[cfg(not(test))]
    {
        return crate::session::command_root(supplied.as_deref())
            .map(|path| path.to_string_lossy().into_owned());
    }
    #[cfg(test)]
    {
        supplied.ok_or_else(|| format!("Missing or invalid argument: {key}"))
    }
}

fn arg_bool(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

fn arg_u32(args: &Value, key: &str) -> Result<u32, String> {
    args.get(key)
        .and_then(|v| {
            v.as_u64()
                .map(|n| n as u32)
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .ok_or_else(|| format!("Missing or invalid argument: {key}"))
}

fn opt_f64(args: &Value, key: &str) -> Result<Option<f64>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => number
            .as_f64()
            .filter(|value| value.is_finite())
            .map(Some)
            .ok_or_else(|| format!("{key} must be a finite number")),
        Some(Value::String(value)) if value.trim().is_empty() => Ok(None),
        Some(Value::String(value)) => value
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|parsed| parsed.is_finite())
            .map(Some)
            .ok_or_else(|| format!("{key} must be a finite number")),
        _ => Err(format!("{key} must be a finite number")),
    }
}

fn opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn provided_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_string)
}

fn result_json(imported: ImportResult) -> Result<Value, String> {
    Ok(json!({
        "state": imported.state,
        "report": imported.report,
        "summary": imported.report.summary,
        "persistedPath": imported
            .persisted_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
    }))
}

fn result_with_note(imported: ImportResult, note: impl Into<String>) -> Result<Value, String> {
    let mut value = result_json(imported)?;
    value["note"] = json!(note.into());
    Ok(value)
}

fn result_with_pending(
    imported: ImportResult,
    note: impl Into<String>,
    pending_action: impl Into<String>,
) -> Result<Value, String> {
    let mut value = result_with_note(imported, note)?;
    value["pendingAction"] = json!(pending_action.into());
    Ok(value)
}

/// Host command: scan a project into V2 state. `persist` defaults to true.
pub fn cmd_import(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let persist = arg_bool(args, "persist", true);
    let imported = import_project(Path::new(&root), ImportOptions { persist })?;
    result_json(imported)
}

/// Host command: load persisted V2 state, or scan without writing.
pub fn cmd_status(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let refresh = arg_bool(args, "refresh", false);
    let imported = if refresh {
        import_project(Path::new(&root), ImportOptions { persist: false })?
    } else {
        load_or_import(Path::new(&root))?
    };
    result_json(imported)
}

pub fn cmd_save_character(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let existing_id = opt_str(args, "characterId");
    let name = opt_str(args, "name").ok_or_else(|| "Missing character name".to_string())?;
    let profile: CharacterProfile =
        serde_json::from_value(args.get("profile").cloned().unwrap_or_else(|| json!({})))
            .map_err(|error| format!("invalid character profile: {error}"))?;
    let state = import::load_state(Path::new(&root))?;
    let path = characters::save_profile(
        Path::new(&root),
        &state,
        existing_id.as_deref(),
        &name,
        profile,
    )?;
    let imported = import::import_project(Path::new(&root), ImportOptions { persist: true })?;
    result_with_note(imported, format!("Saved character profile `{path}`."))
}

pub fn cmd_save_intent(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let mut state = import::load_state(Path::new(&root))?;
    let intent = args
        .get("intent")
        .ok_or_else(|| "Missing intent".to_string())
        .and_then(intent::intent_from_json)?;
    let scene = state
        .scenes
        .iter_mut()
        .find(|s| s.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    scene.intent = Some(intent);
    scene.intent_approved = false;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, "Scene intent saved as draft.")
}

pub fn cmd_approve_intent(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let mut state = import::load_state(Path::new(&root))?;
    let scene = state
        .scenes
        .iter_mut()
        .find(|s| s.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    if scene.intent.as_ref().is_none_or(|i| i.is_empty()) {
        return Err("Draft scene intent before approving it.".into());
    }
    scene.intent_approved = true;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, "Scene intent approved.")
}

pub fn cmd_set_strategy(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let strategy_raw = opt_str(args, "strategy");
    let strategy = strategy_raw
        .as_deref()
        .and_then(SceneStrategy::parse_explicit)
        .ok_or_else(|| {
            "strategy must be cinematic_balanced, performance_heavy, or atmosphere_heavy"
                .to_string()
        })?;
    let mut state = import::load_state(Path::new(&root))?;
    let scene = state
        .scenes
        .iter_mut()
        .find(|s| s.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    scene.strategy = Some(strategy);
    if let Some(over) = args.get("override").and_then(|v| v.as_bool()) {
        scene.strategy_override = over;
    }
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, format!("Strategy set to {}.", strategy.label()))
}

pub fn cmd_save_profile(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let mut profile: crate::workflow::state::ProductionProfile = args
        .get("profile")
        .cloned()
        .ok_or_else(|| "Missing profile".to_string())
        .and_then(|value| {
            serde_json::from_value(value)
                .map_err(|error| format!("invalid production profile: {error}"))
        })?;
    if crate::workflow::state::ProductionRecipe::parse(profile.recipe.as_str()).is_none() {
        let document = profile
            .recipe_document
            .as_ref()
            .ok_or_else(|| format!("Unknown recipe `{}`.", profile.recipe.as_str()))?;
        let loaded = crate::workflow::recipe::parse_recipe_document(document)?;
        if loaded.id != profile.recipe.as_str() {
            return Err(format!(
                "Recipe document `{}` does not match `{}`.",
                loaded.id,
                profile.recipe.as_str()
            ));
        }
    }
    if profile.deliverables.is_empty() {
        return Err("A production profile needs a deliverable.".into());
    }
    for deliverable in &mut profile.deliverables {
        if deliverable.name.trim().is_empty() {
            return Err("A deliverable needs a name.".into());
        }
        if deliverable.id.trim().is_empty() {
            deliverable.id = crate::workflow::state::deliverable_id(&deliverable.name);
        }
    }
    let mut state = import::load_state(Path::new(&root))?;
    state.profile = Some(profile);
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, "Production profile saved.")
}

pub fn cmd_set_requirement_status(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let requirement_id =
        opt_str(args, "requirementId").ok_or_else(|| "Missing requirement id".to_string())?;
    let status = opt_str(args, "status")
        .as_deref()
        .and_then(crate::workflow::state::RequirementStatus::parse)
        .ok_or_else(|| {
            "status must be missing, draft, present, approved, or deferred".to_string()
        })?;
    let mut state = import::load_state(Path::new(&root))?;
    let requirement = state
        .requirement_mut(&requirement_id)
        .ok_or_else(|| format!("requirement {requirement_id} not found"))?;
    requirement.status = status;
    requirement.status_override = true;
    let name = requirement.name.clone();
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, format!("Set {name} to {}.", status.label()))
}

pub fn cmd_add_requirement(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let name = opt_str(args, "name").ok_or_else(|| "Missing requirement name".to_string())?;
    let kind = opt_str(args, "kind")
        .as_deref()
        .and_then(crate::workflow::state::RequirementKind::parse)
        .ok_or_else(|| "Unknown requirement kind".to_string())?;
    let scene_id = args.get("sceneId").and_then(|value| {
        value
            .as_u64()
            .map(|n| n as u32)
            .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
    });
    let mut state = import::load_state(Path::new(&root))?;
    let id = crate::workflow::state::requirement_id(kind, &name);
    if state
        .requirements
        .iter()
        .any(|requirement| requirement.id == id)
    {
        return Err(format!(
            "A {kind} requirement named {name} already exists.",
            kind = kind.label()
        ));
    }
    let status = if kind == crate::workflow::state::RequirementKind::Deferred {
        crate::workflow::state::RequirementStatus::Deferred
    } else {
        crate::workflow::state::RequirementStatus::Missing
    };
    state
        .requirements
        .push(crate::workflow::state::RequirementRecord {
            id,
            kind,
            name: name.clone(),
            status,
            scene_ids: scene_id.into_iter().collect(),
            character_id: None,
            notes: None,
            status_override: kind == crate::workflow::state::RequirementKind::Deferred,
            user_added: true,
        });
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        format!("Added {kind} requirement {name}.", kind = kind.label()),
    )
}

pub fn cmd_backfill_sketches(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let replace = arg_bool(args, "replace", true);
    let mut state = import::load_state(Path::new(&root))?;
    let keep_mouths = crate::workflow::recipe::recipe_requires_role(&state, "mouth_policy");
    let scene = state
        .scenes
        .iter_mut()
        .find(|s| s.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    if scene.strategy.is_none() {
        scene.strategy = Some(SceneStrategy::CinematicBalanced);
    }
    let n = sketches::backfill_scene(scene, replace);
    if !keep_mouths {
        crate::workflow::recipe::strip_mouths(scene);
    }
    if n == 0 {
        return Err("No production notes to backfill into cut sketches.".into());
    }
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        format!("Backfilled {n} cut sketches from production notes. Mix is host-validated — edit shot roles if the strategy rejects it."),
    )
}

pub fn cmd_update_sketch(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_n = arg_u32(args, "cut")?;
    let mut state = import::load_state(Path::new(&root))?;
    let keep_mouths = crate::workflow::recipe::recipe_requires_role(&state, "mouth_policy");
    let scene = state
        .scenes
        .iter_mut()
        .find(|s| s.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    let shot_role_raw = opt_str(args, "shotRole");
    let mouth_raw = opt_str(args, "mouthPolicy");
    let shot_role = shot_role_raw.as_deref().and_then(ShotRole::parse);
    let mouth_policy = mouth_raw.as_deref().and_then(MouthPolicy::parse);
    let title = opt_str(args, "title");
    let visual_prompt = opt_str(args, "visualPrompt");
    let subject = opt_str(args, "subject");
    {
        let cut = scene
            .cuts
            .iter_mut()
            .find(|c| c.id.cut == cut_n)
            .ok_or_else(|| format!("cut {cut_n} not found on scene {scene_id}"))?;
        sketches::patch_sketch(cut, shot_role, mouth_policy, title, visual_prompt, subject);
        if !keep_mouths {
            crate::workflow::recipe::clear_mouth(cut);
        }
    }
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, format!("Updated sketch for cut {cut_n}."))
}

pub fn cmd_save_sketch(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_n = arg_u32(args, "cut")?;
    let sketch = args
        .get("sketch")
        .ok_or_else(|| "Missing sketch".to_string())?;
    let mut state = import::load_state(Path::new(&root))?;
    let keep_mouths = crate::workflow::recipe::recipe_requires_role(&state, "mouth_policy");
    let cut = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .and_then(|scene| scene.cuts.iter_mut().find(|cut| cut.id.cut == cut_n))
        .ok_or_else(|| format!("cut {cut_n} not found on scene {scene_id}"))?;
    sketches::replace_sketch(cut, sketch)?;
    if !keep_mouths {
        crate::workflow::recipe::clear_mouth(cut);
    }
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, format!("Saved sketch for cut {cut_n}."))
}

pub fn cmd_set_animatic_duration(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let shot_id = opt_str(args, "shotId").ok_or_else(|| "Missing shot id".to_string())?;
    let seconds = arg_f64(args, "durationSeconds")?;
    let mut state = import::load_state(Path::new(&root))?;
    let scene = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    crate::workflow::animatic::set_duration(scene, &shot_id, seconds)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        format!("Set timing to {seconds:.2}s. Later shot packages need review. Renders were kept."),
    )
}

pub fn cmd_reorder_animatic(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let shot_ids = arg_string_list(args, "shotIds")?;
    let mut state = import::load_state(Path::new(&root))?;
    let scene = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    crate::workflow::animatic::reorder(scene, &shot_ids)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        "Animatic order saved. Later shot packages need review. Renders were kept.",
    )
}

pub fn cmd_reorder_cuts(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let shot_ids = arg_string_list(args, "shotIds")?;
    let mut state = import::load_state(Path::new(&root))?;
    crate::workflow::identity::reorder_cuts(&mut state, scene_id, &shot_ids)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        "Cut order saved. Shot ids are unchanged and existing takes were kept.",
    )
}

pub fn cmd_approve_animatic(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let mut state = import::load_state(Path::new(&root))?;
    let scene = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    crate::workflow::animatic::approve(scene)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, "Rough cut approved.")
}

pub fn cmd_attach_scratch_audio(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let shot_id = opt_str(args, "shotId").ok_or_else(|| "Missing shot id".to_string())?;
    let source = arg_path(args, "sourcePath")?;
    let mut state = import::load_state(Path::new(&root))?;
    let scene = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    let rel = crate::workflow::animatic::attach_scratch(
        Path::new(&root),
        scene,
        &shot_id,
        Path::new(&source),
    )?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, format!("Attached scratch audio `{rel}`."))
}

pub fn cmd_clear_scratch_audio(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let shot_id = opt_str(args, "shotId").ok_or_else(|| "Missing shot id".to_string())?;
    let mut state = import::load_state(Path::new(&root))?;
    let scene = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    crate::workflow::animatic::clear_scratch(Path::new(&root), scene, &shot_id)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, "Removed scratch audio.")
}

pub fn cmd_clear_package_review(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_n = arg_u32(args, "cut")?;
    let mut state = import::load_state(Path::new(&root))?;
    let cut = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .and_then(|scene| scene.cuts.iter_mut().find(|cut| cut.id.cut == cut_n))
        .ok_or_else(|| format!("cut {cut_n} not found on scene {scene_id}"))?;
    cut.package_needs_review = false;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        format!("Cut {cut_n} shot package marked reviewed."),
    )
}

fn arg_f64(args: &Value, key: &str) -> Result<f64, String> {
    args.get(key)
        .and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_i64().map(|n| n as f64))
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        })
        .ok_or_else(|| format!("Missing or invalid argument: {key}"))
}

fn arg_string_list(args: &Value, key: &str) -> Result<Vec<String>, String> {
    args.get(key)
        .and_then(|value| value.as_array())
        .ok_or_else(|| format!("Missing or invalid argument: {key}"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(|text| text.to_string())
                .ok_or_else(|| format!("Invalid {key}"))
        })
        .collect()
}

pub fn cmd_draft_intent(app: &HostCtx, args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let model = opt_str(args, "modelId").unwrap_or_default();
    let api_id = opt_str(args, "apiId");
    let brief = opt_str(args, "brief").unwrap_or_default();
    let mut state = import::load_state(Path::new(&root))?;
    let excerpt;
    let title;
    {
        let scene = state
            .scenes
            .iter()
            .find(|s| s.id == scene_id)
            .ok_or_else(|| format!("scene {scene_id} not found"))?;
        title = scene.title.clone();
        excerpt = scene
            .source_excerpt
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| {
                let mut bits = Vec::new();
                if let Some(loc) = &scene.location {
                    bits.push(format!("Location: {loc}"));
                }
                bits.extend(scene.action.iter().map(|a| format!("Action: {a}")));
                bits.extend(
                    scene
                        .dialogue
                        .iter()
                        .map(|d| format!("{}: {}", d.speaker, d.text)),
                );
                bits.join("\n")
            });
        if excerpt.trim().is_empty() {
            return Err("This scene has no script excerpt to draft intent from.".into());
        }
    }
    app.emit("director-progress", json!("Drafting scene intent…"));
    let (value, _) = llm::complete_json(
        app,
        api_id.as_deref(),
        &model,
        intent::intent_system_prompt(),
        &intent::intent_user_prompt(&title, &excerpt, &brief),
    )?;
    let drafted = intent::intent_from_json(&value)?;
    let scene = state
        .scenes
        .iter_mut()
        .find(|s| s.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    scene.intent = Some(drafted);
    scene.intent_approved = false;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        "Scene intent drafted. Review and approve before relying on it for sketches.",
    )
}

pub fn cmd_draft_sketches(app: &HostCtx, args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let model = opt_str(args, "modelId").unwrap_or_default();
    let api_id = opt_str(args, "apiId");
    let brief = opt_str(args, "brief").unwrap_or_default();
    let mut state = import::load_state(Path::new(&root))?;
    let keep_mouths = crate::workflow::recipe::recipe_requires_role(&state, "mouth_policy");
    let strategy_raw = opt_str(args, "strategy");
    let strategy = strategy_raw
        .as_deref()
        .and_then(SceneStrategy::parse_explicit)
        .or_else(|| {
            state
                .scenes
                .iter()
                .find(|s| s.id == scene_id)
                .and_then(|s| s.strategy)
        })
        .or_else(|| SceneStrategy::infer(&brief))
        .unwrap_or(SceneStrategy::CinematicBalanced);

    let scene = state
        .scenes
        .iter_mut()
        .find(|s| s.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    scene.strategy = Some(strategy);
    if scene.intent.as_ref().is_none_or(|i| i.is_empty()) {
        return Err("Draft scene intent first. Cut sketches need that planning layer.".into());
    }
    let existing = scene.cuts.len();
    let system = sketches::sketches_system_prompt(existing);
    let user = sketches::sketches_user_prompt(scene, strategy.label(), strategy.guidance(), &brief);
    app.emit(
        "director-progress",
        format!("Drafting cut sketches ({})…", strategy.label()),
    );
    let (value, reply) = llm::complete_json(app, api_id.as_deref(), &model, &system, &user)?;
    sketches::apply_llm_sketches(scene, &value)?;
    let mut verdict = validate::validate_scene_mix(scene);
    if verdict.is_reject() {
        app.emit(
            "director-progress",
            json!("Host rejected the mix — asking the model to rebalance…"),
        );
        if let Ok((retry, _)) = llm::complete_json_with_correction(
            app,
            api_id.as_deref(),
            &model,
            &system,
            &user,
            &sketches::mix_correction(scene),
            &reply,
        ) {
            sketches::apply_llm_sketches(scene, &retry)?;
            verdict = validate::validate_scene_mix(scene);
        }
    }
    if !keep_mouths {
        crate::workflow::recipe::strip_mouths(scene);
    }
    let imported = import::save_project(Path::new(&root), state)?;
    let note = if verdict.is_reject() {
        format!(
            "Cut sketches drafted but {} mix is still rejected. Change shot roles or enable mix override.\n{}",
            verdict.strategy.label(),
            verdict.messages.join("\n")
        )
    } else if verdict.status == validate::VerdictStatus::Warn {
        format!(
            "Cut sketches drafted with warnings for {}.\n{}",
            verdict.strategy.label(),
            verdict.messages.join("\n")
        )
    } else {
        format!(
            "Cut sketches drafted. {} mix accepted ({})",
            verdict.strategy.label(),
            verdict.mix.display()
        )
    };
    result_with_note(imported, note)
}

pub fn cmd_draft_look(app: &HostCtx, args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let model = opt_str(args, "modelId").unwrap_or_default();
    let api_id = opt_str(args, "apiId");
    let brief = opt_str(args, "brief").unwrap_or_default();
    let mut state = import::load_state(Path::new(&root))?;
    let characters: Vec<(String, String)> = state
        .characters
        .iter()
        .map(|c| {
            let mut bits = Vec::new();
            if c.identity_ready {
                bits.push(
                    "identity ready — host attaches model-sheet identity and body images when generating people stills"
                        .to_string(),
                );
            } else {
                bits.push("no model sheet yet".to_string());
            }
            bits.extend(c.notes.iter().cloned());
            (c.name.clone(), bits.join("; "))
        })
        .collect();
    let scene = state
        .scenes
        .iter()
        .find(|s| s.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    let user = look::look_user_prompt(scene, &characters, &brief);
    app.emit("director-progress", json!("Drafting scene look board…"));
    let (value, _) = llm::complete_json(
        app,
        api_id.as_deref(),
        &model,
        look::look_system_prompt(),
        &user,
    )?;
    let scene = state
        .scenes
        .iter_mut()
        .find(|s| s.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    look::apply_llm_look(scene, &value)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        "Scene look drafted. Attach or generate stills, then approve the ones you want to keep.",
    )
}

pub fn cmd_update_look_slot(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let slot_id = opt_str(args, "slotId").ok_or_else(|| "Missing slotId".to_string())?;
    let role =
        opt_str(args, "role").and_then(|r| crate::workflow::state::AssetRole::parse_look(&r));
    let title = provided_str(args, "title");
    let prompt = provided_str(args, "prompt");
    let negative =
        provided_str(args, "negativePrompt").or_else(|| provided_str(args, "negative_prompt"));
    let mut state = import::load_state(Path::new(&root))?;
    let scene = state
        .scenes
        .iter_mut()
        .find(|s| s.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    look::patch_slot(scene, &slot_id, role, title, prompt, negative)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, format!("Updated look slot `{slot_id}`."))
}

pub fn cmd_attach_look(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let slot_id = opt_str(args, "slotId").ok_or_else(|| "Missing slotId".to_string())?;
    let source = opt_str(args, "sourcePath")
        .or_else(|| opt_str(args, "path"))
        .ok_or_else(|| "Missing sourcePath".to_string())?;
    let mut state = import::load_state(Path::new(&root))?;
    let asset_id = look::attach_image(Path::new(&root), &mut state, scene_id, &slot_id, &source)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        format!("Attached look still `{asset_id}` as a draft. Approve it if you want to keep it."),
    )
}

pub fn cmd_approve_look(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let slot_id = opt_str(args, "slotId").ok_or_else(|| "Missing slotId".to_string())?;
    let asset_id = opt_str(args, "assetId");
    let mut state = import::load_state(Path::new(&root))?;
    look::set_asset_status(
        &mut state,
        scene_id,
        &slot_id,
        asset_id.as_deref(),
        AssetStatus::Approved,
    )?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, format!("Approved look still on `{slot_id}`."))
}

pub fn cmd_reject_look(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let slot_id = opt_str(args, "slotId").ok_or_else(|| "Missing slotId".to_string())?;
    let asset_id = opt_str(args, "assetId");
    let mut state = import::load_state(Path::new(&root))?;
    look::set_asset_status(
        &mut state,
        scene_id,
        &slot_id,
        asset_id.as_deref(),
        AssetStatus::Rejected,
    )?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, format!("Rejected look still on `{slot_id}`."))
}

pub fn cmd_delete_look(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let slot_id = opt_str(args, "slotId").ok_or_else(|| "Missing slotId".to_string())?;
    let asset_id = opt_str(args, "assetId").ok_or_else(|| "Missing assetId".to_string())?;
    let mut state = import::load_state(Path::new(&root))?;
    if state.asset(&asset_id).is_none() {
        let imported = import::save_project(Path::new(&root), state)?;
        return result_with_note(
            imported,
            format!("Removed missing look asset `{asset_id}` from the board."),
        );
    }
    let path = look::delete_image(Path::new(&root), &mut state, scene_id, &slot_id, &asset_id)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, format!("Deleted look still `{path}`."))
}

pub fn cmd_generate_look(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let slot_id = opt_str(args, "slotId").ok_or_else(|| "Missing slotId".to_string())?;
    let mut state = import::load_state(Path::new(&root))?;
    let role = opt_str(args, "role")
        .and_then(|value| crate::workflow::state::AssetRole::parse_look(&value));
    let title = provided_str(args, "title");
    let prompt = provided_str(args, "prompt");
    let negative =
        provided_str(args, "negativePrompt").or_else(|| provided_str(args, "negative_prompt"));
    if role.is_some() || title.is_some() || prompt.is_some() || negative.is_some() {
        let scene = state
            .scenes
            .iter_mut()
            .find(|scene| scene.id == scene_id)
            .ok_or_else(|| format!("scene {scene_id} not found"))?;
        look::patch_slot(scene, &slot_id, role, title, prompt, negative)?;
    }
    let (card, note) = look::prepare_generate(Path::new(&root), &mut state, scene_id, &slot_id)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_pending(imported, note, card)
}

pub fn cmd_generate_wardrobe_plate(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let mut state = import::load_state(Path::new(&root))?;
    let (card, note) = look::prepare_wardrobe_plate(Path::new(&root), &mut state, scene_id)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_pending(imported, note, card)
}

pub fn cmd_delete_wardrobe_plate(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let asset_id = arg_path(args, "assetId")?;
    let mut state = import::load_state(Path::new(&root))?;
    let path = look::delete_wardrobe_plate(Path::new(&root), &mut state, &asset_id)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, format!("Deleted wardrobe plate `{path}`."))
}

pub fn cmd_draft_start_frame_prompt(app: &HostCtx, args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_number = arg_u32(args, "cut")?;
    let model = opt_str(args, "modelId").unwrap_or_default();
    let api_id = opt_str(args, "apiId");
    let brief = opt_str(args, "brief").unwrap_or_default();
    let mut state = import::load_state(Path::new(&root))?;
    let scene = state
        .scene(scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    let cut = scene
        .cuts
        .iter()
        .find(|cut| cut.id.cut == cut_number)
        .ok_or_else(|| format!("cut {scene_id}.{cut_number} not found"))?;
    let user = start_frames::prompt_user_prompt(scene, cut, &brief);
    app.emit(
        "director-progress",
        json!(format!(
            "Drafting start-frame prompt for Scene {scene_id} Cut {cut_number}…"
        )),
    );
    let (mut value, reply) = llm::complete_json(
        app,
        api_id.as_deref(),
        &model,
        start_frames::prompt_system_prompt(),
        &user,
    )?;
    let issues = start_frames::prompt_issues(cut, &value)?;
    if !issues.is_empty() {
        app.emit(
            "director-progress",
            json!("Host rejected video-like wording — correcting the still prompt…"),
        );
        value = llm::complete_json_with_correction(
            app,
            api_id.as_deref(),
            &model,
            start_frames::prompt_system_prompt(),
            &user,
            &start_frames::prompt_correction(&issues),
            &reply,
        )?
        .0;
    }
    let cut = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .and_then(|scene| scene.cuts.iter_mut().find(|cut| cut.id.cut == cut_number))
        .ok_or_else(|| format!("cut {scene_id}.{cut_number} not found"))?;
    start_frames::apply_prompt(cut, &value)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        format!("Drafted start-frame prompt for Scene {scene_id} Cut {cut_number}."),
    )
}

pub fn cmd_save_start_frame_prompt(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_number = arg_u32(args, "cut")?;
    let prompt = opt_str(args, "prompt").ok_or_else(|| "Missing start-frame prompt".to_string())?;
    let negative = opt_str(args, "negativePrompt").unwrap_or_default();
    let mut state = import::load_state(Path::new(&root))?;
    let cut = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .and_then(|scene| scene.cuts.iter_mut().find(|cut| cut.id.cut == cut_number))
        .ok_or_else(|| format!("cut {scene_id}.{cut_number} not found"))?;
    start_frames::save_prompt(cut, &prompt, &negative)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        format!("Saved start-frame prompt for Scene {scene_id} Cut {cut_number}."),
    )
}

pub fn cmd_save_production_prompt(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_number = arg_u32(args, "cut")?;
    let prompt = opt_str(args, "prompt").ok_or_else(|| "Missing production prompt".to_string())?;
    let negative = provided_str(args, "negativePrompt").unwrap_or_default();
    let mut state = import::load_state(Path::new(&root))?;
    production::sync_from_planning(Path::new(&root), &mut state, scene_id, cut_number)?;
    let path = production::save_prompt(
        Path::new(&root),
        &mut state,
        scene_id,
        cut_number,
        &prompt,
        &negative,
    )?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        format!("Saved production prompt for Scene {scene_id} Cut {cut_number} in `{path}`."),
    )
}

pub fn cmd_refresh_production_handoff(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_number = arg_u32(args, "cut")?;
    let mut state = import::load_state(Path::new(&root))?;
    let path = production::sync_from_planning(Path::new(&root), &mut state, scene_id, cut_number)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        format!(
            "Refreshed host-owned source, speech, lane, and timing fields for Scene {scene_id} Cut {cut_number} in `{path}`."
        ),
    )
}

pub fn cmd_sync_production_notes(app: &HostCtx, args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let model = opt_str(args, "modelId").unwrap_or_default();
    let api_id = opt_str(args, "apiId");
    let brief = opt_str(args, "brief").unwrap_or_default();
    let requested_cut = args.get("cut").and_then(|value| {
        value
            .as_u64()
            .map(|number| number as u32)
            .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
    });
    let mut state = import::load_state(Path::new(&root))?;
    let cuts: Vec<u32> = match requested_cut {
        Some(cut) => vec![cut],
        None => state
            .scene(scene_id)
            .ok_or_else(|| format!("scene {scene_id} not found"))?
            .cuts
            .iter()
            .map(|cut| cut.id.cut)
            .collect(),
    };
    if cuts.is_empty() {
        return Err(format!("scene {scene_id} has no cuts"));
    }
    let user = production::draft_user_prompt(&state, scene_id, &cuts, &brief)?;
    app.emit(
        "director-progress",
        json!(format!(
            "Building full production video prompt{} for Scene {scene_id}…",
            if cuts.len() == 1 { "" } else { "s" }
        )),
    );
    let (value, _) = llm::complete_json(
        app,
        api_id.as_deref(),
        &model,
        production::draft_system_prompt(),
        &user,
    )?;
    let drafts = production::parse_drafts(&value, &cuts)?;
    let mut updated = Vec::new();
    for (cut, prompt, negative) in drafts {
        production::sync_from_planning(Path::new(&root), &mut state, scene_id, cut)?;
        production::save_prompt(
            Path::new(&root),
            &mut state,
            scene_id,
            cut,
            &prompt,
            &negative,
        )?;
        updated.push(cut);
    }
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        format!(
            "Built {} full production video prompt(s) from the script, cut sequence, and approved visual sources.",
            updated.len()
        ),
    )
}

pub fn cmd_prepare_render(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_number = arg_u32(args, "cut")?;
    let mut state = import::load_state(Path::new(&root))?;
    let (card, note) =
        production::prepare_render(Path::new(&root), &mut state, scene_id, cut_number)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_pending(imported, note, card)
}

pub fn cmd_set_visual_source(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_number = arg_u32(args, "cut")?;
    let strategy_text =
        opt_str(args, "strategy").ok_or_else(|| "Missing visual-source strategy".to_string())?;
    let strategy = VisualSourceStrategy::parse(&strategy_text).ok_or_else(|| {
        "strategy must be preproduction_start_frame, previous_selected_take_exit_frame, shared_reference, no_start_frame, or deferred_manual".to_string()
    })?;
    let predecessor = if strategy == VisualSourceStrategy::PreviousSelectedTakeExitFrame {
        Some(CutId::new(scene_id, arg_u32(args, "predecessorCut")?))
    } else {
        None
    };
    let asset_id = opt_str(args, "assetId");
    let mut state = import::load_state(Path::new(&root))?;
    sources::set_strategy(
        &mut state,
        scene_id,
        cut_number,
        strategy,
        predecessor,
        asset_id.as_deref(),
    )?;
    let ready = state
        .scene(scene_id)
        .and_then(|scene| scene.cuts.iter().find(|cut| cut.id.cut == cut_number))
        .is_some_and(|cut| cut.visual_source_ready());
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        format!(
            "Saved source strategy `{}` for Scene {scene_id} Cut {cut_number} ({}).{}",
            strategy.label(),
            if ready {
                "render source ready"
            } else {
                "planned; concrete source unresolved"
            },
            if strategy == VisualSourceStrategy::PreviousSelectedTakeExitFrame && !ready {
                " Bind a reviewed continuity frame before this cut can render."
            } else {
                ""
            }
        ),
    )
}

pub fn cmd_bind_continuity_frame(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_number = arg_u32(args, "cut")?;
    let frame_time = opt_f64(args, "frameTimeSeconds")?;
    let mut state = import::load_state(Path::new(&root))?;
    let path = sources::bind_selected_exit_frame(
        Path::new(&root),
        &mut state,
        scene_id,
        cut_number,
        frame_time,
    )?;
    let stored = state
        .scene(scene_id)
        .and_then(|scene| scene.cuts.iter().find(|cut| cut.id.cut == cut_number))
        .and_then(|cut| cut.visual_source.as_ref())
        .and_then(|source| source.frame_time_seconds);
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        format!(
            "Bound continuity frame at {}s to Scene {scene_id} Cut {cut_number}: `{path}`.",
            stored
                .map(|value| format!("{value:.3}"))
                .unwrap_or_else(|| "unknown".into())
        ),
    )
}

pub fn cmd_set_take_status(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_number = arg_u32(args, "cut")?;
    let take_id = opt_str(args, "takeId").ok_or_else(|| "Missing takeId".to_string())?;
    let status_text = opt_str(args, "status").ok_or_else(|| "Missing take status".to_string())?;
    let status = TakeStatus::parse(&status_text).ok_or_else(|| {
        "status must be unreviewed, selected, alternate, rejected, needs_edit, needs_rerender, or used_in_assembly".to_string()
    })?;
    let mut state = import::load_state(Path::new(&root))?;
    let invalidated = sources::set_take_status(&mut state, scene_id, cut_number, &take_id, status)?;
    let imported = import::save_project(Path::new(&root), state)?;
    let suffix = if invalidated == 0 {
        String::new()
    } else {
        format!(
            " {invalidated} dependent continuity source(s) are unbound and stay render-blocked until a frame is bound again."
        )
    };
    result_with_note(
        imported,
        format!(
            "Marked take `{take_id}` as `{}` for Scene {scene_id} Cut {cut_number}.{suffix}",
            status.label()
        ),
    )
}

pub fn cmd_delete_take(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_number = arg_u32(args, "cut")?;
    let take_id = opt_str(args, "takeId").ok_or_else(|| "Missing takeId".to_string())?;
    let mut state = import::load_state(Path::new(&root))?;
    let path = sources::delete_take(Path::new(&root), &mut state, scene_id, cut_number, &take_id)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        format!("Deleted video take `{path}` from Scene {scene_id} Cut {cut_number}."),
    )
}

pub fn cmd_prepare_assemble(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let mut state = import::load_state(Path::new(&root))?;
    let scene = state
        .scene(scene_id)
        .ok_or_else(|| format!("Scene {scene_id} not found"))?;
    if scene.cuts.is_empty() {
        return Err(format!("Scene {scene_id} has no cuts to assemble"));
    }
    let mut cuts = scene.cuts.iter().collect::<Vec<_>>();
    cuts.sort_by_key(|cut| cut.id.cut);
    let cut_count = cuts.len();
    let mut missing = Vec::new();
    let mut selected = Vec::new();
    let mut take_ids = Vec::new();
    for cut in cuts {
        let take = cut.selected_take.as_ref().and_then(|selected| {
            cut.takes
                .iter()
                .find(|take| take.id == *selected || take.path == *selected)
        });
        if let Some(take) = take {
            take_ids.push(take.id.clone());
            selected.push(json!({ "id": cut.id.cut, "video": take.path }));
        } else {
            missing.push(cut.id.cut);
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "Select one take for every cut before assembly. Missing: Cut {}.",
            missing
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", Cut ")
        ));
    }
    let output_path = (1..=1000)
        .map(|version| {
            if version == 1 {
                format!("Final/Scene{scene_id}_master.mp4")
            } else {
                format!("Final/Scene{scene_id}_master_v{version}.mp4")
            }
        })
        .find(|relative| !Path::new(&root).join(relative).exists())
        .ok_or_else(|| "Could not allocate a master output version".to_string())?;
    let mut inputs = Map::new();
    inputs.insert("production_path".into(), Value::String("prod.md".into()));
    inputs.insert("output_path".into(), Value::String(output_path.clone()));
    // Each entry is a cut, while the legacy assembler's timing metadata is
    // scene-based. Preserve complete reviewed takes until cut-level metadata lands.
    inputs.insert("trim".into(), Value::String("full".into()));
    inputs.insert("caption_mode".into(), Value::String("none".into()));
    inputs.insert("scenes".into(), Value::Array(selected));
    let action = DirectorAction {
        action: ActionKind::RunSpecialist,
        specialist: Some("assemble".into()),
        inputs,
        message: None,
        op: None,
        path: None,
    };
    let validated =
        validate_action(action.clone(), Some(&root)).map_err(|error| error.to_string())?;
    let mut card =
        format!("Assemble Scene {scene_id}\nCuts: {cut_count}\nMaster: `{output_path}`\n\n");
    card.push_str(&format_approval_request(&action, &validated));
    crate::workflow::identity::assign(&mut state);
    let scene_stable = state
        .scene(scene_id)
        .map(|scene| scene.stable_id.clone())
        .unwrap_or_default();
    let job_id = state.next_job_id();
    let mut job = crate::workflow::state::JobRecord::open(job_id, "assemble");
    job.output_path = Some(output_path.clone());
    job.raw_provider_id = Some(format!("scene_{scene_id}"));
    job.input_asset_ids = take_ids;
    if !scene_stable.is_empty() {
        job.scope = Some(crate::workflow::state::JobScope {
            kind: "scene".into(),
            id: scene_stable,
        });
    }
    state.jobs.push(job);
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_pending(
        imported,
        format!("Prepared Scene {scene_id} assembly from {cut_count} selected takes."),
        card,
    )
}

pub fn cmd_add_alternate_shot(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let mut state = import::load_state(Path::new(&root))?;
    let stable_id = identity::add_alternate_shot(&mut state, scene_id)?;
    let imported = import::save_project(Path::new(&root), state)?;
    let cut = imported
        .state
        .scene(scene_id)
        .and_then(|scene| {
            scene
                .cuts
                .iter()
                .find(|cut| cut.stable_id == stable_id)
        })
        .ok_or_else(|| format!("alternate shot `{stable_id}` was not saved"))?;
    if cut.production_note_path.is_some() || cut.visual_source.is_some() {
        return Err("alternate shot must stay incomplete until a source and note are chosen".into());
    }
    if cut.sketch.is_none() {
        return Err("alternate shot is missing an editable sketch".into());
    }
    let label = cut.id.cut;
    result_with_note(
        imported,
        format!("Added alternate angle Cut {label} as `{stable_id}`. Existing shot ids were left unchanged."),
    )
}

pub fn cmd_mark_cut_version(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let mut state = import::load_state(Path::new(&root))?;
    let id = post::mark_cut_version(&mut state, scene_id)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        format!("Marked selected takes as cut version `{id}` for Scene {scene_id}."),
    )
}

pub fn cmd_set_cut_version_stage(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let version_id = opt_str(args, "versionId").ok_or_else(|| "Missing versionId".to_string())?;
    let stage = opt_str(args, "stage")
        .as_deref()
        .and_then(post::CutVersionStage::parse)
        .ok_or_else(|| "Unknown cut version stage".to_string())?;
    let mut state = import::load_state(Path::new(&root))?;
    post::set_version_stage(&mut state, scene_id, &version_id, stage)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        format!("Set `{version_id}` to {}.", stage.label()),
    )
}

pub fn cmd_lock_picture(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let version_id = opt_str(args, "versionId").ok_or_else(|| "Missing versionId".to_string())?;
    let mut state = import::load_state(Path::new(&root))?;
    post::lock_picture(&mut state, scene_id, &version_id)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        format!("Picture locked `{version_id}` for Scene {scene_id}."),
    )
}

pub fn cmd_set_post_status(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let field = opt_str(args, "field")
        .as_deref()
        .and_then(post::PostField::parse)
        .ok_or_else(|| "Unknown post field".to_string())?;
    let status = opt_str(args, "status")
        .as_deref()
        .and_then(post::PostStatus::parse)
        .ok_or_else(|| "Unknown post status".to_string())?;
    let mut state = import::load_state(Path::new(&root))?;
    post::set_status(&mut state, scene_id, field, status)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        format!(
            "Updated Scene {scene_id} post status to {}.",
            status.label()
        ),
    )
}

pub fn cmd_set_editorial_path(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let path = opt_str(args, "path")
        .as_deref()
        .and_then(post::EditorialPath::parse)
        .ok_or_else(|| "Unknown editorial path".to_string())?;
    let note = opt_str(args, "note");
    let mut state = import::load_state(Path::new(&root))?;
    post::set_editorial(&mut state, scene_id, path, note)?;
    let imported = import::save_project(Path::new(&root), state)?;
    let decision = match path {
        post::EditorialPath::HostFfmpeg => "host ffmpeg",
        post::EditorialPath::ExternalNle => "an external NLE",
    };
    result_with_note(
        imported,
        format!("Recorded Scene {scene_id} editorial as {decision}."),
    )
}

pub fn cmd_check_deliverable(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let deliverable_id =
        opt_str(args, "deliverableId").ok_or_else(|| "Missing deliverableId".to_string())?;
    let mut state = import::load_state(Path::new(&root))?;
    let check = post::check_deliverable(&mut state, scene_id, &deliverable_id)?;
    let imported = import::save_project(Path::new(&root), state)?;
    let verdict = if check.passed { "passed" } else { "failed" };
    result_with_note(
        imported,
        format!(
            "Deliverable `{deliverable_id}` {verdict}. {}",
            check.messages.join(" ")
        ),
    )
}

pub fn cmd_generate_start_frame(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_number = arg_u32(args, "cut")?;
    let mut state = import::load_state(Path::new(&root))?;
    let (card, note) =
        start_frames::prepare_generate(Path::new(&root), &mut state, scene_id, cut_number)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_pending(imported, note, card)
}

pub fn cmd_edit_start_frame(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_number = arg_u32(args, "cut")?;
    let asset_id = opt_str(args, "assetId").ok_or_else(|| "Missing assetId".to_string())?;
    let brief = opt_str(args, "brief").ok_or_else(|| "Missing edit brief".to_string())?;
    let mut state = import::load_state(Path::new(&root))?;
    let (card, note) = start_frames::prepare_edit(
        Path::new(&root),
        &mut state,
        scene_id,
        cut_number,
        &asset_id,
        &brief,
    )?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_pending(imported, note, card)
}

pub fn cmd_attach_start_frame(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_number = arg_u32(args, "cut")?;
    let source = opt_str(args, "sourcePath")
        .or_else(|| opt_str(args, "path"))
        .ok_or_else(|| "Missing sourcePath".to_string())?;
    let mut state = import::load_state(Path::new(&root))?;
    let asset_id =
        start_frames::attach_image(Path::new(&root), &mut state, scene_id, cut_number, &source)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        format!("Attached start-frame draft `{asset_id}` to Scene {scene_id} Cut {cut_number}."),
    )
}

pub fn cmd_approve_start_frame(args: &Value) -> Result<Value, String> {
    set_start_frame_status(args, AssetStatus::Approved, "Approved")
}

pub fn cmd_reject_start_frame(args: &Value) -> Result<Value, String> {
    set_start_frame_status(args, AssetStatus::Rejected, "Rejected")
}

fn set_start_frame_status(args: &Value, status: AssetStatus, verb: &str) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_number = arg_u32(args, "cut")?;
    let asset_id = opt_str(args, "assetId").ok_or_else(|| "Missing assetId".to_string())?;
    let mut state = import::load_state(Path::new(&root))?;
    start_frames::set_status(&mut state, scene_id, cut_number, &asset_id, status)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        format!("{verb} start frame for Scene {scene_id} Cut {cut_number}."),
    )
}

pub fn cmd_delete_start_frame(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_number = arg_u32(args, "cut")?;
    let asset_id = opt_str(args, "assetId").ok_or_else(|| "Missing assetId".to_string())?;
    let mut state = import::load_state(Path::new(&root))?;
    if state.asset(&asset_id).is_none() {
        let imported = import::save_project(Path::new(&root), state)?;
        return result_with_note(
            imported,
            format!("Removed missing cut image `{asset_id}` from the registry."),
        );
    }
    let path = start_frames::delete_image(
        Path::new(&root),
        &mut state,
        scene_id,
        cut_number,
        &asset_id,
    )?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, format!("Deleted cut image `{path}`."))
}

/// Director chat entry for V2 planning. Never writes prod.md.
pub fn draft_from_director(
    app: &HostCtx,
    project_root: &str,
    scene_id: u32,
    model_id: &str,
    api_id: &str,
    brief: &str,
    kind: PlanningKind,
    strategy: Option<&str>,
) -> Result<String, String> {
    let mut args = json!({
        "projectRoot": project_root,
        "sceneId": scene_id,
        "modelId": model_id,
        "apiId": api_id,
        "brief": brief,
    });
    if let Some(strategy) = strategy.filter(|s| !s.is_empty()) {
        args["strategy"] = json!(strategy);
    }
    let value = match kind {
        PlanningKind::SceneIntent => cmd_draft_intent(app, &args)?,
        PlanningKind::CutSketches => cmd_draft_sketches(app, &args)?,
        PlanningKind::SceneLook => cmd_draft_look(app, &args)?,
        PlanningKind::ProductionNotes => {
            return Err(
                "Production notes stay on scene_plan; this runner is scene intent / look / sketches."
                    .into(),
            );
        }
    };
    Ok(format_planning_reply(kind, scene_id, &value))
}

fn format_planning_reply(kind: PlanningKind, scene_id: u32, value: &Value) -> String {
    let note = value
        .get("note")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let scene = value
        .get("state")
        .and_then(|s| s.get("scenes"))
        .and_then(|s| s.as_array())
        .and_then(|scenes| {
            scenes
                .iter()
                .find(|scene| scene.get("id").and_then(|v| v.as_u64()) == Some(u64::from(scene_id)))
        });
    let title = scene
        .and_then(|s| s.get("title"))
        .and_then(|v| v.as_str())
        .unwrap_or("Untitled");
    match kind {
        PlanningKind::SceneIntent => {
            let intent = scene.and_then(|s| s.get("intent"));
            let mut lines = vec![
                format!("Scene {scene_id} — {title} · scene intent (draft)"),
                String::new(),
                "This is scene intent, not production notes. Existing prod.md cuts were left unchanged.".to_string(),
            ];
            if !note.is_empty() {
                lines.push(String::new());
                lines.push(note.to_string());
            }
            push_intent_fields(&mut lines, intent);
            lines.join("\n")
        }
        PlanningKind::CutSketches => {
            let mut lines = vec![format!("Scene {scene_id} — {title} · cut sketches")];
            if !note.is_empty() {
                lines.push(String::new());
                lines.push(note.to_string());
            }
            if let Some(cuts) = scene.and_then(|s| s.get("cuts")).and_then(|v| v.as_array()) {
                lines.push(String::new());
                for cut in cuts.iter().take(16) {
                    let n = cut
                        .get("id")
                        .and_then(|id| id.get("cut"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let sketch = cut.get("sketch");
                    let cut_title = sketch
                        .and_then(|s| s.get("title"))
                        .and_then(|v| v.as_str())
                        .or_else(|| cut.get("title").and_then(|v| v.as_str()))
                        .unwrap_or("");
                    let role = sketch
                        .and_then(|s| s.get("shot_role"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unspecified");
                    let mouth = sketch
                        .and_then(|s| s.get("mouth_policy"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unspecified");
                    lines.push(format!("- Cut {n}: {cut_title} ({role}, {mouth})"));
                }
            }
            lines.join("\n")
        }
        PlanningKind::SceneLook => {
            let board = scene.and_then(|s| s.get("look_board"));
            look::format_look_reply(scene_id, title, note, None) + &look_slots_from_json(board)
        }
        PlanningKind::ProductionNotes => note.to_string(),
    }
}

fn look_slots_from_json(board: Option<&Value>) -> String {
    let Some(slots) = board
        .and_then(|b| b.get("slots"))
        .and_then(|v| v.as_array())
    else {
        return String::new();
    };
    let mut lines = String::new();
    for slot in slots {
        let title = slot
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Look slot");
        let role = slot.get("role").and_then(|v| v.as_str()).unwrap_or("");
        let prompt = slot.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
        lines.push_str(&format!("\n\n{title} ({role})\n{prompt}"));
    }
    lines
}

fn push_intent_fields(lines: &mut Vec<String>, intent: Option<&Value>) {
    let Some(intent) = intent else {
        return;
    };
    fn one(lines: &mut Vec<String>, label: &str, value: Option<&Value>) {
        if let Some(text) = value
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            lines.push(String::new());
            lines.push(format!("{label}"));
            lines.push(text.to_string());
        }
    }
    fn list(lines: &mut Vec<String>, label: &str, value: Option<&Value>) {
        let Some(Value::Array(items)) = value else {
            return;
        };
        let bits: Vec<&str> = items
            .iter()
            .filter_map(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if bits.is_empty() {
            return;
        }
        lines.push(String::new());
        lines.push(label.to_string());
        for bit in bits {
            lines.push(format!("- {bit}"));
        }
    }
    one(lines, "Narrative purpose", intent.get("narrative_purpose"));
    one(lines, "Emotional purpose", intent.get("emotional_purpose"));
    list(lines, "Character focus", intent.get("character_focus"));
    one(lines, "Visual contrast", intent.get("visual_contrast"));
    list(lines, "Important props", intent.get("important_props"));
    list(
        lines,
        "Continuity constraints",
        intent.get("continuity_constraints"),
    );
    list(
        lines,
        "Sound and transition",
        intent.get("sound_and_transition"),
    );
    one(
        lines,
        "Audience must know",
        intent.get("audience_must_know"),
    );
}
