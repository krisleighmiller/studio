use serde_json::json;
use std::{fs, sync::Mutex};
use workbench_host::{ctx::HostCtx, dispatch, voice::RecorderState};

static SESSION: Mutex<()> = Mutex::new(());

#[test]
fn cut_limit_survives_import_and_never_removes_existing_cuts() {
    let _lock = SESSION.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir_all(root.join("Scripts")).unwrap();
    fs::write(root.join("Scripts/test.md"), "# Test\n\n## Scene 1 — Short\nA brief moment.\n").unwrap();
    let app = HostCtx::new(temp.path().join("settings"));
    let recorder = RecorderState::new();
    dispatch(&app, &recorder, "open_project", &json!({"path": root})).unwrap();
    let imported = dispatch(&app, &recorder, "workflow_import", &json!({"projectRoot": root, "persist": true})).unwrap();
    let id = imported["state"]["scenes"][0]["id"].as_u64().unwrap();
    let args = json!({"projectRoot": root, "sceneId": id, "maxCuts": 2});
    let saved = dispatch(&app, &recorder, "workflow_set_max_cuts", &args).unwrap();
    assert_eq!(saved["state"]["scenes"][0]["max_cuts"], 2);
    let refreshed = dispatch(&app, &recorder, "workflow_import", &json!({"projectRoot": root, "persist": true})).unwrap();
    assert_eq!(refreshed["state"]["scenes"][0]["max_cuts"], 2);
    assert!(refreshed["state"]["scenes"][0]["cuts"].as_array().unwrap().is_empty());
    for invalid in [json!(0), json!(13), json!(4294967298u64), json!("2")] {
        let bad = dispatch(&app, &recorder, "workflow_set_max_cuts", &json!({"projectRoot": root, "sceneId": id, "maxCuts": invalid}));
        assert!(bad.unwrap_err().contains("between 1 and 12"));
    }
    let unchanged = dispatch(&app, &recorder, "workflow_import", &json!({"projectRoot": root})).unwrap();
    assert_eq!(unchanged["state"]["scenes"][0]["max_cuts"], 2);
    let add = json!({"projectRoot": root, "sceneId": id});
    dispatch(&app, &recorder, "workflow_add_alternate_shot", &add).unwrap();
    dispatch(&app, &recorder, "workflow_add_alternate_shot", &add).unwrap();
    assert!(dispatch(&app, &recorder, "workflow_add_alternate_shot", &add)
        .unwrap_err().contains("maximum is 2"));
    let after = dispatch(&app, &recorder, "workflow_import", &json!({"projectRoot": root})).unwrap();
    assert_eq!(after["state"]["scenes"][0]["cuts"].as_array().unwrap().len(), 2);
    assert_eq!(after["state"]["scenes"][0]["max_cuts"], 2);
    let cuts = after["state"]["scenes"][0]["cuts"].as_array().unwrap();
    let removed_id = cuts[0]["stable_id"].as_str().unwrap().to_owned();
    let kept_id = cuts[1]["stable_id"].as_str().unwrap().to_owned();
    let kept_animatic_id = after["state"]["scenes"][0]["animatic"]["shots"][1]["id"].clone();
    let remove = |shot: &str| dispatch(&app, &recorder, "workflow_remove_cut",
        &json!({"projectRoot": root, "sceneId": id, "shotId": shot}));
    remove(&removed_id).unwrap();
    assert!(remove(&removed_id).unwrap_err().contains("not in scene"));
    let reopened = dispatch(&app, &recorder, "workflow_import", &json!({"projectRoot": root, "persist": true})).unwrap();
    let surviving = &reopened["state"]["scenes"][0];
    assert_eq!(surviving["cuts"].as_array().unwrap().len(), 1);
    assert_eq!(surviving["cuts"][0]["stable_id"], kept_id);
    assert_eq!(surviving["cuts"][0]["id"]["cut"], 1);
    assert_eq!(surviving["animatic"]["shots"].as_array().unwrap().len(), 1);
    assert_eq!(surviving["animatic"]["shots"][0]["id"], kept_animatic_id);
    assert_eq!(surviving["animatic"]["shots"][0]["cut_id"]["cut"], 1);
    remove(&kept_id).unwrap();
    let empty = dispatch(&app, &recorder, "workflow_import", &json!({"projectRoot": root, "persist": true})).unwrap();
    assert!(empty["state"]["scenes"][0]["cuts"].as_array().unwrap().is_empty());
    assert_eq!(empty["state"]["scenes"][0]["max_cuts"], 2);
    dispatch(&app, &recorder, "close_project", &json!({})).unwrap();
}

#[test]
fn seven_sketch_only_cuts_can_be_reduced_to_two_and_stay_gone() {
    let _lock = SESSION.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir_all(root.join("Scripts")).unwrap();
    fs::write(root.join("Scripts/test.md"), "## Scene 1 — Ten seconds\nA brief moment.\n").unwrap();
    let app = HostCtx::new(temp.path().join("settings"));
    let recorder = RecorderState::new();
    dispatch(&app, &recorder, "open_project", &json!({"path": root})).unwrap();
    let imported = dispatch(&app, &recorder, "workflow_import", &json!({"projectRoot": root, "persist": true})).unwrap();
    let id = imported["state"]["scenes"][0]["id"].as_u64().unwrap();
    let add = json!({"projectRoot": root, "sceneId": id});
    for _ in 0..7 { dispatch(&app, &recorder, "workflow_add_alternate_shot", &add).unwrap(); }
    dispatch(&app, &recorder, "workflow_set_max_cuts", &json!({"projectRoot": root, "sceneId": id, "maxCuts": 2})).unwrap();
    let before = dispatch(&app, &recorder, "workflow_import", &json!({"projectRoot": root})).unwrap();
    let ids: Vec<String> = before["state"]["scenes"][0]["cuts"].as_array().unwrap().iter()
        .map(|cut| cut["stable_id"].as_str().unwrap().to_owned()).collect();
    for shot in ids[2..].iter().rev() {
        dispatch(&app, &recorder, "workflow_remove_cut",
            &json!({"projectRoot": root, "sceneId": id, "shotId": shot})).unwrap();
    }
    let after = dispatch(&app, &recorder, "workflow_import", &json!({"projectRoot": root, "persist": true})).unwrap();
    let scene = &after["state"]["scenes"][0];
    assert_eq!(scene["max_cuts"], 2);
    assert_eq!(scene["cuts"].as_array().unwrap().len(), 2);
    assert_eq!(scene["cuts"][0]["stable_id"], ids[0]);
    assert_eq!(scene["cuts"][1]["stable_id"], ids[1]);
    assert_eq!(scene["animatic"]["shots"].as_array().unwrap().len(), 2);
    dispatch(&app, &recorder, "close_project", &json!({})).unwrap();
}
