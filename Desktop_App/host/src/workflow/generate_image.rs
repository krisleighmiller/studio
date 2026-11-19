//! Gate-free Generate surface: still-image generation with no scene, look
//! slot, production note, sketch, or other prerequisite. Outputs land under
//! Preproduction/GeneratedImages/ and register as `generated_image` assets.
//!
//! Preparation reuses the same `still_image` specialist, batch validation,
//! model routing, approval and spend-consent machinery as scene looks; the
//! only difference is that nothing stage-shaped is required or created.

use crate::director_actions::{
    format_approval_request, validate_action, ActionKind, DirectorAction,
};
use crate::workflow::import;
use crate::workflow::state::{
    AssetRecord, AssetRole, AssetStatus, JobRecord, JobStatus, ProjectState,
};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// Project-relative output folder for gate-free generated images.
pub const GENERATED_IMAGES_DIR: &str = "Preproduction/GeneratedImages";

/// Studio-level library root used when no project is open. Assets live here
/// forever, independent of any project. The sidecar settings dir is
/// `<userData>/config` and userData's own name (the app name) is on the
/// sensitive-path deny list, so the library sits beside userData:
/// `<userData>/../studio`.
pub fn studio_root(ctx: &crate::ctx::HostCtx) -> PathBuf {
    let settings = ctx.settings_dir();
    let user_data = settings.parent().unwrap_or(&settings);
    user_data.parent().unwrap_or(user_data).join("studio")
}

/// Load (or initialize) the studio library state at the given root. The
/// studio library is a project-shaped state with no scenes; it only owns
/// reusable assets and their jobs.
pub fn ensure_studio_state(root: &Path) -> Result<ProjectState, String> {
    fs::create_dir_all(root.join("Preproduction"))
        .map_err(|error| format!("could not create studio library folder: {error}"))?;
    let state_path = crate::project_path::resolve(
        crate::workflow::state::STATE_REL,
        Some(&root.to_string_lossy()),
        crate::project_path::PathIntent::Read,
    )
    .map_err(|error| error.to_string())?
    .into_path_buf();
    if state_path.is_file() {
        return import::load_state(root);
    }
    let state = ProjectState::from_json(
        &serde_json::json!({ "version": 2, "scenes": [] }).to_string(),
    )
    .map_err(|error| error.to_string())?;
    import::persist_state(root, &state).map_err(|error| error.to_string())?;
    Ok(state)
}

/// Aspect ratios the Generate composer offers. The host validates against
/// this list rather than forwarding arbitrary provider geometry.
pub const GENERATE_ASPECT_RATIOS: [&str; 4] = ["9:16", "16:9", "1:1", "4:5"];

/// One prepared Generate request produces between these many stills.
pub const MIN_GENERATE_COUNT: u32 = 1;
pub const MAX_GENERATE_COUNT: u32 = 8;

/// A reference-only request is a legitimate variations request, never a
/// block. The host substitutes this brief so the provider still receives a
/// concrete instruction.
const VARIATION_PROMPT: &str = "Create a variation of the attached reference images: keep the subject, composition, palette and lighting, and refine the details. Preserve the same person, place or object where present.";

fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "webp" | "gif"
            )
        })
}

fn same_path_stem(a: &str, b: &str) -> bool {
    let normalize = |value: &str| {
        Path::new(value)
            .with_extension("")
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase()
    };
    normalize(a) == normalize(b)
}

