//! Prepared render state follows the single already-authorized media dispatch.
use super::{import, state::{JobRecord, JobStatus, ProjectState, STATE_REL}};
use crate::project_path::{self, PathIntent};
use std::path::{Path, PathBuf};

/// Observation only: no import, repair, lock creation, or state write.
pub(crate) fn read_persisted(root: &Path) -> Result<ProjectState, String> {
    let root_text = root.to_str().ok_or("invalid project root")?;
    let path = project_path::resolve(STATE_REL, Some(root_text), PathIntent::Read)
        .map_err(|e| e.to_string())?.into_path_buf();
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read persisted state: {e}"))?;
    ProjectState::from_json(&text)
}

fn output(root: &Path, raw: &str) -> Result<(PathBuf, String), String> {
    let path = project_path::resolve(raw, Some(root.to_str().ok_or("invalid project root")?), PathIntent::Write)
        .map_err(|e| e.to_string())?.into_path_buf();
    let rel = project_path::relative_to_root(root, &path)?;
    Ok((path, rel.to_string_lossy().into_owned()))
}

fn validate_binding(state: &ProjectState, job: &JobRecord, rel: &str) -> Result<String, String> {
    let scope = job.scope.as_ref().filter(|scope| scope.kind == "shot").ok_or("prepared render needs a shot scope")?;
    let mut cuts = state.scenes.iter().flat_map(|scene| scene.cuts.iter().map(move |cut| (scene, cut)))
        .filter(|(_, cut)| cut.stable_id == scope.id);
    let (scene, cut) = cuts.next().ok_or("prepared render shot no longer exists")?;
    if cuts.next().is_some() { return Err("prepared render shot identity is ambiguous".into()); }
    let parts: Vec<_> = rel.split('/').collect();
    if parts.len() != 4 || parts[0] != "Scenes" || parts[1] != format!("Scene{}", scene.id)
        || parts[2] != format!("Cut{}", cut.id.cut) || parts[3].is_empty()
        || cut.id.source_scene != scene.id || job.output_path.as_deref() != Some(rel)
        || job.raw_provider_id.as_deref() != Some(cut.id.code().as_str()) {
        return Err("prepared render output or CutId does not match its exact shot bucket".into());
    }
    Ok(scope.id.clone())
}

#[derive(Clone)]
pub(crate) struct Binding { pub(crate) id: String, pub(crate) path: String, pub(crate) shot: String }

fn begin(root: &Path, raw: &str) -> Result<Option<Binding>, String> {
    let (requested, rel) = output(root, raw)?;
    let _lock = import::lock_project(root)?;
    let mut state = read_persisted(root)?;
    // Resolve saved destinations too: a same-project symlink redirect must not
    // turn a prepared operation into the generic-media no-match fallback.
    let mut matches = Vec::new();
    for (index, job) in state.jobs.iter().enumerate().filter(|(_, j)| j.kind == "render") {
        let Some(saved) = job.output_path.as_deref() else { continue };
        match output(root, saved) {
            Ok((path, canonical)) if path == requested => {
                if saved != canonical { return Err("prepared render output was redirected".into()); }
                matches.push(index);
            }
            Err(error) if saved == raw || saved == rel => return Err(error),
            _ => {}
        }
    }
    if matches.is_empty() { return Ok(None); }
    if matches.len() != 1 { return Err("ambiguous pending render outputs or retained attempts".into()); }
    let index = matches[0];
    if state.jobs[index].status != JobStatus::Pending { return Err("prepared render destination is already reserved by an existing attempt".into()); }
    if requested.exists() { return Err("prepared render destination already exists; prepare a new take".into()); }
    let shot = validate_binding(&state, &state.jobs[index], &rel)?;
    let binding = Binding { id: state.jobs[index].id.clone(), path: rel, shot };
    state.jobs[index].mark_running("actual authorized dispatch in flight");
    import::persist_state_locked(root, &state)?;
    Ok(Some(binding))
}

