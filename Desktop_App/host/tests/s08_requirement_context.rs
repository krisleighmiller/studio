//! Production-library validation: exact scene context or explicit project scope.
use serde_json::{json, Value};
use std::fs;
use workbench_host::{session, workflow};
use workflow::{import, state::ProjectState};

#[test]
fn requirement_context_is_checked_without_mutating_refused_requests() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let mut initial = ProjectState::new();
    initial.profile = Some(serde_json::from_value(json!({"recipe":"narrative_dialogue","deliverables":[{"id":"review","name":"Review"}]})).unwrap());
    initial.scene_mut(1, "Blue room");
    initial.scene_mut(2, "Gold room");
    import::save_project(root, initial).unwrap();
    session::open(root.to_str().unwrap()).unwrap();
    let state_path = root.join("Preproduction/workflow_state.json");
    for scene in [json!(999), json!(0), json!(-1), json!(1.5), json!(true),
        json!([1]), json!({"id":1}), json!(""), json!("abc"),
        json!(4294967297_u64), json!("4294967297"), json!("999"),
        json!("0"), json!("-1"), json!("1.5"), json!(" 1 ")]
    {
        let before = fs::read(&state_path).unwrap();
        let result = workflow::cmd_add_requirement(&json!({
            "projectRoot":root,"name":"Refused","kind":"prop","sceneId":scene
        }));
        assert!(result.is_err(), "invalid context accepted: {scene}: {result:?}");
        assert_eq!(fs::read(&state_path).unwrap(), before, "refused {scene} changed bytes");
    }
    for (index, scene, expected) in [
        (0, Some(json!(1)), vec![1]),
        (1, Some(json!("2")), vec![2]),
        (2, Some(Value::Null), vec![]),
        (3, None, vec![]),
    ] {
        let name = format!("Requirement {index}");
        let mut args = json!({"projectRoot":root,"name":name,"kind":"prop"});
        if let Some(value) = scene { args["sceneId"] = value; }
        workflow::cmd_add_requirement(&args).unwrap();
        let saved = import::load_state(root).unwrap();
        let req = saved.requirements.iter().find(|r| r.name == name).unwrap();
        assert_eq!(req.scene_ids, expected);
        let id = req.id.clone();
        let before = fs::read(&state_path).unwrap();
        assert!(workflow::cmd_add_requirement(&args).unwrap_err().contains("already exists"));
        assert_eq!(fs::read(&state_path).unwrap(), before);
        for status in ["draft", "present", "approved", "deferred", "missing"] {
            workflow::cmd_set_requirement_status(&json!({
                "projectRoot":root,"requirementId":id,"status":status
            })).unwrap();
            let saved = import::load_state(root).unwrap();
            let req = saved.requirements.iter().find(|r| r.id == id).unwrap();
            assert_eq!(serde_json::to_value(req.status).unwrap(), json!(status));
            assert_eq!(req.scene_ids, expected);
            assert!(req.status_override);
        }
    }
    session::close();
}
