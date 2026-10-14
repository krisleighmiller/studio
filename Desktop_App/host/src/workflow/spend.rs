//! Shared fail-closed billable dispatch. A durable reservation precedes each request.
use crate::workflow::cost::{self, Price, SpendGate};
use crate::workflow::import;
use crate::workflow::state::{JobRecord, JobStatus, ProjectState, SpendConfirmation};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);
const OPERATIONS: &[(&str, &str)] = &[
    ("atlas_generate_image", "image.text_to_image"),
    ("atlas_generate_video", "video.text_only"),
    ("atlas_generate_audio", "audio.tts_profile"),
];

fn persisted(root: &Path) -> Result<ProjectState, String> {
    let path = root.join(crate::workflow::state::STATE_REL);
    let text = fs::read_to_string(path)
        .map_err(|_| "persisted project state is required before paid generation".to_string())?;
    ProjectState::from_json(&text)
}

fn revision(state: &ProjectState) -> Result<String, String> {
    let bytes = serde_json::to_vec(&(&state.budget, &state.jobs))
        .map_err(|e| format!("cannot fingerprint ledger: {e}"))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn price(kind: &str, capability: &str) -> Result<(String, i64), String> {
    match cost::resolve_price(kind, capability) {
        Price::Known { currency, minor } if minor > 0 => Ok((currency, minor)),
        _ => Err(format!(
            "unknown price for {kind}/{capability}; generation blocked"
        )),
    }
}

fn valid_scope(category: &str, operation: &str, model: &str) -> Result<(), String> {
    if category.trim().is_empty() || operation.trim().is_empty() || model.trim().is_empty() {
        Err("missing spend scope or model".into())
    } else {
        Ok(())
    }
}

/// Called only following an explicit user gesture acknowledging this exact quote.
/// A token is single-use and valid only until the ledger/budget changes.
pub fn confirm_scoped(
    root: &Path,
    provider: &str,
    category: &str,
    operation: &str,
    capability: &str,
    model: &str,
) -> Result<String, String> {
    valid_scope(category, operation, model)?;
    let (currency, amount_minor) = price(provider, capability)?;
    let _lock = import::lock_project(root)?;
    let mut state = persisted(root)?;
    let ledger = cost::compute_ledger(&state);
    if ledger.unknown_cost_jobs != 0 || !ledger.currency.eq_ignore_ascii_case(&currency) {
        return Err("unknown outstanding spend or currency mismatch".into());
    }
    if cost::gate_spend(&state, amount_minor) != SpendGate::Confirm {
        return Err("quote does not require confirmation or hard stop blocks it".into());
    }
    let mut random = [0u8; 32];
    fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut random))
        .map_err(|e| format!("cannot generate confirmation token: {e}"))?;
    let token: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    state.spend_confirmations.push(SpendConfirmation {
        token: token.clone(),
        provider: provider.into(),
        category: category.into(),
        operation: operation.into(),
        model: model.into(),
        currency,
        amount_minor,
        ledger_revision: revision(&state)?,
    });
    import::persist_state_locked(root, &state)?;
    Ok(token)
}

