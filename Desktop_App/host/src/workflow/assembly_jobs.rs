//! Track an authorized prepared assembly; file discovery is not execution proof.
use super::{import,post,state::{JobRecord,JobStatus,ProjectState,STATE_REL}};
use crate::{director_actions::ValidatedAction,project_path::{self,PathIntent}};
use serde_json::{Map,Value};
use std::path::{Path,PathBuf};
pub(super) fn read_persisted(root:&Path)->Result<ProjectState,String>{
 let p=project_path::resolve(STATE_REL,Some(root.to_str().ok_or("invalid project root")?),PathIntent::Read).map_err(|e|e.to_string())?.into_path_buf();
 ProjectState::from_json(&std::fs::read_to_string(p).map_err(|e|format!("cannot read persisted state: {e}"))?)
}
/// Only the newest completed project master can receive the current review.
fn review_job<'a>(state: &'a ProjectState, id: &str) -> Option<&'a JobRecord> {
    if state.change_impacts.iter().any(|edge| edge.dependent == format!("master:{id}") && edge.disposition == "invalidated") { return None; }
    let job = state.jobs.iter().rev().find(|j| j.kind == "assemble"
        && j.status == JobStatus::Completed
        && j.scope.as_ref().is_some_and(|s| s.kind == "project" && s.id == "project")
        && j.raw_provider_id.as_deref() == Some("project") && j.output_path.is_some())?;
    if job.id != id || state.jobs.iter().filter(|j| j.id == id).count() != 1 { return None; }
    let saved = job.assembly_snapshot.as_deref()?;
    let mut scenes: Vec<_> = state.scenes.iter().collect();
    scenes.sort_by_key(|s| s.id);
    if scenes.is_empty() { return None; }
    let mut offset = 0;
    for scene in scenes {
        let end = offset + scene.cuts.len();
        if end > saved.len() || !post::snapshot_matches_scene(scene, &saved[offset..end]) { return None; }
        offset = end;
    }
    (offset == saved.len()).then_some(job)
}

/// Never infer creative acceptance from sketch flags, timing, or file discovery.
/// Clear the receipt permanently on a persisted change, even if later reverted.
pub(super) fn invalidate_review(state: &mut ProjectState) {
    if state.rendered_review.as_ref().is_some_and(|r| review_job(state, &r.job_id).is_none()) {
        state.rendered_review = None;
    }
}

pub(super) fn approve_review(root: &Path, state: &mut ProjectState, id: &str) -> Result<(), String> {
    let job = review_job(state, id).ok_or("Rendered assembly is stale or unavailable; prepare and review a new project master.")?;
    let raw = job.output_path.as_deref().ok_or("Rendered master missing")?;
    let (path, rel) = output(root, raw)?;
    validate_binding(root, state, job, &rel, None)?;
    if !path.is_file() { return Err("Rendered master is missing; prepare a new project master.".into()); }
    state.rendered_review = Some(super::state::RenderedSequenceReview { job_id: id.into() });
    Ok(())
}

