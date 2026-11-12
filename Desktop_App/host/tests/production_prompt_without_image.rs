use serde_json::json;
use std::fs;
use workbench_host::{ctx::HostCtx, dispatch, voice::RecorderState};

#[test]
fn saves_a_production_prompt_without_image_or_preexisting_note() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir_all(root.join("Scripts")).unwrap();
    fs::write(root.join("Scripts/test.md"), "## Scene 1 — Simple shot\nA room with a laptop.\n").unwrap();
    let app = HostCtx::new(temp.path().join("settings"));
    let recorder = RecorderState::new();
    dispatch(&app, &recorder, "open_project", &json!({"path": root})).unwrap();
    let imported = dispatch(&app, &recorder, "workflow_import", &json!({"projectRoot": root, "persist": true})).unwrap();
    let scene = imported["state"]["scenes"][0]["id"].as_u64().unwrap();
    dispatch(&app, &recorder, "workflow_add_alternate_shot", &json!({"projectRoot": root, "sceneId": scene})).unwrap();
    dispatch(&app, &recorder, "workflow_save_sketch", &json!({"projectRoot": root, "sceneId": scene, "cut": 1,
        "sketch": {"title":"Opening","shot_role":"atmosphere","visual_prompt":"Slow camera across the room.","dialogue":[]}})).unwrap();
    dispatch(&app, &recorder, "workflow_set_visual_source", &json!({"projectRoot": root, "sceneId": scene, "cut": 1,"strategy":"no_start_frame"})).unwrap();
    let result = dispatch(&app, &recorder, "workflow_save_production_prompt", &json!({"projectRoot": root, "sceneId": scene, "cut": 1,
        "prompt": "Camera moves slowly across the room.", "negativePrompt": "No extra props."})).unwrap();
    let cut = &result["state"]["scenes"][0]["cuts"][0];
    assert_eq!(cut["prompt"], "Camera moves slowly across the room.");
    let path = cut["production_note_path"].as_str().unwrap();
    assert!(root.join(path).is_file());
    let text = fs::read_to_string(root.join(path)).unwrap();
    assert!(text.contains("Camera moves slowly across the room."));
    let parsed = workbench_host::scene_plan::generation::RenderRequest::from_document(&text).unwrap();
    assert_eq!(parsed.id.cut, 1);
    let reopened = dispatch(&app, &recorder, "workflow_import", &json!({"projectRoot": root, "persist": true})).unwrap();
    assert_eq!(reopened["state"]["scenes"][0]["cuts"][0]["production_note_path"], path);
    assert_eq!(reopened["state"]["scenes"][0]["cuts"][0]["prompt"], "Camera moves slowly across the room.");
    dispatch(&app, &recorder, "close_project", &json!({})).unwrap();
}
