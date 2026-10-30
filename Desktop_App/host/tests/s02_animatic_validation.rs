//! Connected timing/trim invariant at the production command boundary.
use serde_json::json;
use std::fs;
use workbench_host::{scene_plan::handoff::CutId, session};
use workbench_host::workflow::{self, import, state::{CutRecord, ProjectState}};

#[test]
fn duration_edits_and_approval_preserve_positive_trimmed_runtime() {
    let tmp=tempfile::tempdir().unwrap();let root=tmp.path();
    let mut state=ProjectState::new();
    state.scene_mut(1,"S02").cuts.push(CutRecord::new(CutId::new(1,1)));
    workflow::animatic::sync(&mut state);
    let shot=state.scenes[0].animatic.as_ref().unwrap().shots[0].id.clone();
    import::save_project(root,state).unwrap();session::open(root.to_str().unwrap()).unwrap();
    workflow::cmd_set_animatic_duration(&json!({"projectRoot":root,"sceneId":1,"shotId":shot,"durationSeconds":3})).unwrap();
    workflow::cmd_set_shot_trim(&json!({"projectRoot":root,"sceneId":1,"shotId":shot,"trimInSeconds":0.3,"trimOutSeconds":0.4})).unwrap();
    let p=root.join("Preproduction/workflow_state.json");let before=fs::read(&p).unwrap();
    let invalid=json!({"projectRoot":root,"sceneId":1,"shotId":shot,"durationSeconds":0.5});
    assert!(workflow::cmd_set_animatic_duration(&invalid).is_err(),"duration shorter than retained trims must be rejected");
    assert_eq!(fs::read(&p).unwrap(),before);
    // Equality after host rounding also has zero runtime and must fail.
    let mut rounded=invalid.clone();rounded["durationSeconds"]=json!(0.704);
    assert!(workflow::cmd_set_animatic_duration(&rounded).is_err());
    assert_eq!(fs::read(&p).unwrap(),before);
    let mut valid=invalid.clone();valid["durationSeconds"]=json!(0.8);
    workflow::cmd_set_animatic_duration(&valid).unwrap();
    workflow::cmd_approve_animatic(&json!({"projectRoot":root,"sceneId":1})).unwrap();
    // Legacy/externally edited invalid saved state must not gain approval.
    let mut legacy:serde_json::Value=serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    legacy["scenes"][0]["animatic"]["approved"]=json!(false);
    legacy["scenes"][0]["animatic"]["shots"][0]["duration_seconds"]=json!(0.5);
    fs::write(&p,serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();
    let bad=fs::read(&p).unwrap();
    assert!(workflow::cmd_approve_animatic(&json!({"projectRoot":root,"sceneId":1})).is_err(),"invalid legacy trim cannot be approved");
    assert_eq!(fs::read(&p).unwrap(),bad);
    session::close();
}
