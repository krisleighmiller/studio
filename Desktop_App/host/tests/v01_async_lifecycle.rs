//! V01 bounded truthful asynchronous job repair — test-first.
//! Exercise dispatch seam: awaiting-provider result must not complete job/attempt.
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use workbench_host::workflow::{import, spend, state::{ProjectState, JobStatus, SpendRequestMetadata}};

fn req_char(ch: char) -> SpendRequestMetadata {
    let base = format!("{:02x}", ch as u8);
    let hash = base.repeat(32); // 2 * 32 = 64 hex chars
    SpendRequestMetadata {
        request_hash: hash,
        disclosure: None,
        duration_seconds: None,
        width: None,
        height: None,
        prediction_id: None,
    }
}

fn setup() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    import::persist_state(d.path(), &ProjectState::new()).unwrap();
    d
}

fn token(root: &Path, hash: &str) -> String {
    let q = spend::quote_unbounded_scoped(root, hash).unwrap();
    spend::confirm_unbounded_scoped(root, "fixture", "media", "generate", "model", hash, &q.ledger_revision).unwrap()
}

#[test]
fn v01_awaiting_result_does_not_complete_job_or_attempt() {
    let d = setup();
    let req = req_char('v');
    let t = token(d.path(), &req.request_hash);
    let calls = AtomicUsize::new(0);
    // Dispatch submits a prediction, then returns acknowledged envelope (no download).
    let result = spend::run_scoped_with_submission_outcome(
        d.path(),
        "fixture",
        "media",
        "generate",
        "unknown.capability",
        "model",
        Some(&t),
        None,
        Some(req.clone()),
        None,
        |id, submitted| {
            calls.fetch_add(1, Ordering::SeqCst);
            submitted("v01-original-http-id")?;
            assert_eq!(spend::recovery_job(d.path(), id).unwrap().1, "v01-original-http-id");
            Ok(spend::DispatchOutcome::AwaitingResult(r#"{"submitted":{"prediction_id":"v01-original-http-id","status":"starting"},"saved":null,"error":"polling incomplete","note":"awaiting provider result"}"#.into()))
        },
    );
    assert!(result.is_ok(), "dispatch returned ok acknowledgement: {}", result.unwrap_err());
    let s = import::load_state(d.path()).unwrap();
    let j = s.jobs.last().unwrap();
    // AwaitingResult must stay non-terminal — exactly Running.
    assert_eq!(j.status, JobStatus::Running, "awaiting result must stay running not merely !=Completed");
    assert!(j.started_at.is_some(), "job must have started_at");
    assert!(j.attempts.last().unwrap().started_at.is_some(), "attempt must have started_at");
    assert_eq!(j.attempts[0].started_at, j.started_at, "first attempt started_at must match job");
    assert!(j.completed_at.is_none(), "awaiting result incorrectly stamped completed_at on job");
    assert_eq!(j.attempts.last().unwrap().status, JobStatus::Running, "attempt incorrectly completed");
    assert!(j.attempts.last().unwrap().completed_at.is_none(), "attempt incorrectly stamped completed_at");
    assert!(j.progress.as_deref().unwrap_or("").contains("awaiting"), "progress must reflect awaiting");
    // Durable recovery ID preserved before any polling (observed before return is already true).
    assert_eq!(spend::recovery_job(d.path(), &j.id).unwrap().1, "v01-original-http-id");
    let before_jobs = serde_json::to_value(&s.jobs).unwrap();
    workbench_host::session::open(d.path().to_str().unwrap()).unwrap();
    workbench_host::session::close();
    workbench_host::session::open(d.path().to_str().unwrap()).unwrap();
    for _ in 0..2 {
        import::import_project(d.path(), import::ImportOptions { persist: true }).unwrap();
    }
    assert_eq!(serde_json::to_value(import::load_state(d.path()).unwrap().jobs).unwrap(), before_jobs);
    let fresh = token(d.path(), &req.request_hash);
    for permission in [&t, &fresh] {
        assert!(spend::run_scoped_with_submission_outcome(
            d.path(), "fixture", "media", "generate", "unknown.capability", "model",
            Some(permission), None, Some(req.clone()), None,
            |_, _| { calls.fetch_add(1, Ordering::SeqCst); Ok(spend::DispatchOutcome::Completed("must not run".into())) },
        ).unwrap_err().contains("ambiguous"));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(serde_json::to_value(import::load_state(d.path()).unwrap().jobs).unwrap(), before_jobs);
    workbench_host::session::close();
}

#[test]
fn v01_completed_synchronous_delivery_still_finishes() {
    let d = setup();
    let req = req_char('w');
    let t = token(d.path(), &req.request_hash);
    let result = spend::run_scoped_with_submission_outcome(
        d.path(),
        "fixture",
        "media",
        "generate",
        "unknown.capability",
        "model",
        Some(&t),
        None,
        Some(req.clone()),
        None,
        |_id, submitted| {
            submitted("w01-done")?;
            Ok(spend::DispatchOutcome::Completed("{\"saved\":\"output.mp4\"}".into()))
        },
    );
    assert!(result.is_ok());
    let s = import::load_state(d.path()).unwrap();
    let j = s.jobs.last().unwrap();
    assert_eq!(j.status, JobStatus::Completed, "synchronous delivery must finish");
    assert!(j.completed_at.is_some());
}

#[test]
fn v01_terminal_error_stays_failed_not_awaiting() {
    let d = setup();
    let req = req_char('x');
    let t = token(d.path(), &req.request_hash);
    let result = spend::run_scoped_with_submission_outcome(
        d.path(),
        "fixture",
        "media",
        "generate",
        "unknown.capability",
        "model",
        Some(&t),
        None,
        Some(req.clone()),
        None,
        |_, _| Err("provider-failed: terminal error".into()),
    );
    assert!(result.is_err());
    let s = import::load_state(d.path()).unwrap();
    let j = s.jobs.last().unwrap();
    assert_eq!(j.status, JobStatus::Failed, "terminal error must stay failed");
    // Loading the terminal job retains its identity and failure.
    let reopened = import::load_state(d.path()).unwrap();
    assert_eq!(reopened.jobs.len(), 1);
    assert!(reopened.jobs[0].status == JobStatus::Failed);
    assert!(reopened.jobs[0].reconciliation.is_none());
}
