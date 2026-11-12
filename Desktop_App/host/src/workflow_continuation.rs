//! Narrow, host-owned continuation of an already approved talking shot.
//! Media consent is still exact-request/one-use; no stage is automatically replayed.
use crate::{ctx::HostCtx, director::{DirectorContext, DirectorTurnResponse}, director_actions::{self, DirectorAction, ValidatedAction}, pending, session};
use crate::workflow::{import, render_jobs, spend::DispatchOutcome, state::JobStatus};
use serde_json::{json, Value};
use std::{collections::HashMap, path::{Path, PathBuf}, sync::{Arc, Mutex, OnceLock}, time::{Duration, Instant}};

pub(crate) struct Parent {
    admission: HostCtx,
    root: PathBuf,
    action: DirectorAction,
    context: DirectorContext,
    inputs: Value,
    epoch: Mutex<u64>,
    output: PathBuf,
    audio: Mutex<Option<PathBuf>>,
    binding: Option<render_jobs::Binding>,
}
#[derive(Clone, Copy, PartialEq)]
enum Stage { Audio, Video, Image }
#[derive(Clone)]
struct Child { parent: Arc<Parent>, stage: Stage, path: PathBuf, prediction: Option<String>, hash: String, job_id: Option<String>, at: Instant }
fn children() -> &'static Mutex<HashMap<String, Child>> {
    static STORE: OnceLock<Mutex<HashMap<String, Child>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}
fn key(root: &Path, hash: &str) -> String { format!("{}:{hash}", root.display()) }
fn path(root: &Path, raw: &str) -> Result<PathBuf, String> {
    crate::project_path::resolve(raw, Some(root.to_str().ok_or("invalid root")?), crate::project_path::PathIntent::Write)
        .map(|p| p.into_path_buf()).map_err(|e| e.to_string())
}
fn input(action: &DirectorAction, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| action.inputs.get(*k).and_then(Value::as_str).map(str::to_owned))
}
impl Parent {
    fn valid_inputs(&self) -> Result<(), String> {
        if self.admission.director_cancelled() || session::root().as_ref() != Some(&self.root) {
            return Err("The originating talking-shot operation was stopped or its project changed.".into());
        }
        crate::director::check_workflow_routing(&self.admission, &self.context)?;
        let mut validated = director_actions::validate_action(self.action.clone(), self.root.to_str()).map_err(|e| e.to_string())?;
        director_actions::append_context_attachments(&mut validated, &self.context.attached_paths, self.root.to_str())?;
        if director_actions::snapshot_input_files(&validated)? != self.inputs {
            return Err("An originating talking-shot input changed while permission was pending.".into());
        }
        if self.action.specialist.as_deref() == Some("dialogue_audio") { crate::workflow::dialogue_audio::validate(&self.root, &self.action)?; }
        if let Some(binding) = &self.binding { render_jobs::check_binding(&self.root, binding)?; }
        Ok(())
    }
    pub(crate) fn valid(&self) -> Result<(), String> {
        self.valid_inputs()?;
        if pending::generation() != *self.epoch.lock().expect("workflow epoch") {
            return Err("A newer approval or project transition superseded this continuation.".into());
        }
        Ok(())
    }
}

