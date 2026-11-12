//! V2 workflow: typed project state and import from existing folders.
pub mod animatic;
pub mod audio;
pub mod dialogue_audio;
pub mod audio_edit;
pub mod image_edit;
pub mod performance;
pub mod performance_import;
pub mod breakdown;
pub mod captions;
pub mod characters;
pub mod character_wardrobe;
pub mod cost;
pub mod identity;
pub mod import;
pub mod intent;
pub mod llm;
pub mod look;
pub mod post;
pub mod production;
pub mod qc;
pub mod recipe;
pub mod assembly_jobs;
pub mod render_jobs;
pub mod report;
pub mod route;
pub mod scope;
pub mod sketches;
pub mod sources;
pub mod spend;
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
    format_approval_request_with_route, validate_action, ActionKind, DirectorAction,
};
use crate::scene_plan::handoff::CutId;
use crate::scene_plan::strategy::SceneStrategy;
use crate::workflow::state::{AssetStatus, MouthPolicy, ProjectBudget, ShotRole, TakeStatus};
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
    let edited = if let Some(source) = opt_str(args, "editedScriptPath") {
        if !persist {
            return Err("Edited script refresh requires persisted workflow state.".into());
        }
        import::validate_saved_script(Path::new(&root), &source)?;
        let previous = import::load_previous(Path::new(&root));
        let bytes = import::saved_script_bytes(Path::new(&root), &source)?;
        Some((source, previous, bytes))
    } else {
        None
    };
    let imported = import_project(Path::new(&root), ImportOptions { persist })?;
    let mut result = result_json(imported)?;
    if let Some((source, previous, bytes)) = edited {
        use sha2::{Digest, Sha256};
        let state = &result["state"];
        let scenes = state["scenes"].as_array().ok_or("import returned no scenes")?;
        let old = previous.as_ref().map(|p| &p.scenes);
        let mut affected: Vec<u32> = scenes.iter().filter(|scene| scene["source_path"] == source)
            .filter(|scene| {
                let id = scene["id"].as_u64();
                let prior = old.and_then(|items| items.iter().find(|item| Some(item.id as u64) == id));
                prior.is_none_or(|prior| prior.source_excerpt != scene["source_excerpt"].as_str().map(str::to_string)
                    || prior.source_missing != scene["source_missing"].as_bool().unwrap_or(false))
            })
            .filter_map(|scene| scene["id"].as_u64().and_then(|id| u32::try_from(id).ok()))
            .collect();
        if let Some(old) = old {
            affected.extend(old.iter().filter(|scene| scene.source_path.as_deref() == Some(source.as_str()))
                .filter(|scene| !scenes.iter().any(|fresh| fresh["id"] == scene.id && fresh["source_path"] == source && !fresh["source_missing"].as_bool().unwrap_or(false)))
                .map(|scene| scene.id));
        }
        affected.sort_unstable();
        affected.dedup();
        let digest = Sha256::digest(&bytes).iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        result["editedScript"] = json!({
            "path": source, "sourceSha256": digest,
            "stateRevision": state["state_revision"], "affectedSceneIds": affected,
        });
    }
    Ok(result)
}

/// Host command: load persisted V2 state, or scan without writing.
pub fn cmd_status(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let refresh = arg_bool(args, "refresh", false);
    let jobs_only = args.get("jobsOnly").and_then(|v| v.as_bool()).unwrap_or(false);
    if jobs_only {
        let state = render_jobs::read_persisted(Path::new(&root))?;
        return Ok(json!({
            "state": {
                "jobs": state.jobs,
            }
        }));
    }
    let imported = if refresh {
        import_project(Path::new(&root), ImportOptions { persist: false })?
    } else {
        load_or_import(Path::new(&root))?
    };
    result_json(imported)
}

pub fn cmd_unsettled_spend(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    serde_json::to_value(spend::unsettled_jobs(Path::new(&root))?)
        .map_err(|error| error.to_string())
}

pub fn cmd_reconcile_spend(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let required = |key| opt_str(args, key).ok_or_else(|| format!("Missing {key}"));
    let actual = args
        .get("actualMinor")
        .and_then(Value::as_i64)
        .ok_or("actualMinor must be integer minor units")?;
    spend::reconcile_job(
        Path::new(&root),
        &required("jobId")?,
        actual,
        &required("currency")?,
        &required("ledgerRevision")?,
        &required("providerOutcome")?,
        &required("evidenceReference")?,
        &required("reviewNote")?,
        &required("acknowledgment")?,
    )?;
    let state = import::load_state(Path::new(&root))?;
    Ok(
        json!({ "state": state, "note": "Manual evidence review recorded; provider verification was not performed." }),
    )
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

pub fn cmd_save_character_reference_selection(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let character_id = args.get("characterId").and_then(Value::as_str)
        .filter(|id| !id.is_empty()).ok_or("associationError: Missing characterId")?;
    let asset_ids: Vec<String> = serde_json::from_value(
        args.get("assetIds").cloned().ok_or("associationError: Missing assetIds")?
    ).map_err(|error| format!("associationError: invalid assetIds: {error}"))?;
    characters::save_character_reference_selection(Path::new(&root), character_id, asset_ids)
        .and_then(|character| serde_json::to_value(character).map_err(|error| error.to_string()))
        .map_err(|error| format!("associationError: {error}"))
}

pub fn cmd_delete_character_reference(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let character_id = opt_str(args, "characterId").ok_or("Missing characterId")?;
    let asset_id = opt_str(args, "assetId").ok_or("Missing assetId")?;
    let state = characters::delete_character_reference(Path::new(&root), &character_id, &asset_id)?;
    Ok(json!({
        "state": state,
        "note": "Moved uploaded character reference to recoverable project trash."
    }))
}

pub fn cmd_delete_model_sheet_photo(args: &Value) -> Result<Value, String> {
    delete_model_sheet_photo_with_state_saver(args, import::save_project)
}

fn delete_model_sheet_photo_with_state_saver(
    args: &Value,
    save: impl FnOnce(&Path, ProjectState) -> Result<ImportResult, String>,
) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let character_id = opt_str(args, "characterId").ok_or("Missing characterId")?;
    let asset_id = opt_str(args, "assetId").ok_or("Missing assetId")?;
    let mut state = import::load_state(Path::new(&root))?;
    let character_index = state.characters.iter().position(|character| character.id == character_id)
        .ok_or_else(|| format!("character `{character_id}` not found"))?;
    let character = &state.characters[character_index];
    if !character.asset_ids.iter().any(|id| id == &asset_id) {
        return Err("photo is not registered on this character's model sheet".into());
    }
    let asset = state.assets.iter().find(|asset| {
        asset.id == asset_id
            && asset.character_id.as_deref() == Some(character_id.as_str())
            && matches!(asset.role, AssetRole::IdentityReference | AssetRole::BodyReference | AssetRole::ExpressionReference)
            && asset.notes.as_deref() != Some("uploaded reference image")
    }).ok_or("saved model-sheet photo not found")?;
    let photo_path = asset.path.clone();
    let manifest_path = character.model_sheet_manifest_path.clone()
        .ok_or("character has no model-sheet manifest")?;
    let original_state = state.clone();
    let receipt = crate::specialists::delete_model_sheet_photo(
        Path::new(&root),
        &manifest_path,
        &photo_path,
    )?;
    state.assets.retain(|asset| asset.id != asset_id);
    state.characters[character_index].asset_ids.retain(|id| id != &asset_id);
    state.characters[character_index].reference_asset_ids.retain(|id| id != &asset_id);
    for job in &mut state.jobs {
        job.input_asset_ids.retain(|id| id != &asset_id);
        job.output_asset_ids.retain(|id| id != &asset_id);
    }
    let imported = match save(Path::new(&root), state) {
        Ok(imported) => imported,
        Err(error) => {
            let mut rollback_errors = Vec::new();
            if let Err(rollback) = crate::specialists::restore_deleted_model_sheet_photo(receipt) {
                rollback_errors.push(rollback);
            }
            let state_restore = match import::load_state(Path::new(&root)) {
                Ok(current) if current == original_state => Ok(()),
                _ => import::persist_state(Path::new(&root), &original_state).map(|_| ()),
            };
            if let Err(rollback) = state_restore {
                rollback_errors.push(format!("could not restore workflow state: {rollback}"));
            }
            return if rollback_errors.is_empty() {
                Err(error)
            } else {
                Err(format!("{error}; deletion rollback failed: {}", rollback_errors.join("; ")))
            };
        }
    };
    crate::specialists::finalize_deleted_model_sheet_photo(Path::new(&root), &receipt);
    result_with_note(imported, format!("Moved model-sheet photo `{photo_path}` to project trash."))
}

