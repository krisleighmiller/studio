//! Explicit local audition selection and video-only preparation. Never submits media.
use super::{import, production, render_jobs, state::{AssetRecord, JobRecord, JobScope, JobStatus, ProjectState}};
use crate::scene_plan::handoff::{CutId, SpeechMode};
use crate::{ctx::HostCtx, director_actions::{ActionKind, DirectorAction}, project_path::{self, PathIntent}, session};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

fn root(args: &Value) -> Result<PathBuf, String> {
    session::command_root(args.get("projectRoot").and_then(Value::as_str))
}
fn number(args: &Value, key: &str) -> Result<u32, String> {
    args.get(key).and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()).filter(|n| *n > 0).ok_or_else(||format!("Invalid {key}"))
}
fn shot(args: &Value) -> Result<CutId, String> { Ok(CutId::new(number(args,"sceneId")?,number(args,"cut")?)) }
fn cut(state: &ProjectState, id: CutId) -> Result<&super::state::CutRecord,String> {
    state.scene(id.source_scene).and_then(|s| s.cuts.iter().find(|c| c.id == id)).ok_or_else(||"Performance shot not found".into())
}
/// A version is eligible only in the exact canonical shot bucket, never another shot or a redirected path.
pub(super) fn asset<'a>(root: &Path, state: &'a ProjectState, id: CutId, asset_id: &str) -> Result<&'a AssetRecord,String> {
    let a=state.asset(asset_id).filter(|a| a.role.is_audio()).ok_or("Select a registered audio version")?;
    let path=project_path::resolve(&a.path,root.to_str(),PathIntent::Read).map_err(|e|e.to_string())?.into_path_buf();
    let relative=project_path::relative_to_root(root,&path)?.to_string_lossy().replace('\\',"/");
    let prefix=format!("Scenes/Scene{}/Cut{}/",id.source_scene,id.cut);
    let tail=relative.strip_prefix(&prefix).ok_or("Performance WAV must belong to this exact shot")?;
    if relative!=a.path || tail.contains('/') || !tail.to_ascii_lowercase().ends_with(".wav") || !path.is_file() {
        return Err("Performance WAV is missing or redirected outside its exact shot bucket".into());
    }
    Ok(a)
}
fn valid_audio(root:&Path,a:&AssetRecord)->Result<(),String>{
    use std::io::Read;
    let mut file=std::fs::File::open(root.join(&a.path)).map_err(|e|e.to_string())?;
    if file.metadata().map_err(|e|e.to_string())?.len()>128*1024*1024 {return Err("Performance WAV exceeds the local 128MiB limit".into());}
    let mut header=[0u8;12];file.read_exact(&mut header).map_err(|_|"Performance must be a readable WAV")?;
    if !matches!(&header[..4],b"RIFF"|b"RF64") || &header[8..]!=b"WAVE" {return Err("Performance must be WAV media, not a renamed container".into());}
    let findings=super::qc::check_audio_decode(&root.join(&a.path));
    if let Some(finding)=findings.iter().find(|f| f.severity==super::qc::QCSeverity::Error) {
        return Err(format!("Select a complete decodable WAV; no speech will be regenerated. {}",finding.message));
    }
    Ok(())
}
fn result(mut state:ProjectState,note:&str)->Value {
    super::recipe::apply_stages(&mut state);
    json!({"report":super::report::project_report(&state),"state":state,"note":note})
}
fn unsettled(state:&ProjectState)->bool {
    state.jobs.iter().any(|j|(j.kind.starts_with("atlas_generate_") || j.kind=="render") && j.status==JobStatus::Running
        || j.kind.starts_with("atlas_generate_") && j.status==JobStatus::Pending)
}
fn cancel_prepared(state:&mut ProjectState,shot:&str){
    for job in &mut state.jobs {
        if job.kind=="render" && job.status==JobStatus::Pending && job.performance_asset_id.is_some()
            && job.scope.as_ref().is_some_and(|s|s.kind=="shot" && s.id==shot) {
            job.status=JobStatus::Cancelled;job.completed_at=Some(super::state::utc_now());
            job.progress=Some("Audio selection changed; unsubmitted video card invalidated".into());
            for attempt in &mut job.attempts { if attempt.status==JobStatus::Pending {attempt.status=JobStatus::Cancelled;attempt.completed_at=job.completed_at.clone();} }
        }
    }
}
pub fn select(args:&Value)->Result<Value,String>{
    let root=root(args)?;let id=shot(args)?;
    let _lock=import::lock_project(&root)?;
    let mut state=render_jobs::read_persisted(&root)?;
    let selected=match args.get("assetId") { Some(Value::Null)=>None,Some(Value::String(s)) if !s.is_empty()=>Some(s.as_str()),_=>return Err("assetId must be an exact version ID or explicit null".into()) };
    let current=cut(&state,id)?;
    if current.speech_mode!=Some(SpeechMode::SourceVideoLipsync) {return Err("Performance selection requires a talking shot".into());}
    if let Some(selected)=selected {
        if args.get("reviewedAssetId").and_then(Value::as_str)!=Some(selected) {
            return Err("Review the full dialogue against the current cut and production note, then confirm this exact WAV version before selecting it".into());
        }
        let a=asset(&root,&state,id,selected)?;valid_audio(&root,a)?;
    }
    if current.selected_performance_asset_id.as_deref()==selected {return Ok(result(state,"This exact WAV is already selected; no approval or job changed."));}
    if unsettled(&state) {return Err("A media operation is unsettled; collect it before changing audio".into());}
    let stable=current.stable_id.clone();
    cancel_prepared(&mut state,&stable);
    state.scene_mut(id.source_scene,"").cut_mut(id.cut).selected_performance_asset_id=selected.map(str::to_string);
    import::persist_state_locked(&root,&state)?;
    // An old one-use card may never survive a different explicit creative selection.
    crate::pending::cancel_all();
    Ok(result(render_jobs::read_persisted(&root)?,"Performance selection saved. Older approval invalidated; prepare a fresh video-only card. No speech/video submitted."))
}
pub fn version(args:&Value)->Result<Value,String>{
    let root=root(args)?;let id=shot(args)?;let state=render_jobs::read_persisted(&root)?;cut(&state,id)?;
    let a=asset(&root,&state,id,args.get("assetId").and_then(Value::as_str).ok_or("Select a source WAV")?)?;
    let note = match (args.get("speed"), args.get("pitch"), args.get("trim")) {
        (Some(speed), None, None) => {
            super::audio_edit::version_speed(&root,&a.path,speed.as_f64().ok_or("Speed must be a number")?)?;
            "Created a draft local speed version (pitch preserved). Audition and select it explicitly before video. No provider request."
        }
        (None, Some(pitch), None) => {
            super::audio_edit::version_pitch(&root,&a.path,pitch.as_f64().ok_or("Pitch must be a number")?)?;
            "Created a draft local pitch version (duration approximately preserved). Audition and select it explicitly before video. No provider request."
        }
        (None, None, Some(trim)) => {
            let start=trim.get("startSeconds").and_then(Value::as_f64).ok_or("Trim start must be a number")?;
            let end=trim.get("endSeconds").and_then(Value::as_f64).ok_or("Trim end must be a number")?;
            super::audio_edit::version_trim(&root,&a.path,start,end)?;
            "Created a draft local WAV source-window trim. Audition and select it explicitly before video. Check the complete dialogue; no provider request."
        }
        _ => return Err("Supply exactly one of speed, pitch or trim".into()),
    };
    Ok(result(render_jobs::read_persisted(&root)?,note))
}
/// Recheck the prepared exact-version binding before text_sync can copy or synthesize audio.
/// A generic/planner action must not reuse this reserved output with different audio or TTS.
pub fn validate_request(root: &Path, output: &Path, performance: Option<&str>) -> Result<(), String> {
    match std::fs::symlink_metadata(root.join(super::state::STATE_REL)) {
        Err(e) if e.kind()==std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.to_string()),
        Ok(_) => {}
    }
    let state=render_jobs::read_persisted(root)?;
    let relative=project_path::relative_to_root(root,output)?.to_string_lossy().replace('\\',"/");
    for job in state.jobs.iter().filter(|j|j.kind=="render" && j.output_path.as_deref()==Some(relative.as_str())) {
        let Some(selected)=job.performance_asset_id.as_deref() else {continue};
        if job.status!=JobStatus::Pending {return Err("Video-only destination is already reserved by a submitted or cancelled attempt".into());}
        let scope=job.scope.as_ref().ok_or("Video-only shot binding missing")?;
        let c=state.scenes.iter().flat_map(|s|&s.cuts).find(|c|c.stable_id==scope.id).ok_or("Video-only shot missing")?;
        if c.selected_performance_asset_id.as_deref()!=Some(selected) {return Err("Performance selection changed; prepare a fresh video-only card".into());}
        let a=asset(root,&state,c.id,selected)?;
        let supplied=performance.ok_or("Video-only reservation forbids speech synthesis; use its exact selected performance WAV")?;
        let actual=project_path::resolve(supplied,root.to_str(),PathIntent::Read).map_err(|e|e.to_string())?.into_path_buf();
        if actual!=root.join(&a.path) {return Err("Prepared video requires its exact selected WAV version".into());}
        valid_audio(root,a)?;
    }
    Ok(())
}