pub(crate) fn attach(app: &HostCtx, context: &DirectorContext, validated: &ValidatedAction, epoch: u64) -> Result<Option<Arc<Parent>>, String> {
    if !matches!(validated.action.specialist.as_deref(), Some("text_sync" | "model_sheet" | "dialogue_audio")) || validated.action.inputs.contains_key("collect_prediction_id") { return Ok(None); }
    let Some(root) = session::root() else { return Ok(None) };
    let Some(raw) = input(&validated.action, &["output_video", "output_path", "save_path"]) else { return Ok(None) };
    if !root.join(crate::workflow::state::STATE_REL).is_file() { return Ok(None); }
    let (binding, output) = if validated.action.specialist.as_deref() == Some("model_sheet") {
        let Some(output) = crate::specialists::model_sheet_pending_output(&validated.action.inputs, root.to_str())? else { return Ok(None) };
        (None, output)
    } else if validated.action.specialist.as_deref() == Some("dialogue_audio") {
        (None, crate::workflow::dialogue_audio::validate(&root, &validated.action)?)
    } else {
        let Some(binding) = render_jobs::pending_binding(&root, &raw)? else { return Ok(None) };
        (Some(binding), path(&root, &raw)?)
    };
    Ok(Some(Arc::new(Parent { admission: app.clone(), root, action: validated.action.clone(), context: context.clone(), inputs: director_actions::snapshot_input_files(validated)?, epoch: Mutex::new(epoch), output, audio: Mutex::new(None), binding })))
}
pub(crate) fn set_outputs(app: &HostCtx, video: &Path, audio: &Path) -> Result<(), String> {
    if let Some(parent) = &app.workflow_parent {
        if video != parent.output || audio.parent() != video.parent() || audio == video {
            return Err("Talking-shot audio/video outputs do not match the approved shot bucket.".into());
        }
        *parent.audio.lock().expect("audio output") = Some(audio.to_path_buf());
    }
    Ok(())
}
pub(crate) fn defer(app: &HostCtx, root: &Path, hash: &str, operation: &str, save: Option<&str>) -> Result<(), String> {
    let Some(parent) = &app.workflow_parent else { return Ok(()) };
    parent.valid()?;
    if root != parent.root { return Err("Deferred media belongs to a different project.".into()); }
    let destination = path(root, save.ok_or("Deferred talking-shot media needs an output")?)?;
    let stage = if operation == "atlas_generate_audio" && parent.action.specialist.as_deref() == Some("dialogue_audio") && destination == parent.output {
        Stage::Audio
    } else if operation == "atlas_generate_video" && destination == parent.output { Stage::Video }
    else if operation == "atlas_generate_image" && parent.action.specialist.as_deref() == Some("model_sheet") && destination == parent.output { Stage::Image }
    else { return Err("Deferred media does not match its originating talking-shot stage.".into()); };
    if destination.exists() { return Err("Deferred media output already exists; review/reuse the saved result instead.".into()); }
    let mut store = children().lock().expect("workflow children");
    store.retain(|_, c| c.at.elapsed() < Duration::from_secs(900));
    if store.len() >= 32 { return Err("Too many deferred talking shots; stop stale work first.".into()); }
    let id = key(root, hash);
    if let Some(old) = store.get(&id) {
        if !Arc::ptr_eq(&old.parent, parent) { return Err("An exact media request is already owned by another paused operation.".into()); }
    }
    store.insert(id, Child { parent: parent.clone(), stage, path: destination, prediction: None, hash: hash.into(), job_id: None, at: Instant::now() });
    Ok(())
}
pub(crate) fn before_confirmation(root: &Path, hash: &str) -> Result<(), String> {
    let child = children().lock().expect("workflow children").get(&key(root, hash)).cloned();
    if let Some(child) = child {
        child.parent.valid()?;
        if child.path.exists() { return Err("A saved result appeared while confirmation was pending; refusing to overwrite or repeat it.".into()); }
    }
    Ok(())
}
fn publish(parent: &Arc<Parent>, action: DirectorAction, origin: Option<String>) -> Result<String, String> {
    parent.valid()?;
    let raw = crate::director::prepare_workflow_approval(&parent.admission, &parent.context, &action)?;
    parent.valid_inputs()?;
    pending::publish_guarded(&raw, *parent.epoch.lock().expect("workflow epoch"), origin, || {
        !parent.admission.director_cancelled() && session::root().as_ref() == Some(&parent.root)
    })
}
fn continuation(parent: &Arc<Parent>, stage: Stage, audio: &Path) -> Result<DirectorTurnResponse, String> {
    if stage == Stage::Image { return settle_image(parent, audio, None, true, None); }
    if stage == Stage::Audio {
        parent.valid()?;
        crate::workflow::dialogue_audio::settle_guarded(&parent.root, &parent.action, audio, || parent.valid())?;
        Ok(crate::director::workflow_response(format!("Dialogue audio saved as Draft to `{}`. Audition and select the complete WAV in Scene Look. Video requires separate preparation and approval; nothing advances automatically.", audio.display())))
    } else {
        if let Some(binding) = &parent.binding { render_jobs::finish_collected(&parent.root, binding)?; }
        Ok(crate::director::workflow_response(format!("Talking shot saved to `{}`. No speech or video was resubmitted.", parent.output.display())))
    }
}
fn settle_image(parent: &Arc<Parent>, output: &Path, prediction: Option<&str>, saved: bool, error: Option<&str>) -> Result<DirectorTurnResponse, String> {
    parent.valid()?;
    let (message, next) = crate::specialists::settle_model_sheet_plate(&parent.action, &parent.root, output, prediction, saved, error)?;
    let card = if let Some(action) = next {
        let raw = crate::director::prepare_workflow_approval(&parent.admission, &parent.context, &action)?;
        pending::publish_guarded(&raw, *parent.epoch.lock().expect("workflow epoch"), None, || !parent.admission.director_cancelled() && session::root().as_ref() == Some(&parent.root))?
    } else { String::new() };
    Ok(crate::director::workflow_response(format!("{message}\n\n{card}")))
}
pub(crate) fn after_generation(app: &HostCtx, root: &Path, hash: &str, result: &Result<DispatchOutcome, String>) {
    let id = key(root, hash);
    let Some(mut child) = children().lock().expect("workflow children").get(&id).cloned() else { return };
    if child.stage == Stage::Image {
        let outcome = (|| -> Result<DirectorTurnResponse, String> {
            let state = import::load_state(root)?;
            let job = state.jobs.iter().find(|j| j.request_metadata.as_ref().is_some_and(|m| m.request_hash == hash));
            let prediction = job.and_then(|j| j.request_metadata.as_ref()).and_then(|m| m.prediction_id.as_deref());
            let saved = matches!(result, Ok(DispatchOutcome::Completed(_))) && job.is_some_and(|j| j.status == JobStatus::Completed);
            settle_image(&child.parent, &child.path, prediction, saved, result.as_ref().err().map(String::as_str))
        })();
        children().lock().expect("workflow children").remove(&id);
        let response = outcome.unwrap_or_else(|e| crate::director::workflow_response(format!("Model-sheet continuation held without resubmission: {e}")));
        app.set_workflow_reply(serde_json::to_value(response).expect("workflow response"));
        return;
    }
    if let Err(error) = result {
        if child.stage == Stage::Audio { let _ = crate::workflow::dialogue_audio::fail(root, &child.parent.action, error); }
        children().lock().expect("workflow children").remove(&id); return;
    }
    let outcome = (|| -> Result<DirectorTurnResponse, String> {
        child.parent.valid()?;
        let state = import::load_state(root)?;
        let jobs: Vec<_> = state.jobs.iter().filter(|j| j.request_metadata.as_ref().is_some_and(|m| m.request_hash == hash)).collect();
        if jobs.len() != 1 { return Err("Deferred child outcome is not uniquely attributable; inspect Jobs without resubmitting.".into()); }
        child.job_id = Some(jobs[0].id.clone());
        child.prediction = jobs[0].request_metadata.as_ref().and_then(|m| m.prediction_id.clone());
        let completed = matches!(result, Ok(DispatchOutcome::Completed(_))) && jobs[0].status == JobStatus::Completed;
        let delivered = completed && child.path.is_file() && std::fs::metadata(&child.path).map(|m| m.len() > 0).unwrap_or(false);
        if delivered {
            children().lock().expect("workflow children").remove(&id);
            return continuation(&child.parent, child.stage, &child.path);
        }
        if child.path.exists() { return Err("A file exists but delivery is not verified; inspect it without resubmission".into()); }
        let pred = child.prediction.clone().ok_or("Remote outcome is not verified; no automatic continuation or retry")?;
        let mut collect = child.parent.action.clone();
        collect.inputs.insert("collect_prediction_id".into(), json!(pred));
        collect.inputs.insert("output_video".into(), json!(child.path));
        let card = publish(&child.parent, collect, Some(id.clone()))?;
        children().lock().expect("workflow children").insert(id.clone(), child.clone());
        Ok(crate::director::workflow_response(format!("The existing prediction is still awaiting verified delivery. This approval only collects that same prediction; it does not upload or generate again.\n\n{card}")))
    })();
    let response = match outcome {
        Ok(v) => v,
        Err(e) => { children().lock().expect("workflow children").remove(&id); crate::director::workflow_response(format!("The media operation settled, but its parent continuation was not issued: {e}")) }
    };
    app.set_workflow_reply(serde_json::to_value(response).expect("workflow response"));
}
pub(crate) fn admit_collect(id: &str, app: &HostCtx, epoch: u64, action: &DirectorAction) -> Result<(), String> {
    let child = children().lock().expect("workflow children").get(id).cloned().ok_or("The deferred collection is no longer active; inspect Jobs")?;
    child.parent.valid_inputs()?;
    if input(action, &["collect_prediction_id"]) != child.prediction || input(action, &["output_video"]).as_deref() != child.path.to_str() {
        return Err("Collection approval does not match its exact deferred prediction/output.".into());
    }
    if app.director_cancelled() { return Err("Collection was stopped".into()); }
    *child.parent.epoch.lock().expect("workflow epoch") = epoch;
    Ok(())
}

