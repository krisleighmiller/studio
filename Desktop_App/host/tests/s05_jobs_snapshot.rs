//! Manager-authored production boundary regression: polling is observation, not import/recovery.
use serde_json::json;
use std::fs;
use workbench_host::{session, workflow::{self, import, state::{JobRecord, ProjectState}}};

#[test]
fn jobs_only_snapshot_is_pure_scoped_and_truthful() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("project");
    let outside = t.path().join("outside");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&outside).unwrap();
    session::open(root.to_str().unwrap()).unwrap();
    let mut state = ProjectState::new(); // No profile: ordinary load_or_import would repair it.
    let mut running = JobRecord::open("running-job", "render");
    running.mark_running("actual local dispatch in flight");
    state.jobs.push(running);
    let mut failed = JobRecord::open("failed-job", "render");
    failed.fail(None, "provider-failed: offline fixture");
    state.jobs.push(failed);
    import::persist_state(&root, &state).unwrap();
    let state_path = root.join("Preproduction/workflow_state.json");
    let before = fs::read(&state_path).unwrap();
    fs::create_dir(outside.join("Preproduction")).unwrap();
    let outside_state = outside.join("Preproduction/workflow_state.json");
    fs::write(&outside_state, &before).unwrap();
    for _ in 0..3 {
        let result = workflow::cmd_status(&json!({"projectRoot":root,"refresh":false,"jobsOnly":true})).unwrap();
        let jobs = result["state"]["jobs"].as_array().unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0]["id"], "running-job");
        assert_eq!(jobs[0]["status"], "running");
        assert_eq!(jobs[0]["progress"], "actual local dispatch in flight");
        assert_eq!(jobs[1]["id"], "failed-job");
        assert_eq!(jobs[1]["status"], "failed");
        assert_eq!(jobs[1]["error"], "provider-failed: offline fixture");
        assert_eq!(fs::read(&state_path).unwrap(), before, "polling must not repair profile/state or advance revision");
    }
    assert!(workflow::cmd_status(&json!({"projectRoot":outside,"jobsOnly":true})).unwrap_err().contains("not the open project"));
    assert_eq!(fs::read(&outside_state).unwrap(), before);
    assert_eq!(fs::read(&state_path).unwrap(), before);
    session::close();
}