#[cfg(test)]
#[test]
fn model_sheet_delete_state_failure_rollback() {
    use std::fs;

    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("Characters/Alice")).unwrap();
    fs::write(
        root.path().join("Characters/Alice/character.yaml"),
        "name: Alice\n",
    )
    .unwrap();
    let request = json!({
        "output_path":"Characters/Alice/model_sheet",
        "character_files":["Characters/Alice/character.yaml"],
        "character_slug":"Alice",
        "identity_text":"Disposable rollback fixture",
        "reference_images":[]
    });
    crate::specialists::prepare_model_sheet_manifest(
        request.as_object().unwrap(),
        None,
        root.path().to_str(),
    )
    .unwrap();

    let manifest_path = root
        .path()
        .join("Characters/Alice/model_sheet/model_sheet_manifest.yaml");
    let mut manifest: Value = serde_yaml_ng::from_str(
        &fs::read_to_string(&manifest_path).unwrap(),
    )
    .unwrap();
    let photo = Path::new(manifest["shots"][3]["path"].as_str().unwrap()).to_path_buf();
    let photo_bytes = b"disposable state rollback photo bytes";
    fs::write(&photo, photo_bytes).unwrap();
    manifest["shots"][3]["status"] = json!("saved");
    manifest["shots"][3]["prediction_id"] = json!("selected-prediction");
    fs::write(&manifest_path, serde_yaml_ng::to_string(&manifest).unwrap()).unwrap();
    let photo_relative = photo
        .strip_prefix(root.path())
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");

    let mut state = ProjectState::default();
    state.profile = Some(Default::default());
    state.characters = serde_json::from_value(json!([{
        "id":"alice", "name":"Alice", "dir":"Characters/Alice",
        "profile_path":"Characters/Alice/character.yaml",
        "model_sheet_manifest_path":"Characters/Alice/model_sheet/model_sheet_manifest.yaml",
        "identity_ready":true, "voice_ready":false,
        "asset_ids":["selected-photo"], "reference_asset_ids":["selected-photo"]
    }]))
    .unwrap();
    state.assets = serde_json::from_value(json!([{
        "id":"selected-photo", "path":photo_relative, "role":"body_reference",
        "status":"present", "character_id":"alice", "notes":"standing_front"
    }]))
    .unwrap();
    import::persist_state(root.path(), &state).unwrap();

    let state_path = root.path().join(STATE_REL);
    let original_state_bytes = fs::read(&state_path).unwrap();
    let original_manifest_bytes = fs::read(&manifest_path).unwrap();
    let error = delete_model_sheet_photo_with_state_saver(
        &json!({
            "projectRoot":root.path(), "characterId":"alice", "assetId":"selected-photo"
        }),
        |_root, changed| {
            assert!(changed.assets.iter().all(|asset| asset.id != "selected-photo"));
            Err("injected state save failure".into())
        },
    )
    .unwrap_err();

    assert_eq!(error, "injected state save failure");
    assert_eq!(fs::read(&photo).unwrap(), photo_bytes);
    assert_eq!(fs::read(&manifest_path).unwrap(), original_manifest_bytes);
    assert_eq!(fs::read(&state_path).unwrap(), original_state_bytes);
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

pub fn cmd_set_max_cuts(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let max_cuts = args.get("maxCuts").and_then(Value::as_u64)
        .filter(|count| (1..=12).contains(count))
        .ok_or("Maximum cuts must be between 1 and 12.")? as u32;
    let mut state = import::load_state(Path::new(&root))?;
    let scene = state.scenes.iter_mut().find(|s| s.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    scene.max_cuts = Some(max_cuts);
    let existing = scene.cuts.len();
    let imported = import::save_project(Path::new(&root), state)?;
    let note = if existing > max_cuts as usize {
        format!("Maximum set to {max_cuts}. This scene already has {existing} cuts; remove unwanted cuts before drafting again. Existing work was not changed.")
    } else {
        format!("Maximum set to {max_cuts} cuts for this scene.")
    };
    result_with_note(imported, note)
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

fn opt_i64_field(value: &Value, key: &str) -> Result<Option<i64>, String> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => number
            .as_i64()
            .map(Some)
            .ok_or_else(|| format!("{key} must be an integer number of minor units")),
        Some(Value::String(text)) if text.trim().is_empty() => Ok(None),
        Some(Value::String(text)) => text
            .trim()
            .parse::<i64>()
            .map(Some)
            .map_err(|_| format!("{key} must be an integer number of minor units")),
        _ => Err(format!("{key} must be an integer number of minor units")),
    }
}

