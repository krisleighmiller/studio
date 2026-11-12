//! Exact-operation cancellation through production dispatch; no provider or media calls.
use serde_json::json;
use workbench_host::{ctx::HostCtx, dispatch, pending, session, voice::RecorderState};

#[test]
fn denied_direct_id_cannot_replay_and_stale_cancel_preserves_newer_request() {
    let dir = tempfile::tempdir().unwrap();
    session::open(dir.path().to_str().unwrap()).unwrap();
    let app = HostCtx::new(dir.path().join("settings"));
    let recorder = RecorderState::new();
    let park = || pending::park(serde_json::from_value(json!({"action":"done","message":"direct"})).unwrap(), Some(dir.path().to_path_buf()), None, None);
    let first = park();
    let newer_admission = app.for_request();
    let cancelled = dispatch(&app, &recorder, "director_cancel", &json!({"id":first})).unwrap();
    assert!(!newer_admission.director_cancelled(), "denying an unconsumed ID must not interrupt a newer in-flight request");
    assert_eq!(cancelled["cancelled"], true);
    let replay = dispatch(&app, &recorder, "director_approve", &json!({"id":first}));
    assert!(replay.as_ref().is_err_and(|e| e.contains("no longer active")), "denied ID replay: {replay:?}");
    // Completed/consumed A must not retain process-wide Stop authority over B,
    // including the interval after B admission but before it parks an approval.
    let completed = park();
    let approved = dispatch(&app, &recorder, "director_approve", &json!({"id":completed})).unwrap();
    assert_eq!(approved["stopped"], false);
    let unparked_b = app.for_request();
    let delayed = dispatch(&app, &recorder, "director_cancel", &json!({"id":completed})).unwrap();
    assert!(!unparked_b.director_cancelled(), "completed A's delayed Stop interrupted admitted unparked B");
    assert_eq!(delayed["cancelled"], false, "completed execution no longer owns Stop");
    let newer = park();
    let admitted_newer = app.for_request();
    let stale = dispatch(&app, &recorder, "director_cancel", &json!({"id":first})).unwrap();
    assert_eq!(stale["cancelled"], false);
    assert!(!admitted_newer.director_cancelled(), "old cancellation interrupted a newer request");
    assert!(pending::take(&newer).is_ok());
    // Taking a token without an execution cannot grant Stop over another admission.
    let stop = dispatch(&app, &recorder, "director_cancel", &json!({"id":newer})).unwrap();
    assert_eq!(stop["cancelled"], false);
    assert!(!admitted_newer.director_cancelled());
    let freshest = park();
    let malformed = dispatch(&app, &recorder, "director_cancel", &json!({"id":null}));
    assert!(malformed.is_err(), "malformed scoped cancellation must not become global Stop");
    assert!(pending::take(&freshest).is_ok());
    pending::cancel_all();
    session::close();
}
