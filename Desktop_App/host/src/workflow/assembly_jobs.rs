//! Track an authorized prepared assembly; file discovery is not execution proof.
use super::{import,post,state::{JobRecord,JobStatus,ProjectState,STATE_REL}};
use crate::{director_actions::ValidatedAction,project_path::{self,PathIntent}};
use serde_json::{Map,Value};
use std::path::{Path,PathBuf};
pub(super) fn read_persisted(root:&Path)->Result<ProjectState,String>{
 let p=project_path::resolve(STATE_REL,Some(root.to_str().ok_or("invalid project root")?),PathIntent::Read).map_err(|e|e.to_string())?.into_path_buf();
 ProjectState::from_json(&std::fs::read_to_string(p).map_err(|e|format!("cannot read persisted state: {e}"))?)
}
fn output(root:&Path,raw:&str)->Result<(PathBuf,String),String>{
 let p=project_path::resolve(raw,Some(root.to_str().ok_or("invalid project root")?),PathIntent::Write).map_err(|e|e.to_string())?.into_path_buf();let rel=project_path::relative_to_root(root,&p)?.to_string_lossy().into_owned();Ok((p,rel))
}
fn validate_binding(root:&Path,state:&ProjectState,job:&JobRecord,rel:&str,inputs:Option<&Map<String,Value>>)->Result<(),String>{
 let scope=job.scope.as_ref().filter(|s|s.kind=="scene").ok_or("prepared assembly needs a scene scope")?;
 let scenes:Vec<_>=state.scenes.iter().filter(|s|s.stable_id==scope.id).collect();if scenes.len()!=1{return Err("prepared assembly scene missing or ambiguous".into());}let scene=scenes[0];
 if job.output_path.as_deref()!=Some(rel)||job.raw_provider_id.as_deref()!=Some(&format!("scene_{}",scene.id)){return Err("prepared assembly scope/output changed".into());}
 let saved=job.assembly_snapshot.as_deref().ok_or("prepared assembly missing snapshot")?;
 if !post::snapshot_matches_scene(scene,saved){return Err("Prepared assembly is stale; selected media or timing changed. Prepare again.".into());}
 if let Some(inputs)=inputs{
  let allowed=["production_path","output_path","trim","caption_mode","scenes","fade_in","fade_out"];
  if inputs.keys().any(|k|!allowed.contains(&k.as_str())) || inputs.get("trim").and_then(Value::as_str)!=Some("custom") || inputs.get("caption_mode").and_then(Value::as_str)!=Some("none") || inputs.get("fade_in")!=Some(&Value::Bool(false)) || inputs.get("fade_out")!=Some(&Value::Bool(false)) {return Err("prepared assembly effective options changed".into());}
  let production=inputs.get("production_path").and_then(Value::as_str).ok_or("prepared production path missing")?;
  let production=project_path::resolve(production,Some(root.to_str().unwrap()),PathIntent::Read).map_err(|e|e.to_string())?.into_path_buf();
  if production!=root.join("prod.md"){return Err("prepared production path changed".into());}

  let entries=inputs.get("scenes").and_then(Value::as_array).ok_or("prepared assembly scenes missing")?;if entries.len()!=saved.len(){return Err("prepared assembly scene count changed".into());}
  for (entry,shot) in entries.iter().zip(saved){
   if entry.get("trim").is_some_and(|v|v.as_str()!=Some("custom")) || entry.as_object().is_none_or(|obj|obj.keys().any(|k|!["id","cut","video","start","duration","transition_out","trim"].contains(&k.as_str()))) {return Err("prepared assembly per-shot options changed".into());}
   let raw=entry.get("video").and_then(Value::as_str).ok_or("prepared assembly source missing")?;
   let actual=project_path::resolve(raw,Some(root.to_str().unwrap()),PathIntent::Read).map_err(|e|e.to_string())?.into_path_buf();let expected=project_path::resolve(&shot.path,Some(root.to_str().unwrap()),PathIntent::Read).map_err(|e|e.to_string())?.into_path_buf();
   if actual!=expected || entry.get("id").and_then(Value::as_u64)!=scene.cuts.iter().find(|c|c.stable_id==shot.shot_id).map(|c|c.id.source_scene as u64) || entry.get("cut").and_then(Value::as_u64)!=Some(shot.cut_label as u64) || entry.get("start").and_then(Value::as_f64).is_none_or(|v|(v-shot.start_seconds).abs()>0.000001) || entry.get("duration").and_then(Value::as_f64).is_none_or(|v|(v-shot.duration_seconds).abs()>0.000001) || entry.get("transition_out").and_then(Value::as_str)!=Some(shot.transition.as_str()) {return Err("prepared assembly action does not match reserved snapshot".into());}
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