fn finish<T>(root: &Path, binding: &Binding, result: &Result<T, String>) -> Result<(), String> {
    let _lock = import::lock_project(root)?;
    let mut state = read_persisted(root)?;
    let indices: Vec<_> = state.jobs.iter().enumerate().filter(|(_, job)| job.id == binding.id).map(|(i, _)| i).collect();
    if indices.len() != 1 { return Err("prepared render job disappeared or became ambiguous".into()); }
    let index = indices[0];
    let job = &state.jobs[index];
    if job.kind != "render" || !matches!(job.status, JobStatus::Running | JobStatus::Completed)
        || validate_binding(&state, job, &binding.path)? != binding.shot {
        return Err("prepared render binding changed during dispatch".into());
    }
    let (actual, rel) = output(root, &binding.path)?;
    if rel != binding.path { return Err("prepared render output was redirected during dispatch".into()); }
    let job = &mut state.jobs[index];
    match result {
        Err(error) => job.fail(None, error.clone()),
        Ok(_) if actual.is_file() => job.finish(Some(&binding.path), None),
        Ok(_) => {
            if job.status == JobStatus::Completed { return Err("completed render output disappeared".into()); }
            job.note_progress("awaiting verified output asset; reconciliation required");
        }
    }
    import::persist_state_locked(root, &state)?;
    Ok(())
}

/// This wrapper does not grant permission. Its sole caller is inside the media
/// reservation's authorized dispatch closure, after exact-request replay checks.
pub fn with_render_job<F>(root: &Path, save_path: &str, dispatch: F) -> Result<String, String>
where F: FnOnce() -> Result<String, String> {
    with_render_job_outcome(root, save_path, dispatch)
}

/// Preserve the adapter's typed result through the existing bound lifecycle.
pub fn with_render_job_outcome<F, T>(root: &Path, save_path: &str, dispatch: F) -> Result<T, String>
where F: FnOnce() -> Result<T, String> {
    let root = crate::session::command_root(Some(root.to_str().ok_or("invalid project root")?))?;
    let binding = begin(&root, save_path)?;
    let result = dispatch();
    if let Some(binding) = binding {
        if let Err(error) = finish(&root, &binding, &result) {
            return Err(match result {
                Err(original) => format!("{original}; render lifecycle readback failed: {error}"),
                Ok(_) => format!("render lifecycle readback failed: {error}; inspect Jobs before retrying"),
            });
        }
    }
    result
}

pub(crate) fn pending_binding(root: &Path, raw: &str) -> Result<Option<Binding>, String> {
    let (actual, rel) = output(root, raw)?; let state = read_persisted(root)?;
    let jobs: Vec<_> = state.jobs.iter().filter(|j| j.kind == "render" && j.output_path.as_deref() == Some(rel.as_str())).collect();
    if jobs.is_empty() { return Ok(None); } if jobs.len()!=1 { return Err("Ambiguous render destination".into()); }
    let job=jobs[0]; if job.status!=JobStatus::Pending || actual.exists() { return Err("Render already submitted or output exists; collect the existing result instead".into()); }
    let shot=validate_binding(&state,job,&rel)?; Ok(Some(Binding{id:job.id.clone(),path:rel,shot}))
}
pub(crate) fn check_binding(root: &Path, b: &Binding) -> Result<(),String> {
    let s=read_persisted(root)?; let js:Vec<_>=s.jobs.iter().filter(|j|j.id==b.id).collect();
    if js.len()!=1 || js[0].kind!="render" || validate_binding(&s,js[0],&b.path)?!=b.shot { return Err("Render ownership changed".into()); } Ok(())
}
pub(crate) fn finish_collected(root: &Path,b: &Binding)->Result<(),String> {
    check_binding(root,b)?; finish(root,b,&Ok::<(),String>(()))?;
    import::import_project(root, import::ImportOptions{persist:true})?; Ok(())
}

pub(crate) fn fail_collected(root:&Path,b:&Binding,error:&str)->Result<(),String>{ check_binding(root,b)?;finish::<()>(root,b,&Err(error.to_string())) }