/// Parse and validate a budget object from the command payload. `None` (or
/// `null`) clears the budget — it does not mean zero. A warning level above the
/// limit, or any negative amount, is rejected outright.
pub fn parse_budget(value: Option<&Value>) -> Result<Option<ProjectBudget>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let currency = value
                .get("currency")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .unwrap_or("USD")
                .to_string();
            let limit_minor = opt_i64_field(value, "limit_minor")?;
            let warning_at_minor = opt_i64_field(value, "warning_at_minor")?;
            let hard_stop = value
                .get("hard_stop")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let policy = match value.get("policy") {
                None | Some(Value::Null) => None,
                Some(value) => Some(serde_json::from_value::<state::BudgetPolicy>(value.clone())
                    .map_err(|_| "Budget policy must be strict_hard_stop, approval_required, or warn_only".to_string())?),
            };
            if let Some(limit) = limit_minor {
                if limit < 0 {
                    return Err("Budget limit cannot be negative.".into());
                }
            }
            if let Some(warning) = warning_at_minor {
                if warning < 0 {
                    return Err("Budget warning level cannot be negative.".into());
                }
            }
            if let (Some(limit), Some(warning)) = (limit_minor, warning_at_minor) {
                if warning > limit {
                    return Err("Budget warning level cannot exceed the limit.".into());
                }
            }
            Ok(Some(ProjectBudget {
                currency,
                limit_minor,
                warning_at_minor,
                hard_stop,
                policy,
            }))
        }
    }
}

pub fn cmd_set_budget(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let budget = parse_budget(args.get("budget"))?;
    let mut state = import::load_state(Path::new(&root))?;
    state.budget = budget;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, "Project budget saved.")
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
    let scene_id = match args.get("sceneId") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            value
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .or_else(|| value.as_str().and_then(|text| text.parse::<u32>().ok()))
                .ok_or_else(|| "sceneId must be a valid u32 integer or numeric string".to_string())?,
        ),
    };
    let mut state = import::load_state(Path::new(&root))?;
    if let Some(id) = scene_id {
        if id == 0 || !state.scenes.iter().any(|scene| scene.id == id) {
            return Err(format!("scene {id} not found"));
        }
    }
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
    // Card identity survives display-number changes. Only legacy callers that
    // omit shotId may select by number; an invalid supplied ID never falls back.
    let shot_id = match args.get("shotId") {
        None => None,
        Some(value) => Some(
            value
                .as_str()
                .filter(|id| !id.trim().is_empty())
                .ok_or_else(|| "Invalid shotId".to_string())?,
        ),
    };
    let legacy_cut = if shot_id.is_none() {
        Some(arg_u32(args, "cut")?)
    } else {
        None
    };
    let sketch = args
        .get("sketch")
        .ok_or_else(|| "Missing sketch".to_string())?;
    let mut state = import::load_state(Path::new(&root))?;
    let keep_mouths = crate::workflow::recipe::recipe_requires_role(&state, "mouth_policy");
    let cut = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .and_then(|scene| scene.cuts.iter_mut().find(|cut| match shot_id {
            Some(id) => cut.stable_id == id,
            None => Some(cut.id.cut) == legacy_cut,
        }))
        .ok_or_else(|| format!("Card cut not found on scene {scene_id}"))?;
    let cut_n = cut.id.cut;
    sketches::replace_sketch(cut, sketch)?;
    if !keep_mouths {
        crate::workflow::recipe::clear_mouth(cut);
    }
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, format!("Saved sketch for cut {cut_n}."))
}