pub fn prepare(app:&HostCtx,args:&Value)->Result<Value,String>{
    let root=root(args)?;let id=shot(args)?;
    let epoch=crate::pending::generation();
    let mut state=render_jobs::read_persisted(&root)?;
    if unsettled(&state) {return Err("A media operation is unsettled; do not resubmit".into());}
    let c=cut(&state,id)?;
    if c.speech_mode!=Some(SpeechMode::SourceVideoLipsync) {return Err("Video-only performance requires a talking shot".into());}
    let selected=c.selected_performance_asset_id.as_deref().ok_or("Audition and select a complete WAV first")?;
    if args.get("reviewedAssetId").and_then(Value::as_str)!=Some(selected) {
        return Err("Review the full dialogue and timing of this exact selected WAV before preparing a fresh video card".into());
    }
    let a=asset(&root,&state,id,selected)?.clone();valid_audio(&root,&a)?;
    if c.stable_id.is_empty() {return Err("Shot has no stable identity".into());}
    let source=match super::sources::resolve_render_source(&state,id)? { super::sources::RenderSourceResolution::Bound(path)=>path,_=>return Err("Select an approved image before video preparation".into()) };
    let handoff=c.production_note_path.clone().ok_or("Save a production prompt first")?;
    let stable=c.stable_id.clone();
    let existing:Vec<_>=state.jobs.iter().filter(|j|j.kind=="render" && j.status==JobStatus::Pending && j.performance_asset_id.as_deref()==Some(&a.id) && j.scope.as_ref().is_some_and(|s|s.id==stable && s.kind=="shot")).collect();
    if existing.len()>1 {return Err("Ambiguous pending video-only attempts".into());}
    let output=existing.first().and_then(|j|j.output_path.clone()).unwrap_or_else(||production::next_take_path(&root,&state,id));
    let target=project_path::resolve(&output,root.to_str(),PathIntent::Write).map_err(|e|e.to_string())?.into_path_buf();
    if target.exists() || target.with_extension("wav").exists() {return Err("Prepared video/audio destination is occupied; existing media preserved".into());}
    let mut inputs=Map::new();
    inputs.insert("handoff_path".into(),json!(handoff));inputs.insert("source_frame".into(),json!(source));
    inputs.insert("performance_wav".into(),json!(a.path));inputs.insert("output_path".into(),json!(output));
    inputs.insert("output_audio".into(),json!(target.with_extension("wav").to_string_lossy()));
    let action=DirectorAction{action:ActionKind::RunSpecialist,specialist:Some("text_sync".into()),inputs,message:None,op:None,path:None};
    let context=crate::director::current_workflow_context(app,&root)?;
    let raw=crate::director::prepare_workflow_approval(app,&context,&action)?;
    if existing.is_empty(){
        let mut job=JobRecord::open(state.next_job_id(),"render");job.scope=Some(JobScope{kind:"shot".into(),id:stable});
        job.output_path=Some(output);job.raw_provider_id=Some(id.code());job.input_asset_ids=vec![a.id.clone()];job.performance_asset_id=Some(a.id.clone());
        state.jobs.push(job);import::persist_state(&root,&state)?;
    }
    let card=crate::pending::publish_guarded(&raw,epoch,None,||!app.director_cancelled() && session::root().as_deref()==Some(root.as_path()))?;
    let mut out=result(render_jobs::read_persisted(&root)?,"Video-only card prepared; nothing submitted. Review the exact selected WAV before separate approval.");
    out["pendingAction"]=json!(format!("Video only · Scene {} Cut {}\nSelected WAV: `{}` (asset `{}`)\nTTS requests: 0 — no speech synthesis. Selection is your creative choice, not automatic transcript verification.\n\n{}",id.source_scene,id.cut,a.path,a.id,card));
    Ok(out)
}
