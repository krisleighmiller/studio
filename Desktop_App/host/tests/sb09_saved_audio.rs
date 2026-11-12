//! Offline ordinary-library retry history/readback; never approve or dispatch media.
use serde_json::json;
use std::{fs, path::Path, process::Command};
use workbench_host::{ctx::HostCtx, session, voice::RecorderState, workflow::{import, production, state::{ProjectState, JobStatus}}};
fn read(root: &Path) -> ProjectState { ProjectState::from_json(&fs::read_to_string(root.join("Preproduction/workflow_state.json")).unwrap()).unwrap() }
fn ui_snapshot(state: &ProjectState) -> serde_json::Value {
    let mut state=state.clone();
    // Use the ordinary loader's readiness enrichment, not hand-enabled UI tabs.
    workbench_host::workflow::recipe::apply_stages(&mut state);
    json!({"report":workbench_host::workflow::report::project_report(&state),"state":state})
}
fn restore(root: &Path, state: &ProjectState) {
    let mut state=state.clone(); state.state_revision=read(root).state_revision;
    import::persist_state(root,&state).unwrap();
    import::load_state(root).unwrap();
}
fn fixture(root: &Path) -> ProjectState {
    fs::create_dir_all(root.join("Scenes/Scene1/Cut1")).unwrap();
    fs::create_dir_all(root.join("Preproduction/Scene1/references")).unwrap();
    fs::create_dir_all(root.join("Scene_Prompts/Scene1")).unwrap();
    let wav=root.join("Scenes/Scene1/Cut1/S01C01.wav");
    assert!(Command::new("ffmpeg").args(["-v","error","-f","lavfi","-i","sine=frequency=440:duration=3","-c:a","pcm_s16le"]).arg(&wav).status().unwrap().success());
    fs::copy(&wav,root.join("Scenes/Scene1/Cut1/S01C01_v2.wav")).unwrap();
    fs::write(root.join("Preproduction/Scene1/references/interior.png"),b"fixture image").unwrap();
    let err="HOST PRE-SUBMIT VALIDATION: incompatible image field. No generation submitted.";
    let scope=json!({"kind":"shot","id":"shot_1"});
    let attempt=|status:&str, error:&str, start:&str, end:&str| json!({"id":"attempt_1","status":status,"error":error,"started_at":start,"completed_at":end});
    let state=json!({"version":2,"state_revision":0,"scenes":[{"id":1,"title":"Offline fixture","cuts":[{"id":{"source_scene":1,"cut":1},"stable_id":"shot_1","speech_mode":"source_video_lipsync","scene_type":"character_performance","subject":"Kris","title":"Fixture","prompt":"Speak the fixture line.","dialogue":[{"speaker":"Kris","text":"This is one complete fixture line."}],"production_note_path":"Scene_Prompts/Scene1/Cut1.txt","visual_source":{"strategy":"shared_reference","asset_id":"asset_18"}}]}],"assets":[{"id":"asset_18","path":"Preproduction/Scene1/references/interior.png","role":"interior_reference","status":"approved","scene_id":1}],"jobs":[
      {"id":"job_3","kind":"render","status":"failed","scope":scope,"raw_provider_id":"S01C01","output_path":"Scenes/Scene1/Cut1/S01C01.mp4","error":err,"created_at":"2026-11-01T23:00:00Z","attempts":[attempt("failed",err,"2026-11-01T23:07:34Z","2026-11-01T23:07:36Z")]},
      {"id":"spend_original","kind":"atlas_generate_video","status":"failed","error":err,"attempts":[attempt("failed",err,"2026-11-01T23:07:34Z","2026-11-01T23:07:36Z")]},
      {"id":"job_4","kind":"render","status":"pending","scope":scope,"raw_provider_id":"S01C01","output_path":"Scenes/Scene1/Cut1/S01C01_v2.mp4","retry_of":"job_3","created_at":"2026-11-02T00:38:43Z","attempts":[{"id":"job_4_attempt_1","status":"pending"}]},
      {"id":"spend_retry","kind":"atlas_generate_video","status":"failed","error":"saved-audio retry is no longer proven pre-submit; no video submitted","created_at":"2026-11-02T00:39:12Z","request_metadata":{"request_hash":"offline-fixture"},"attempts":[attempt("failed","saved-audio retry is no longer proven pre-submit; no video submitted","2026-11-02T00:39:12Z","2026-11-02T00:39:12Z")]}
    ]});
    let mut state=ProjectState::from_json(&state.to_string()).unwrap();
    let cut=&mut state.scenes[0].cuts[0];
    cut.production_note_path=None;
    cut.sketch=Some(serde_json::from_value(json!({"title":"Fixture","shot_role":"performance","speaker":"Kris","mouth_policy":"speaking_visible","visual_prompt":"Speak the fixture line.","dialogue":[{"speaker":"Kris","text":"This is one complete fixture line.","heard_as":"on_screen"}]})).unwrap());
    production::sync_from_planning_with_prompt(root,&mut state,1,1,Some("Speak the fixture line.")).unwrap();
    import::persist_state(root,&state).unwrap(); state
}
#[test]
fn stranded_identical_wav_retry_is_history_not_permission() {
    let dir=tempfile::tempdir().unwrap(); let root=dir.path(); session::open(root.to_str().unwrap()).unwrap();
    let original=fixture(root); let wav=root.join("Scenes/Scene1/Cut1/S01C01.wav"); let copy=root.join("Scenes/Scene1/Cut1/S01C01_v2.wav"); let bytes=fs::read(&wav).unwrap();
    let appdir=tempfile::tempdir().unwrap();
    let settings=appdir.path().join("settings"); fs::create_dir_all(&settings).unwrap();
    fs::write(settings.join("settings.json"),json!({"apis":[{"id":"offline","name":"Offline no-submit fixture","kind":"atlas-cloud","baseUrl":"http://127.0.0.1:9","enabled":true}],"capabilityDefaults":{"video.lip_sync_text_narration":{"apiId":"offline","modelId":"atlascloud/van-2.5/image-to-video"}}}).to_string()).unwrap();
    let app=HostCtx::new(settings); let rec=RecorderState::new();
    let call=|| workbench_host::dispatch(&app,&rec,"workflow_prepare_render",&json!({"projectRoot":root,"sceneId":1,"cut":1,"resumeSavedAudio":true}));
    // Optional native transport receipts for SB10's mounted, no-submit UI replay.
    let mut ui_receipts = json!({"initial": ui_snapshot(&original), "refusals": {}});
    // Unsafe variations must refuse with no persistent history changes.
    for case in ["prediction","different","video","ambiguous","unbound"] {
        let mut bad=original.clone();
        if case=="prediction" { bad.jobs[3].request_metadata.as_mut().unwrap().prediction_id=Some("uncertain_remote".into()); }
        if case=="ambiguous" { let mut child=bad.jobs[3].clone(); child.id="other_child".into(); bad.jobs.push(child); }
        if case=="unbound" { bad.jobs[2].retry_of=None; }
        if case=="different" { fs::write(&copy,b"different bytes").unwrap(); }
        if case=="video" { fs::write(copy.with_extension("mp4"),b"occupied video").unwrap(); }
        restore(root,&bad); let before=fs::read(root.join("Preproduction/workflow_state.json")).unwrap();
        let error=call().expect_err(&format!("must refuse {case}"));
        ui_receipts["refusals"][case]=json!({"initial":ui_snapshot(&bad),"error":error});
        assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(),before,"no write on {case}");
        fs::write(&copy,&bytes).unwrap(); if case=="video" { fs::remove_file(copy.with_extension("mp4")).unwrap(); }
    }
    restore(root,&original);
    let result=call();
    assert!(result.is_ok(),"complete single-cue fixture must prepare without submission: {result:?}");
    let result=result.unwrap(); let persisted=read(root);
    ui_receipts["prepared"]=result.clone();
    assert_eq!(persisted.jobs[0],original.jobs[0]); assert_eq!(persisted.jobs[1],original.jobs[1]); assert_eq!(persisted.jobs[3],original.jobs[3]);
    assert_eq!(persisted.jobs[2].status,JobStatus::Failed,"stranded job_4 is a failed pre-submit history record, not pending");
    assert_eq!(persisted.jobs.len(),5); assert_eq!(persisted.jobs[4].retry_of.as_deref(),Some("job_3"));
    assert_eq!(persisted.jobs[4].output_path.as_deref(),Some("Scenes/Scene1/Cut1/S01C01_v3.mp4"));
    assert_eq!(fs::read(&wav).unwrap(),bytes); assert_eq!(fs::read(&copy).unwrap(),bytes);
    let card=result["pendingPlan"].as_str().or_else(||result["pendingAction"].as_str()).unwrap_or("");
    assert!(card.contains("performance_wav"),"{result}"); assert!(!card.contains("voice_clone"));
    assert!(!copy.with_extension("mp4").exists()); assert!(!wav.with_file_name("S01C01_v3.wav").exists());
    // Repeated preparation may replace consent, never allocate another retry.
    call().unwrap(); assert_eq!(read(root).jobs.len(),5);
    // Historical multiple-cue WAV has no proof of complete speech; do not infer it from duration.
    let mut incomplete=original.clone(); let extra=incomplete.scenes[0].cuts[0].dialogue[0].clone(); incomplete.scenes[0].cuts[0].dialogue.push(extra);
    restore(root,&incomplete); let err=call().unwrap_err(); assert!(err.contains("complete"),"{err}");
    ui_receipts["refusals"]["incomplete"]=json!({"initial":ui_snapshot(&incomplete),"error":err});
    assert_eq!(read(root).jobs[2].status,JobStatus::Failed); assert_eq!(read(root).jobs.len(),4,"history repaired but no fresh retry for incomplete audio");
    // Physically truncated audio is not a complete performance either.
    restore(root,&original); fs::write(&wav,&bytes[..bytes.len()/2]).unwrap(); fs::write(&copy,&bytes[..bytes.len()/2]).unwrap();
    assert!(call().is_err()); assert_eq!(read(root).jobs.len(),4);
    if let Ok(path)=std::env::var("SB10_UI_RECEIPTS") { fs::write(path,serde_json::to_vec_pretty(&ui_receipts).unwrap()).unwrap(); }
    println!("SB09 expected=actual: failed job_4 reconciled; original job_3/child/WAV identical; ambiguous/different/occupied outputs refuse unchanged; multi-cue/truncated audio blocked; complete fixture prepares performance_wav-only card and reserves v3 once. No approval/dispatch/TTS/video.");
    session::close();
    if let Ok(copy_root)=std::env::var("SB09_PROJECT_COPY") {
        let root=Path::new(&copy_root); assert!(root.join(".sb09-disposable").is_file());
        session::open(root.to_str().unwrap()).unwrap();
        let before=import::load_state(root).unwrap();
        let old=before.jobs.iter().find(|j|j.id=="job_3").unwrap().clone();
        let retry=before.jobs.iter().find(|j|j.id=="job_4").unwrap().clone();
        let original_wav=fs::read(root.join("Scenes/Scene1/Cut1/S01C01.wav")).unwrap();
        let retry_wav=fs::read(root.join("Scenes/Scene1/Cut1/S01C01_v2.wav")).unwrap();
        let err=workbench_host::dispatch(&app,&rec,"workflow_prepare_render",&json!({"projectRoot":root,"sceneId":1,"cut":1,"resumeSavedAudio":true})).unwrap_err();
        assert!(err.contains("complete multi-line dialogue"),"{err}");
        let after=read(root); assert_eq!(after.jobs.len(),before.jobs.len());
        assert_eq!(after.jobs.iter().find(|j|j.id=="job_3").unwrap(),&old);
        let repaired=after.jobs.iter().find(|j|j.id=="job_4").unwrap();
        assert_eq!(repaired.status,JobStatus::Failed); assert_eq!(repaired.output_path,retry.output_path);
        for job in before.jobs.iter().filter(|j|j.id!="job_4") { assert_eq!(after.jobs.iter().find(|j|j.id==job.id).unwrap(),job); }
        assert_eq!(fs::read(root.join("Scenes/Scene1/Cut1/S01C01.wav")).unwrap(),original_wav);
        assert_eq!(fs::read(root.join("Scenes/Scene1/Cut1/S01C01_v2.wav")).unwrap(),retry_wav);
        assert!(!root.join("Scenes/Scene1/Cut1/S01C01_v2.mp4").exists());
        println!("SB09 full disposable-copy expected=actual: {} jobs retained; only job_4 history changed Pending→Failed; complete-dialogue refusal; paid WAVs and all other jobs preserved; no retry allocated/card published/provider dispatch.",after.jobs.len());
        session::close();
    }
}
