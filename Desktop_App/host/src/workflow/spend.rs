//! Shared fail-closed billable dispatch. A durable reservation precedes each request.
use crate::workflow::cost::{self, Price, SpendGate};
use crate::workflow::import;
use crate::workflow::state::{
    JobRecord, JobStatus, ProjectState, SpendConfirmation, SpendEvidenceProvenance,
    SpendReconciliation,
};
use serde::Serialize;
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

/// Read-only host quote; neither this nor renderer input authorizes dispatch.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendQuote {
    pub provider: String,
    pub category: String,
    pub operation: String,
    pub capability: String,
    pub model: String,
    pub currency: String,
    pub amount_minor: i64,
    pub ledger_revision: String,
}

pub fn quote_scoped(
    root: &Path,
    provider: &str,
    category: &str,
    operation: &str,
    capability: &str,
    model: &str,
) -> Result<SpendQuote, String> {
    valid_scope(category, operation, model)?;
    let (currency, amount_minor) = price(provider, capability)?;
    let _lock = import::lock_project(root)?;
    let state = persisted(root)?;
    let ledger = cost::compute_ledger(&state);
    if ledger.unknown_cost_jobs != 0 || !ledger.currency.eq_ignore_ascii_case(&currency) {
        return Err("unknown outstanding spend or currency mismatch".into());
    }
    if cost::gate_spend(&state, amount_minor) != SpendGate::Confirm {
        return Err("quote does not require confirmation or hard stop blocks it".into());
    }
    Ok(SpendQuote {
        provider: provider.into(),
        category: category.into(),
        operation: operation.into(),
        capability: capability.into(),
        model: model.into(),
        currency,
        amount_minor,
        ledger_revision: revision(&state)?,
    })
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
    confirm_with_quote(root, provider, category, operation, capability, model, None)
}

/// A UI gesture must bind to the host quote the user actually saw.
pub fn confirm_quoted(
    root: &Path,
    provider: &str,
    category: &str,
    operation: &str,
    capability: &str,
    model: &str,
    revision: &str,
    amount_minor: i64,
) -> Result<String, String> {
    confirm_with_quote(
        root,
        provider,
        category,
        operation,
        capability,
        model,
        Some((revision, amount_minor)),
    )
}

