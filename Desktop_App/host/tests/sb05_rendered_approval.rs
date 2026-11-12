//! Native persisted review on a disposable copy; no generation or operation approval.
use serde_json::{json, Value};
use std::{fs, path::Path};
use workbench_host::{ctx::HostCtx, voice::RecorderState, session, workflow::{import, state::ProjectState}};
fn read(root: &Path) -> ProjectState { ProjectState::from_json(&fs::read_to_string(root.join("Preproduction/workflow_state.json")).unwrap()).unwrap() }
fn raw(root: &Path) -> Value { serde_json::from_slice(&fs::read(root.join("Preproduction/workflow_state.json")).unwrap()).unwrap() }
#[test]
#[ignore = "requires SB05_PROJECT_COPY disposable project"]
fn rendered_review_persists_and_invalidates() {
    let root = std::path::PathBuf::from(std::env::var("SB05_PROJECT_COPY").expect("disposable fixture"));
    assert!(root.join(".sb05-disposable").is_file());
    session::open(root.to_str().unwrap()).unwrap();
    let app_dir = tempfile::tempdir().unwrap(); let app = HostCtx::new(app_dir.path().join("settings")); let rec = RecorderState::new();
    let call = |name: &str, args: Value| workbench_host::dispatch(&app, &rec, name, &args);
    let initial = import::load_state(&root).unwrap();
    let scene2 = initial.scene(2).unwrap().animatic.clone();
    let job = initial.jobs.last().unwrap().clone();
    let args = json!({"projectRoot":root,"jobId":job.id});
    assert!(serde_json::to_value(&initial).unwrap().get("rendered_review").is_none(), "legacy/sketch approved is not rendered approval");
    let approved = call("workflow_approve_rendered_sequence", args.clone()).expect("explicit rendered-sequence review must persist");
    assert_eq!(approved["state"]["rendered_review"]["job_id"], job.id);
    assert_eq!(raw(&root)["rendered_review"]["job_id"], job.id, "actual persisted disk receipt");
    session::close(); session::open(root.to_str().unwrap()).unwrap();
    let reopened = import::import_project(&root, import::ImportOptions{persist:true}).unwrap().state;
    assert_eq!(serde_json::to_value(&reopened).unwrap()["rendered_review"]["job_id"],job.id);
    assert_eq!(reopened.scene(2).unwrap().animatic,scene2,"manual timing untouched");
    assert_eq!(reopened.jobs,initial.jobs,"review does not alter execution history");
    let shot = reopened.scene(2).unwrap().animatic.as_ref().unwrap().shots[0].clone();
    let changed = call("workflow_set_animatic_duration",json!({"projectRoot":root,"sceneId":2,"shotId":shot.id,"durationSeconds":4.5})).unwrap();
    assert!(changed["state"].get("rendered_review").is_none());
    assert!(raw(&root).get("rendered_review").is_none());
    assert!(call("workflow_approve_rendered_sequence",args.clone()).unwrap_err().contains("stale"));
    call("workflow_set_animatic_duration",json!({"projectRoot":root,"sceneId":2,"shotId":shot.id,"durationSeconds":shot.duration_seconds})).unwrap();
    assert!(raw(&root).get("rendered_review").is_none(),"reverting timing cannot auto-approve");
    call("workflow_approve_rendered_sequence",args.clone()).unwrap();
    let before = read(&root); let cut = &before.scene(1).unwrap().cuts[0];
    let alternate = cut.takes.iter().find(|t|Some(&t.id)!=cut.selected_take.as_ref()).expect("real alternate");
    let changed = call("workflow_set_take_status",json!({"projectRoot":root,"sceneId":1,"cut":cut.id.cut,"takeId":alternate.id,"status":"selected"})).unwrap();
    assert!(changed["state"].get("rendered_review").is_none());
    assert!(raw(&root).get("rendered_review").is_none());
    assert!(call("workflow_approve_rendered_sequence",args).unwrap_err().contains("stale"));
    let final_timing = read(&root).scene(2).unwrap().animatic.clone().unwrap();
    assert_eq!(final_timing.shots,scene2.as_ref().unwrap().shots,"saved manual timing retained after explicit edit/revert");
    assert!(!final_timing.approved,"deliberate timing edit also clears the independent sketch approval; reverting cannot auto-approve it");
    println!("SB05 expected=actual: explicit review survives importing reopen; duration edit and alternate selection clear persisted/UI receipt; stale master refused; reverted timing not auto-approved; legacy/sketch approval never upgrades; Scene2 manual 5s/dissolve retained; jobs unchanged.");
    session::close();
}