pub fn cmd_resolve_script_dialogue(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let shot_id = args.get("shotId").and_then(Value::as_str)
        .filter(|id| !id.is_empty()).ok_or("Missing shotId")?;
    let choice = args.get("choice").and_then(Value::as_str).ok_or("Missing choice")?;
    if choice != "keep" {
        return Err("Choice must be keep; revise dialogue in the sketch editor before resolving".into());
    }
    let mut state = import::load_state(Path::new(&root))?;
    let scene = state.scenes.iter_mut().find(|scene| scene.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    let cut = scene.cuts.iter_mut().find(|cut| cut.stable_id == shot_id)
        .ok_or("Shot identity no longer matches; refresh Studio")?;
    let review = cut.script_dialogue_review.as_ref().ok_or("No script dialogue review pending")?;
    if review.current != scene.dialogue {
        return Err("Script changed since this review; refresh Studio".into());
    }
    cut.script_dialogue_kept = Some(scene.dialogue.clone());
    cut.script_dialogue_review = None;
    if scene.cuts.iter().all(|cut| cut.script_dialogue_review.is_none()) {
        scene.script_dialogue_review = None;
    }
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, format!("Script dialogue for shot {shot_id}: {choice}. Authored cut and production-note dialogue was not overwritten."))
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
    let mut state = import::load_for_assembly(Path::new(&root))?;
    let scene = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    crate::workflow::animatic::reorder(scene, &shot_ids)?;
    let imported = import::save_for_assembly(Path::new(&root), state)?;
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

pub fn cmd_approve_rendered_sequence(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let job_id = opt_str(args, "jobId").ok_or("Missing rendered assembly job id")?;
    let mut state = import::load_for_assembly(Path::new(&root))?;
    assembly_jobs::approve_review(Path::new(&root), &mut state, &job_id)?;
    let imported = import::save_for_assembly(Path::new(&root), state)?;
    result_with_note(imported, "Rendered rough cut approved. Sketch approval is separate; changes to selected takes or timing clear this review.")
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
    result_with_note(imported, "Sketch animatic approved. Rendered rough-cut review is separate.")
}

pub fn cmd_attach_scratch_audio(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let shot_id = opt_str(args, "shotId").ok_or_else(|| "Missing shot id".to_string())?;
    let source_path = args
        .get("sourcePath")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Missing or invalid argument: sourcePath".to_string())?;
    let source = crate::session::authorize(source_path, crate::project_path::PathIntent::Read)?;
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
        &source,
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

pub fn cmd_set_shot_trim(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let shot_id = opt_str(args, "shotId").ok_or_else(|| "Missing shot id".to_string())?;
    let trim_in = opt_f64(args, "trimInSeconds")?;
    let trim_out = opt_f64(args, "trimOutSeconds")?;
    let source_end = opt_f64(args, "sourceEndSeconds")?;
    if source_end.is_some() && (opt_str(args, "expectedTakeId").is_none() || trim_out.unwrap_or(0.0) != 0.0) {
        return Err("An explicit source end requires a selected take and zero tail trim.".into());
    }
    let mut state = import::load_for_assembly(Path::new(&root))?;
    let expected_take = opt_str(args, "expectedTakeId");
    let scene = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    if let Some(expected) = &expected_take {
        let shot = scene.animatic.as_ref().and_then(|anim| anim.shots.iter().find(|shot| shot.id == shot_id))
            .ok_or("Shot is missing from the animatic")?;
        let cut = scene.cuts.iter().find(|cut| cut.id == shot.cut_id).ok_or("Shot's cut is missing")?;
        let take = cut.takes.iter().find(|take| take.id == *expected).ok_or("Selected take is missing")?;
        if cut.selected_take.as_deref() != Some(expected) && cut.selected_take.as_deref() != Some(&take.path) {
            return Err("Selected take changed since opening the edit; review its source before saving".into());
        }
        let absolute = crate::project_path::resolve(&take.path, Some(&root), crate::project_path::PathIntent::Read)
            .map_err(|e| e.to_string())?.into_path_buf();
        let source_duration = crate::workflow::qc::probe_media(&absolute)?.duration_secs
            .filter(|duration| duration.is_finite() && *duration > 0.0).ok_or("Selected take has no measured duration")?;
        let start = trim_in.unwrap_or(0.0);
        let end = source_end.unwrap_or(shot.duration_seconds - trim_out.unwrap_or(0.0));
        let window = end - start;
        if source_end.is_some() && (end * 100.0 - (end * 100.0).round()).abs() > 0.000001 {
            return Err("Source end must use hundredths of a second.".into());
        }
        if !start.is_finite() || !window.is_finite() || start < 0.0 || window <= 0.0 || start + window > source_duration + 0.001 {
            return Err("Edit window exceeds the selected source; adjust duration or trim".into());
        }
    }
    if let Some(end) = source_end {
        // Clear old points in memory first so either shortening or lengthening is valid.
        // No state is persisted until every operation succeeds.
        crate::workflow::animatic::set_trim(scene, &shot_id, Some(0.0), Some(0.0))?;
        crate::workflow::animatic::set_duration(scene, &shot_id, end)?;
    }
    crate::workflow::animatic::set_trim(scene, &shot_id, trim_in, trim_out)?;
    if let Some(expected) = expected_take {
        if let Some(shot) = scene.animatic.as_mut().and_then(|anim| anim.shots.iter_mut().find(|shot| shot.id == shot_id)) {
            shot.trim_source_take_id = Some(expected);
        }
    }
    let imported = import::save_for_assembly(Path::new(&root), state)?;
    result_with_note(
        imported,
        "Trim saved. Later shot packages need review. Renders were kept.",
    )
}

pub fn cmd_set_shot_transition(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let shot_id = opt_str(args, "shotId").ok_or_else(|| "Missing shot id".to_string())?;
    let transition = opt_str(args, "transition");
    let mut state = import::load_state(Path::new(&root))?;
    let scene = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    let effective =
        crate::workflow::animatic::set_transition(scene, &shot_id, transition.as_deref())?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(
        imported,
        format!("Transition set to {}.", effective.label()),
    )
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

// Script-wide drafting is additive only. Revalidate after the provider returns so
// edits or a reimport during the request never get overwritten by its response.
fn script_detail_scene(state: &state::ProjectState, args: &Value, look: bool) -> Result<state::SceneRecord, String> {
    let scene_id = arg_u32(args, "sceneId")?;
    let source = opt_str(args, "scriptSource").ok_or("Missing script source")?;
    let stable = opt_str(args, "sceneStableId").ok_or("Missing stable scene identity")?;
    if !state.scripts.iter().any(|s| s.path == source) { return Err("Saved script is no longer registered.".into()); }
    let scene = state.scenes.iter().find(|s| s.id == scene_id).ok_or("Scene no longer exists.")?;
    if scene.source_missing || scene.source_path.as_deref() != Some(source.as_str()) || scene.stable_id != stable {
        return Err("Script scene changed; generate details again.".into());
    }
    if (look && scene.look_board.is_some()) || (!look && (scene.intent.is_some() || scene.intent_approved)) {
        return Err("Existing scene details were preserved; nothing overwritten.".into());
    }
    Ok(scene.clone())
}

#[cfg(test)]
mod script_detail_guard_tests {
    use super::*;

    #[test]
    fn guards_saved_script_identity_and_existing_details() {
        let mut project = state::ProjectState::new();
        project.scripts.push(state::ScriptRecord { path: "a.md".into(), title: None, aspect_ratio: None });
        let mut scene = state::SceneRecord::new(1, "Scene");
        scene.source_path = Some("a.md".into());
        scene.stable_id = "stable-1".into();
        project.scenes.push(scene);
        let args = json!({"sceneId": 1, "scriptSource": "a.md", "sceneStableId": "stable-1"});
        assert!(script_detail_scene(&project, &args, false).is_ok());
        assert!(script_detail_scene(&project, &args, true).is_ok());
        project.scenes[0].intent = Some(Default::default());
        project.scenes[0].look_board = Some(Default::default());
        assert!(script_detail_scene(&project, &args, false).is_err());
        assert!(script_detail_scene(&project, &args, true).is_err());
        project.scenes[0].intent = None;
        project.scenes[0].look_board = None;
        project.scenes[0].stable_id = "replaced".into();
        assert!(script_detail_scene(&project, &args, false).is_err());
        project.scenes[0].stable_id = "stable-1".into();
        project.scripts.clear();
        assert!(script_detail_scene(&project, &args, true).is_err());
    }
}

pub fn cmd_draft_intent(app: &HostCtx, args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let model = opt_str(args, "modelId").unwrap_or_default();
    let api_id = opt_str(args, "apiId");
    let brief = opt_str(args, "brief").unwrap_or_default();
    let mut state = import::load_state(Path::new(&root))?;
    let guarded = if args.get("scriptSource").is_some() { Some(script_detail_scene(&state, args, false)?) } else { None };
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
    if let Some(original) = guarded {
        state = import::load_state(Path::new(&root))?;
        if script_detail_scene(&state, args, false)? != original {
            return Err("Scene changed during drafting; existing work was preserved.".into());
        }
    }
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
    let max_cuts = scene.max_cuts.unwrap_or(12);
    if existing > max_cuts as usize {
        return Err(format!("Scene has {existing} cuts, above its maximum of {max_cuts}. Remove unwanted cuts before drafting; no model request was sent."));
    }
    let system = sketches::sketches_system_prompt(existing, max_cuts);
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
    let guarded = if args.get("scriptSource").is_some() { Some(script_detail_scene(&state, args, true)?) } else { None };
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
    if let Some(original) = guarded {
        state = import::load_state(Path::new(&root))?;
        if script_detail_scene(&state, args, true)? != original {
            return Err("Scene changed during drafting; existing work was preserved.".into());
        }
    }
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

pub fn cmd_save_reference_description(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let slot_id = opt_str(args, "slotId");
    let description = opt_str(args, "description").ok_or("Enter a reference image description.")?;
    let category = opt_str(args, "category").unwrap_or_else(|| "user_defined".into());
    let mut state = import::load_state(Path::new(&root))?;
    let scene = state.scenes.iter_mut().find(|s| s.id == scene_id).ok_or_else(|| format!("scene {scene_id} not found"))?;
    look::save_reference_description(scene, slot_id.as_deref(), &description, &category)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, "Reference description saved.")
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
    let slot_id = match args.get("slotId") {
        None | Some(Value::Null) => None,
        Some(Value::String(id)) if !id.trim().is_empty() => Some(id.as_str()),
        _ => return Err("Invalid slotId".into()),
    };
    let role = opt_str(args, "role").and_then(|r| AssetRole::parse_look(&r));
    let source = opt_str(args, "sourcePath")
        .or_else(|| opt_str(args, "path"))
        .ok_or_else(|| "Missing sourcePath".to_string())?;
    let mut state = import::load_state(Path::new(&root))?;
    let (slot_id, asset_id) = look::attach_to_slot(Path::new(&root), &mut state, scene_id, slot_id, role, &source)?;
    let imported = import::save_project(Path::new(&root), state)?;
    let mut result = result_with_note(
        imported,
        format!("Attached look still `{asset_id}` as a draft. Approve it if you want to keep it."),
    )?;
    result["attachedLook"] = json!({"slotId":slot_id,"assetId":asset_id});
    Ok(result)
}

pub fn cmd_crop_look(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    image_edit::crop(args, Path::new(&root))
}

pub fn cmd_approve_look(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let slot_id = opt_str(args, "slotId").ok_or_else(|| "Missing slotId".to_string())?;
    let asset_id = opt_str(args, "assetId");
    let mut state = import::load_state(Path::new(&root))?;
    let old_approved_path = state.scene(scene_id)
        .and_then(|scene| scene.look_board.as_ref())
        .and_then(|board| board.slots.iter().find(|slot| slot.id == slot_id))
        .and_then(|slot| slot.approved_asset_id.as_deref())
        .and_then(|id| state.asset(id))
        .map(|asset| asset.path.clone());
    let approved = look::set_asset_status(
        &mut state,
        scene_id,
        &slot_id,
        asset_id.as_deref(),
        AssetStatus::Approved,
    )?;
    let revised = old_approved_path.is_some() && state.scene(scene_id)
        .and_then(|scene| scene.look_board.as_ref())
        .and_then(|board| board.slots.iter().find(|slot| slot.id == slot_id))
        .and_then(|slot| slot.approved_asset_id.as_deref()) == Some(approved.as_str())
        && state.assets.iter().any(|asset| asset.path == old_approved_path.as_deref().unwrap_or("") && asset.status == AssetStatus::Superseded);
    let imported = import::save_project(Path::new(&root), state)?;
    if revised && old_approved_path.as_deref().is_some_and(|path| {
        crate::pending::current().is_some_and(|action| {
            action.inputs.get("source_frame").and_then(Value::as_str) == Some(path)
                || action.inputs.get("reference_images").and_then(Value::as_array)
                    .is_some_and(|images| images.iter().any(|image| image.as_str() == Some(path)))
        })
    }) {
        crate::pending::cancel_all();
    }
    result_with_note(imported, format!("Approved look still on `{slot_id}`. Bound cuts retain their prior source until reviewed and rebound; prepared video approval using that image is revoked."))
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

pub fn cmd_delete_look_slot(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let slot_id = opt_str(args, "slotId").ok_or_else(|| "Missing slotId".to_string())?;
    let mut state = import::load_state(Path::new(&root))?;
    look::delete_slot(&mut state, scene_id, &slot_id)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, format!("Removed look entry `{slot_id}`; image files and registered assets were preserved."))
}

pub fn cmd_attach_character_wardrobe(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let id = opt_str(args, "characterId").ok_or("Missing characterId")?;
    let source = opt_str(args, "sourcePath").ok_or("Missing sourcePath")?;
    let mut state = import::load_state(Path::new(&root))?;
    let asset = character_wardrobe::attach(Path::new(&root), &mut state, &id, &source)?;
    let imported = import::save_project(Path::new(&root), state)?;
    let mut result = result_with_note(imported, format!("Attached character wardrobe `{asset}`."))?;
    result["attachedWardrobe"] = json!({"assetId":asset});
    Ok(result)
}

pub fn cmd_generate_character_wardrobe(app: &HostCtx, args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let id = opt_str(args, "characterId").ok_or("Missing characterId")?;
    let prompt = opt_str(args, "prompt").ok_or("Missing prompt")?;
    let refs: Vec<String> = match args.get("referenceAssetIds") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(ids)) => ids.iter().map(|v| v.as_str().filter(|s| !s.trim().is_empty()).map(str::to_owned).ok_or_else(|| "referenceAssetIds must contain nonempty strings".to_string())).collect::<Result<_,_>>()?,
        _ => return Err("referenceAssetIds must be an array".into()),
    };
    let mut state = import::load_state(Path::new(&root))?;
    let (card, note) = character_wardrobe::prepare(app, Path::new(&root), &mut state, &id, &prompt, &refs)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_pending(imported, note, card)
}

pub fn cmd_generate_look(app: &HostCtx, args: &Value) -> Result<Value, String> {
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
    let attach_character_image = args.get("attachCharacterImage").and_then(Value::as_bool).unwrap_or(false);
    let image_capability = match args.get("imageCapability") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) if !value.trim().is_empty() => Some(value.as_str()),
        _ => return Err("imageCapability must be a nonempty Settings image capability".into()),
    };
    let reference_asset_ids: Vec<String> = match args.get("referenceAssetIds") {
        Some(Value::Array(ids)) => ids.iter().map(|id| id.as_str().filter(|s| !s.is_empty()).map(str::to_owned)
            .ok_or_else(|| "referenceAssetIds must contain nonempty strings".to_string())).collect::<Result<_, _>>()?,
        Some(Value::Null) | None => opt_str(args, "referenceAssetId").into_iter().collect(),
        _ => return Err("referenceAssetIds must be an array".into()),
    };
    let (card, note) = look::prepare_generate_with_host(
        app, Path::new(&root), &mut state, scene_id, &slot_id, attach_character_image, &reference_asset_ids, image_capability,
    )?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_pending(imported, note, card)
}