fn output(root:&Path,raw:&str)->Result<(PathBuf,String),String>{
 let p=project_path::resolve(raw,Some(root.to_str().ok_or("invalid project root")?),PathIntent::Write).map_err(|e|e.to_string())?.into_path_buf();let rel=project_path::relative_to_root(root,&p)?.to_string_lossy().into_owned();Ok((p,rel))
}
fn validate_binding(root:&Path,state:&ProjectState,job:&JobRecord,rel:&str,inputs:Option<&Map<String,Value>>)->Result<(),String>{
 let scope=job.scope.as_ref().ok_or("prepared assembly needs a scope")?;
 let mut scenes:Vec<_>=state.scenes.iter().filter(|s|scope.kind=="project" || (scope.kind=="scene" && s.stable_id==scope.id)).collect();
 scenes.sort_by_key(|s|s.id);
 if scenes.is_empty() || (scope.kind!="project" && scenes.len()!=1) {return Err("prepared assembly scope missing or ambiguous".into());}
 let key=if scope.kind=="project" {"project".into()} else {format!("scene_{}",scenes[0].id)};
 if job.output_path.as_deref()!=Some(rel)||job.raw_provider_id.as_deref()!=Some(key.as_str()){return Err("prepared assembly scope/output changed".into());}
 let saved=job.assembly_snapshot.as_deref().ok_or("prepared assembly missing snapshot")?;
 let mut offset=0;
 for scene in &scenes {
  let end=offset+scene.cuts.len();
  if end>saved.len() || !post::snapshot_matches_scene(scene,&saved[offset..end]) {return Err("Prepared assembly is stale; selected media or timing changed. Prepare again.".into());}
  offset=end;
 }
 if offset!=saved.len(){return Err("prepared assembly shot count changed".into());}
 if let Some(inputs)=inputs{
  let allowed=["production_path","output_path","trim","caption_mode","scenes","fade_in","fade_out"];
  if inputs.keys().any(|k|!allowed.contains(&k.as_str())) || inputs.get("trim").and_then(Value::as_str)!=Some("custom") || inputs.get("caption_mode").and_then(Value::as_str)!=Some("none") || inputs.get("fade_in")!=Some(&Value::Bool(false)) || inputs.get("fade_out")!=Some(&Value::Bool(false)) {return Err("prepared assembly effective options changed".into());}
  let production=inputs.get("production_path").and_then(Value::as_str).ok_or("prepared production path missing")?;
  let production=project_path::resolve(production,Some(root.to_str().unwrap()),PathIntent::Read).map_err(|e|e.to_string())?.into_path_buf();
  let expected=production_relative(rel);
  let expected=project_path::resolve(&expected,Some(root.to_str().unwrap()),PathIntent::Read).map_err(|e|e.to_string())?.into_path_buf();
  if production!=expected || std::fs::read_to_string(&production).map_err(|e|e.to_string())? != production_document(root,state,saved)? {return Err("prepared production path or canonical notes changed; prepare again".into());}

  let entries=inputs.get("scenes").and_then(Value::as_array).ok_or("prepared assembly scenes missing")?;if entries.len()!=saved.len(){return Err("prepared assembly scene count changed".into());}
  for (entry,shot) in entries.iter().zip(saved){
   if entry.get("trim").is_some_and(|v|v.as_str()!=Some("custom")) || entry.as_object().is_none_or(|obj|obj.keys().any(|k|!["id","cut","video","start","duration","transition_out","trim"].contains(&k.as_str()))) {return Err("prepared assembly per-shot options changed".into());}
   let raw=entry.get("video").and_then(Value::as_str).ok_or("prepared assembly source missing")?;
   let actual=project_path::resolve(raw,Some(root.to_str().unwrap()),PathIntent::Read).map_err(|e|e.to_string())?.into_path_buf();let expected=project_path::resolve(&shot.path,Some(root.to_str().unwrap()),PathIntent::Read).map_err(|e|e.to_string())?.into_path_buf();
   if actual!=expected || entry.get("id").and_then(Value::as_u64)!=scenes.iter().flat_map(|s|&s.cuts).find(|c|c.stable_id==shot.shot_id).map(|c|c.id.source_scene as u64) || entry.get("cut").and_then(Value::as_u64)!=Some(shot.cut_label as u64) || entry.get("start").and_then(Value::as_f64).is_none_or(|v|(v-shot.start_seconds).abs()>0.000001) || entry.get("duration").and_then(Value::as_f64).is_none_or(|v|(v-shot.duration_seconds).abs()>0.000001) || entry.get("transition_out").and_then(Value::as_str)!=Some(shot.transition.as_str()) {return Err("prepared assembly action does not match reserved snapshot".into());}
  }
 }
 Ok(())
}
fn begin(root:&Path,raw:&str,inputs:Option<&Map<String,Value>>)->Result<Option<(String,String)>,String>{
 let(requested,rel)=output(root,raw)?;let _lock=import::lock_project(root)?;
 let statepath=project_path::resolve(STATE_REL,Some(root.to_str().unwrap()),PathIntent::Read).map_err(|e|e.to_string())?.into_path_buf();if !statepath.exists(){return Ok(None);}let mut state=read_persisted(root)?;
 let mut indices=Vec::new();for(i,j)in state.jobs.iter().enumerate().filter(|(_,j)|j.kind=="assemble") {let Some(raw)=j.output_path.as_deref()else{continue;};let(p,canonical)=output(root,raw)?;if p==requested{if canonical!=raw{return Err("assembly destination redirected".into());}indices.push(i);}}
 if indices.is_empty(){return Ok(None);}if indices.len()!=1{return Err("ambiguous prepared assembly output".into());}let i=indices[0];if state.jobs[i].status!=JobStatus::Pending{return Err("assembly destination reserved by non-pending attempt".into());}
 validate_binding(root,&state,&state.jobs[i],&rel,inputs)?;if requested.exists(){return Err("prepared assembly output already exists; prepare a new destination".into());}
 state.jobs[i].mark_running("authorized local assembly in flight");import::persist_state_locked(root,&state)?;Ok(Some((state.jobs[i].id.clone(),rel)))
}
fn settle(root:&Path,id:&str,rel:&str,mut result:Result<String,String>)->Result<String,String>{
 let _lock=import::lock_project(root)?;let mut state=read_persisted(root)?;let indices:Vec<_>=state.jobs.iter().enumerate().filter(|(_,j)|j.id==id).map(|(i,_)|i).collect();if indices.len()!=1{return Err("assembly job disappeared or became ambiguous".into());}let i=indices[0];
 if state.jobs[i].kind!="assemble"||state.jobs[i].status!=JobStatus::Running||state.jobs[i].output_path.as_deref()!=Some(rel){return Err("assembly binding changed during dispatch".into());}
 if result.is_ok(){let(actual,canonical)=output(root,rel)?;let expected=state.jobs[i].assembly_snapshot.as_deref().map(post::snapshot_runtime);if canonical!=rel{result=Err("assembly output redirected during dispatch".into());}else{
  let mut findings=super::qc::check_media_file(&actual,expected,0.15);findings.extend(super::qc::check_full_video_decode(&actual));
  if findings.iter().any(|f|f.severity==super::qc::QCSeverity::Error || (f.id=="runtime" && expected.is_some())){result=Err(format!("assembly produced no valid complete output: {}",findings.iter().map(|f|f.message.as_str()).collect::<Vec<_>>().join(" ")));}
 }}
 match &result{Err(e)=>state.jobs[i].fail(None,e.clone()),Ok(_)=>{state.jobs[i].finish(Some(rel),None);import::restore_assemble_jobs(root,&mut state);}}
 import::persist_state_locked(root,&state)?;result
}
fn bound<F>(root:&Path,path:&str,inputs:Option<&Map<String,Value>>,dispatch:F)->Result<String,String>where F:FnOnce()->Result<String,String>{let root=crate::session::command_root(Some(root.to_str().ok_or("invalid project root")?))?;let binding=begin(&root,path,inputs)?;let result=dispatch();match binding{Some((id,rel))=>settle(&root,&id,&rel,result),None=>result}}
/// Adapter seam used by tests that already hold operation authorization.
pub fn with_assembly_job<F>(root:&Path,path:&str,dispatch:F)->Result<String,String>where F:FnOnce()->Result<String,String>{bound(root,path,None,dispatch)}
/// The ordinary Director dispatch additionally binds the exact approved plan.
pub fn with_validated_assembly_job<F>(root:&Path,validated:&ValidatedAction,dispatch:F)->Result<String,String>where F:FnOnce()->Result<String,String>{let inputs=&validated.action.inputs;let path=inputs.get("output_path").and_then(Value::as_str).ok_or("assembly output_path missing")?;bound(root,path,Some(inputs),dispatch)}