/// Validate one reference asset id: must exist, be un-rejected, name an
/// image path, and have its file on disk. Mirrors scene-look reference
/// validation without any scene scoping.
pub fn selected_reference(root: &Path, state: &ProjectState, id: &str) -> Result<String, String> {
    let asset = state
        .asset(id)
        .filter(|a| a.status != AssetStatus::Rejected)
        .ok_or_else(|| format!("reference `{id}` is invalid or rejected"))?;
    let ext = Path::new(&asset.path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !["png", "jpg", "jpeg", "webp", "gif"].contains(&ext.as_str()) {
        return Err(format!("reference `{id}` is not an image"));
    }
    let root_s = root.to_string_lossy();
    let path = crate::project_path::resolve(
        &asset.path,
        Some(&root_s),
        crate::project_path::PathIntent::Read,
    )
    .map_err(|e| e.to_string())?
    .into_path_buf();
    if !path.is_file() {
        return Err(format!("reference `{id}` file is missing"));
    }
    Ok(asset.path.clone())
}

fn approval_details(
    app: Option<&crate::ctx::HostCtx>,
    action: &DirectorAction,
    validated: &crate::director_actions::ValidatedAction,
    root: &str,
    project_scoped: bool,
) -> Result<String, String> {
    match (app, project_scoped) {
        // Project-scoped preparation may consult the open project session.
        (Some(app), true) => {
            crate::director::format_approval_request_for_app(app, action, validated, Some(root))
        }
        // Studio-library preparation has no project session but must still
        // park the operation and embed the one-use approval token.
        (Some(app), false) => {
            crate::director::format_approval_request_for_studio(app, action, validated, root)
        }
        // Test-only formatting without an app; never parks an approval.
        _ => Ok(format_approval_request(action, validated)),
    }
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    for ch in value.trim().chars().take(48) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

/// State-level preparation shared by the offline tests and the host command.
fn prepare_inner(
    root: &Path,
    state: &mut ProjectState,
    prompt: &str,
    aspect_ratio: &str,
    count: u32,
    image_capability: Option<&str>,
    reference_asset_ids: &[String],
    title: Option<&str>,
    approval_app: Option<&crate::ctx::HostCtx>,
    project_scoped: bool,
) -> Result<(String, String), String> {
    let prompt = prompt.trim();
    if !GENERATE_ASPECT_RATIOS.contains(&aspect_ratio) {
        return Err(format!(
            "aspectRatio must be one of {}",
            GENERATE_ASPECT_RATIOS.join(", ")
        ));
    }
    if !(MIN_GENERATE_COUNT..=MAX_GENERATE_COUNT).contains(&count) {
        return Err(format!(
            "count must be between {MIN_GENERATE_COUNT} and {MAX_GENERATE_COUNT}"
        ));
    }
    let selected_paths: Vec<String> = reference_asset_ids
        .iter()
        .map(|id| selected_reference(root, state, id))
        .collect::<Result<_, _>>()?;
    // Explicit selections lead the list so ordinal references match the picker.
    let mut reference_images: Vec<String> = Vec::new();
    for path in &selected_paths {
        if !reference_images.contains(path) {
            reference_images.push(path.clone());
        }
    }
    let mut input_asset_ids: Vec<String> = Vec::new();
    for id in reference_asset_ids {
        if !input_asset_ids.contains(id) {
            input_asset_ids.push(id.clone());
        }
    }
    // "With references but no description, Generate makes variations."
    let mut gen_prompt = prompt.to_string();
    if gen_prompt.is_empty() {
        if reference_images.is_empty() {
            return Err("Describe an image, or add reference images to vary.".into());
        }
        gen_prompt = VARIATION_PROMPT.to_string();
    }
    let run_seq = state.next_job_id().trim_start_matches("job_").to_string();
    let run_id = format!("run_{run_seq}");
    let stem = slug(title.unwrap_or("image"));
    let stem = if stem.is_empty() { "image".to_string() } else { stem };
    let rels: Vec<String> = (1..=count)
        .map(|index| format!("{GENERATED_IMAGES_DIR}/{stem}_{run_seq}_{index}.png"))
        .collect();
    let root_string = root.to_string_lossy();
    let dir = crate::project_path::resolve(
        GENERATED_IMAGES_DIR,
        Some(root_string.as_ref()),
        crate::project_path::PathIntent::Write,
    )
    .map_err(|error| error.to_string())?
    .into_path_buf();
    fs::create_dir_all(&dir)
        .map_err(|error| format!("could not create GeneratedImages folder: {error}"))?;

    let items: Vec<Value> = rels
        .iter()
        .map(|rel| {
            let mut item = json!({
                "prompt": gen_prompt,
                "output_path": rel,
                "aspect_ratio": aspect_ratio,
            });
            if !reference_images.is_empty() {
                item["reference_images"] = json!(reference_images);
            }
            if let Some(capability) = image_capability {
                item["image_capability"] = json!(capability);
            }
            item
        })
        .collect();
    let mut inputs = Map::new();
    // Mirror the selected capability at the action top level so the approval
    // route resolver (`approval_model_routes`) can name the chosen model; the
    // batch items still carry it for per-item execution.
    if let Some(capability) = image_capability {
        inputs.insert("image_capability".into(), json!(capability));
    }
    inputs.insert("batch".into(), Value::Array(items));
    let action = DirectorAction {
        action: ActionKind::RunSpecialist,
        specialist: Some("still_image".into()),
        inputs,
        message: None,
        op: None,
        path: None,
    };
    let validated = validate_action(action.clone(), Some(&root_string)).map_err(|e| e.to_string())?;
    if let Some(app) = approval_app {
        // Validate Settings model routing against the first batch item; every
        // item shares the same prompt, references and capability, so one
        // resolution covers the whole run.
        let context = crate::director::current_workflow_context(app, &root.to_path_buf())?;
        let first = validated
            .action
            .inputs
            .get("batch")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.as_object())
            .map(|object| {
                object
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect::<Map<String, Value>>()
            })
            .unwrap_or_default();
        crate::specialists::resolve_still_image_model(&context, &first)?;
    }
    let mut card = format!(
        "Generate images · {count} still{} · {aspect_ratio}\nHost will save under `{GENERATED_IMAGES_DIR}/`.\n",
        if count == 1 { "" } else { "s" }
    );
    if let Some(title) = title.filter(|t| !t.trim().is_empty()) {
        card.push_str(&format!("Title: {title}\n"));
    }
    card.push_str(&format!("\nPrompt:\n```\n{gen_prompt}\n```\n"));
    for path in &reference_images {
        card.push_str(&format!("Selected reference: `{path}`\n"));
    }
    card.push('\n');
    card.push_str(&approval_details(
        approval_app,
        &action,
        &validated,
        &root_string,
        project_scoped,
    )?);
    let estimate = approval_app.and_then(|app| {
        crate::director::estimate_for_action(app, &action, Some(&root_string))
            .ok()
            .flatten()
    });
    for rel in &rels {
        let job_id = state.next_job_id();
        let mut job = JobRecord {
            input_asset_ids: input_asset_ids.clone(),
            output_path: Some(rel.clone()),
            raw_provider_id: Some(run_id.clone()),
            ..JobRecord::open(job_id, "generated_image")
        };
        if let Some(estimate) = &estimate {
            job.provider = Some(estimate.kind.clone());
            job.model = estimate.model.clone();
            job.currency = Some(estimate.currency.clone());
            job.estimated_cost_minor = Some(estimate.minor);
        }
        state.jobs.push(job);
    }
    let note = if approval_app.is_some() {
        format!(
            "{count} images ready under `{GENERATED_IMAGES_DIR}/`. Use the one-time approval button; chat text cannot approve it."
        )
    } else {
        format!(
            "{count} images prepared under `{GENERATED_IMAGES_DIR}/`; no provider operation is authorized here."
        )
    };
    Ok((card, note))
}

pub fn prepare(
    root: &Path,
    state: &mut ProjectState,
    prompt: &str,
    aspect_ratio: &str,
    count: u32,
    image_capability: Option<&str>,
    reference_asset_ids: &[String],
    title: Option<&str>,
) -> Result<(String, String), String> {
    prepare_inner(
        root,
        state,
        prompt,
        aspect_ratio,
        count,
        image_capability,
        reference_asset_ids,
        title,
        None,
        false,
    )
}

pub fn prepare_with_host(
    app: &crate::ctx::HostCtx,
    root: &Path,
    state: &mut ProjectState,
    prompt: &str,
    aspect_ratio: &str,
    count: u32,
    image_capability: Option<&str>,
    reference_asset_ids: &[String],
    title: Option<&str>,
    project_scoped: bool,
) -> Result<(String, String), String> {
    prepare_inner(
        root,
        state,
        prompt,
        aspect_ratio,
        count,
        image_capability,
        reference_asset_ids,
        title,
        Some(app),
        project_scoped,
    )
}

fn is_generated_image_rel(rel: &str) -> bool {
    let normalized = rel.replace('\\', "/");
    let parts: Vec<_> = normalized.split('/').collect();
    parts.len() == 3
        && parts[0] == "Preproduction"
        && parts[1] == "GeneratedImages"
        && is_image_path(Path::new(parts[2]))
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

fn relativize(root: &Path, path: &str) -> Option<String> {
    let root_string = root.to_string_lossy();
    let confined = crate::project_path::resolve(
        path,
        Some(root_string.as_ref()),
        crate::project_path::PathIntent::Read,
    )
    .ok()?;
    crate::project_path::relative_to_root(root, confined.as_path())
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

fn matching_job<'a>(state: &'a ProjectState, rel: &str) -> Option<&'a JobRecord> {
    state
        .jobs
        .iter()
        .find(|job| {
            job.kind == "generated_image"
                && job.status == JobStatus::Pending
                && job
                    .output_path
                    .as_deref()
                    .is_some_and(|path| same_path_stem(path, rel))
        })
        .or_else(|| {
            state.jobs.iter().find(|job| {
                job.kind == "generated_image"
                    && job
                        .output_path
                        .as_deref()
                        .is_some_and(|path| same_path_stem(path, rel))
            })
        })
}

fn complete_matching_job(state: &mut ProjectState, rel: &str) {
    if let Some(job) = state.jobs.iter_mut().find(|job| {
        job.kind == "generated_image"
            && job.status == JobStatus::Pending
            && job
                .output_path
                .as_deref()
                .is_some_and(|path| same_path_stem(path, rel))
    }) {
        job.finish(Some(rel), None);
    }
}

/// State-level core of `on_still_saved`: registers an already-downloaded
/// generated image against the given state. Performs no file or state I/O
/// beyond the in-memory update, so import-time recovery can call it without
/// re-entering the load/save cycle. Idempotent: an already-registered path
/// only completes its matching job; a second call changes nothing.
pub(crate) fn register_generated_output(
    state: &mut ProjectState,
    root_path: &Path,
    save_path: &str,
) -> Option<String> {
    let actual = actual_path(root_path, save_path)?;
    let rel = relativize(root_path, &actual.to_string_lossy())?;
    if !is_generated_image_rel(&rel) {
        return None;
    }
    // Content must actually be an image: a corrupt or partial download is
    // rejected here rather than registered as a usable asset.
    if crate::media_util::ensure_media_file(&actual).is_err() {
        return None;
    }
    if state.assets.iter().any(|asset| asset.path == rel) {
        complete_matching_job(state, &rel);
        return Some(format!("Generated image already registered: `{rel}`."));
    }
    let job = matching_job(state, &rel)?;
    let job_id = job.id.clone();
    let asset_id = state.push_asset(AssetRecord {
        id: String::new(),
        path: rel.clone(),
        role: AssetRole::GeneratedImage,
        status: AssetStatus::Draft,
        scene_id: None,
        cut_id: None,
        character_id: None,
        source_job_id: Some(job_id.clone()),
        parent_asset_id: None,
        notes: Some("generated image".into()),
    });
    if let Some(job) = state.jobs.iter_mut().find(|job| job.id == job_id) {
        job.finish(Some(&rel), Some(&asset_id));
    }
    Some(format!("Registered generated image `{asset_id}` at `{rel}`."))
}

/// After still_image writes a file under Preproduction/GeneratedImages/,
/// register it.
pub fn on_still_saved(project_root: Option<&str>, save_path: &str) -> Option<String> {
    let root = project_root.filter(|s| !s.trim().is_empty())?;
    let root_path = Path::new(root);
    if !crate::project_path::resolve(
        crate::workflow::state::STATE_REL,
        Some(root),
        crate::project_path::PathIntent::Read,
    )
    .is_ok_and(|path| path.as_path().is_file())
    {
        return None;
    }
    let mut state = import::load_state(root_path).ok()?;
    let note = register_generated_output(&mut state, root_path, save_path)?;
    let _ = import::save_project(root_path, state);
    Some(note)
}

/// Terminal provider failure for a generated-image run: fail the pending
/// job whose output path matches. Idempotent; completed jobs are untouched.
pub fn on_still_failed(project_root: Option<&str>, save_path: &str, error: &str) -> Option<String> {
    let root = project_root.filter(|s| !s.trim().is_empty())?;
    let root_path = Path::new(root);
    if !crate::project_path::resolve(
        crate::workflow::state::STATE_REL,
        Some(root),
        crate::project_path::PathIntent::Read,
    )
    .is_ok_and(|path| path.as_path().is_file())
    {
        return None;
    }
    let mut state = import::load_state(root_path).ok()?;
    let rel = relativize(root_path, save_path)?;
    let mut failed = false;
    if let Some(job) = state.jobs.iter_mut().find(|job| {
        job.kind == "generated_image"
            && job.status == JobStatus::Pending
            && job
                .output_path
                .as_deref()
                .is_some_and(|path| same_path_stem(path, &rel))
    }) {
        job.fail(None, error.to_string());
        failed = true;
    }
    if failed {
        let _ = import::save_project(root_path, state);
        Some(format!(
            "Marked generated image job failed for {rel}: {error}"
        ))
    } else {
        None
    }
}

/// Remove one generated image asset. The file moves to the project trash;
/// the asset record is deleted from state.
pub fn delete(root: &Path, state: &mut ProjectState, asset_id: &str) -> Result<String, String> {
    let asset = state
        .asset(asset_id)
        .filter(|asset| asset.role == AssetRole::GeneratedImage)
        .cloned()
        .ok_or_else(|| format!("generated image `{asset_id}` not found"))?;
    if !is_generated_image_rel(&asset.path) {
        return Err("that asset is not under Preproduction/GeneratedImages".into());
    }
    let root_s = root.to_string_lossy().to_string();
    let path = crate::director_actions::resolve_under_project(&asset.path, Some(&root_s))
        .map_err(|error| error.to_string())?;
    if path.is_file() {
        crate::project_path::move_to_trash(root, &path).map_err(|error| error.to_string())?;
    }
    state.assets.retain(|item| item.id != asset_id);
    Ok(asset.path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture_state(root: &Path) -> ProjectState {
        let exists = crate::project_path::resolve(
            crate::workflow::state::STATE_REL,
            Some(&root.to_string_lossy()),
            crate::project_path::PathIntent::Read,
        )
        .is_ok_and(|path| path.as_path().is_file());
        if !exists {
            let state = ProjectState::from_json(
                &json!({
                    "version": 2,
                    "scenes": [],
                    "assets": [{"id": "ref_a", "path": "Preproduction/Reference/ref_a.png", "role": "interior_reference", "status": "draft"}]
                })
                .to_string(),
            )
            .unwrap();
            fs::create_dir_all(root.join("Preproduction/Reference")).unwrap();
            fs::write(root.join("Preproduction/Reference/ref_a.png"), b"\x89PNG\r\n\x1a\n").unwrap();
            import::persist_state(root, &state).unwrap();
        }
        // Load back so the in-memory revision matches the on-disk record.
        import::load_state(root).unwrap()
    }

    #[test]
    fn prepare_builds_one_job_per_still_with_prompt_and_aspect() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = fixture_state(root);
        let (card, note) = prepare(
            root,
            &mut state,
            "a moody corridor",
            "9:16",
            3,
            None,
            &[],
            Some("Corridor study"),
        )
        .unwrap();
        assert!(card.contains("3 stills"));
        assert!(card.contains("9:16"));
        assert!(card.contains("a moody corridor"));
        assert!(card.contains("Corridor study"));
        assert!(note.contains("no provider operation is authorized"));
        assert_eq!(state.jobs.len(), 3);
        for job in &state.jobs {
            assert_eq!(job.kind, "generated_image");
            assert_eq!(job.status, JobStatus::Pending);
            let rel = job.output_path.as_deref().unwrap();
            assert!(rel.starts_with("Preproduction/GeneratedImages/corridor_study_"));
            assert!(is_generated_image_rel(rel));
            assert_eq!(job.raw_provider_id.as_deref(), Some("run_1"));
        }
    }

    #[test]
    fn prepare_rejects_invalid_or_missing_reference_ids() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = fixture_state(root);
        let error = prepare(
            root,
            &mut state,
            "room",
            "16:9",
            1,
            None,
            &["missing".into()],
            None,
        )
        .unwrap_err();
        assert!(error.contains("missing"));
        // Existing id whose file disappeared.
        fs::remove_file(root.join("Preproduction/Reference/ref_a.png")).unwrap();
        let error = prepare(
            root,
            &mut state,
            "room",
            "16:9",
            1,
            None,
            &["ref_a".into()],
            None,
        )
        .unwrap_err();
        assert!(error.contains("file is missing"));
        // Rejected asset is refused.
        fs::write(root.join("Preproduction/Reference/ref_a.png"), b"\x89PNG\r\n\x1a\n").unwrap();
        state.asset_mut("ref_a").unwrap().status = AssetStatus::Rejected;
        assert!(prepare(root, &mut state, "room", "16:9", 1, None, &["ref_a".into()], None).is_err());
    }

    #[test]
    fn prepare_enforces_count_bounds_and_aspect_list() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = fixture_state(root);
        let error = prepare(root, &mut state, "room", "9:16", 0, None, &[], None).unwrap_err();
        assert!(error.contains("count must be between 1 and 8"));
        let error = prepare(root, &mut state, "room", "9:16", 9, None, &[], None).unwrap_err();
        assert!(error.contains("count must be between 1 and 8"));
        let error = prepare(root, &mut state, "room", "21:9", 1, None, &[], None).unwrap_err();
        assert!(error.contains("aspectRatio"));
        assert!(prepare(root, &mut state, "room", "4:5", 8, None, &[], None).is_ok());
        assert_eq!(state.jobs.len(), 8);
    }

    #[test]
    fn empty_prompt_with_refs_becomes_variations_and_never_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = fixture_state(root);
        let (card, _) = prepare(
            root,
            &mut state,
            "   ",
            "1:1",
            2,
            None,
            &["ref_a".into()],
            None,
        )
        .unwrap();
        assert!(card.contains(VARIATION_PROMPT));
        assert_eq!(state.jobs.len(), 2);
        assert_eq!(state.jobs[0].input_asset_ids, vec!["ref_a"]);
        assert!(card.contains("ref_a.png"));
        let mut other = fixture_state(root);
        let error = prepare(root, &mut other, "", "1:1", 1, None, &[], None).unwrap_err();
        assert!(error.contains("Describe an image"));
    }

    #[test]
    fn settlement_registers_n_assets_and_finishes_jobs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = fixture_state(root);
        prepare(root, &mut state, "grid city", "16:9", 2, None, &[], Some("City"))
            .unwrap();
        import::save_project(root, state).unwrap();
        let prepared = import::load_state(root).unwrap();
        let rels: Vec<String> = prepared
            .jobs
            .iter()
            .map(|job| job.output_path.clone().unwrap())
            .collect();
        for rel in &rels {
            let absolute = root.join(rel);
            fs::create_dir_all(absolute.parent().unwrap()).unwrap();
            fs::write(&absolute, b"\x89PNG\r\n\x1a\n").unwrap();
            let note = on_still_saved(Some(root.to_str().unwrap()), &absolute.to_string_lossy())
                .unwrap();
            assert!(note.contains("Registered generated image"));
        }
        let settled = import::load_state(root).unwrap();
        let generated: Vec<_> = settled
            .assets
            .iter()
            .filter(|asset| asset.role == AssetRole::GeneratedImage)
            .collect();
        assert_eq!(generated.len(), 2);
        for asset in &generated {
            assert_eq!(asset.status, AssetStatus::Draft);
            assert_eq!(asset.scene_id, None);
            assert_eq!(asset.cut_id, None);
            assert!(asset.source_job_id.is_some());
            // The run id survives on the matching job so results can be
            // grouped by run from the source job.
            let source_job = settled
                .jobs
                .iter()
                .find(|job| job.id == asset.source_job_id.clone().unwrap())
                .unwrap();
            assert_eq!(source_job.raw_provider_id.as_deref(), Some("run_1"));
            assert!(source_job.output_asset_ids.contains(&asset.id));
        }
        assert!(settled
            .jobs
            .iter()
            .all(|job| job.status == JobStatus::Completed));
        // Idempotent: a second settlement changes nothing.
        let first = settled.clone();
        let note = on_still_saved(
            Some(root.to_str().unwrap()),
            &root.join(&rels[0]).to_string_lossy(),
        )
        .unwrap();
        assert!(note.contains("already registered"));
        assert_eq!(import::load_state(root).unwrap().assets, first.assets);
    }

    #[test]
    fn settlement_rejects_non_generated_paths() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = fixture_state(root);
        prepare(root, &mut state, "room", "9:16", 1, None, &[], None).unwrap();
        import::save_project(root, state).unwrap();
        let other = root.join("Preproduction/Scene1/references/other.png");
        fs::create_dir_all(other.parent().unwrap()).unwrap();
        fs::write(&other, b"\x89PNG\r\n\x1a\n").unwrap();
        assert!(on_still_saved(Some(root.to_str().unwrap()), &other.to_string_lossy()).is_none());
        assert_eq!(import::load_state(root).unwrap().assets.len(), 1);
    }

    #[test]
    fn delete_removes_asset_and_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = fixture_state(root);
        prepare(root, &mut state, "room", "9:16", 1, None, &[], None).unwrap();
        import::save_project(root, state).unwrap();
        let prepared = import::load_state(root).unwrap();
        let rel = prepared.jobs[0].output_path.clone().unwrap();
        let absolute = root.join(&rel);
        fs::create_dir_all(absolute.parent().unwrap()).unwrap();
        fs::write(&absolute, b"\x89PNG\r\n\x1a\n").unwrap();
        on_still_saved(Some(root.to_str().unwrap()), &absolute.to_string_lossy()).unwrap();
        let mut settled = import::load_state(root).unwrap();
        let asset_id = settled
            .assets
            .iter()
            .find(|asset| asset.role == AssetRole::GeneratedImage)
            .unwrap()
            .id
            .clone();
        let removed = delete(root, &mut settled, &asset_id).unwrap();
        assert_eq!(removed, rel);
        assert!(!settled.assets.iter().any(|asset| asset.id == asset_id));
        assert!(!absolute.is_file());
        assert!(delete(root, &mut settled, &asset_id).is_err());
    }

    #[test]
    fn studio_library_exists_and_generates_without_any_project() {
        let dir = tempfile::tempdir().unwrap();
        // The real settings dir is <userData>/config with userData named like
        // the app (video-workbench); the library must sit beside userData.
        let settings = dir.path().join("config").join("video-workbench").join("config");
        let ctx = crate::ctx::HostCtx::new(settings);
        let root = studio_root(&ctx);
        assert!(!root.to_string_lossy().contains("video-workbench"));
        assert!(root.starts_with(dir.path().join("config")));
        let empty = ensure_studio_state(&root).unwrap();
        assert!(empty.scenes.is_empty());
        // Load back so the in-memory revision matches the on-disk record.
        let mut state = import::load_state(&root).unwrap();
        prepare(&root, &mut state, "a lighthouse at dusk", "9:16", 2, None, &[], Some("Lighthouse"))
            .unwrap();
        assert_eq!(state.jobs.len(), 2);
        import::save_project(&root, state).unwrap();
        let reloaded = import::load_state(&root).unwrap();
        assert_eq!(reloaded.jobs.len(), 2);
        assert!(reloaded.scenes.is_empty());
        // Settlement registers assets in the studio library.
        let rels: Vec<String> = reloaded.jobs.iter().map(|job| job.output_path.clone().unwrap()).collect();
        for rel in &rels {
            let absolute = root.join(rel);
            fs::create_dir_all(absolute.parent().unwrap()).unwrap();
            fs::write(&absolute, b"\x89PNG\r\n\x1a\n").unwrap();
            assert!(on_still_saved(Some(root.to_str().unwrap()), &absolute.to_string_lossy()).is_some());
        }
        let settled = import::load_state(&root).unwrap();
        assert_eq!(
            settled.assets.iter().filter(|asset| asset.role == AssetRole::GeneratedImage).count(),
            2
        );
        // Second open reuses the same library, still project-free.
        let again = ensure_studio_state(&root).unwrap();
        assert_eq!(again.assets.len(), 2);
    }
}