pub fn cmd_generate_wardrobe_plate(app: &HostCtx, args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let character_id = arg_path(args, "characterId")?;
    let wardrobe_id = arg_path(args, "wardrobeAssetId")?;
    let mut state = import::load_state(Path::new(&root))?;
    let (card, note) =
        look::prepare_wardrobe_plate_with_host(app, Path::new(&root), &mut state, scene_id, &character_id, &wardrobe_id)?;
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
    if state.scene(scene_id).and_then(|scene| scene.cuts.iter().find(|cut| cut.id.cut == cut_number))
        .and_then(|cut| cut.production_note_path.as_ref()).is_none() {
        production::sync_from_planning_with_prompt(Path::new(&root), &mut state, scene_id, cut_number, Some(&prompt))?;
    }
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

pub fn cmd_preview_production_handoff(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_number = arg_u32(args, "cut")?;
    let state = import::load_state(Path::new(&root))?;
    production::preview_from_planning(Path::new(&root), &state, scene_id, cut_number)
}

pub fn cmd_refresh_production_handoff(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_number = arg_u32(args, "cut")?;
    let mut state = import::load_state(Path::new(&root))?;
    let expected = opt_str(args, "revision").ok_or("Preview this production note before refreshing it.")?;
    let preview = production::preview_from_planning(Path::new(&root), &state, scene_id, cut_number)?;
    if preview["revision"].as_str() != Some(expected.as_str()) {
        return Err("Production note or planning changed since preview. Review the latest values before refreshing.".into());
    }
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
        if state.scene(scene_id).and_then(|scene| scene.cuts.iter().find(|item| item.id.cut == cut))
            .and_then(|item| item.production_note_path.as_ref()).is_none() {
            production::sync_from_planning_with_prompt(Path::new(&root), &mut state, scene_id, cut, Some(&prompt))?;
        }
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

pub fn cmd_prepare_dialogue_audio(app: &HostCtx, args: &Value) -> Result<Value, String> {
    dialogue_audio::prepare(app, args)
}

pub fn cmd_prepare_render(app: &HostCtx, args: &Value) -> Result<Value, String> {
    if arg_bool(args, "selectedPerformance", false) {
        return performance::prepare(app, args);
    }
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_number = arg_u32(args, "cut")?;
    let mut state = import::load_state(Path::new(&root))?;
    if arg_bool(args,"resumeSavedAudio",false) {
        if state.scene(scene_id).and_then(|s| s.cuts.iter().find(|c| c.id.cut == cut_number)).is_some_and(|c| c.selected_performance_asset_id.is_some()) {
            return Err("An explicit WAV version is selected. Use Prepare video only with selected WAV, not legacy Resume.".into());
        }
        if production::reconcile_stranded_audio_retry(Path::new(&root), &mut state, scene_id, cut_number)? {
            // History reconciliation is useful even when the old first-line WAV
            // is refused below. It does not create or publish an approval.
            import::persist_state(Path::new(&root), &state)?;
            state = import::load_state(Path::new(&root))?;
        }
        production::ensure_failed_pre_submit_audio_retry(Path::new(&root), &mut state, scene_id, cut_number)?;
        // Persist the new bound attempt before publishing a one-use approval.
        let imported = import::save_project(Path::new(&root), state)?;
        let (card,note) = production::prepare_saved_audio_with_host(app,Path::new(&root),&imported.state,scene_id,cut_number)?;
        return result_with_pending(imported,note,card);
    }
    let (card,note) = production::prepare_render_with_host(app,Path::new(&root),&mut state,scene_id,cut_number)?;
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
        Some(CutId::new(
            match args.get("predecessorScene") {
                None => scene_id,
                Some(_) => arg_u32(args, "predecessorScene")?,
            },
            arg_u32(args, "predecessorCut")?,
        ))
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
    if status == TakeStatus::Selected {
        // A text-sync take keeps its unpadded performance beside the raw video.
        // Only a matching saved WAV can revise an untouched word-count animatic
        // seed; a missing WAV (e.g. imported footage) leaves editorial time alone.
        let cut_id = crate::scene_plan::handoff::CutId::new(scene_id, cut_number);
        let take_path = state.scene(scene_id)
            .and_then(|scene| scene.cuts.iter().find(|cut| cut.id == cut_id))
            .filter(|cut| cut.speech_mode == Some(crate::scene_plan::handoff::SpeechMode::SourceVideoLipsync))
            .and_then(|cut| cut.takes.iter().find(|take| take.id == take_id))
            .map(|take| take.path.clone());
        if let Some(path) = take_path {
            let video = crate::project_path::resolve(&path, Some(&root), crate::project_path::PathIntent::Read)
                .map_err(|e| e.to_string())?.into_path_buf();
            let wav = video.with_extension("wav");
            if wav.is_file() && video.is_file() {
                let stem = wav.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let prefix = format!("{stem}_pad_h");
                let padded: Vec<_> = std::fs::read_dir(video.parent().ok_or("take has no parent directory")?)
                    .map_err(|e| e.to_string())?
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .filter(|p| p.file_name().and_then(|s| s.to_str()).is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".wav")))
                    .collect();
                // A unique padded companion proves the actual lead including
                // the sync offset. Ambiguous or absent companions are not a
                // license to infer a speech start from a provider bucket.
                if let [pad] = padded.as_slice() {
                    let tail = pad.file_stem().and_then(|s| s.to_str())
                        .and_then(|s| s.rsplit_once("_t"))
                        .and_then(|(_, tail)| tail.parse::<f64>().ok());
                    if let Some(tail) = tail {
                        let speech = crate::fs_ops::probe_duration_seconds(&wav.to_string_lossy())?;
                        let padded_seconds = crate::fs_ops::probe_duration_seconds(&pad.to_string_lossy())?;
                        let source = crate::fs_ops::probe_duration_seconds(&video.to_string_lossy())?;
                        let lead = padded_seconds - speech - tail;
                        if lead.is_finite() && lead >= 0.0 && lead <= padded_seconds - speech {
                            if let Some(scene) = state.scenes.iter_mut().find(|scene| scene.id == scene_id) {
                                animatic::reconcile_measured_performance(scene, cut_id, speech, lead, source)?;
                            }
                        }
                    }
                }
            }
        }
    }
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
    cmd_prepare_assemble_inner(None, args)
}