fn production_relative(output: &str) -> String {
 Path::new(output).with_extension("production.md").to_string_lossy().into_owned()
}
/// Derive actual typed cut metadata, never placeholder production notes.
fn production_document(root: &Path, state: &ProjectState, snapshot: &[super::state::AssemblyShotSnapshot]) -> Result<String,String> {
 let mut text=String::new();
 for shot in snapshot {
  let cut=state.scenes.iter().flat_map(|s|&s.cuts).find(|c|c.stable_id==shot.shot_id).ok_or("assembly shot missing")?;
  let key=format!("Scene {} Cut {} production_note_path",cut.id.source_scene,cut.id.cut);
  let raw=cut.production_note_path.as_deref().ok_or_else(||format!("{key} is missing; save production notes for this cut before assembly."))?;
  let path=project_path::resolve(raw,Some(root.to_str().ok_or("invalid root")?),PathIntent::Read).map_err(|e|format!("{key}: {e}"))?.into_path_buf();
  let content=std::fs::read_to_string(path).map_err(|e|format!("{key} `{raw}` must be an existing readable file: {e}"))?;
  let handoff=crate::scene_plan::extract::load_handoff_document(&content).or_else(|_|crate::scene_plan::extract::parse_handoff_yaml(&content)).map_err(|e|format!("{key} `{raw}`: {e}"))?;
  if handoff.id()!=cut.source_origin.unwrap_or(cut.id) {return Err(format!("{key} has mismatched cut identity"));}
  let yaml=crate::scene_plan::render::canonical_yaml(&handoff)?;
  text.push_str(&crate::scene_plan::render::master_block(&handoff,&yaml));text.push('\n');
 }
 Ok(text)
}
pub(super) fn prepare_production(root:&Path,state:&ProjectState,snapshot:&[super::state::AssemblyShotSnapshot],output:&str)->Result<String,String>{
 use std::io::Write;
 let content=production_document(root,state,snapshot)?;
 let relative=production_relative(output);
 let (path,_)=self::output(root,&relative)?;
 if let Some(parent)=path.parent(){std::fs::create_dir_all(parent).map_err(|e|e.to_string())?;}
 let (path,_)=self::output(root,&relative)?;
 let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|e|format!("cannot reserve assembly production record: {e}"))?;
 file.write_all(content.as_bytes()).map_err(|e|e.to_string())?;
 Ok(relative)
}