/// Category is a data string; every attempt has an independent job and reservation.
/// A retry requires a reconciled failed job of the same scope; the old cost remains in the ledger.
pub fn run_scoped<F>(
    root: &Path,
    provider: &str,
    category: &str,
    operation: &str,
    capability: &str,
    model: &str,
    confirmation: Option<&str>,
    retry_of: Option<&str>,
    dispatch: F,
) -> Result<String, String>
where
    F: FnOnce() -> Result<String, String>,
{
    valid_scope(category, operation, model)?;
    let (currency, minor) = price(provider, capability)?;
    let _lock = import::lock_project(root)?;
    let mut state = persisted(root)?;
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
        SpendGate::Confirm => {
            let current = revision(&state)?;
            let index = state
                .spend_confirmations
                .iter()
                .position(|item| {
                    Some(item.token.as_str()) == confirmation
                        && item.provider == provider
                        && item.category == category
                        && item.operation == operation
                        && item.model == model
                        && item.currency == currency
                        && item.amount_minor == minor
                        && item.ledger_revision == current
                })
                .ok_or("budget confirmation required; generation blocked")?;
            state.spend_confirmations.remove(index);
        }
        SpendGate::NoLimit | SpendGate::Allow | SpendGate::Warn => {}
    }
    if let Some(previous) = retry_of {
        let old = state
            .jobs
            .iter()
            .find(|job| job.id == previous)
            .ok_or("retry source job not found")?;
        if old.status != JobStatus::Failed
            || old.cost_minor.is_none()
            || old.provider.as_deref() != Some(provider)
            || old.spend_category.as_deref() != Some(category)
            || old.kind != operation
            || old.model.as_deref() != Some(model)
        {
            return Err("retry requires a reconciled failed attempt with matching scope".into());
        }
    }
    let id = format!(
        "spend-{}-{}-{}",
        std::process::id(),
        crate::workflow::state::utc_now(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    );
    let mut job = JobRecord::open(id.clone(), operation);
    job.provider = Some(provider.into());
    job.spend_category = Some(category.into());
    job.model = Some(model.into());
    job.currency = Some(currency);
    job.estimated_cost_minor = Some(minor);
    job.retry_of = retry_of.map(str::to_string);
    job.status = JobStatus::Running;
    state.jobs.push(job);
    import::persist_state_locked(root, &state)?;
    let result = dispatch();
    // The request may have reached the provider even if the connection failed.
    // Never infer zero charge on failure or from a successful HTTP response.
    let mut state = persisted(root).map_err(|e| {
        format!("request dispatched; reconciliation needs manual review for job {id}: {e}")
    })?;
    let job = state
        .jobs
        .iter_mut()
        .find(|job| job.id == id)
        .ok_or_else(|| {
            format!("reservation disappeared; outcome requires manual reconciliation for job {id}")
        })?;
    match &result {
        Ok(_) => job.finish(None, None),
        Err(error) => job.fail(None, error.clone()),
    }
    import::persist_state_locked(root, &state).map_err(|e| {
        format!("request dispatched but spend reconciliation failed: {e}; inspect job {id}")
    })?;
    result
}

pub fn run_generation<F>(
    root: &Path,
    provider: &str,
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
    run_scoped(
        root, provider, "media", operation, capability, model, None, None, dispatch,
    )
}