pub fn cmd_prepare_assemble_with_host(app: &HostCtx, args: &Value) -> Result<Value, String> {
    cmd_prepare_assemble_inner(Some(app), args)
}

fn cmd_prepare_assemble_inner(
    approval_app: Option<&HostCtx>,
    args: &Value,
) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = match args.get("sceneId") {
        None | Some(Value::Null) => None,
        Some(_) => Some(arg_u32(args, "sceneId")?),
    };
    let scope_key = scene_id.map(|id|format!("scene_{id}")).unwrap_or_else(||"project".into());
    let label = scene_id.map(|id|format!("Scene {id}")).unwrap_or_else(||"project".into());
    let mut state = import::load_for_assembly(Path::new(&root))?;
    if let Some(id)=scene_id { if state.scene(id).is_none(){return Err(format!("Scene {id} not found"));} }
    let mut scenes:Vec<_>=state.scenes.iter().filter(|s|scene_id.is_none_or(|id|s.id==id)).collect();
    scenes.sort_by_key(|s|s.id);
    if scenes.is_empty(){return Err("No scenes to assemble".into());}
    let mut missing = Vec::new();
    let mut selected = Vec::new();
    let mut take_ids = Vec::new();
    let mut snapshot = Vec::new();
    for scene in scenes {
    if scene.cuts.is_empty(){return Err(format!("Scene {} has no cuts to assemble",scene.id));}
    let use_animatic = scene.animatic.as_ref().map(|a| !a.shots.is_empty()).unwrap_or(false);

    if use_animatic {
        let anim = scene.animatic.as_ref().unwrap();
        let anim_ids: Vec<u32> = anim.shots.iter().map(|s| s.cut_id.cut).collect();
        let mut seen = std::collections::HashSet::new();
        for id in &anim_ids {
            if !seen.insert(*id) {
                return Err(format!("Animatic duplicate cut id: {}", id));
            }
        }
        let cut_ids_in_scene: std::collections::HashSet<u32> = scene.cuts.iter().map(|c| c.id.cut).collect();
        for id in &anim_ids {
            if !cut_ids_in_scene.contains(id) {
                return Err(format!("Animatic references missing cut: {}", id));
            }
        }
        for id in &cut_ids_in_scene {
            if !anim_ids.contains(id) {
                return Err(format!("Foreigner cut not in animatic: {}", id));
            }
        }
        for shot in &anim.shots {
            let cut = scene.cuts.iter().find(|c| c.id.cut == shot.cut_id.cut).unwrap();
            let take = cut.selected_take.as_ref().and_then(|selected| {
                cut.takes.iter().find(|t| t.id == *selected || t.path == *selected)
            });
            if let Some(bound) = &shot.trim_source_take_id {
                if take.is_none_or(|take| &take.id != bound) {
                    return Err(format!("Cut {} edit belongs to take {}; review and save a new source window for the selected take", shot.cut_id.cut, bound));
                }
            }
            if take.is_none() {
                missing.push(format!("Scene {} Cut {}", scene.id, shot.cut_id.cut));
            } else {
                let take = take.unwrap();
                take_ids.push(take.id.clone());
                let start = shot.trim_in_seconds.unwrap_or(0.0);
                let tail = shot.trim_out_seconds.unwrap_or(0.0);
                let dur = shot.duration_seconds - start - tail;
                if !start.is_finite() || start < 0.0 {
                    return Err(format!("Animatic cut {} invalid start {}", shot.cut_id.cut, start));
                }
                if shot.trim_out_seconds.unwrap_or(0.0) < 0.0 {
                    return Err(format!("Animatic cut {} negative tail", shot.cut_id.cut));
                }
                if !dur.is_finite() || dur <= 0.0 {
                    return Err(format!("Animatic cut {} invalid duration {} (dur = {} - {} - {})", shot.cut_id.cut, dur, shot.duration_seconds, start, tail));
                }
                let mut entry = serde_json::Map::new();
                entry.insert("id".into(), Value::from(cut.id.source_scene));
                entry.insert("cut".into(), Value::from(cut.id.cut));
                entry.insert("video".into(), Value::String(take.path.clone()));
                entry.insert("start".into(), Value::Number(serde_json::Number::from_f64(start).unwrap_or(serde_json::Number::from(0))));
                entry.insert("duration".into(), Value::Number(serde_json::Number::from_f64(dur).unwrap_or(serde_json::Number::from(0))));
                entry.insert("transition_out".into(), Value::String(shot.transition.map(|t|t.label()).unwrap_or("cut").to_string()));
                selected.push(Value::Object(entry));
                snapshot.push(crate::workflow::state::AssemblyShotSnapshot {
                    shot_id: cut.stable_id.clone(),
                    cut_label: shot.cut_id.cut,
                    take_id: take.id.clone(),
                    path: take.path.clone(),
                    start_seconds: start,
                    duration_seconds: dur,
                    transition: shot.transition.as_ref().map(|tr| {
                        serde_json::to_string(tr).unwrap_or_default().trim_matches('"').to_string()
                    }).unwrap_or_else(|| "cut".into()),
                });
            }
        }
    } else {
        let mut cuts = scene.cuts.iter().collect::<Vec<_>>();
        cuts.sort_by_key(|cut| cut.id.cut);
        for cut in cuts {
            let take = cut.selected_take.as_ref().and_then(|selected| {
                cut.takes
                    .iter()
                    .find(|take| take.id == *selected || take.path == *selected)
            });
            if let Some(take) = take {
                take_ids.push(take.id.clone());
                let absolute=crate::project_path::resolve(&take.path,Some(&root),crate::project_path::PathIntent::Read).map_err(|e|e.to_string())?.into_path_buf();
                let duration=crate::workflow::qc::probe_media(&absolute)?.duration_secs.filter(|d|d.is_finite() && *d>0.0).ok_or("selected take has no measured runtime")?;
                selected.push(json!({ "id": cut.id.source_scene, "cut": cut.id.cut, "video": take.path,"start":0.0,"duration":duration,"transition_out":"cut" }));
                snapshot.push(crate::workflow::state::AssemblyShotSnapshot{shot_id:cut.stable_id.clone(),cut_label:cut.id.cut,take_id:take.id.clone(),path:take.path.clone(),start_seconds:0.0,duration_seconds:duration,transition:"cut".into()});
            } else {
                missing.push(format!("Scene {} Cut {}", scene.id, cut.id.cut));
            }
        }
    }
    }
    if !missing.is_empty() {
        return Err(format!(
            "Select one take for every cut before assembly. Missing: {}.", missing.join(", ")
        ));
    }
    if state.jobs.iter().any(|j| j.kind == "assemble" && matches!(j.status, crate::workflow::state::JobStatus::Pending | crate::workflow::state::JobStatus::Running) && j.raw_provider_id.as_deref() == Some(scope_key.as_str()) && j.assembly_snapshot.as_ref() == Some(&snapshot)) {
        return Err("An unchanged assembly is already pending or running. Approve the existing request or resolve it in Jobs.".into());
    }
    let reserved_paths: std::collections::HashSet<String> = state.jobs.iter()
        .filter(|j| j.kind == "assemble" && matches!(j.status, crate::workflow::state::JobStatus::Pending | crate::workflow::state::JobStatus::Running))
        .filter_map(|j| j.output_path.clone())
        .collect();
    let output_path = (1..=1000)
        .map(|version| {
            if version == 1 {
                format!("Final/{}_master.mp4", scene_id.map(|id|format!("Scene{id}")).unwrap_or_else(||"Project".into()))
            } else {
                format!("Final/{}_master_v{version}.mp4", scene_id.map(|id|format!("Scene{id}")).unwrap_or_else(||"Project".into()))
            }
        })
        .find(|relative| !Path::new(&root).join(relative).exists() && !reserved_paths.contains(relative.as_str()) && !Path::new(&root).join(Path::new(relative).with_extension("production.md")).exists())
        .ok_or_else(|| "Could not allocate a master output version".to_string())?;
    let cut_count = snapshot.len();
    let mut inputs = Map::new();
    let production_path = assembly_jobs::prepare_production(Path::new(&root), &state, &snapshot, &output_path)?;
    inputs.insert("production_path".into(), Value::String(production_path));
    inputs.insert("output_path".into(), Value::String(output_path.clone()));
    inputs.insert("trim".into(), Value::String("custom".into()));
    inputs.insert("fade_in".into(), Value::Bool(false));
    inputs.insert("fade_out".into(), Value::Bool(false));
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
        format!("Assemble {label}\nCuts: {cut_count}\nMaster: `{output_path}`\n\n");
    let approval = match approval_app {
        Some(app) => {
            crate::director::format_approval_request_for_app(app, &action, &validated, Some(&root))?
        }
        None => format_approval_request_with_route(
            &action,
            &validated,
            "Local (FFmpeg)",
            "None (host-side media assembly)",
            "No provider charge; local FFmpeg processing.",
        ),
    };
    card.push_str(&approval);
    crate::workflow::identity::assign(&mut state);
    let scene_stable = state
        .scenes.iter().find(|s|Some(s.id)==scene_id)
        .map(|scene| scene.stable_id.clone())
        .unwrap_or_default();
    let job_id = state.next_job_id();
    let mut job = crate::workflow::state::JobRecord::open(job_id, "assemble");
    job.output_path = Some(output_path.clone());
    job.raw_provider_id = Some(scope_key);
    job.input_asset_ids = take_ids;
    if !snapshot.is_empty() {
        job.assembly_snapshot = Some(snapshot);
    }
    if scene_id.is_none() {
        job.scope=Some(crate::workflow::state::JobScope{kind:"project".into(),id:"project".into()});
    } else if !scene_stable.is_empty() {
        job.scope = Some(crate::workflow::state::JobScope {
            kind: "scene".into(),
            id: scene_stable,
        });
    }
    state.jobs.push(job);
    let imported = import::save_for_assembly(Path::new(&root), state)?;
    result_with_pending(
        imported,
        format!("Prepared {label} assembly from {cut_count} selected takes."),
        card,
    )
}

