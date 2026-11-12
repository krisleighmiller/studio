use serde_json::json;
use std::fs;
use workbench_host::{ctx::HostCtx, dispatch, voice::RecorderState, workflow::{import, state::ProjectState}};

#[test]
fn native_slot_delete_persists_without_deleting_media_or_registry() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    let rel = "Preproduction/Scene1/references/selected.png";
    fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
    fs::write(root.join(rel), b"image contents").unwrap();
    let state = ProjectState::from_json(&json!({"version":2,"scenes":[{"id":1,"title":"Scene","look_board":{"slots":[{"id":"slot","role":"interior_reference","title":"Interior","asset_ids":["a"]}]}}],"assets":[{"id":"a","path":rel,"role":"interior_reference","status":"draft","scene_id":1}]}).to_string()).unwrap();
    import::persist_state(&root, &state).unwrap();
    let app = HostCtx::new(dir.path().join("settings"));
    let recorder = RecorderState::new();
    dispatch(&app, &recorder, "open_project", &json!({"path":root})).unwrap();
    dispatch(&app, &recorder, "workflow_delete_look_slot", &json!({"projectRoot":root,"sceneId":1,"slotId":"slot"})).unwrap();
    let saved = ProjectState::from_json(&fs::read_to_string(root.join("Preproduction/workflow_state.json")).unwrap()).unwrap();
    assert!(saved.scenes[0].look_board.as_ref().unwrap().slots.is_empty());
    assert!(saved.asset("a").is_some());
    assert_eq!(fs::read(root.join(rel)).unwrap(), b"image contents");
    dispatch(&app, &recorder, "close_project", &json!({})).unwrap();
}
