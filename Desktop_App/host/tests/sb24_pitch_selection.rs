//! Offline integration of pitch lineage with exact native render approval binding.
//! No approval is executed and no voice/video provider is called.
use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};
use workbench_host::{ctx::HostCtx, pending, session, voice::RecorderState,
    workflow::{import, production, state::{AssetStatus, JobStatus, ProjectState}}};

fn read(root: &Path) -> ProjectState {
    ProjectState::from_json(&fs::read_to_string(root.join("Preproduction/workflow_state.json")).unwrap()).unwrap()
}
fn token(card: &Value) -> String {
    card["pendingAction"].as_str().unwrap().split("[[workbench_approve:").nth(1).unwrap().split("]]").next().unwrap().into()
}
fn decode(path: &Path) {
    let result = Command::new("ffmpeg").args(["-nostdin", "-v", "error", "-xerror", "-err_detect", "explode", "-i"])
        .arg(path).args(["-f", "null", "-"]).output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
}
#[test]
fn pitch_creation_keeps_consent_but_selection_revokes_it_and_stale_writers_fail() {
    let dir = tempfile::tempdir().unwrap(); let root = dir.path();
    session::open(root.to_str().unwrap()).unwrap();
    fs::create_dir_all(root.join("Scenes/Scene1/Cut1")).unwrap();
    fs::create_dir_all(root.join("Preproduction/Scene1/references")).unwrap();
    let source = "Scenes/Scene1/Cut1/performance.wav";
    assert!(Command::new("ffmpeg").args(["-v", "error", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=4", "-c:a", "pcm_s16le"])
        .arg(root.join(source)).status().unwrap().success());
    fs::write(root.join("Preproduction/Scene1/references/interior.png"), b"offline reference").unwrap();
    let mut state = ProjectState::from_json(&json!({"version":2,"scenes":[{"id":1,"title":"Pitch consent fixture","cuts":[{"id":{"source_scene":1,"cut":1},"stable_id":"shot_1","speech_mode":"source_video_lipsync","scene_type":"character_performance","subject":"Test","prompt":"Speak the fixture line.","dialogue":[{"speaker":"Test","text":"Complete fixture line."}],"sketch":{"title":"Fixture","shot_role":"performance","speaker":"Test","mouth_policy":"speaking_visible","visual_prompt":"Speak the fixture line.","dialogue":[{"speaker":"Test","text":"Complete fixture line.","heard_as":"on_screen"}]},"visual_source":{"strategy":"shared_reference","asset_id":"asset_18"}}]}],"assets":[{"id":"asset_18","path":"Preproduction/Scene1/references/interior.png","role":"interior_reference","status":"approved","scene_id":1},{"id":"asset_19","path":source,"role":"source_audio","status":"draft","scene_id":1,"cut_id":{"source_scene":1,"cut":1}}],"jobs":[{"id":"history","kind":"render","status":"failed","error":"Historical record retained"}]}).to_string()).unwrap();
    production::sync_from_planning_with_prompt(root, &mut state, 1, 1, Some("Speak the fixture line.")).unwrap();
    import::persist_state(root, &state).unwrap();
    let historical = read(root).jobs[0].clone(); let original = fs::read(root.join(source)).unwrap();
    let config = tempfile::tempdir().unwrap(); let settings = config.path().join("settings");
    fs::create_dir_all(&settings).unwrap();
    fs::write(settings.join("settings.json"), json!({"apis":[{"id":"offline","name":"Offline","kind":"atlas-cloud","baseUrl":"http://127.0.0.1:9","enabled":true}],"capabilityDefaults":{"video.lip_sync_text_narration":{"apiId":"offline","modelId":"atlascloud/van-2.5/image-to-video"}}}).to_string()).unwrap();
    let app = HostCtx::new(settings); let rec = RecorderState::new();
    let call = |cmd: &str, extra: Value| {
        let mut args = json!({"projectRoot":root,"sceneId":1,"cut":1});
        args.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        if cmd == "workflow_select_performance" && args["assetId"].is_string() { args["reviewedAssetId"] = args["assetId"].clone(); }
        if cmd == "workflow_prepare_render" && args["selectedPerformance"] == true { args["reviewedAssetId"] = json!(read(root).scenes[0].cuts[0].selected_performance_asset_id); }
        workbench_host::dispatch(&app, &rec, cmd, &args)
    };
    call("workflow_select_performance", json!({"assetId":"asset_19"})).unwrap();
    let old = token(&call("workflow_prepare_render", json!({"selectedPerformance":true})).unwrap());
    let before = read(root); let stale = before.clone();
    call("workflow_version_audio", json!({"assetId":"asset_19","pitch":1.5})).unwrap();
    let after = read(root);
    let high = after.assets.iter().find(|a| a.parent_asset_id.as_deref() == Some("asset_19")).unwrap().clone();
    assert_eq!(after.scenes[0].cuts[0].selected_performance_asset_id.as_deref(), Some("asset_19"));
    assert_eq!(after.jobs, before.jobs, "unselected creation must not cancel prepared video");
    let retained = pending::take(&old).expect("creation retains exact approval ID; consume without execution");
    assert_eq!(retained.action.inputs["performance_wav"], source);
    let old = token(&call("workflow_prepare_render", json!({"selectedPerformance":true})).unwrap());
    assert_eq!(pending::current().unwrap().inputs["performance_wav"], source);
    let persisted = fs::read(root.join("Preproduction/workflow_state.json")).unwrap();
    assert!(import::persist_state(root, &stale).unwrap_err().contains("revision"));
    assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(), persisted);
    call("workflow_select_performance", json!({"assetId":high.id})).unwrap();
    assert!(pending::take(&old).is_err(), "source approval must be unusable after explicit pitch switch");
    assert_eq!(read(root).jobs.iter().find(|j| j.performance_asset_id.as_deref() == Some("asset_19")).unwrap().status, JobStatus::Cancelled);
    let high_card = token(&call("workflow_prepare_render", json!({"selectedPerformance":true})).unwrap());
    assert_eq!(pending::current().unwrap().inputs["performance_wav"], high.path);
    let jobs_before_chain = read(root).jobs;
    // Chained derivative records its immediate parent, not a flattened root parent.
    call("workflow_version_audio", json!({"assetId":high.id,"pitch":0.5})).unwrap();
    let chained_state = read(root);
    let low = chained_state.assets.iter().find(|a| a.parent_asset_id.as_deref() == Some(high.id.as_str())).unwrap().clone();
    assert!(pending::take(&high_card).is_ok(), "chained creation retains its exact approval without execution");
    let high_card = token(&call("workflow_prepare_render", json!({"selectedPerformance":true})).unwrap());
    assert_eq!(chained_state.jobs, jobs_before_chain);
    for asset in [&high, &low] {
        assert_eq!(asset.status, AssetStatus::Draft);
        let notes: Value = serde_json::from_str(asset.notes.as_deref().unwrap()).unwrap();
        assert_eq!(notes["operation"], "local_wav_pitch");
        assert!(chained_state.versions.iter().any(|v| v.asset_id == asset.id && v.path == asset.path));
        decode(&root.join(&asset.path));
    }
    let high_bytes = fs::read(root.join(&high.path)).unwrap();
    let low_bytes = fs::read(root.join(&low.path)).unwrap();
    call("workflow_select_performance", json!({"assetId":low.id})).unwrap();
    assert!(pending::take(&high_card).is_err());
    let low_card = token(&call("workflow_prepare_render", json!({"selectedPerformance":true})).unwrap());
    assert_eq!(pending::current().unwrap().inputs["performance_wav"], low.path);
    let before_duplicate = fs::read(root.join("Preproduction/workflow_state.json")).unwrap();
    call("workflow_select_performance", json!({"assetId":low.id})).unwrap();
    assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(), before_duplicate);
    assert!(pending::take(&low_card).is_ok(), "duplicate selection retains exact approval without execution");
    let low_card = token(&call("workflow_prepare_render", json!({"selectedPerformance":true})).unwrap());
    call("workflow_select_performance", json!({"assetId":null})).unwrap();
    assert!(pending::take(&low_card).is_err());
    for _ in 0..2 {
        let imported = import::import_project(root, Default::default()).unwrap().state;
        for asset in [&high, &low] {
            assert_eq!(imported.asset(&asset.id), Some(asset));
            assert_eq!(imported.assets.iter().filter(|a| a.path == asset.path).count(), 1);
        }
        assert_eq!(imported.jobs[0], historical);
    }
    assert_eq!(fs::read(root.join(source)).unwrap(), original);
    assert_eq!(fs::read(root.join(&high.path)).unwrap(), high_bytes);
    assert_eq!(fs::read(root.join(&low.path)).unwrap(), low_bytes);
    assert!(read(root).jobs.iter().all(|j| !j.kind.starts_with("atlas_generate_")));
    assert!(!root.join("Scenes/Scene1/Cut1/S01C01.mp4").exists());
    session::close();
    println!("PASS: pitch creation preserves exact consent/jobs; source→pitch→chained-pitch switch and clear revoke old native IDs; duplicate selection unchanged; strict WAV decode; two reimports retain immediate lineage/unique IDs; stale writer refused byte-identically; original and derivative bytes preserved; no provider approval executed");
}