pub fn cmd_add_alternate_shot(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let mut state = import::load_state(Path::new(&root))?;
    let scene = state.scenes.iter().find(|scene| scene.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    let max_cuts = scene.max_cuts.unwrap_or(12);
    if scene.cuts.len() >= max_cuts as usize {
        return Err(format!("Scene already has {} cuts; its maximum is {max_cuts}. No alternate was added.", scene.cuts.len()));
    }
    let stable_id = identity::add_alternate_shot(&mut state, scene_id)?;
    let imported = import::save_project(Path::new(&root), state)?;
    let cut = imported
        .state
        .scene(scene_id)
        .and_then(|scene| scene.cuts.iter().find(|cut| cut.stable_id == stable_id))
        .ok_or_else(|| format!("alternate shot `{stable_id}` was not saved"))?;
    if cut.production_note_path.is_some() || cut.visual_source.is_some() {
        return Err(
            "alternate shot must stay incomplete until a source and note are chosen".into(),
        );
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

pub fn cmd_reopen_sketch(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let shot_id = opt_str(args, "shotId").ok_or("Missing shotId")?;
    let mut state = import::load_state(Path::new(&root))?;
    identity::reopen_planning_sketch(&mut state, scene_id, &shot_id)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, "Scene sketch animatic reopened for revision. Remove cut is a separate action; media, production work and dependent shots stay protected.")
}

pub fn cmd_remove_cut(args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let shot_id = opt_str(args, "shotId").ok_or("Missing shotId")?;
    let mut state = import::load_state(Path::new(&root))?;
    identity::remove_planning_cut(&mut state, scene_id, &shot_id)?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_note(imported, "Removed sketch cut and its rough-cut timing. No project files or media were deleted.")
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
    let check = post::check_deliverable(&mut state, scene_id, &deliverable_id, Path::new(&root))?;
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

pub fn cmd_generate_start_frame(app: &HostCtx, args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_number = arg_u32(args, "cut")?;
    let mut state = import::load_state(Path::new(&root))?;
    let reference_asset_ids: Vec<String> = match args.get("referenceAssetIds") {
        Some(Value::Array(ids)) => ids.iter().map(|id| id.as_str().filter(|s| !s.is_empty()).map(str::to_owned)
            .ok_or_else(|| "referenceAssetIds must contain nonempty strings".to_string())).collect::<Result<_, _>>()?,
        Some(Value::Null) | None => Vec::new(),
        _ => return Err("referenceAssetIds must be an array".into()),
    };
    let (card, note) = start_frames::prepare_generate_with_host(
        app,
        Path::new(&root),
        &mut state,
        scene_id,
        cut_number,
        &reference_asset_ids,
    )?;
    let imported = import::save_project(Path::new(&root), state)?;
    result_with_pending(imported, note, card)
}

pub fn cmd_edit_start_frame(app: &HostCtx, args: &Value) -> Result<Value, String> {
    let root = arg_path(args, "projectRoot")?;
    let scene_id = arg_u32(args, "sceneId")?;
    let cut_number = arg_u32(args, "cut")?;
    let asset_id = opt_str(args, "assetId").ok_or_else(|| "Missing assetId".to_string())?;
    let brief = opt_str(args, "brief").ok_or_else(|| "Missing edit brief".to_string())?;
    let mut state = import::load_state(Path::new(&root))?;
    let (card, note) = start_frames::prepare_edit_with_host(
        app,
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
    let source_path = crate::session::authorize(&source, crate::project_path::PathIntent::Read)
        .map_err(|error| error.to_string())?;
    let mut state = import::load_state(Path::new(&root))?;
    let asset_id = start_frames::attach_image(
        Path::new(&root),
        &mut state,
        scene_id,
        cut_number,
        &source_path.to_string_lossy(),
    )?;
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

#[cfg(test)]
mod budget_tests {
    use super::*;

    #[test]
    fn parse_budget_nulls_clear_the_budget() {
        assert_eq!(parse_budget(None).unwrap(), None);
        assert_eq!(parse_budget(Some(&Value::Null)).unwrap(), None);
    }

    #[test]
    fn parse_budget_reads_limit_warning_and_hard_stop() {
        let value = json!({
            "currency": "EUR",
            "limit_minor": 5000,
            "warning_at_minor": 4000,
            "hard_stop": true,
        });
        let budget = parse_budget(Some(&value)).unwrap().unwrap();
        assert_eq!(budget.currency, "EUR");
        assert_eq!(budget.limit_minor, Some(5000));
        assert_eq!(budget.warning_at_minor, Some(4000));
        assert!(budget.hard_stop);
    }

    #[test]
    fn parse_budget_defaults_currency_to_usd() {
        let value = json!({ "limit_minor": 1000, "hard_stop": false });
        let budget = parse_budget(Some(&value)).unwrap().unwrap();
        assert_eq!(budget.currency, "USD");
        assert_eq!(budget.limit_minor, Some(1000));
        assert!(!budget.hard_stop);
    }

    #[test]
    fn parse_budget_rejects_a_warning_above_the_limit() {
        let value = json!({ "limit_minor": 1000, "warning_at_minor": 1500 });
        assert!(parse_budget(Some(&value)).is_err());
    }

    #[test]
    fn parse_budget_rejects_negative_amounts() {
        assert!(parse_budget(Some(&json!({ "limit_minor": -1 }))).is_err());
        assert!(parse_budget(Some(&json!({ "warning_at_minor": -5 }))).is_err());
    }
}
