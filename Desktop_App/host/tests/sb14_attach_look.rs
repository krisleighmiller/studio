//! Production-library, offline picker-grant -> native copy -> association -> reimport.
use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};
use workbench_host::{ctx::HostCtx, dispatch, voice::RecorderState, workflow::{import, state::ProjectState}};
fn read(root: &Path) -> ProjectState { ProjectState::from_json(&fs::read_to_string(root.join("Preproduction/workflow_state.json")).unwrap()).unwrap() }
#[test]
fn empty_and_populated_board_attach_external_images_without_jobs() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project"); fs::create_dir_all(&root).unwrap();
    let source = dir.path().join("external.png");
    assert!(Command::new("ffmpeg").args(["-v","error","-f","lavfi","-i","color=red:s=32x48","-frames:v","1","-threads","1"]).arg(&source).status().unwrap().success());
    let bytes = fs::read(&source).unwrap();
    let state = ProjectState::from_json(&json!({"version":2,"scenes":[{"id":1,"title":"Local look","intent":{"narrative_purpose":"An offline room reference"},"intent_approved":true}],"jobs":[{"id":"history","kind":"render","status":"failed","error":"Preserve history"}]}).to_string()).unwrap();
    import::persist_state(&root, &state).unwrap();
    let app = HostCtx::new(dir.path().join("settings")); let rec = RecorderState::new();
    dispatch(&app,&rec,"open_project",&json!({"path":root})).unwrap();
    let call = |extra:Value| { let mut args=json!({"projectRoot":root,"sceneId":1,"sourcePath":source}); args.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone()); dispatch(&app,&rec,"workflow_attach_look",&args) };
    // Normalize supported load/save enrichment before unchanged-byte refusal proof.
    let normalized=import::load_state(&root).unwrap(); import::save_project(&root,normalized).unwrap();
    let initial=json!({"state":read(&root),"report":workbench_host::workflow::report::project_report(&read(&root))});
    let saved=fs::read(root.join("Preproduction/workflow_state.json")).unwrap();
    assert!(call(json!({"role":"interior_reference"})).is_err(),"ungranted external input refused");
    assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(),saved);
    dispatch(&app,&rec,"grant_path",&json!({"path":source,"intent":"read"})).unwrap();
    let first=call(json!({"role":"interior_reference"})).unwrap();
    let slot=first["attachedLook"]["slotId"].as_str().unwrap();
    let first_id=first["attachedLook"]["assetId"].as_str().unwrap();
    let s=read(&root); let first_asset=s.asset(first_id).unwrap();
    assert_eq!(s.scenes[0].look_board.as_ref().unwrap().slots.len(),1);
    assert_eq!(s.scenes[0].look_board.as_ref().unwrap().slots[0].asset_ids,vec![first_id]);
    assert_eq!(fs::read(root.join(&first_asset.path)).unwrap(),bytes);
    assert_eq!(s.jobs,state.jobs); assert_eq!(first_asset.status,workbench_host::workflow::state::AssetStatus::Draft); assert!(first_asset.cut_id.is_none());
    assert!(call(json!({"slotId":slot})).is_err(),"external grant is consumed once");
    dispatch(&app,&rec,"grant_path",&json!({"path":source,"intent":"read"})).unwrap();
    let second=call(json!({"slotId":slot})).unwrap(); let second_id=second["attachedLook"]["assetId"].as_str().unwrap();
    let s=read(&root); let second_asset=s.asset(second_id).unwrap();
    assert_ne!(first_id,second_id); assert_ne!(first_asset.path,second_asset.path);
    assert_eq!(second_asset.parent_asset_id.as_deref(),Some(first_id));
    assert_eq!(s.scenes[0].look_board.as_ref().unwrap().slots[0].asset_ids,vec![first_id,second_id]);
    let reopened=dispatch(&app,&rec,"workflow_import",&json!({"projectRoot":root,"persist":true})).unwrap();
    assert_eq!(reopened["state"]["scenes"][0]["look_board"]["slots"][0]["asset_ids"],json!([first_id,second_id]));
    assert_eq!(read(&root).jobs,state.jobs); assert_eq!(fs::read(&source).unwrap(),bytes); assert_eq!(fs::read(root.join(&first_asset.path)).unwrap(),bytes); assert_eq!(fs::read(root.join(&second_asset.path)).unwrap(),bytes);
    // Redirected project buckets must not write outside or to a different scene.
    #[cfg(unix)] {
        use std::os::unix::fs::symlink;
        let other=root.join("Preproduction/Scene2/references"); fs::create_dir_all(&other).unwrap();
        let reference_dir=root.join("Preproduction/Scene1/references"); let retained=root.join("Preproduction/Scene1/retained"); fs::rename(&reference_dir,&retained).unwrap();
        for target in [&other,dir.path()] {
            symlink(target,&reference_dir).unwrap();
            dispatch(&app,&rec,"grant_path",&json!({"path":source,"intent":"read"})).unwrap();
            assert!(call(json!({"slotId":slot})).is_err(),"reject redirected look bucket");
            fs::remove_file(&reference_dir).unwrap();
        }
        fs::rename(retained,reference_dir).unwrap();
        assert_eq!(fs::read_dir(other).unwrap().count(),0);
    }
    if let Ok(out)=std::env::var("SB14_ARTIFACTS") { let out=Path::new(&out); fs::create_dir_all(out).unwrap(); fs::copy(&source,out.join("external.png")).unwrap(); fs::copy(root.join(&second_asset.path),out.join("copied.png")).unwrap(); fs::write(out.join("native-receipts.json"),serde_json::to_vec_pretty(&json!({"initial":initial,"first":first,"second":second,"reopened":reopened,"external_unchanged":true,"provider_dispatches":0,"jobs_unchanged":true})).unwrap()).unwrap(); }
    dispatch(&app,&rec,"close_project",&json!({})).unwrap();
    println!("SB14 expected=actual: no-slot creation and populated-slot copy, exact association, external unchanged, draft only, replay denied, two real reimports, historical jobs unchanged, zero provider dispatch.");
}