pub(crate) fn before_collect(root: &Path, pred: &str, save: &str) -> Result<(),String> {
    let target=path(root,save)?;
    let matches:Vec<_>=children().lock().expect("workflow children").values().filter(|c|c.parent.root==root && c.prediction.as_deref()==Some(pred)).cloned().collect();
    if matches.is_empty() { return Ok(()); }
    if matches.len()!=1 || matches[0].path!=target { return Err("Collection does not match one exact deferred output/prediction".into()); }
    matches[0].parent.valid()?;
    if target.exists() { return Err("A collection output already exists; inspect the saved result without overwriting it".into()); }
    Ok(())
}
fn settle_child(root:&Path,child:&Child,error:Option<&str>)->Result<(),String> {
    let _lock=import::lock_project(root)?;
    let mut state=render_jobs::read_persisted(root)?;
    let js:Vec<_>=state.jobs.iter().enumerate().filter(|(_,j)|Some(j.id.as_str())==child.job_id.as_deref()).map(|(i,_)|i).collect();
    if js.len()!=1 { return Err("Deferred provider job disappeared or is ambiguous".into()); }
    let j=&mut state.jobs[js[0]];
    let kind=match child.stage { Stage::Audio => "atlas_generate_audio", Stage::Image => "atlas_generate_image", Stage::Video => "atlas_generate_video" };
    if !j.request_metadata.as_ref().is_some_and(|m|m.request_hash==child.hash && m.prediction_id==child.prediction) || j.kind!=kind { return Err("Deferred provider job ownership changed".into()); }
    if let Some(error)=error { j.fail(None,error.to_string()); }
    else if j.status==JobStatus::Running { j.finish(None,None); }
    else if j.status!=JobStatus::Completed { return Err("Deferred provider job is no longer collectable".into()); }
    import::persist_state_locked(root,&state)?; Ok(())
}
pub(crate) fn after_collect(app: &HostCtx, root: &Path, pred: &str, save: &str, result: &Result<String, String>) {
    let target=match path(root,save){Ok(p)=>p,Err(_)=>return};
    let entries:Vec<_>=children().lock().expect("workflow children").iter().filter(|(_,c)|c.parent.root==root && c.prediction.as_deref()==Some(pred) && c.path==target).map(|(k,c)|(k.clone(),c.clone())).collect();
    if entries.len()!=1 { return; }
    let (id,child)=entries[0].clone();
    let outcome=(||->Result<DirectorTurnResponse,String>{
        child.parent.valid()?;
        if let Err(error)=result {
            if error.starts_with("provider-failed:") {
                settle_child(root,&child,Some(error))?;
                if child.stage == Stage::Audio { crate::workflow::dialogue_audio::fail(root,&child.parent.action,error)?; }
                if let Some(binding) = &child.parent.binding { render_jobs::fail_collected(root,binding,error)?; }
                children().lock().expect("workflow children").remove(&id);
                return Ok(crate::director::workflow_response("The existing provider prediction failed. No replacement was submitted.".into()));
            }
        }
        let delivered=result.is_ok() && target.is_file() && std::fs::metadata(&target).map(|m|m.len()>0).unwrap_or(false);
        if delivered {
            settle_child(root,&child,None)?;
            children().lock().expect("workflow children").remove(&id);
            return continuation(&child.parent,child.stage,&target);
        }
        if target.exists() { return Err("Partial or unverified collection output exists; inspect it rather than repeat or overwrite".into()); }
        let mut collect=child.parent.action.clone();
        collect.inputs.insert("collect_prediction_id".into(),json!(pred));
        collect.inputs.insert("output_video".into(),json!(target));
        let card=publish(&child.parent,collect,Some(id.clone()))?;
        Ok(crate::director::workflow_response(format!("The same prediction is not delivered yet. This fresh approval only collects it; it never repeats speech, uploads or video generation.\n\n{card}")))
    })();
    let response=outcome.unwrap_or_else(|e|crate::director::workflow_response(format!("Collection held without resubmission: {e}")));
    app.set_workflow_reply(serde_json::to_value(response).expect("workflow response"));
}
