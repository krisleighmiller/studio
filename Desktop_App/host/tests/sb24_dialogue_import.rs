//! Offline production dispatch: acquisition, identity and unexecuted consent only.
use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};
use workbench_host::{ctx::HostCtx, pending, project_path::PathIntent, session, voice::RecorderState,
    workflow::{import, production, state::{AssetStatus, ProjectState}}};
fn read(root: &Path)->ProjectState { ProjectState::from_json(&fs::read_to_string(root.join("Preproduction/workflow_state.json")).unwrap()).unwrap() }
fn token(card:&Value)->String {card["pendingAction"].as_str().unwrap().split("[[workbench_approve:").nth(1).unwrap().split("]]").next().unwrap().into()}
#[test]
fn local_import_is_exact_granted_draft_preserves_consent_and_reconciles_identity() {
    let dir=tempfile::tempdir().unwrap();let root=dir.path();let external=tempfile::tempdir().unwrap();
    session::open(root.to_str().unwrap()).unwrap();
    fs::create_dir_all(root.join("Scenes/Scene1/Cut1")).unwrap();fs::create_dir_all(root.join("Preproduction/Scene1/references")).unwrap();
    let source=external.path().join("dialogue.wav");
    assert!(Command::new("ffmpeg").args(["-v","error","-f","lavfi","-i","sine=frequency=440:sample_rate=48000:duration=1","-c:a","pcm_s16le"]).arg(&source).status().unwrap().success());
    let bytes=fs::read(&source).unwrap();
    let prior="Scenes/Scene1/Cut1/performance.wav";fs::write(root.join(prior),&bytes).unwrap();
    fs::write(root.join("Preproduction/Scene1/references/interior.png"),b"offline reference").unwrap();
    let mut state=ProjectState::from_json(&json!({"version":2,"scenes":[{"id":1,"title":"Import consent fixture","cuts":[{"id":{"source_scene":1,"cut":1},"stable_id":"shot_1","speech_mode":"source_video_lipsync","scene_type":"character_performance","subject":"Test","prompt":"Speak fixture.","dialogue":[{"speaker":"Test","text":"Fixture."}],"sketch":{"title":"Fixture","shot_role":"performance","speaker":"Test","mouth_policy":"speaking_visible","visual_prompt":"Speak fixture.","dialogue":[{"speaker":"Test","text":"Fixture.","heard_as":"on_screen"}]},"visual_source":{"strategy":"shared_reference","asset_id":"asset_18"}}]}],"assets":[{"id":"asset_18","path":"Preproduction/Scene1/references/interior.png","role":"interior_reference","status":"approved","scene_id":1},{"id":"asset_19","path":prior,"role":"source_audio","status":"draft","scene_id":1,"cut_id":{"source_scene":1,"cut":1}}]}).to_string()).unwrap();
    production::sync_from_planning_with_prompt(root,&mut state,1,1,Some("Speak fixture.")).unwrap();import::persist_state(root,&state).unwrap();
    let config=tempfile::tempdir().unwrap();let settings=config.path().join("settings");fs::create_dir_all(&settings).unwrap();
    fs::write(settings.join("settings.json"),json!({"apis":[{"id":"offline","name":"Offline","kind":"atlas-cloud","baseUrl":"http://127.0.0.1:9","enabled":true}],"capabilityDefaults":{"video.lip_sync_text_narration":{"apiId":"offline","modelId":"atlascloud/van-2.5/image-to-video"}}}).to_string()).unwrap();
    let app=HostCtx::new(settings);let rec=RecorderState::new();
    let call=|cmd:&str,extra:Value| {let mut args=json!({"projectRoot":root,"sceneId":1,"cut":1});args.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        if cmd=="workflow_select_performance" && args["assetId"].is_string(){args["reviewedAssetId"]=args["assetId"].clone();}
        if cmd=="workflow_prepare_render" && args["selectedPerformance"]==true {args["reviewedAssetId"]=json!(read(root).scenes[0].cuts[0].selected_performance_asset_id);}
        workbench_host::dispatch(&app,&rec,cmd,&args)};
    call("workflow_select_performance",json!({"assetId":"asset_19"})).unwrap();
    let old=token(&call("workflow_prepare_render",json!({"selectedPerformance":true})).unwrap());
    let before=read(root);let disk=fs::read(root.join("Preproduction/workflow_state.json")).unwrap();
    let request=json!({"shotStableId":"shot_1","expectedRevision":before.state_revision,"sourcePath":source});
    let denied=call("workflow_import_performance",request.clone()).unwrap_err();assert!(denied.contains("grant"),"{denied}");
    assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(),disk);
    session::grant(source.to_str().unwrap(),PathIntent::Read).unwrap();
    for (key,value) in [("shotStableId",json!("other")),("expectedRevision",json!(before.state_revision+1)),("sceneId",json!(4294967297_u64)),("projectRoot",json!(external.path()))] {
        let mut wrong=request.clone();wrong[key]=value;assert!(call("workflow_import_performance",wrong).is_err());
        assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(),disk);
    }
    let success=call("workflow_import_performance",request.clone()).unwrap();
    let receipt=&success["performanceImport"];assert_eq!(receipt["created"],true);assert_eq!(receipt["registered"],true);
    let id=receipt["assetId"].as_str().unwrap();let path=receipt["path"].as_str().unwrap();
    let after=read(root);let asset=after.asset(id).unwrap().clone();assert_eq!(asset.status,AssetStatus::Draft);assert!(asset.parent_asset_id.is_none());
    assert_eq!(asset.scene_id,Some(1));assert_eq!(asset.cut_id,Some(workbench_host::scene_plan::handoff::CutId::new(1,1)));
    assert_eq!(fs::read(root.join(path)).unwrap(),bytes);assert_eq!(fs::read(&source).unwrap(),bytes);
    assert_eq!(after.jobs,before.jobs);assert_eq!(after.scenes[0].cuts[0].selected_performance_asset_id.as_deref(),Some("asset_19"));
    assert!(after.versions.iter().any(|v|v.asset_id==id && v.path==path));
    let notes:Value=serde_json::from_str(asset.notes.as_deref().unwrap()).unwrap();assert_eq!(notes["speech_completeness_verified"],false);assert!(!asset.notes.as_ref().unwrap().contains(source.to_str().unwrap()));
    assert!(pending::take(&old).is_ok(),"unselected import retains exact native consent without execution");
    let replay=json!({"expectedRevision":after.state_revision,"shotStableId":"shot_1","sourcePath":source});
    assert!(call("workflow_import_performance",replay).unwrap_err().contains("grant"));
    let reuse=call("workflow_import_performance",json!({"expectedRevision":after.state_revision,"shotStableId":"shot_1","sourcePath":root.join(path)})).unwrap();
    assert_eq!(reuse["performanceImport"]["assetId"],id);assert_eq!(reuse["performanceImport"]["created"],false);assert_eq!(reuse["performanceImport"]["registered"],false);
    let old=token(&call("workflow_prepare_render",json!({"selectedPerformance":true})).unwrap());
    call("workflow_select_performance",json!({"assetId":id})).unwrap();assert!(pending::take(&old).is_err());
    let old=token(&call("workflow_prepare_render",json!({"selectedPerformance":true})).unwrap());
    call("workflow_select_performance",json!({"assetId":null})).unwrap();assert!(pending::take(&old).is_err());
    for _ in 0..2 {let imported=import::import_project(root,Default::default()).unwrap().state;assert_eq!(imported.asset(id),Some(&asset));assert_eq!(imported.assets.iter().filter(|a|a.path==path).count(),1);}
    // Invalid content and directory redirection are denied without registration.
    let bad=external.path().join("bad.wav");fs::write(&bad,b"RIFFxxxxWAVEbroken").unwrap();session::grant(bad.to_str().unwrap(),PathIntent::Read).unwrap();
    let snapshot=fs::read(root.join("Preproduction/workflow_state.json")).unwrap();
    assert!(call("workflow_import_performance",json!({"expectedRevision":read(root).state_revision,"shotStableId":"shot_1","sourcePath":bad})).is_err());assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(),snapshot);
    assert_eq!(fs::read(&source).unwrap(),bytes);assert_eq!(fs::read(root.join(prior)).unwrap(),bytes);
    session::close();
    println!("PASS run120: production dispatch; ungranted/replay and identity/revision/root refusals; byte-identical Draft source/version; no auto-selection; exact consent retained on import, revoked on explicit switch/clear; two native reimports one ID/path; malformed WAV rejected; no provider execution, no pitch/speed rerun");
}
