//! Shared fail-closed billable dispatch. A durable reservation precedes each request.
use crate::workflow::cost::{self, Price, SpendGate};
use crate::workflow::import;
use crate::workflow::state::{
    JobRecord, JobStatus, ProjectState, SpendConfirmation, SpendEvidenceProvenance,
    SpendReconciliation, SpendRequestMetadata,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;
#[cfg(test)]
use std::sync::atomic::Ordering;

/// Transient adapter result, not persisted schema or permission. Acknowledged
/// asynchronous work stays nonterminal until its result is actually available.
#[derive(Debug, Clone, PartialEq)]
pub enum DispatchOutcome {
    Completed(String),
    AwaitingResult(String),
}

/// An adapter-proven ceiling for an exact request. Deliberately no production
/// constructor: a registry estimate or renderer number cannot mint one.
/// Numeric fields are inert unit markers retained only to keep the test
/// constructor ABI stable; they carry no authorization authority.
#[derive(Debug, Clone)]
pub struct VerifiedMaximum {
    _currency: String,
    _minor: i64,
}

#[cfg(test)]
impl VerifiedMaximum {
    pub(crate) fn synthetic_for_test(currency: &str, minor: i64) -> Self {
        Self {
            _currency: currency.into(),
            _minor: minor,
        }
    }
}

/// Constant disclosure for the single-provider operation; never stamps
/// numeric budget, cost, or ceiling text.
fn unbounded_disclosure(_state: &ProjectState) -> String {
    "Authorize this single provider operation.".into()
}

/// Stable SHA-256 fingerprint of the exact request bytes; never persists secrets.
pub fn fingerprint(bytes: &[u8]) -> Result<String, String> {
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}
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
    let bytes = serde_json::to_vec(&(&state.jobs))
        .map_err(|e| format!("cannot fingerprint request-state: {e}"))?;
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

/// Display this disclosure before approving one request with no enforced maximum.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnboundedSpendQuote {
    pub currency: String,
    pub request_hash: String,
    pub ledger_revision: String,
    pub disclosure: String,
}

pub fn quote_unbounded_scoped(
    root: &Path,
    request_hash: &str,
) -> Result<UnboundedSpendQuote, String> {
    if !valid_hash(request_hash) {
        return Err("invalid request fingerprint".into());
    }
    let _lock = import::lock_project(root)?;
    let state = persisted(root)?;
    Ok(UnboundedSpendQuote {
        currency: String::new(),
        request_hash: request_hash.into(),
        ledger_revision: revision(&state)?,
        disclosure: unbounded_disclosure(&state),
    })
}

/// Call only after the user affirmatively approves the displayed quote.
pub fn confirm_unbounded_scoped(
    root: &Path,
    provider: &str,
    category: &str,
    operation: &str,
    model: &str,
    request_hash: &str,
    shown_revision: &str,
) -> Result<String, String> {
    valid_scope(category, operation, model)?;
    if !valid_hash(request_hash) {
        return Err("invalid request fingerprint".into());
    }
    let _lock = import::lock_project(root)?;
    let mut state = persisted(root)?;
    if revision(&state)? != shown_revision {
        return Err("unbounded quote stale or policy changed".into());
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
        currency: String::new(),
        amount_minor: 0,
        ledger_revision: shown_revision.into(),
        request_hash: request_hash.into(),
    });
    import::persist_state_locked(root, &state)?;
    Ok(token)
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
        request_hash: String::new(),
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
    run_scoped_with_request(
        root,
        provider,
        category,
        operation,
        capability,
        model,
        confirmation,
        retry_of,
        None,
        None,
        dispatch,
    )
}

