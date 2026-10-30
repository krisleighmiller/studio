//! Exercise the REAL host response with an enabled provider configured.
//! Only a staged local text overwrite runs; provider access must not be needed.
//! This does not prove automatic planner handoff or an Electron end-to-end flow.

use std::fs;
use workbench_host::workflow::{import::persist_state, state::ProjectState};
use workbench_host::{ctx::HostCtx, director, pending, session, text_writes};

struct SessionGuard;
impl Drop for SessionGuard {
    fn drop(&mut self) {
        pending::cancel_all();
        session::close();
    }
}

#[test]
fn completed_approved_local_write_has_no_pending_token_and_false_continuation_flag() {
    // This integration-test binary has a single test, hence no shared-session
    // competitor. Other integration binaries have separate process globals.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir_all(root.join("Research")).unwrap();
    let root = root.canonicalize().unwrap();
    let root_str = root.to_str().unwrap();
    let path = root.join("Research/production-note.md");
    let previous = "Old production note.\n";
    let body = "Mom/Sam car exchange: preserve the approved dialogue.\n";
    fs::write(&path, previous).unwrap();
    persist_state(&root, &ProjectState::default()).unwrap();
    session::open(root_str).unwrap();
    let _guard = SessionGuard;
    let app = HostCtx::new(dir.path().join("isolated-settings"));
    // A real-shaped enabled route reproduces the live user's settings without
    // credentials. This is a local save, so approval must never contact it.
    fs::write(app.settings_path(), r#"{"apis":[{"id":"configured-test","name":"Configured test provider","kind":"test-only","baseUrl":"http://127.0.0.1:1","apiKey":"","secretKey":"","enabled":true}],"capabilityDefaults":{"text":{"apiId":"configured-test","modelId":"test-model"}}}"#).unwrap();

    let write = text_writes::authorize_text_write(
        "production_helper",
        "Research/production-note.md",
        Some(root_str),
        body.len(),
        0,
        0,
    )
    .unwrap();
    assert!(text_writes::approval_required(&write));
    let summary = text_writes::approval_summary(&write, Some(previous), body);
    let staged_id =
        text_writes::stage_authorized_text_writes("production_helper", vec![(write, body.into())]);
    let card =
        text_writes::text_write_approval_card(&staged_id, &[summary], Some(root_str)).unwrap();
    let id = card
        .split(pending::APPROVE_TOKEN_PREFIX)
        .nth(1)
        .and_then(|rest| rest.strip_suffix("]]"))
        .expect("one-time approval ID")
        .to_string();
    assert_eq!(fs::read_to_string(&path).unwrap(), previous);

    let response = director::director_approve(app.clone(), id.clone()).unwrap();
    // Assert actual execution before interpreting the response contract.
    assert_eq!(fs::read_to_string(&path).unwrap(), body);
    assert_eq!(response.tool_events, vec!["approve:text_write"]);
    assert!(
        response.reply.contains("Research/production-note.md"),
        "{}",
        response.reply
    );
    assert!(!response.reply.contains(pending::APPROVE_TOKEN_PREFIX));
    assert!(!response.reply.contains("[[workbench_pending:"));
    // Current specialist completion convention. False does NOT establish that
    // a user's original multi-step request is finished; no planner ran here.
    assert!(!response.continue_suggested);
    let wire = serde_json::to_value(&response).unwrap();
    assert_eq!(wire["continueSuggested"], false);
    assert_eq!(
        wire["toolEvents"],
        serde_json::json!(["approve:text_write"])
    );

    assert!(
        director::director_approve(app, id).is_err(),
        "approval cannot be replayed"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), body);
}
