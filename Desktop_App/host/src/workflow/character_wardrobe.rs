//! Character-owned wardrobe references, independent of scene look boards.
use super::{
    import, look,
    state::{AssetRecord, AssetRole, AssetStatus, JobRecord, JobStatus, ProjectState},
};
use crate::director_actions::{validate_action, ActionKind, DirectorAction};
use serde_json::{json, Map};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn character<'a>(
    state: &'a ProjectState,
    id: &str,
) -> Result<&'a super::state::CharacterRecord, String> {
    state
        .characters
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| format!("character `{id}` not found"))
}
fn destination(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let dest = crate::project_path::resolve(
        rel,
        Some(&root.to_string_lossy()),
        crate::project_path::PathIntent::Write,
    )
    .map_err(|e| e.to_string())?
    .into_path_buf();
    if crate::project_path::relative_to_root(root, &dest)? != Path::new(rel) {
        return Err("Wardrobe destination redirected".into());
    }
    Ok(dest)
}
fn next_path(root: &Path, state: &ProjectState, ext: &str) -> String {
    for i in 1.. {
        let rel = format!("Preproduction/CharacterWardrobe/wardrobe_{i:06}.{ext}");
        let stem = format!("Preproduction/CharacterWardrobe/wardrobe_{i:06}.");
        let disk_taken = ["png", "jpg", "jpeg", "webp", "gif"]
            .iter()
            .any(|e| root.join(format!("{stem}{e}")).symlink_metadata().is_ok());
        if !disk_taken
            && !state.assets.iter().any(|a| a.path.starts_with(&stem))
            && !state.jobs.iter().any(|j| {
                j.output_path
                    .as_deref()
                    .is_some_and(|p| p.starts_with(&stem))
            })
        {
            return rel;
        }
    }
    unreachable!()
}
fn add_asset(
    state: &mut ProjectState,
    rel: &str,
    character_id: &str,
    job: Option<String>,
) -> String {
    let id = state.push_asset(AssetRecord {
        id: String::new(),
        path: rel.into(),
        role: AssetRole::WardrobeReference,
        status: AssetStatus::Draft,
        scene_id: None,
        cut_id: None,
        character_id: Some(character_id.into()),
        source_job_id: job,
        parent_asset_id: None,
        notes: Some("character wardrobe reference".into()),
    });
    if let Some(c) = state.characters.iter_mut().find(|c| c.id == character_id) {
        if !c.asset_ids.contains(&id) {
            c.asset_ids.push(id.clone());
        }
    }
    id
}
pub fn attach(
    root: &Path,
    state: &mut ProjectState,
    id: &str,
    source: &str,
) -> Result<String, String> {
    character(state, id)?;
    let source =
        crate::session::authorize_user_path(source, crate::project_path::PathIntent::Read)?;
    let ext = source
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif") {
        return Err("Choose an image".into());
    }
    crate::media_util::ensure_media_file_as(&source, crate::media_util::MediaRole::Image)?;
    let rel = next_path(root, state, &ext);
    let dest = destination(root, &rel)?;
    fs::create_dir_all(dest.parent().ok_or("Invalid destination")?).map_err(|e| e.to_string())?;
    let dest = destination(root, &rel)?;
    use std::io::Write;
    let bytes = fs::read(source).map_err(|e| e.to_string())?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&dest)
        .map_err(|e| e.to_string())?;
    if let Err(e) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(dest);
        return Err(e.to_string());
    }
    Ok(add_asset(state, &rel, id, None))
}
pub fn prepare(
    app: &crate::ctx::HostCtx,
    root: &Path,
    state: &mut ProjectState,
    id: &str,
    prompt: &str,
    refs: &[String],
) -> Result<(String, String), String> {
    let name = character(state, id)?.name.clone();
    if prompt.trim().is_empty() {
        return Err("Enter a wardrobe prompt".into());
    }
    let selected: Vec<_> = refs
        .iter()
        .map(|r| {
            let reference = look::selected_look_reference(root, state, 0, r)?;
            let path = crate::project_path::resolve(
                &reference.1,
                Some(&root.to_string_lossy()),
                crate::project_path::PathIntent::Read,
            )
            .map_err(|e| e.to_string())?
            .into_path_buf();
            crate::media_util::ensure_media_file_as(&path, crate::media_util::MediaRole::Image)?;
            Ok(reference)
        })
        .collect::<Result<_, String>>()?;
    let rel = next_path(root, state, "png");
    let dest = destination(root, &rel)?;
    fs::create_dir_all(dest.parent().ok_or("Invalid destination")?).map_err(|e| e.to_string())?;
    destination(root, &rel)?;
    let mut inputs = Map::new();
    inputs.insert("prompt".into(), json!(prompt));
    inputs.insert("output_path".into(), json!(rel));
    if let Some(aspect) = &state.settings.aspect_ratio {
        inputs.insert("aspect_ratio".into(), json!(aspect));
    }
    if !selected.is_empty() {
        inputs.insert(
            "reference_images".into(),
            json!(selected.iter().map(|(_, p)| p).collect::<Vec<_>>()),
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
    let root_s = root.to_string_lossy();
    let validated = validate_action(action.clone(), Some(&root_s)).map_err(|e| e.to_string())?;
    let details =
        crate::director::format_approval_request_for_app(app, &action, &validated, Some(&root_s))?;
    let mut job = JobRecord {
        input_asset_ids: refs.to_vec(),
        output_path: Some(rel.clone()),
        raw_provider_id: Some(id.into()),
        ..JobRecord::open(state.next_job_id(), "character_wardrobe")
    };
    if let Ok(Some(e)) = crate::director::estimate_for_action(app, &action, Some(&root_s)) {
        job.provider = Some(e.kind);
        job.model = e.model;
        job.currency = Some(e.currency);
        job.estimated_cost_minor = Some(e.minor);
    }
    state.jobs.push(job);
    Ok((
        format!("Character wardrobe · {name}\nHost will save to `{rel}`.\n\n{details}"),
        "Character wardrobe ready for one-time approval.".into(),
    ))
}
pub(crate) fn register(state: &mut ProjectState, root: &Path, requested: &str) -> Option<String> {
    let root_s = root.to_string_lossy();
    let path = crate::project_path::resolve(
        requested,
        Some(&root_s),
        crate::project_path::PathIntent::Read,
    )
    .ok()?
    .into_path_buf();
    let path = if path.is_file() {
        path
    } else {
        let parent = path.parent()?;
        let stem = path.file_stem()?;
        fs::read_dir(parent)
            .ok()?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p.file_stem() == Some(stem) && p.is_file())?
    };
    let path = crate::project_path::resolve(
        &path.to_string_lossy(),
        Some(&root_s),
        crate::project_path::PathIntent::Read,
    )
    .ok()?
    .into_path_buf();
    crate::media_util::ensure_media_file_as(&path, crate::media_util::MediaRole::Image).ok()?;
    let rel = crate::project_path::relative_to_root(root, &path)
        .ok()?
        .to_string_lossy()
        .replace('\\', "/");
    let stem = Path::new(&rel).with_extension("");
    let index = state.jobs.iter().position(|j| {
        j.kind == "character_wardrobe"
            && j.status == JobStatus::Pending
            && j.output_path
                .as_deref()
                .is_some_and(|p| Path::new(p).with_extension("") == stem)
    })?;
    let id = state.jobs[index].raw_provider_id.clone()?;
    character(state, &id).ok()?;
    let jobid = state.jobs[index].id.clone();
    let asset = if let Some(a) = state.assets.iter().find(|a| a.path == rel) {
        a.id.clone()
    } else {
        add_asset(state, &rel, &id, Some(jobid))
    };
    state.jobs[index].finish(Some(&rel), Some(&asset));
    Some(format!(
        "Registered character wardrobe `{asset}` at `{rel}`."
    ))
}
pub fn on_saved(project: Option<&str>, path: &str) -> Option<String> {
    let root = Path::new(project?);
    let mut state = import::load_state(root).ok()?;
    let note = register(&mut state, root, path)?;
    import::save_project(root, state).ok()?;
    Some(note)
}
pub fn on_failed(project: Option<&str>, path: &str, error: &str) -> Option<String> {
    let root = Path::new(project?);
    let mut state = import::load_state(root).ok()?;
    let rel = crate::project_path::relative_to_root(root, Path::new(path))
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| path.into());
    let job = state.jobs.iter_mut().find(|j| {
        j.kind == "character_wardrobe"
            && j.status == JobStatus::Pending
            && j.output_path.as_deref() == Some(&rel)
    })?;
    job.fail(None, error);
    import::save_project(root, state).ok()?;
    Some(format!("Character wardrobe generation failed: {error}"))
}