/// Persists only nonsecret numeric parameters and a request fingerprint. The
/// maximum is opaque and cannot be supplied by the UI or registry pricing.
pub fn run_scoped_with_request<F>(
    root: &Path,
    provider: &str,
    category: &str,
    operation: &str,
    capability: &str,
    model: &str,
    confirmation: Option<&str>,
    retry_of: Option<&str>,
    request: Option<SpendRequestMetadata>,
    maximum: Option<VerifiedMaximum>,
    dispatch: F,
) -> Result<String, String>
where
    F: FnOnce() -> Result<String, String>,
{
    run_scoped_with_submission(
        root,
        provider,
        category,
        operation,
        capability,
        model,
        confirmation,
        retry_of,
        request,
        maximum,
        |_, _| dispatch(),
    )
}

/// Reserve, submit, then durably record the provider id before any polling/download.
/// The callback targets the immutable job id, never a potentially shared request hash.
pub fn run_scoped_with_submission<F>(
    root: &Path,
    provider: &str,
    category: &str,
    operation: &str,
    _capability: &str,
    model: &str,
    confirmation: Option<&str>,
    retry_of: Option<&str>,
    request: Option<SpendRequestMetadata>,
    _maximum: Option<VerifiedMaximum>,
    dispatch: F,
) -> Result<String, String>
where
    F: FnOnce(&str, &dyn Fn(&str) -> Result<(), String>) -> Result<String, String>,
{
    let outcome = run_scoped_with_submission_outcome(
        root, provider, category, operation, _capability, model, confirmation,
        retry_of, request, _maximum,
        |id, submitted| dispatch(id, submitted).map(DispatchOutcome::Completed),
    )?;
    match outcome {
        DispatchOutcome::Completed(text) | DispatchOutcome::AwaitingResult(text) => Ok(text),
    }
}

/// Typed media outcome; authorization and reservation remain identical to the
/// compatibility wrapper, while acknowledgement alone does not finish a job.
pub fn run_scoped_with_submission_outcome<F>(
    root: &Path,
    provider: &str,
    category: &str,
    operation: &str,
    _capability: &str,
    model: &str,
    confirmation: Option<&str>,
    retry_of: Option<&str>,
    request: Option<SpendRequestMetadata>,
    _maximum: Option<VerifiedMaximum>,
    dispatch: F,
) -> Result<DispatchOutcome, String>
where
    F: FnOnce(&str, &dyn Fn(&str) -> Result<(), String>) -> Result<DispatchOutcome, String>,
{
    valid_scope(category, operation, model)?;
    if retry_of.is_some() {
        return Err("retry unsupported: provider outcome is not independently verified and original request is not stored".into());
    }
    let _lock = import::lock_project(root)?;
    let mut state = persisted(root)?;
    // Permission is independent of budget/ledger/mode/currency/minor.
    // Never infer a verified outcome from a job's recorded cost or user text.
    // A verified retry requires a provider adapter and persisted nonsecret input;
    // neither exists here, so no retry lineage can be authorized.
    if let Some(request_ref) = &request {
        if !valid_hash(&request_ref.request_hash) || request_ref.prediction_id.is_some() {
            return Err("invalid request fingerprint or pre-dispatch prediction id".into());
        }
        // Identical payloads can be intentional, but an unresolved earlier attempt
        // could already have been accepted. Refuse any ambiguous replay by default;
        // manual historical accounting must NEVER authorize resubmission.
        if state.jobs.iter().any(|job| {
            job.request_metadata
                .as_ref()
                .is_some_and(|old| old.request_hash == request_ref.request_hash)
        }) {
            return Err("ambiguous previous provider outcome for identical request; recover by job id, do not resubmit".into());
        }
        let hash = request_ref.request_hash.as_str();
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
                    && item.request_hash == hash
                    && item.ledger_revision == current
            })
            .ok_or("unbounded request approval required")?;
        state.spend_confirmations.remove(index);
    } else {
        return Err("unbounded approval needs an exact request fingerprint".into());
    }
    // Never infer a verified outcome from a job's recorded cost or user text.
    // A verified retry requires a provider adapter and persisted nonsecret input;
    // neither exists here, so no retry lineage can be authorized.
    let mut nonce = [0u8; 16];
    fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut nonce))
        .map_err(|_| "cannot allocate unique spend job id".to_string())?;
    let id = format!(
        "spend-{}",
        nonce.iter().map(|b| format!("{b:02x}")).collect::<String>()
    );
    if state.jobs.iter().any(|job| job.id == id) {
        return Err("spend job id collision; request not dispatched".into());
    }
    let mut job = JobRecord::open(id.clone(), operation);
    job.provider = Some(provider.into());
    job.spend_category = Some(category.into());
    job.model = Some(model.into());
    job.request_metadata = request.map(|mut metadata| {
        metadata.disclosure = None; // NEW metadata only; legacy persisted jobs untouched
        metadata
    });
    job.retry_of = retry_of.map(str::to_string);
    job.mark_running("dispatching provider request");
    state.jobs.push(job);
    import::persist_state_locked(root, &state)?;
    drop(_lock);
    let record_submission = |prediction_id: &str| -> Result<(), String> {
        // Never persist response bodies, URLs, or other provider-controlled strings.
        if prediction_id.is_empty()
            || prediction_id.len() > 256
            || !prediction_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-./:|".contains(&b))
        {
            return Err(
                "provider returned an unsafe prediction id; manual reconciliation required".into(),
            );
        }
        let _lock = import::lock_project(root)?;
        let mut state = persisted(root)?;
        let job = state
            .jobs
            .iter_mut()
            .find(|job| job.id == id)
            .ok_or("reserved spend job disappeared")?;
        let metadata = job
            .request_metadata
            .as_mut()
            .ok_or("missing request metadata")?;
        if metadata
            .prediction_id
            .as_deref()
            .is_some_and(|old| old != prediction_id)
        {
            return Err("provider prediction id changed for job".into());
        }
        metadata.prediction_id = Some(prediction_id.to_owned());
        job.note_progress("provider accepted request; awaiting result");
        import::persist_state_locked(root, &state).map(|_| ())
    };
    let result = dispatch(&id, &record_submission);
    let _lock = import::lock_project(root)
        .map_err(|e| format!("request dispatched; job {id} needs reconciliation: {e}"))?;
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
        Ok(DispatchOutcome::Completed(_)) => job.finish(None, None),
        Ok(DispatchOutcome::AwaitingResult(_)) => {
            job.note_progress("awaiting provider result/recovery");
        }
        Err(error) => job.fail(None, error.clone()),
    }
    import::persist_state_locked(root, &state).map_err(|e| {
        format!("request dispatched but spend reconciliation failed: {e}; inspect job {id}")
    })?;
    result
}

