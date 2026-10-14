//! Fail-closed billable media dispatch. Reservations are serialized across host processes.
use crate::workflow::cost::{self, Price, SpendGate};
use crate::workflow::import;
use crate::workflow::state::{JobRecord, JobStatus};
use std::fs::{self, OpenOptions};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

// Tool names are operation identifiers, not an enum of providers/capabilities.
// Unlisted operations cannot be dispatched. Only fixed-price capabilities have a rate.
const OPERATIONS: &[(&str, &str)] = &[
    ("atlas_generate_image", "image.text_to_image"),
    ("atlas_generate_video", "video.text_only"),
    ("atlas_generate_audio", "audio.tts_profile"),
];

pub fn run_generation<F>(
    root: &Path,
    provider_kind: &str,
    operation: &str,
    model: &str,
    dispatch: F,
) -> Result<String, String>
where
    F: FnOnce() -> Result<String, String>,
{
    let capability = OPERATIONS
        .iter()
        .find(|(name, _)| *name == operation)
        .map(|(_, capability)| *capability)
        .ok_or_else(|| format!("unknown billable operation: {operation}"))?;
    let (currency, minor) = match cost::resolve_price(provider_kind, capability) {
        Price::Known { currency, minor } if minor > 0 => (currency, minor),
        _ => {
            return Err(format!(
                "unknown price for {provider_kind}/{capability}; generation blocked"
            ))
        }
    };
    if model.trim().is_empty() {
        return Err("missing model".into());
    }
    let lock_path = root.join("Preproduction/.media-spend.lock");
    fs::create_dir_all(lock_path.parent().expect("lock has parent"))
        .map_err(|e| format!("cannot prepare spend lock: {e}"))?;
    let lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|e| format!("cannot open spend lock: {e}"))?;
    lock.lock()
        .map_err(|e| format!("cannot acquire spend lock: {e}"))?;
    // Hold the file lock across reservation, provider dispatch and reconciliation.
    // Another media invocation cannot read stale spend or reserve the same funds.
    // load_state imports a fresh project when the state file is absent. That is
    // useful for browsing, but cannot authorize a paid request with an empty
    // ledger and an absent budget.
    let state_path = root.join(crate::workflow::state::STATE_REL);
    if !state_path.is_file() {
        return Err("persisted project state is required before paid generation".into());
    }
    let mut state = import::load_state(root)?;
    let ledger = cost::compute_ledger(&state);
    if ledger.unknown_cost_jobs != 0 {
        return Err("unknown outstanding spend; generation blocked".into());
    }
    if !ledger.currency.eq_ignore_ascii_case(&currency) {
        return Err(format!(
            "budget currency {} differs from price currency {currency}",
            ledger.currency
        ));
    }
    match cost::gate_spend(&state, minor) {
        SpendGate::Blocked => return Err("budget hard stop; generation blocked".into()),
        SpendGate::Confirm => return Err("budget confirmation required; generation blocked".into()),
        SpendGate::NoLimit | SpendGate::Allow | SpendGate::Warn => {}
    }
    let id = format!(
        "media-{}-{}-{}",
        std::process::id(),
        crate::workflow::state::utc_now(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    );
    let mut job = JobRecord::open(id.clone(), operation);
    job.provider = Some(provider_kind.into());
    job.model = Some(model.into());
    job.currency = Some(currency);
    job.estimated_cost_minor = Some(minor);
    job.status = JobStatus::Running;
    state.jobs.push(job);
    import::persist_state(root, &state)?;

    let result = dispatch();
    // Reload under the lock to preserve any independent edits that occurred during
    // a long-running provider request; never silently lose the reservation.
    let mut state = import::load_state(root)?;
    let job = state
        .jobs
        .iter_mut()
        .find(|job| job.id == id)
        .ok_or_else(|| {
            "media spend reservation disappeared; outcome requires manual reconciliation"
                .to_string()
        })?;
    match &result {
        Ok(_) => {
            job.finish(None, None);
            // No provider invoice exists here: retain the estimate as committed
            // unsettled spend, not a fabricated actual charge.
        }
        Err(error) => job.fail(None, error.clone()),
    }
    import::persist_state(root, &state).map_err(|error| {
        format!("media was dispatched but spend reconciliation failed: {error}; inspect job {id}")
    })?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::state::{ProjectBudget, ProjectState};
    use std::cell::Cell;

    fn state_with_budget(root: &Path, limit: i64, hard_stop: bool) {
        let mut state = ProjectState::new();
        state.budget = Some(ProjectBudget {
            currency: "USD".into(),
            limit_minor: Some(limit),
            warning_at_minor: None,
            hard_stop,
        });
        import::persist_state(root, &state).unwrap();
    }

    fn rejected_without_dispatch(root: &Path, kind: &str, operation: &str, expected: &str) {
        let calls = Cell::new(0);
        let result = run_generation(root, kind, operation, "model", || {
            calls.set(calls.get() + 1);
            Ok("sent".into())
        });
        assert!(result.unwrap_err().contains(expected));
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn hard_stop_rejection_does_not_dispatch_provider() {
        let dir = tempfile::tempdir().unwrap();
        state_with_budget(dir.path(), 0, true);
        rejected_without_dispatch(
            dir.path(),
            "atlas-cloud",
            "atlas_generate_image",
            "hard stop",
        );
    }

    #[test]
    fn soft_limit_confirmation_does_not_dispatch_provider() {
        let dir = tempfile::tempdir().unwrap();
        state_with_budget(dir.path(), 0, false);
        rejected_without_dispatch(
            dir.path(),
            "atlas-cloud",
            "atlas_generate_image",
            "confirmation required",
        );
    }

    #[test]
    fn unknown_operation_or_price_does_not_dispatch_provider() {
        let dir = tempfile::tempdir().unwrap();
        rejected_without_dispatch(
            dir.path(),
            "atlas-cloud",
            "unregistered",
            "unknown billable operation",
        );
        rejected_without_dispatch(dir.path(), "kling", "atlas_generate_image", "unknown price");
        rejected_without_dispatch(
            dir.path(),
            "atlas-cloud",
            "atlas_generate_video",
            "unknown price",
        );
    }

    #[test]
    fn reservation_precedes_dispatch_and_result_remains_unsettled() {
        let dir = tempfile::tempdir().unwrap();
        state_with_budget(dir.path(), 7, true);
        let output = run_generation(
            dir.path(),
            "atlas-cloud",
            "atlas_generate_image",
            "model",
            || {
                let state = import::load_state(dir.path()).unwrap();
                assert_eq!(cost::compute_ledger(&state).committed_minor, 7);
                assert_eq!(state.jobs[0].status, JobStatus::Running);
                Ok("submitted".into())
            },
        )
        .unwrap();
        assert_eq!(output, "submitted");
        let state = import::load_state(dir.path()).unwrap();
        assert_eq!(state.jobs[0].status, JobStatus::Completed);
        assert_eq!(state.jobs[0].cost_minor, None);
        assert_eq!(cost::compute_ledger(&state).unsettled_estimated_minor, 7);
    }

    #[test]
    fn rejected_second_operation_never_dispatches() {
        let dir = tempfile::tempdir().unwrap();
        state_with_budget(dir.path(), 7, true);
        run_generation(
            dir.path(),
            "atlas-cloud",
            "atlas_generate_image",
            "model",
            || Ok("submitted".into()),
        )
        .unwrap();
        rejected_without_dispatch(
            dir.path(),
            "atlas-cloud",
            "atlas_generate_image",
            "unknown outstanding spend",
        );
    }

    #[test]
    fn unreadable_state_does_not_dispatch_provider() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(crate::workflow::state::STATE_REL);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "{not valid json").unwrap();
        let calls = Cell::new(0);
        let result = run_generation(
            dir.path(),
            "atlas-cloud",
            "atlas_generate_image",
            "model",
            || {
                calls.set(calls.get() + 1);
                Ok("submitted".into())
            },
        );
        assert!(result.is_err());
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn concurrent_requests_cannot_both_dispatch_against_one_remaining_slot() {
        use std::sync::{Arc, Barrier};
        let dir = tempfile::tempdir().unwrap();
        state_with_budget(dir.path(), 7, true);
        let start = Arc::new(Barrier::new(3));
        let dispatches = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..2 {
                let start = start.clone();
                let dispatches = dispatches.clone();
                let root = dir.path();
                handles.push(scope.spawn(move || {
                    start.wait();
                    run_generation(root, "atlas-cloud", "atlas_generate_image", "model", || {
                        dispatches.fetch_add(1, Ordering::SeqCst);
                        Ok("submitted".into())
                    })
                }));
            }
            start.wait();
            let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
            assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        });
        assert_eq!(dispatches.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn missing_persisted_project_state_never_authorizes_paid_dispatch() {
        let dir = tempfile::tempdir().unwrap();
        rejected_without_dispatch(
            dir.path(),
            "atlas-cloud",
            "atlas_generate_image",
            "persisted project state",
        );
    }

    #[test]
    fn provider_failure_stays_unknown_and_prevents_retry() {
        let dir = tempfile::tempdir().unwrap();
        state_with_budget(dir.path(), 100, true);
        let error = run_generation(
            dir.path(),
            "atlas-cloud",
            "atlas_generate_image",
            "model",
            || Err("provider failed".into()),
        )
        .unwrap_err();
        assert_eq!(error, "provider failed");
        let state = import::load_state(dir.path()).unwrap();
        assert_eq!(state.jobs[0].status, JobStatus::Failed);
        assert_eq!(state.jobs[0].cost_minor, None);
        rejected_without_dispatch(
            dir.path(),
            "atlas-cloud",
            "atlas_generate_image",
            "unknown outstanding spend",
        );
    }
}
