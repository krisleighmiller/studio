use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};
use workbench_host::{ctx::HostCtx, dispatch, voice::RecorderState, workflow::{import, state::{AssetStatus, ProjectState}}};
fn saved(root: &Path) -> ProjectState {
    ProjectState::from_json(&fs::read_to_string(root.join("Preproduction/workflow_state.json")).unwrap()).unwrap()
}
#[test]
fn crops_approved_still_to_explicit_draft_preserving_source_and_lineage() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project"); fs::create_dir_all(&root).unwrap();
    let outside = dir.path().join("source.png");
    assert!(Command::new("ffmpeg").args(["-v","error","-f","lavfi","-i","testsrc=size=80x60:rate=1","-frames:v","1","-threads","1"]).arg(&outside).status().unwrap().success());
    let initial = ProjectState::from_json(&json!({"version":2,"scenes":[{"id":1,"title":"Disposable still","intent_approved":true}]}).to_string()).unwrap();
    import::persist_state(&root, &initial).unwrap();
    let app = HostCtx::new(dir.path().join("settings")); let recorder = RecorderState::new();
    let call = |cmd: &str, args: Value| dispatch(&app, &recorder, cmd, &args);
    call("open_project", json!({"path":root})).unwrap();
    call("grant_path",json!({"path":outside,"intent":"read"})).unwrap();
    let attached = call("workflow_attach_look",json!({"projectRoot":root,"sceneId":1,"sourcePath":outside,"role":"interior_reference"})).unwrap();
    let slot = attached["attachedLook"]["slotId"].as_str().unwrap();
    let source = attached["attachedLook"]["assetId"].as_str().unwrap();
    let source_path = root.join(&saved(&root).asset(source).unwrap().path);
    let source_bytes = fs::read(&source_path).unwrap();
    call("workflow_approve_look",json!({"projectRoot":root,"sceneId":1,"slotId":slot,"assetId":source})).unwrap();
    let args = json!({"projectRoot":root,"sceneId":1,"slotId":slot,"assetId":source,"x":11,"y":5,"width":51,"height":41});
    let edited = call("workflow_crop_look",args.clone()).unwrap();
    let id = edited["editedLook"]["assetId"].as_str().unwrap();
    let state = saved(&root); let derivative = state.asset(id).unwrap();
    assert_eq!(derivative.parent_asset_id.as_deref(),Some(source));
    assert_eq!(derivative.status,AssetStatus::Draft);
    assert_eq!(state.scene(1).unwrap().look_board.as_ref().unwrap().slots[0].approved_asset_id.as_deref(),Some(source));
    assert_eq!(fs::read(&source_path).unwrap(),source_bytes);
    assert_ne!(fs::read(root.join(&derivative.path)).unwrap(),source_bytes);
    assert!(derivative.path.ends_with(".png"));
    let dimensions = Command::new("ffprobe").args(["-v","error","-show_entries","stream=width,height","-of","csv=p=0"])
        .arg(root.join(&derivative.path)).output().unwrap();
    assert!(dimensions.status.success());
    assert_eq!(String::from_utf8_lossy(&dimensions.stdout).trim(),"51,41");
    let first_path = derivative.path.clone();
    assert!(call("workflow_crop_look",json!({"projectRoot":root,"sceneId":1,"slotId":slot,"assetId":source,"x":70,"y":0,"width":40,"height":40})).is_err());
    assert!(call("workflow_crop_look",json!({"projectRoot":root,"sceneId":1,"slotId":slot,"assetId":source,"x":0,"y":0,"width":80,"height":60})).is_err());
    assert!(call("workflow_crop_look",json!({"projectRoot":root,"sceneId":1,"slotId":slot,"assetId":"not-the-slot","x":10,"y":5,"width":50,"height":40})).is_err());
    let next = workbench_host::workflow::look::next_look_rel(&root, saved(&root).scene(1).unwrap(), saved(&root).asset(source).unwrap().role, "png");
    fs::write(root.join(&next), b"unregistered collision").unwrap();
    let second = call("workflow_crop_look",args).unwrap();
    assert_ne!(second["editedLook"]["path"],first_path);
    assert_ne!(second["editedLook"]["path"],next);
    assert_eq!(fs::read(root.join(&next)).unwrap(),b"unregistered collision");
    assert!(root.join(&first_path).is_file());
    call("workflow_approve_look",json!({"projectRoot":root,"sceneId":1,"slotId":slot,"assetId":id})).unwrap();
    let reopened = call("workflow_import",json!({"projectRoot":root,"persist":true})).unwrap();
    assert_eq!(reopened["state"]["scenes"][0]["look_board"]["slots"][0]["approved_asset_id"], id);
    assert_eq!(saved(&root).asset(id).unwrap().parent_asset_id.as_deref(),Some(source));
    assert_eq!(fs::read(source_path).unwrap(),source_bytes);
    call("close_project",json!({})).unwrap();
}