pub fn recovery_job(root: &Path, job_id: &str) -> Result<(String, String), String> {
    if job_id.len() > 100
        || !job_id.starts_with("spend-")
        || !job_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("invalid spend job id".into());
    }
    let state = persisted(root)?;
    let job = state
        .jobs
        .iter()
        .find(|j| j.id == job_id)
        .ok_or("spend job not found")?;
    let provider = job.provider.as_deref().ok_or("job provider missing")?;
    let id = job
        .request_metadata
        .as_ref()
        .and_then(|r| r.prediction_id.as_deref())
        .ok_or("no provider prediction id persisted; outcome unknown; do not resubmit")?;
    Ok((provider.to_owned(), id.to_owned()))
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
    use crate::workflow::state::{BudgetPolicy, JobStatus, ProjectBudget};
    use std::sync::atomic::AtomicUsize;

    fn fingerprint(ch: char) -> String {
        ch.to_string().repeat(64)
    }
    fn request(hash: &str) -> SpendRequestMetadata {
        SpendRequestMetadata {
            request_hash: hash.into(),
            disclosure: None,
            duration_seconds: None,
            width: None,
            height: None,
            prediction_id: None,
        }
    }

    #[test]
    fn no_budget_generation_accepts_unknown_cost_after_approval() {
        for budget in [
            None,
            Some(ProjectBudget {
                limit_minor: None,
                hard_stop: true,
                policy: Some(BudgetPolicy::StrictHardStop),
                ..Default::default()
            }),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let mut state = ProjectState::new();
            state.budget = budget;
            import::persist_state(dir.path(), &state).unwrap();
            let attempted = AtomicUsize::new(0);
            for ch in ['a', 'b'] {
                let hash = fingerprint(ch);
                let quote = quote_unbounded_scoped(dir.path(), &hash)
                    .expect("no numeric project limit must not require a verified price");
                let invoke = |token: Option<&str>| {
                    run_scoped_with_request(
                        dir.path(),
                        "unpriced-provider",
                        "media",
                        "generate",
                        "video.text_only",
                        "test-model",
                        token,
                        None,
                        Some(request(&hash)),
                        None,
                        || {
                            attempted.fetch_add(1, Ordering::Relaxed);
                            Ok("done".into())
                        },
                    )
                };
                assert!(invoke(None).unwrap_err().contains("approval required"));
                let token = confirm_unbounded_scoped(
                    dir.path(),
                    "unpriced-provider",
                    "media",
                    "generate",
                    "test-model",
                    &hash,
                    &quote.ledger_revision,
                )
                .unwrap();
                assert_eq!(invoke(Some(&token)).unwrap(), "done");
                assert!(invoke(Some(&token)).is_err(), "no duplicate submissions");
            }
            assert_eq!(attempted.load(Ordering::Relaxed), 2);
            let state = persisted(dir.path()).unwrap();
            assert!(state.jobs.iter().all(|job| job.cost_minor.is_none()
                && job.estimated_cost_minor.is_none()
                && job.reserved_max_minor.is_none()));
        }
    }

    #[test]
    fn submission_id_is_durable_before_status_and_duplicate_unknown_outcome_is_blocked() {
        let dir = setup(100, false);
        let mut state = persisted(dir.path()).unwrap();
        state.budget.as_mut().unwrap().policy = Some(BudgetPolicy::ApprovalRequired);
        import::persist_state(dir.path(), &state).unwrap();
        let req = request(&fingerprint('d'));
        let quote = quote_unbounded_scoped(dir.path(), &req.request_hash).unwrap();
        let token = confirm_unbounded_scoped(
            dir.path(),
            "atlas-cloud",
            "media",
            "atlas_generate_image",
            "model",
            &req.request_hash,
            &quote.ledger_revision,
        )
        .unwrap();
        let invoke = |dispatch: &dyn Fn(
            &str,
            &dyn Fn(&str) -> Result<(), String>,
        ) -> Result<String, String>| {
            run_scoped_with_submission(
                dir.path(),
                "atlas-cloud",
                "media",
                "atlas_generate_image",
                "image.text_to_image",
                "model",
                Some(&token),
                None,
                Some(req.clone()),
                None,
                |id, submitted| dispatch(id, submitted),
            )
        };
        let result = invoke(&|id, submitted| {
            submitted("prediction_123")?;
            let state = persisted(dir.path()).unwrap();
            let job = state.jobs.iter().find(|j| j.id == id).unwrap();
            assert_eq!(
                job.request_metadata
                    .as_ref()
                    .unwrap()
                    .prediction_id
                    .as_deref(),
                Some("prediction_123")
            );
            Err("poll failed".into())
        });
        assert!(result.is_err());
        assert!(invoke(&|_, _| panic!("duplicate submitted"))
            .unwrap_err()
            .contains("ambiguous"));
        let id = persisted(dir.path()).unwrap().jobs[0].id.clone();
        assert_eq!(recovery_job(dir.path(), &id).unwrap().1, "prediction_123");
        let status =
            crate::media::recovery_status_with(dir.path(), &id, |provider, prediction_id| {
                assert_eq!(provider, "atlas-cloud");
                assert_eq!(prediction_id, "prediction_123");
                Ok(r#"{"status":"processing","input":{"api_key":"secret"}}"#.into())
            })
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&status).unwrap()["status"],
            "processing"
        );
        assert!(!status.contains("secret"));
        assert_eq!(persisted(dir.path()).unwrap().jobs.len(), 1);
    }

    #[test]
    fn unbounded_approval_is_one_use_and_bound_to_payload_and_revision() {
        let dir = setup(100, false);
        let mut state = persisted(dir.path()).unwrap();
        state.budget.as_mut().unwrap().policy = Some(BudgetPolicy::ApprovalRequired);
        import::persist_state(dir.path(), &state).unwrap();
        let hash = fingerprint('a');
        let quote = quote_unbounded_scoped(dir.path(), &hash).unwrap();
        let token = confirm_unbounded_scoped(
            dir.path(),
            "atlas-cloud",
            "media",
            "atlas_generate_image",
            "model",
            &hash,
            &quote.ledger_revision,
        )
        .unwrap();
        let attempted = AtomicUsize::new(0);
        let invoke = |hash: &str, token: &str| {
            run_scoped_with_request(
                dir.path(),
                "atlas-cloud",
                "media",
                "atlas_generate_image",
                "image.text_to_image",
                "model",
                Some(token),
                None,
                Some(request(hash)),
                None,
                || {
                    attempted.fetch_add(1, Ordering::Relaxed);
                    Ok("done".into())
                },
            )
        };
        assert!(invoke(&fingerprint('b'), &token).is_err());
        assert_eq!(attempted.load(Ordering::Relaxed), 0);
        invoke(&hash, &token).unwrap();
        assert_eq!(attempted.load(Ordering::Relaxed), 1);
        assert!(invoke(&hash, &token).is_err());
        assert_eq!(attempted.load(Ordering::Relaxed), 1);
        let job = &persisted(dir.path()).unwrap().jobs[0];
        assert!(!job.unbounded_exposure);
        assert!(job.request_metadata.as_ref().unwrap().disclosure.is_none());
    }

    #[test]
    fn two_consecutive_unbounded_operations_each_require_fresh_authorization() {
        let dir = setup(100, false);
        let mut state = persisted(dir.path()).unwrap();
        state.budget.as_mut().unwrap().policy = Some(BudgetPolicy::ApprovalRequired);
        import::persist_state(dir.path(), &state).unwrap();
        let attempted = AtomicUsize::new(0);

        for ch in ['a', 'b'] {
            let hash = fingerprint(ch);
            let quote = quote_unbounded_scoped(dir.path(), &hash).unwrap();
            let token = confirm_unbounded_scoped(
                dir.path(),
                "atlas-cloud",
                "media",
                "atlas_generate_image",
                "model",
                &hash,
                &quote.ledger_revision,
            )
            .unwrap();
            run_scoped_with_request(
                dir.path(),
                "atlas-cloud",
                "media",
                "atlas_generate_image",
                "image.text_to_image",
                "model",
                Some(&token),
                None,
                Some(request(&hash)),
                None,
                || {
                    attempted.fetch_add(1, Ordering::Relaxed);
                    Ok("done".into())
                },
            )
            .unwrap();
        }

        assert_eq!(attempted.load(Ordering::Relaxed), 2);
        let state = persisted(dir.path()).unwrap();
        assert_eq!(state.jobs.len(), 2);
        assert!(state.jobs.iter().all(|job| job.cost_minor.is_none()));
    }

    #[test]
    fn warn_only_unbounded_requires_explicit_one_use_authorization() {
        let dir = setup(100, false);
        let mut state = persisted(dir.path()).unwrap();
        state.budget.as_mut().unwrap().policy = Some(BudgetPolicy::WarnOnly);
        import::persist_state(dir.path(), &state).unwrap();
        assert!(run_scoped(
            dir.path(),
            "atlas-cloud",
            "media",
            "atlas_generate_image",
            "image.text_to_image",
            "model",
            None,
            None,
            || panic!("sent")
        )
        .unwrap_err()
        .contains("fingerprint"));
        let hash = fingerprint('c');
        let attempted = AtomicUsize::new(0);
        assert!(run_scoped_with_request(
            dir.path(),
            "atlas-cloud",
            "media",
            "atlas_generate_image",
            "image.text_to_image",
            "model",
            None,
            None,
            Some(request(&hash)),
            None,
            || {
                attempted.fetch_add(1, Ordering::Relaxed);
                Ok("done".into())
            },
        )
        .unwrap_err()
        .contains("approval required"));
        assert_eq!(attempted.load(Ordering::Relaxed), 0);
        let quote = quote_unbounded_scoped(dir.path(), &hash).unwrap();
        let token = confirm_unbounded_scoped(
            dir.path(),
            "atlas-cloud",
            "media",
            "atlas_generate_image",
            "model",
            &hash,
            &quote.ledger_revision,
        )
        .unwrap();
        run_scoped_with_request(
            dir.path(),
            "atlas-cloud",
            "media",
            "atlas_generate_image",
            "image.text_to_image",
            "model",
            Some(&token),
            None,
            Some(request(&hash)),
            None,
            || {
                attempted.fetch_add(1, Ordering::Relaxed);
                Ok("done".into())
            },
        )
        .unwrap();
        assert_eq!(attempted.load(Ordering::Relaxed), 1);
        let state = persisted(dir.path()).unwrap();
        assert!(state.jobs[0].request_metadata.as_ref().unwrap().disclosure.is_none());
    }





    #[test]
    fn dispatched_job_records_created_started_and_progress_lifecycle() {
        let dir = setup(100, false);
        let mut state = persisted(dir.path()).unwrap();
        state.budget.as_mut().unwrap().policy = Some(BudgetPolicy::ApprovalRequired);
        import::persist_state(dir.path(), &state).unwrap();
        let req = request(&fingerprint('e'));
        let quote = quote_unbounded_scoped(dir.path(), &req.request_hash).unwrap();
        let token = confirm_unbounded_scoped(
            dir.path(),
            "atlas-cloud",
            "media",
            "atlas_generate_image",
            "model",
            &req.request_hash,
            &quote.ledger_revision,
        )
        .unwrap();
        run_scoped_with_submission(
            dir.path(),
            "atlas-cloud",
            "media",
            "atlas_generate_image",
            "image.text_to_image",
            "model",
            Some(&token),
            None,
            Some(req.clone()),
            None,
            |id, submitted| {
                submitted("prediction_life")?;
                let mid = persisted(dir.path()).unwrap();
                let job = mid.jobs.iter().find(|j| j.id == id).unwrap();
                assert!(job.created_at.is_some(), "reservation records creation");
                assert!(job.started_at.is_some(), "dispatch records a real start");
                assert_eq!(job.status, JobStatus::Running);
                assert_eq!(job.attempts[0].started_at, job.started_at);
                assert_eq!(
                    job.progress.as_deref(),
                    Some("provider accepted request; awaiting result")
                );
                assert_eq!(job.currency.as_deref(), None);
                Ok("ok".into())
            },
        )
        .unwrap();
        let done = persisted(dir.path()).unwrap();
        let job = done
            .jobs
            .iter()
            .find(|job| job.status == JobStatus::Completed)
            .expect("completed dispatch job");
        assert!(job.completed_at.is_some());
        assert!(job.started_at.is_some());
        assert_eq!(job.progress.as_deref(), Some("completed"));
    }

    fn setup(limit: i64, hard_stop: bool) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let mut state = ProjectState::new();
        state.budget = Some(ProjectBudget {
            currency: "USD".into(),
            limit_minor: Some(limit),
            warning_at_minor: None,
            hard_stop,
            policy: None,
        });
        import::persist_state(dir.path(), &state).unwrap();
        dir
    }
    // Fixture authorization is explicit; each helper call models a distinct request.
    fn image(root: &Path, token: Option<&str>, retry: Option<&str>, dispatch: impl FnOnce() -> Result<String, String>) -> Result<String, String> {
        let state = persisted(root)?;
        let hash = super::fingerprint(format!("fixture-request-{}", state.jobs.len()).as_bytes())?;
        let quote = quote_unbounded_scoped(root, &hash)?;
        let issued = if token.is_none() { confirm_unbounded_scoped(root, "atlas-cloud", "media", "atlas_generate_image", "model", &hash, &quote.ledger_revision)? } else { String::new() };
        run_scoped_with_request(root, "atlas-cloud", "media", "atlas_generate_image", "image.text_to_image", "model", Some(token.unwrap_or(&issued)), retry, Some(request(&hash)), None, dispatch)
    }




    #[test]
    fn second_queued_quote_goes_stale_after_first_operation_settles() {
        // Two distinct operations quoted against the same ledger revision
        // (the renderer's queued-requests case): once the first operation
        // settles, the second quote's revision no longer matches and its
        // confirmation must be rejected without dispatch. A fresh quote
        // against the settled ledger confirms and dispatches independently.
        let dir = setup(100, false);
        let invoke = |hash: &str, token: &str, ok: bool| {
            run_scoped_with_submission(
                dir.path(),
                "atlas-cloud",
                "media",
                "atlas_generate_image",
                "image.text_to_image",
                "model",
                Some(token),
                None,
                Some(request(hash)),
                None,
                |_id, _submitted| {
                    if ok {
                        Ok("done".into())
                    } else {
                        Err("boom".into())
                    }
                },
            )
        };
        let req_a = request(&fingerprint('a'));
        let req_b = request(&fingerprint('b'));
        let quote_a = quote_unbounded_scoped(dir.path(), &req_a.request_hash).unwrap();
        let quote_b = quote_unbounded_scoped(dir.path(), &req_b.request_hash).unwrap();
        assert_eq!(quote_a.ledger_revision, quote_b.ledger_revision);

        let token_a = confirm_unbounded_scoped(
            dir.path(),
            "atlas-cloud",
            "media",
            "atlas_generate_image",
            "model",
            &req_a.request_hash,
            &quote_a.ledger_revision,
        )
        .unwrap();
        invoke(&req_a.request_hash, &token_a, true).unwrap();
        assert_eq!(persisted(dir.path()).unwrap().jobs.len(), 1);

        // The queued second request still holds the pre-settlement revision.
        let stale = confirm_unbounded_scoped(
            dir.path(),
            "atlas-cloud",
            "media",
            "atlas_generate_image",
            "model",
            &req_b.request_hash,
            &quote_b.ledger_revision,
        )
        .unwrap_err();
        assert!(stale.contains("stale"), "{stale}");
        assert_eq!(persisted(dir.path()).unwrap().jobs.len(), 1);

        // A fresh quote against the settled ledger works; the second
        // operation is usable once re-quoted, no cross-request reuse.
        let quote_b2 = quote_unbounded_scoped(dir.path(), &req_b.request_hash).unwrap();
        let token_b = confirm_unbounded_scoped(
            dir.path(),
            "atlas-cloud",
            "media",
            "atlas_generate_image",
            "model",
            &req_b.request_hash,
            &quote_b2.ledger_revision,
        )
        .unwrap();
        invoke(&req_b.request_hash, &token_b, true).unwrap();
        let state = persisted(dir.path()).unwrap();
        assert_eq!(state.jobs.len(), 2);
        // Each reservation carries its own request fingerprint.
        let hashes: Vec<_> = state
            .jobs
            .iter()
            .map(|j| j.request_metadata.as_ref().unwrap().request_hash.clone())
            .collect();
        assert!(hashes.contains(&fingerprint('a')));
        assert!(hashes.contains(&fingerprint('b')));
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
    fn a_legacy_verified_maximum_cannot_waive_operation_permission() {
        let dir = setup(100, true);
        assert!(run_scoped_with_request(dir.path(), "atlas-cloud", "media", "atlas_generate_image", "image.text_to_image", "model", None, None, Some(request(&fingerprint(char::from(97)))), Some(VerifiedMaximum::synthetic_for_test("USD", 7)), || panic!("maximum bypassed permission")).unwrap_err().contains("approval required"));
    }

}
