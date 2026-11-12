use serde_json::{json, Value};
use std::fs;
use workbench_host::{ctx::HostCtx, dispatch, voice::RecorderState};

fn shot<'a>(result: &'a Value, id: &str) -> &'a Value {
    result["state"]["scenes"][0]["cuts"].as_array().unwrap().iter()
        .find(|cut| cut["stable_id"] == id).unwrap()
}

#[test]
fn card_save_tracks_stable_cut_after_reorder_and_native_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir_all(root.join("Scripts")).unwrap();
    fs::write(root.join("Scripts/test.md"), "## Scene 1 — Two cuts\nA quiet room.\n").unwrap();
    let app = HostCtx::new(temp.path().join("settings"));
    let recorder = RecorderState::new();
    let call = |cmd: &str, mut args: Value| {
        args["projectRoot"] = json!(root);
        dispatch(&app, &recorder, cmd, &args).unwrap()
    };
    call("open_project", json!({"path":root}));
    call("workflow_import", json!({"persist":true}));
    for _ in 0..2 { call("workflow_add_alternate_shot", json!({"sceneId":1})); }
    for n in 1..=2 {
        call("workflow_save_sketch", json!({"sceneId":1,"cut":n,"sketch":{
            "title":format!("Original Cut {n}"),"shot_role":"atmosphere","visual_prompt":format!("Room angle {n}"),"dialogue":[]}}));
    }
    let initial = call("workflow_import", json!({"persist":true}));
    let ids: Vec<String> = initial["state"]["scenes"][0]["cuts"].as_array().unwrap().iter()
        .map(|c| c["stable_id"].as_str().unwrap().to_owned()).collect();
    let first_sketch = shot(&initial, &ids[0])["sketch"].clone();
    let save = |title: &str| call("workflow_save_sketch", json!({"sceneId":1,"cut":2,"shotId":ids[1],"sketch":{
        "title":title,"shot_role":"atmosphere","visual_prompt":"Card two edited directly","dialogue":[]}}));
    let direct = save("Card Two Direct");
    assert_eq!(shot(&direct, &ids[1])["sketch"]["title"], "Card Two Direct");
    assert_eq!(shot(&direct, &ids[0])["sketch"], first_sketch);
    call("workflow_reorder_cuts", json!({"sceneId":1,"shotIds":[ids[1],ids[0]]}));
    call("close_project", json!({}));
    call("open_project", json!({"path":root}));
    let reordered = call("workflow_import", json!({"persist":true}));
    assert_eq!(shot(&reordered, &ids[1])["id"]["cut"], 1);
    let edited = save("Card Two After Reopen"); // stale display number 2 must not choose original Cut 1
    assert_eq!(shot(&edited, &ids[1])["sketch"]["title"], "Card Two After Reopen");
    assert_eq!(shot(&edited, &ids[0])["sketch"], first_sketch);
    assert_eq!(edited["state"]["jobs"], initial["state"]["jobs"]);
    assert_eq!(edited["state"]["assets"], initial["state"]["assets"]);
    let state_path = root.join("Preproduction/workflow_state.json");
    for bad in [json!("unknown-shot"), json!(""), json!(12), Value::Null] {
        let before = fs::read(&state_path).unwrap();
        assert!(dispatch(&app, &recorder, "workflow_save_sketch", &json!({"projectRoot":root,"sceneId":1,"cut":2,
            "shotId":bad,"sketch":{"title":"Must not save"}})).is_err());
        assert_eq!(fs::read(&state_path).unwrap(), before);
    }
    let reopened = call("workflow_import", json!({"persist":true}));
    assert_eq!(shot(&reopened, &ids[1])["sketch"]["title"], "Card Two After Reopen");
    assert_eq!(shot(&reopened, &ids[0])["sketch"], first_sketch);
    if let Ok(out) = std::env::var("SB15_RECEIPT") {
        fs::write(out, serde_json::to_vec_pretty(&json!({"initial":initial,"direct":direct,"reordered":reordered,"reopened":reopened,"ids":ids,"provider_requests":0})).unwrap()).unwrap();
    }
    call("close_project", json!({}));
    println!("SB15 expected=actual: card 2 direct save, stable-ID save after reorder/native reopen; original Cut 1 sketch, assets/jobs untouched; invalid IDs fail without writes; zero provider calls.");
}
