use serde_json::json;
use std::fs;
use workbench_host::{ctx::HostCtx, dispatch, pending, voice::RecorderState, workflow::{import, state::ProjectState}};

#[test]
fn selected_route_reaches_pending_approval_without_changing_settings() {
    let dir = tempfile::tempdir().unwrap(); let root = dir.path().join("project"); fs::create_dir_all(&root).unwrap();
    let state = ProjectState::from_json(&json!({"version":2,"scenes":[{"id":1,"title":"Room","look_board":{"slots":[{"id":"slot","role":"interior_reference","title":"Room","prompt":"A room","asset_ids":[]}]}}],"assets":[{"id":"a","path":"a.png","role":"interior_reference","status":"draft","scene_id":1},{"id":"b","path":"b.png","role":"interior_reference","status":"draft","scene_id":1}]}).to_string()).unwrap();
    fs::write(root.join("a.png"),b"fixture a").unwrap(); fs::write(root.join("b.png"),b"fixture b").unwrap();
    import::persist_state(&root,&state).unwrap();
    let app=HostCtx::new(dir.path().join("settings")); fs::create_dir_all(dir.path().join("settings")).unwrap();
    let settings=json!({"apis":[{"id":"offline","name":"Offline","kind":"atlas-cloud","baseUrl":"http://127.0.0.1:9","enabled":true}],"capabilityDefaults":{"image.text_to_image":{"apiId":"offline","modelId":"text-model"},"image.image_to_image":{"apiId":"offline","modelId":"chosen-model"}}}).to_string();
    fs::write(app.settings_path(),&settings).unwrap(); let rec=RecorderState::new();
    dispatch(&app,&rec,"open_project",&json!({"path":root})).unwrap();
    let result=dispatch(&app,&rec,"workflow_generate_look",&json!({"projectRoot":root,"sceneId":1,"slotId":"slot","imageCapability":"image.image_to_image","referenceAssetIds":[]})).unwrap();
    let action=pending::current().unwrap();
    assert_eq!(action.inputs["image_capability"], "image.image_to_image");
    assert!(result["pendingAction"].as_str().unwrap().contains("chosen-model"));
    assert_eq!(fs::read_to_string(app.settings_path()).unwrap(),settings);
    pending::cancel_all();
    let error=dispatch(&app,&rec,"workflow_generate_look",&json!({"projectRoot":root,"sceneId":1,"slotId":"slot","imageCapability":"image.edit","referenceAssetIds":[]})).unwrap_err();
    assert!(error.contains("image.edit"), "error must name unconfigured selected slot: {error}");
    assert!(pending::current().is_none());
    let error=dispatch(&app,&rec,"workflow_generate_look",&json!({"projectRoot":root,"sceneId":1,"slotId":"slot","imageCapability":42,"referenceAssetIds":[]})).unwrap_err();
    assert!(error.contains("imageCapability"),"{error}");
    assert!(pending::current().is_none());
    let card=dispatch(&app,&rec,"workflow_generate_look",&json!({"projectRoot":root,"sceneId":1,"slotId":"slot","imageCapability":"image.image_to_image","referenceAssetIds":["b","a"]})).unwrap();
    let action=pending::current().unwrap(); assert_eq!(action.inputs["reference_images"],json!(["b.png","a.png"]));
    let text=card["pendingAction"].as_str().unwrap();
    assert!(text.find("b.png").unwrap()<text.find("a.png").unwrap());
    let id=text.split(pending::APPROVE_TOKEN_PREFIX).nth(1).unwrap().split("]]").next().unwrap();
    let mut changed:serde_json::Value=serde_json::from_str(&settings).unwrap(); changed["capabilityDefaults"]["image.image_to_image"]["modelId"]=json!("replacement-model");
    fs::write(app.settings_path(),changed.to_string()).unwrap();
    let error=workbench_host::director::director_approve(app.clone(),id.to_string()).unwrap_err();
    assert!(error.contains("settings changed"),"{error}");
    dispatch(&app,&rec,"close_project",&json!({})).unwrap();
}