/// Apply a verified provider charge to a completed/failed attempt. This is a
/// manual accounting entry, not a prediction from the HTTP status.
pub fn reconcile_job(
    root: &Path,
    id: &str,
    actual_minor: i64,
    currency: &str,
) -> Result<(), String> {
    if actual_minor < 0 || currency.trim().is_empty() {
        return Err("reconciliation needs a nonnegative provider charge and currency".into());
    }
    let _lock = import::lock_project(root)?;
    let mut state = persisted(root)?;
    let budget_currency = state
        .budget
        .as_ref()
        .map_or("USD", |budget| budget.currency.as_str());
    if !budget_currency.eq_ignore_ascii_case(currency) {
        return Err("reconciliation currency differs from project budget".into());
    }
    let job = state
        .jobs
        .iter_mut()
        .find(|job| job.id == id)
        .ok_or("reconciliation job not found")?;
    if job.spend_category.is_none()
        || !matches!(job.status, JobStatus::Completed | JobStatus::Failed)
        || job.cost_minor.is_some()
        || job.currency.as_deref() != Some(currency)
    {
        return Err("job is not an unsettled billable attempt in this currency".into());
    }
    job.cost_minor = Some(actual_minor);
    import::persist_state_locked(root, &state).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::state::ProjectBudget;
    use std::sync::atomic::AtomicUsize;

    fn setup(limit: i64, hard_stop: bool) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let mut state = ProjectState::new();
        state.budget = Some(ProjectBudget {
            currency: "USD".into(),
            limit_minor: Some(limit),
            warning_at_minor: None,
            hard_stop,
        });
        import::persist_state(dir.path(), &state).unwrap();
        dir
    }
    fn image(
        root: &Path,
        token: Option<&str>,
        retry: Option<&str>,
        dispatch: impl FnOnce() -> Result<String, String>,
    ) -> Result<String, String> {
        run_scoped(
            root,
            "atlas-cloud",
            "media",
            "atlas_generate_image",
            "image.text_to_image",
            "model",
            token,
            retry,
            dispatch,
        )
    }
    #[test]
    fn rejects_before_dispatch_and_excludes_local_jobs() {
        let dir = setup(0, true);
        let mut state = persisted(dir.path()).unwrap();
        state.jobs.push(JobRecord::open("local", "assembly"));
        import::persist_state(dir.path(), &state).unwrap();
        let calls = AtomicUsize::new(0);
        let error = image(dir.path(), None, None, || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok("sent".into())
        })
        .unwrap_err();
        assert!(error.contains("hard stop"));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn confirmation_binds_scope_and_is_consumed_once() {
        let dir = setup(0, false);
        let token = confirm_scoped(
            dir.path(),
            "atlas-cloud",
            "media",
            "atlas_generate_image",
            "image.text_to_image",
            "model",
        )
        .unwrap();
        assert!(image(dir.path(), Some("wrong"), None, || panic!("sent"))
            .unwrap_err()
            .contains("confirmation"));
        assert!(run_scoped(
            dir.path(),
            "atlas-cloud",
            "voice",
            "atlas_generate_image",
            "image.text_to_image",
            "model",
            Some(&token),
            None,
            || panic!("sent")
        )
        .is_err());
        image(dir.path(), Some(&token), None, || Ok("sent".into())).unwrap();
        assert!(image(dir.path(), Some(&token), None, || panic!("sent")).is_err());
    }
    #[test]
    fn stale_confirmation_is_rejected_without_dispatch() {
        let dir = setup(0, false);
        let token = confirm_scoped(
            dir.path(),
            "atlas-cloud",
            "media",
            "atlas_generate_image",
            "image.text_to_image",
            "model",
        )
        .unwrap();
        let mut state = persisted(dir.path()).unwrap();
        state.budget.as_mut().unwrap().limit_minor = Some(1);
        import::persist_state(dir.path(), &state).unwrap();
        assert!(image(dir.path(), Some(&token), None, || panic!("sent")).is_err());
    }
    #[test]
    fn failure_is_unknown_and_retry_requires_reconciliation() {
        let dir = setup(100, true);
        assert_eq!(
            image(dir.path(), None, None, || Err("timeout".into())).unwrap_err(),
            "timeout"
        );
        let state = persisted(dir.path()).unwrap();
        let failed = &state.jobs[0];
        assert_eq!(failed.status, JobStatus::Failed);
        assert_eq!(failed.cost_minor, None);
        let id = failed.id.clone();
        assert!(image(dir.path(), None, Some(&id), || panic!("sent")).is_err());
        assert!(reconcile_job(dir.path(), &id, -1, "USD").is_err());
        reconcile_job(dir.path(), &id, 7, "USD").unwrap();
        assert!(reconcile_job(dir.path(), &id, 0, "USD").is_err());
        image(dir.path(), None, Some(&id), || Ok("ok".into())).unwrap();
        let state = persisted(dir.path()).unwrap();
        assert_eq!(state.jobs[1].retry_of.as_deref(), Some(id.as_str()));
        assert_eq!(cost::compute_ledger(&state).failed_minor, 7);
    }
    #[test]
    fn unknown_price_and_missing_project_never_dispatch() {
        let dir = tempfile::tempdir().unwrap();
        assert!(image(dir.path(), None, None, || panic!("sent")).is_err());
        let dir = setup(100, true);
        for (operation, capability) in [
            ("elevenlabs_design_voice", "voice_design"),
            ("elevenlabs_create_designed_voice", "voice_clone"),
            ("elevenlabs_tts_preview", "audio.tts_profile"),
            ("elevenlabs_clone_voice", "audio.tts_clone"),
        ] {
            assert!(run_scoped(
                dir.path(),
                "elevenlabs",
                "voice",
                operation,
                capability,
                "model",
                None,
                None,
                || panic!("sent")
            )
            .unwrap_err()
            .contains("unknown price"));
        }
    }
    #[test]
    fn stale_ui_save_cannot_erase_reservation_or_confirmation() {
        let dir = setup(100, true);
        let stale = import::load_state(dir.path()).unwrap();
        image(dir.path(), None, None, || Ok("sent".into())).unwrap();
        let error = import::save_project(dir.path(), stale).unwrap_err();
        assert!(error.contains("changed"), "{error}");
        assert_eq!(persisted(dir.path()).unwrap().jobs.len(), 1);

        let soft = setup(0, false);
        let stale = import::load_state(soft.path()).unwrap();
        confirm_scoped(
            soft.path(),
            "atlas-cloud",
            "media",
            "atlas_generate_image",
            "image.text_to_image",
            "model",
        )
        .unwrap();
        assert!(import::save_project(soft.path(), stale)
            .unwrap_err()
            .contains("changed"));
        assert_eq!(persisted(soft.path()).unwrap().spend_confirmations.len(), 1);
    }

    #[test]
    fn stale_budget_edit_cannot_erase_failure_or_reconciliation() {
        let dir = setup(100, true);
        let mut stale = import::load_state(dir.path()).unwrap();
        stale.budget.as_mut().unwrap().limit_minor = Some(25);
        image(dir.path(), None, None, || Err("timeout".into())).unwrap_err();
        assert!(import::save_project(dir.path(), stale.clone())
            .unwrap_err()
            .contains("changed"));
        let failed = persisted(dir.path()).unwrap();
        let id = failed.jobs[0].id.clone();
        assert_eq!(failed.jobs[0].status, JobStatus::Failed);
        assert_eq!(failed.jobs[0].cost_minor, None);
        reconcile_job(dir.path(), &id, 7, "USD").unwrap();
        assert!(import::save_project(dir.path(), stale)
            .unwrap_err()
            .contains("changed"));
        let reconciled = persisted(dir.path()).unwrap();
        assert_eq!(reconciled.jobs[0].cost_minor, Some(7));
        assert_eq!(reconciled.budget.unwrap().limit_minor, Some(100));

        // A fresh snapshot is a legitimate budget edit and must remain writable.
        let mut fresh = import::load_state(dir.path()).unwrap();
        fresh.budget.as_mut().unwrap().limit_minor = Some(25);
        import::save_project(dir.path(), fresh).unwrap();
        let saved = persisted(dir.path()).unwrap();
        assert_eq!(saved.jobs[0].cost_minor, Some(7));
        assert_eq!(saved.budget.unwrap().limit_minor, Some(25));
    }

    #[test]
    fn save_waits_for_reserved_dispatch_and_cannot_erase_it() {
        use std::sync::mpsc;
        let dir = setup(7, true);
        let stale = import::load_state(dir.path()).unwrap();
        let (reserved_tx, reserved_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (save_tx, save_rx) = mpsc::channel();
        let root = dir.path();
        std::thread::scope(|scope| {
            let dispatch = scope.spawn(move || {
                image(root, None, None, || {
                    reserved_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok("sent".into())
                })
            });
            reserved_rx.recv().unwrap();
            // The reservation is on disk while the provider is in flight.
            assert_eq!(
                persisted(dir.path()).unwrap().jobs[0].status,
                JobStatus::Running
            );
            let save = scope.spawn(|| {
                save_tx
                    .send(import::save_project(dir.path(), stale))
                    .unwrap();
            });
            // The save can only win after the dispatch releases the project lock.
            release_tx.send(()).unwrap();
            dispatch.join().unwrap().unwrap();
            save.join().unwrap();
            assert!(save_rx.recv().unwrap().unwrap_err().contains("changed"));
        });
        assert_eq!(
            persisted(dir.path()).unwrap().jobs[0].status,
            JobStatus::Completed
        );
    }

    #[test]
    fn stale_budget_edit_and_failure_reconciliation_cannot_clobber_ledger() {
        let dir = setup(100, true);
        let mut budget_edit = import::load_state(dir.path()).unwrap();
        budget_edit.budget.as_mut().unwrap().limit_minor = Some(1);
        image(dir.path(), None, None, || Err("timeout".into())).unwrap_err();
        let failure = persisted(dir.path()).unwrap();
        let id = failure.jobs[0].id.clone();
        assert!(import::save_project(dir.path(), budget_edit)
            .unwrap_err()
            .contains("changed"));
        reconcile_job(dir.path(), &id, 7, "USD").unwrap();
        assert!(import::save_project(dir.path(), failure)
            .unwrap_err()
            .contains("changed"));
        let fresh = import::load_state(dir.path()).unwrap();
        let mut fresh_budget_edit = fresh.clone();
        fresh_budget_edit.budget.as_mut().unwrap().limit_minor = Some(7);
        import::save_project(dir.path(), fresh_budget_edit).unwrap();
        assert_eq!(persisted(dir.path()).unwrap().jobs[0].cost_minor, Some(7));
        assert!(image(dir.path(), None, None, || panic!("sent"))
            .unwrap_err()
            .contains("hard stop"));
    }

    #[test]
    fn concurrent_requests_compete_for_one_slot() {
        use std::sync::{Arc, Barrier};
        let dir = setup(7, true);
        let start = Arc::new(Barrier::new(3));
        let calls = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..2 {
                let start = start.clone();
                let calls = &calls;
                let root = dir.path();
                handles.push(scope.spawn(move || {
                    start.wait();
                    image(root, None, None, || {
                        calls.fetch_add(1, Ordering::SeqCst);
                        Ok("sent".into())
                    })
                }));
            }
            start.wait();
            assert_eq!(
                handles
                    .into_iter()
                    .map(|handle| handle.join().unwrap())
                    .filter(Result::is_ok)
                    .count(),
                1
            );
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
