//! Offline production dispatch: exact creative audio selection, no approval execution.
use serde_json::{json,Value};
use std::{fs,path::Path,process::Command};
use workbench_host::{ctx::HostCtx,session,pending,voice::RecorderState,workflow::{import,production,state::{ProjectState,JobStatus}}};
fn read(root:&Path)->ProjectState{ProjectState::from_json(&fs::read_to_string(root.join("Preproduction/workflow_state.json")).unwrap()).unwrap()}
fn token(card:&Value)->String{card["pendingAction"].as_str().unwrap().split("[[workbench_approve:").nth(1).unwrap().split("]]").next().unwrap().into()}
#[test]
fn selected_exact_wav_prepares_fresh_video_only_and_invalidates_prior_consent(){
 let dir=tempfile::tempdir().unwrap();let root=dir.path();session::open(root.to_str().unwrap()).unwrap();
 fs::create_dir_all(root.join("Scenes/Scene1/Cut1")).unwrap();fs::create_dir_all(root.join("Preproduction/Scene1/references")).unwrap();
 let source="Scenes/Scene1/Cut1/performance.wav";
 assert!(Command::new("ffmpeg").args(["-v","error","-f","lavfi","-i","sine=frequency=440:duration=4","-c:a","pcm_s16le"]).arg(root.join(source)).status().unwrap().success());
 fs::write(root.join("Preproduction/Scene1/references/interior.png"),b"offline reference").unwrap();
 let mut state=ProjectState::from_json(&json!({"version":2,"scenes":[{"id":1,"title":"Audition fixture","cuts":[{"id":{"source_scene":1,"cut":1},"stable_id":"shot_1","speech_mode":"source_video_lipsync","scene_type":"character_performance","subject":"Test","prompt":"Speak the fixture line.","dialogue":[{"speaker":"Test","text":"Complete fixture line."}],"sketch":{"title":"Fixture","shot_role":"performance","speaker":"Test","mouth_policy":"speaking_visible","visual_prompt":"Speak the fixture line.","dialogue":[{"speaker":"Test","text":"Complete fixture line.","heard_as":"on_screen"}]},"visual_source":{"strategy":"shared_reference","asset_id":"asset_18"}}]}],"assets":[{"id":"asset_18","path":"Preproduction/Scene1/references/interior.png","role":"interior_reference","status":"approved","scene_id":1},{"id":"asset_19","path":source,"role":"source_audio","status":"draft","scene_id":1,"cut_id":{"source_scene":1,"cut":1}}],"jobs":[{"id":"history","kind":"render","status":"failed","error":"Historical record retained"}]}).to_string()).unwrap();
 production::sync_from_planning_with_prompt(root,&mut state,1,1,Some("Speak the fixture line.")).unwrap();import::persist_state(root,&state).unwrap();
 let historical=read(root).jobs[0].clone();let source_bytes=fs::read(root.join(source)).unwrap();
 let config=tempfile::tempdir().unwrap();let settings=config.path().join("settings");fs::create_dir_all(&settings).unwrap();
 fs::write(settings.join("settings.json"),json!({"apis":[{"id":"offline","name":"Offline","kind":"atlas-cloud","baseUrl":"http://127.0.0.1:9","enabled":true}],"capabilityDefaults":{"video.lip_sync_text_narration":{"apiId":"offline","modelId":"atlascloud/van-2.5/image-to-video"}}}).to_string()).unwrap();
 let app=HostCtx::new(settings);let rec=RecorderState::new();
 let call=|cmd:&str,extra:Value|{let mut args=json!({"projectRoot":root,"sceneId":1,"cut":1});args.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
  if cmd=="workflow_select_performance" && args["assetId"].is_string(){args["reviewedAssetId"]=args["assetId"].clone();}
  if cmd=="workflow_prepare_render" && args["selectedPerformance"]==true {args["reviewedAssetId"]=json!(read(root).scenes[0].cuts[0].selected_performance_asset_id);}
  workbench_host::dispatch(&app,&rec,cmd,&args)};
 let initial=json!({"state":read(root),"report":workbench_host::workflow::report::project_report(&read(root))});
 let version=call("workflow_version_audio",json!({"assetId":"asset_19","speed":1.25})).unwrap();
 assert!(workbench_host::dispatch(&app,&rec,"workflow_select_performance",&json!({"projectRoot":root,"sceneId":1,"cut":1,"assetId":"asset_19"})).unwrap_err().contains("Review the full dialogue"));
 let derivative=read(root).assets.iter().find(|a|a.parent_asset_id.as_deref()==Some("asset_19")).unwrap().clone();
 let derivative_bytes=fs::read(root.join(&derivative.path)).unwrap();
 call("workflow_select_performance",json!({"assetId":"asset_19"})).unwrap();
 assert!(workbench_host::dispatch(&app,&rec,"workflow_prepare_render",&json!({"projectRoot":root,"sceneId":1,"cut":1,"selectedPerformance":true,"reviewedAssetId":"wrong"})).unwrap_err().contains("Review the full dialogue"));
 let old=call("workflow_prepare_render",json!({"selectedPerformance":true})).unwrap();let old_token=token(&old);
 assert_eq!(pending::current().unwrap().inputs["performance_wav"],json!(source));
 let selected=call("workflow_select_performance",json!({"assetId":derivative.id})).unwrap();
 assert!(pending::take(&old_token).is_err(),"changed WAV invalidates actual old native token, no dispatch");
 assert_eq!(read(root).jobs.iter().find(|j|j.performance_asset_id.as_deref()==Some("asset_19")).unwrap().status,JobStatus::Cancelled);
 let prepared=call("workflow_prepare_render",json!({"selectedPerformance":true})).unwrap();let first_token=token(&prepared);
 let action=pending::current().unwrap();assert_eq!(action.inputs["performance_wav"],json!(derivative.path));
 let video=root.join(action.inputs["output_path"].as_str().unwrap());
 assert!(workbench_host::workflow::performance::validate_request(root,&video,None).is_err(),"same reservation must never accept TTS");
 assert!(workbench_host::workflow::performance::validate_request(root,&video,Some(source)).is_err(),"same reservation must reject a different WAV version");
 workbench_host::workflow::performance::validate_request(root,&video,Some(&derivative.path)).unwrap();
 assert!(!action.inputs.contains_key("voice_clone"));assert!(!action.inputs.contains_key("voice_profile"));
 assert!(prepared["pendingAction"].as_str().unwrap().contains(&derivative.path));assert!(prepared["pendingAction"].as_str().unwrap().contains("TTS requests: 0"));
 let before=fs::read(root.join("Preproduction/workflow_state.json")).unwrap();
 call("workflow_select_performance",json!({"assetId":derivative.id})).unwrap();
 assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(),before,"duplicate selection is byte-identical and leaves card active");assert!(pending::current().is_some());
 let again=call("workflow_prepare_render",json!({"selectedPerformance":true})).unwrap();assert_ne!(token(&again),first_token);assert!(pending::take(&first_token).is_err());assert_eq!(read(root).jobs.len(),3,"repeat preparation reuses only same unsubmitted reservation");
 for args in [json!({"assetId":"missing"}),json!({"assetId":42}),json!({"assetId":"asset_18"}),json!({"assetId":"asset_19","sceneId":4294967297u64})]{
  let bytes=fs::read(root.join("Preproduction/workflow_state.json")).unwrap();assert!(call("workflow_select_performance",args).is_err());assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(),bytes);
 }
 // Duplicate selection still validates the actual bytes, never blessing a changed/truncated file.
 let saved_bytes=fs::read(root.join("Preproduction/workflow_state.json")).unwrap();
 fs::write(root.join(&derivative.path),b"not WAV media").unwrap();
 assert!(call("workflow_select_performance",json!({"assetId":derivative.id})).is_err());
 assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(),saved_bytes);
 fs::write(root.join(&derivative.path),&derivative_bytes[..derivative_bytes.len()/2]).unwrap();
 assert!(call("workflow_prepare_render",json!({"selectedPerformance":true})).is_err());
 assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(),saved_bytes);
 fs::write(root.join(&derivative.path),&derivative_bytes).unwrap();
 // Legacy Render/Resume cannot ignore the explicit selection and regenerate TTS.
 assert!(call("workflow_prepare_render",json!({})).unwrap_err().contains("video only"));
 assert!(call("workflow_prepare_render",json!({"resumeSavedAudio":true})).is_err());
 let imported=import::import_project(root,Default::default()).unwrap().state;
 assert_eq!(imported.scenes[0].cuts[0].selected_performance_asset_id.as_deref(),Some(derivative.id.as_str()));assert_eq!(imported.asset(&derivative.id).unwrap().path,derivative.path);
 assert_eq!(imported.jobs[0],historical);
 let fresh=call("workflow_prepare_render",json!({"selectedPerformance":true})).unwrap();let cancel_token=token(&fresh);
 let cleared=call("workflow_select_performance",json!({"assetId":null})).unwrap();assert!(pending::take(&cancel_token).is_err());assert!(read(root).scenes[0].cuts[0].selected_performance_asset_id.is_none());
 call("workflow_select_performance",json!({"assetId":derivative.id})).unwrap();let switched=call("workflow_prepare_render",json!({"selectedPerformance":true})).unwrap();let switched_token=token(&switched);
 let other=tempfile::tempdir().unwrap();workbench_host::dispatch(&app,&rec,"open_project",&json!({"path":other.path()})).unwrap();assert!(pending::take(&switched_token).is_err());assert!(call("workflow_select_performance",json!({"assetId":"asset_19"})).is_err());
 assert_eq!(fs::read(root.join(source)).unwrap(),source_bytes);assert_eq!(fs::read(root.join(&derivative.path)).unwrap(),derivative_bytes);
 assert!(read(root).jobs.iter().all(|j|!j.kind.starts_with("atlas_generate_")));assert!(!root.join("Scenes/Scene1/Cut1/S01C01.mp4").exists());
 if let Ok(dest)=std::env::var("SB12_ARTIFACTS") {let dest=Path::new(&dest);fs::create_dir_all(dest).unwrap();fs::write(dest.join("source.wav"),source_bytes).unwrap();fs::write(dest.join("derived.wav"),derivative_bytes).unwrap();fs::write(dest.join("receipts.json"),serde_json::to_vec_pretty(&json!({"initial":initial,"version":version,"selected":selected,"prepared":prepared,"cleared":cleared,"final":read(root),"provider_dispatches":0,"tts":0})).unwrap()).unwrap();}
 session::close();println!("SB12 expected=actual: source/derived exact WAV version selected, no-TTS card, old token invalidated, duplicate selection unchanged, repeated prepare no extra job, cancel/project switch invalidated, real reimport selection retained, historical job/WAV bytes unchanged, zero provider dispatch.");
}
