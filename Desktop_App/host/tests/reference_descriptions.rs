use serde_json::json;
use std::fs;
use workbench_host::{ctx::HostCtx, dispatch, voice::RecorderState, workflow::{import, state::ProjectState}};

#[test]
fn descriptions_roundtrip_without_disturbing_existing_slots() {
    let dir = tempfile::tempdir().unwrap(); let root = dir.path().join("project"); fs::create_dir_all(&root).unwrap();
    let state = ProjectState::from_json(&json!({"version":2,"scenes":[{"id":1,"title":"Room","look_board":{"slots":[{"id":"s1_look_01","role":"prop_reference","title":"Key","prompt":"Brass key","asset_ids":["old"],"approved_asset_id":"old"}]}}]}).to_string()).unwrap();
    import::persist_state(&root,&state).unwrap();
    let app=HostCtx::new(dir.path().join("settings")); let rec=RecorderState::new();
    dispatch(&app,&rec,"open_project",&json!({"path":root})).unwrap();
    let call=|v:serde_json::Value| dispatch(&app,&rec,"workflow_save_reference_description",&v);
    call(json!({"projectRoot":root,"sceneId":1,"description":"A silver clock","category":"user_defined"})).unwrap();
    let saved=import::load_state(&root).unwrap(); let slots=&saved.scenes[0].look_board.as_ref().unwrap().slots;
    assert_eq!(slots.len(),2); assert_eq!(slots[0].approved_asset_id.as_deref(),Some("old"));
    let id=slots[1].id.clone(); assert_eq!(slots[1].prompt.as_deref(),Some("A silver clock"));
    call(json!({"projectRoot":root,"sceneId":1,"slotId":id,"description":"A red clock","category":"prop_reference"})).unwrap();
    let saved=import::load_state(&root).unwrap(); let slots=&saved.scenes[0].look_board.as_ref().unwrap().slots;
    assert_eq!(slots.len(),2); assert_eq!(slots[1].prompt.as_deref(),Some("A red clock")); assert_eq!(slots[1].role,workbench_host::workflow::state::AssetRole::PropReference); assert_eq!(slots[0].approved_asset_id.as_deref(),Some("old"));
}
