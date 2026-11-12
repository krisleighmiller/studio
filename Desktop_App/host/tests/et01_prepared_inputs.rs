//! Offline prepared-input invalidation. Never approve or submit a provider request.
use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};
use workbench_host::{ctx::HostCtx, dispatch, pending, session, voice::RecorderState,
    workflow::{self, import, production, state::{JobStatus, ProjectState}}};

struct Guard;
impl Drop for Guard { fn drop(&mut self) { pending::cancel_all(); session::close(); } }
fn read(root: &Path) -> ProjectState { import::load_state(root).unwrap() }
fn token(card: &Value) -> String { card["pendingAction"].as_str().unwrap().split(pending::APPROVE_TOKEN_PREFIX).nth(1).unwrap().split("]]").next().unwrap().into() }
fn fixture() -> (tempfile::TempDir, std::path::PathBuf, HostCtx, RecorderState) {
    let dir = tempfile::tempdir().unwrap(); let root = dir.path().join("project");
    fs::create_dir_all(root.join("Scenes/Scene1/Cut1")).unwrap();
    fs::create_dir_all(root.join("Preproduction/Scene1/references")).unwrap();
    fs::create_dir_all(root.join("Scripts")).unwrap();
    fs::write(root.join("Scripts/script.md"), "## Scene 1 — Test\n- **Dialogue:**\n  - **TEST:** Complete fixture line.\n").unwrap();
    assert!(Command::new("ffmpeg").args(["-v","error","-f","lavfi","-i","sine=frequency=440:duration=4","-c:a","pcm_s16le"]).arg(root.join("Scenes/Scene1/Cut1/performance.wav")).status().unwrap().success());
    // Synthetically valid image bytes are needed for a real source switch.
    for (name, color) in [("one.png","red"),("two.png","blue")] {
        assert!(Command::new("ffmpeg").args(["-v","error","-f","lavfi","-i",&format!("color=c={color}:s=32x32"),"-frames:v","1"]).arg(root.join("Preproduction/Scene1/references").join(name)).status().unwrap().success());
    }
    let source="Scenes/Scene1/Cut1/performance.wav";
    let mut state: ProjectState = serde_json::from_value(json!({"version":2,"scenes":[{"id":1,"stable_id":"scene_1","title":"Test","source_path":"Scripts/script.md","cuts":[{"id":{"source_scene":1,"cut":1},"stable_id":"shot_1","speech_mode":"source_video_lipsync","scene_type":"character_performance","subject":"Test","prompt":"Speak the fixture line.","dialogue":[{"speaker":"TEST","text":"Complete fixture line."}],"sketch":{"title":"Fixture","shot_role":"performance","speaker":"TEST","mouth_policy":"speaking_visible","visual_prompt":"Speak the fixture line.","dialogue":[{"speaker":"TEST","text":"Complete fixture line.","heard_as":"on_screen"}]},"visual_source":{"strategy":"shared_reference","asset_id":"asset_18"}}]}],"assets":[{"id":"asset_18","path":"Preproduction/Scene1/references/one.png","role":"interior_reference","status":"approved","scene_id":1},{"id":"asset_20","path":"Preproduction/Scene1/references/two.png","role":"interior_reference","status":"approved","scene_id":1},{"id":"asset_19","path":source,"role":"source_audio","status":"draft","scene_id":1,"cut_id":{"source_scene":1,"cut":1}}],"jobs":[{"id":"history","kind":"render","status":"failed","error":"Retained"}]})).unwrap();
    let original_script = fs::read_to_string(root.join("Scripts/script.md")).unwrap();
    state.scene_mut(1,"Test").source_excerpt = Some(original_script.trim().to_string());
    state.scene_mut(1,"Test").source_scene_number = Some(1);
    let initial_dialogue = state.scene(1).unwrap().cuts[0].dialogue.clone();
    state.scene_mut(1,"Test").dialogue = initial_dialogue;
    production::sync_from_planning_with_prompt(&root,&mut state,1,1,Some("Speak the fixture line.")).unwrap();
    import::persist_state(&root,&state).unwrap();
    fs::create_dir_all(dir.path().join("settings")).unwrap();
    fs::write(dir.path().join("settings/settings.json"),json!({"apis":[{"id":"offline","name":"Offline","kind":"atlas-cloud","baseUrl":"http://127.0.0.1:9","enabled":true}],"capabilityDefaults":{"video.lip_sync_text_narration":{"apiId":"offline","modelId":"atlascloud/van-2.5/image-to-video"}}}).to_string()).unwrap();
    let app=HostCtx::new(dir.path().join("settings")); let rec=RecorderState::new();
    session::open(root.to_str().unwrap()).unwrap();
    dispatch(&app,&rec,"workflow_select_performance",&json!({"projectRoot":root,"sceneId":1,"cut":1,"assetId":"asset_19","reviewedAssetId":"asset_19"})).unwrap();
    (dir,root,app,rec)
}
fn prepared(root:&Path,app:&HostCtx,rec:&RecorderState)->(String,String) {
    let card=dispatch(app,rec,"workflow_prepare_render",&json!({"projectRoot":root,"sceneId":1,"cut":1,"selectedPerformance":true,"reviewedAssetId":"asset_19"})).unwrap();
    let action=pending::current().unwrap();
    assert_eq!(action.inputs["performance_wav"],"Scenes/Scene1/Cut1/performance.wav");
    let output=action.inputs["output_path"].as_str().unwrap().to_string();
    (token(&card),output)
}
fn refused(root:&Path,old:&str,output:&str,label:&str) {
    assert!(pending::take(old).is_err(),"{label}: exact old approval still active");
    let state=read(root);
    assert!(state.jobs.iter().any(|j|j.output_path.as_deref()==Some(output) && j.status==JobStatus::Cancelled),"{label}: prepared job not cancelled");
    assert_eq!(state.jobs[0].id,"history");
    assert_eq!(state.jobs[0].status,JobStatus::Failed);
    assert!(!root.join(output).exists());
    println!("{label}: old exact approval refused, pending job cancelled, historical job/media preserved, loopback-only setting and zero provider submissions");
}
#[test]
fn script_reimport_revokes_prepared_video() {
    let (_dir,root,app,rec)=fixture(); let _g=Guard;
    let (old,output)=prepared(&root,&app,&rec);
    fs::write(root.join("Scripts/script.md"),"## Scene 1 — Test\n- **Dialogue:**\n  - **TEST:** Revised fixture line.\n").unwrap();
    dispatch(&app,&rec,"workflow_import",&json!({"projectRoot":root,"persist":true})).unwrap();
    assert!(read(&root).scene(1).unwrap().source_excerpt.as_deref().unwrap_or("").contains("Revised fixture line"));
    refused(&root,&old,&output,"X1-script");
}
#[test]
fn image_switch_revokes_prepared_video_and_retains_prior_bytes() {
    let (_dir,root,app,rec)=fixture(); let _g=Guard;
    let first=root.join("Preproduction/Scene1/references/one.png");let bytes=fs::read(&first).unwrap();
    let (old,output)=prepared(&root,&app,&rec);
    dispatch(&app,&rec,"workflow_set_visual_source",&json!({"projectRoot":root,"sceneId":1,"cut":1,"strategy":"shared_reference","assetId":"asset_20"})).unwrap();
    assert_eq!(read(&root).scene(1).unwrap().cuts[0].visual_source.as_ref().unwrap().asset_id.as_deref(),Some("asset_20"));
    assert_eq!(fs::read(first).unwrap(),bytes);
    refused(&root,&old,&output,"X1-image");
}
#[test]
fn timing_change_revokes_prepared_video_and_keeps_history() {
    let (_dir,root,app,rec)=fixture(); let _g=Guard;
    let (old,output)=prepared(&root,&app,&rec);
    let shot=read(&root).scene(1).unwrap().animatic.as_ref().unwrap().shots[0].id.clone();
    workflow::cmd_set_animatic_duration(&json!({"projectRoot":root,"sceneId":1,"shotId":shot,"durationSeconds":8.0})).unwrap();
    assert_eq!(read(&root).scene(1).unwrap().animatic.as_ref().unwrap().shots[0].duration_seconds,8.0);
    refused(&root,&old,&output,"X1-timeline");
}