fn confirm_with_quote(
    root: &Path,
    provider: &str,
    category: &str,
    operation: &str,
    capability: &str,
    model: &str,
    expected: Option<(&str, i64)>,
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
    if let Some((shown_revision, shown_amount)) = expected {
        if shown_amount != amount_minor || shown_revision != revision(&state)? {
            return Err("spend quote is stale; request a new quote".into());
        }
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
/// Retry is unavailable until a provider-backed verifier and reconstructible request exist.
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
    if retry_of.is_some() {
        return Err("retry unsupported: provider outcome is not independently verified and original request is not stored".into());
    }
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
    // Never infer a verified outcome from a job's recorded cost or user text.
    // A verified retry requires a provider adapter and persisted nonsecret input;
    // neither exists here, so no retry lineage can be authorized.
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

/// Reviewable list of unsettled attempts. The revision binds a manual decision to
/// the exact ledger shown to the reviewer; it is not provider verification.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsettledJobs {
    pub ledger_revision: String,
    pub jobs: Vec<JobRecord>,
}

pub fn unsettled_jobs(root: &Path) -> Result<UnsettledJobs, String> {
    let _lock = import::lock_project(root)?;
    let state = persisted(root)?;
    Ok(UnsettledJobs {
        ledger_revision: revision(&state)?,
        jobs: state
            .jobs
            .iter()
            .filter(|job| {
                job.spend_category.is_some()
                    && (job.cost_minor.is_none() || job.reconciliation.is_some())
            })
            .cloned()
            .collect(),
    })
}

/// Manual evidence review; the provider is NOT queried or verified by this path.
/// No cost is authoritative without an attributable reference and explicit review.
pub fn reconcile_job(
    root: &Path,
    id: &str,
    actual_minor: i64,
    currency: &str,
    expected_revision: &str,
    outcome: &str,
    evidence_reference: &str,
    review_note: &str,
    acknowledgment: &str,
) -> Result<(), String> {
    if actual_minor < 0 || currency.trim().is_empty() {
        return Err("reconciliation needs a nonnegative provider charge and currency".into());
    }
    if acknowledgment != "I reviewed the provider evidence and accept this manual accounting entry"
        || evidence_reference.trim().len() < 8
        || review_note.trim().len() < 8
        || !matches!(
            (outcome, actual_minor),
            ("charged", 1..) | ("not_charged", 0)
        )
    {
        return Err("manual evidence review requires an attributable reference, note, explicit acknowledgment, and a matching provider outcome (zero requires not_charged)".into());
    }
    let _lock = import::lock_project(root)?;
    let mut state = persisted(root)?;
    if revision(&state)? != expected_revision {
        return Err("reconciliation view is stale; refresh unsettled jobs".into());
    }
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
    job.reconciliation = Some(SpendReconciliation {
        source: SpendEvidenceProvenance::ManualEvidenceReview,
        evidence_reference: evidence_reference.trim().into(),
        review_note: review_note.trim().into(),
        reported_outcome: outcome.into(),
        reviewed_at: crate::workflow::state::utc_now(),
    });
    import::persist_state_locked(root, &state).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::state::{JobStatus, ProjectBudget};
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
    fn reviewed(root: &Path, id: &str, amount: i64) -> Result<(), String> {
        let revision = unsettled_jobs(root)?.ledger_revision;
        reconcile_job(
            root,
            id,
            amount,
            "USD",
            &revision,
            if amount == 0 {
                "not_charged"
            } else {
                "charged"
            },
            "provider-history-request-123",
            "Reviewed billing history for this attempt",
            "I reviewed the provider evidence and accept this manual accounting entry",
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
    fn quoted_confirmation_requires_exact_amount_and_live_ledger_revision() {
        let dir = setup(0, false);
        let quote = quote_scoped(
            dir.path(),
            "atlas-cloud",
            "media",
            "atlas_generate_image",
            "image.text_to_image",
            "model",
        )
        .unwrap();
        let confirm = |revision: &str, amount| {
            confirm_quoted(
                dir.path(),
                &quote.provider,
                &quote.category,
                &quote.operation,
                &quote.capability,
                &quote.model,
                revision,
                amount,
            )
        };
        assert!(confirm(&quote.ledger_revision, quote.amount_minor + 1).is_err());
        assert!(confirm("stale", quote.amount_minor).is_err());
        let mut state = persisted(dir.path()).unwrap();
        state.budget.as_mut().unwrap().limit_minor = Some(1);
        import::persist_state(dir.path(), &state).unwrap();
        assert!(confirm(&quote.ledger_revision, quote.amount_minor).is_err());
        assert!(persisted(dir.path())
            .unwrap()
            .spend_confirmations
            .is_empty());
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
        assert!(reviewed(dir.path(), &id, -1).is_err());
        reviewed(dir.path(), &id, 7).unwrap();
        assert!(reviewed(dir.path(), &id, 0).is_err());
        let error = image(dir.path(), None, Some(&id), || panic!("retry dispatched")).unwrap_err();
        assert!(error.contains("retry unsupported"), "{error}");
        let state = persisted(dir.path()).unwrap();
        assert_eq!(state.jobs.len(), 1);
        assert_eq!(cost::compute_ledger(&state).failed_minor, 7);
        assert_eq!(cost::compute_ledger(&state).unknown_cost_jobs, 1);
        assert!(image(dir.path(), None, None, || panic!(
            "new dispatch after manual review"
        ))
        .unwrap_err()
        .contains("unknown outstanding spend"));
    }
    #[test]
    fn stale_revision_and_missing_evidence_cannot_reconcile_or_retry() {
        let dir = setup(100, true);
        image(dir.path(), None, None, || Err("timeout".into())).unwrap_err();
        let snapshot = unsettled_jobs(dir.path()).unwrap();
        let id = snapshot.jobs[0].id.clone();
        assert!(reconcile_job(
            dir.path(),
            &id,
            0,
            "USD",
            &snapshot.ledger_revision,
            "not_charged",
            "",
            "",
            "I reviewed the provider evidence and accept this manual accounting entry"
        )
        .is_err());
        assert!(image(dir.path(), None, Some(&id), || panic!("sent")).is_err());
        let mut state = persisted(dir.path()).unwrap();
        state.budget.as_mut().unwrap().limit_minor = Some(101);
        import::persist_state(dir.path(), &state).unwrap();
        assert!(reconcile_job(
            dir.path(),
            &id,
            0,
            "USD",
            &snapshot.ledger_revision,
            "not_charged",
            "provider-record-001",
            "Checked provider history",
            "I reviewed the provider evidence and accept this manual accounting entry"
        )
        .unwrap_err()
        .contains("stale"));
        assert_eq!(persisted(dir.path()).unwrap().jobs[0].cost_minor, None);
        reviewed(dir.path(), &id, 0).unwrap();
        let job = &persisted(dir.path()).unwrap().jobs[0];
        assert_eq!(job.cost_minor, Some(0));
        assert_eq!(
            job.reconciliation.as_ref().unwrap().source,
            SpendEvidenceProvenance::ManualEvidenceReview
        );
        assert_eq!(
            job.reconciliation.as_ref().unwrap().reported_outcome,
            "not_charged"
        );
        assert_eq!(
            cost::compute_ledger(&persisted(dir.path()).unwrap()).unknown_cost_jobs,
            1
        );
        assert!(
            image(dir.path(), None, Some(&id), || panic!("retry dispatched"))
                .unwrap_err()
                .contains("retry unsupported")
        );
        assert!(
            image(dir.path(), None, None, || panic!("new request dispatched"))
                .unwrap_err()
                .contains("unknown outstanding spend")
        );
        assert_eq!(persisted(dir.path()).unwrap().jobs.len(), 1);
    }
    #[test]
    fn fabricated_reference_and_zero_never_verify_outcome_or_authorize_spend() {
        let dir = setup(100, true);
        image(dir.path(), None, None, || Err("connection lost".into())).unwrap_err();
        let id = persisted(dir.path()).unwrap().jobs[0].id.clone();
        let rev = unsettled_jobs(dir.path()).unwrap().ledger_revision;
        reconcile_job(
            dir.path(),
            &id,
            0,
            "USD",
            &rev,
            "not_charged",
            "fake-provider-reference-000",
            "I cannot verify this reference",
            "I reviewed the provider evidence and accept this manual accounting entry",
        )
        .unwrap();
        let state = persisted(dir.path()).unwrap();
        let entry = state.jobs[0].reconciliation.as_ref().unwrap();
        assert_eq!(entry.source, SpendEvidenceProvenance::ManualEvidenceReview);
        assert_eq!(entry.reported_outcome, "not_charged");
        assert_eq!(state.jobs[0].cost_minor, Some(0));
        assert_eq!(cost::compute_ledger(&state).unknown_cost_jobs, 1);
        assert!(image(dir.path(), None, Some(&id), || panic!("retry sent"))
            .unwrap_err()
            .contains("retry unsupported"));
        assert!(
            image(dir.path(), None, None, || panic!("paid request sent"))
                .unwrap_err()
                .contains("unknown outstanding spend")
        );
        assert!(quote_scoped(
            dir.path(),
            "atlas-cloud",
            "media",
            "atlas_generate_image",
            "image.text_to_image",
            "model"
        )
        .is_err());
        assert_eq!(persisted(dir.path()).unwrap().jobs.len(), 1);
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
        reviewed(dir.path(), &id, 7).unwrap();
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
        reviewed(dir.path(), &id, 7).unwrap();
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
            .contains("unknown outstanding spend"));
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
