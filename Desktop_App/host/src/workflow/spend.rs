//! Shared fail-closed billable dispatch. A durable reservation precedes each request.
use crate::workflow::cost::{self, Price, SpendGate};
use crate::workflow::import;
use crate::workflow::state::{
    BudgetPolicy, JobRecord, JobStatus, ProjectState, SpendConfirmation, SpendEvidenceProvenance,
    SpendReconciliation, SpendRequestMetadata,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;
#[cfg(test)]
use std::sync::atomic::Ordering;

/// An adapter-proven ceiling for an exact request. Deliberately no production
/// constructor: a registry estimate or renderer number cannot mint one.
#[derive(Debug, Clone)]
pub struct VerifiedMaximum {
    currency: String,
    minor: i64,
}

#[cfg(test)]
impl VerifiedMaximum {
    pub(crate) fn synthetic_for_test(currency: &str, minor: i64) -> Self {
        Self {
            currency: currency.into(),
            minor,
        }
    }
}

fn policy(state: &ProjectState) -> BudgetPolicy {
    state
        .budget
        .as_ref()
        .and_then(|b| b.policy)
        .unwrap_or_else(|| {
            if state.budget.as_ref().is_some_and(|b| b.hard_stop) {
                BudgetPolicy::StrictHardStop
            } else {
                BudgetPolicy::ApprovalRequired
            }
        })
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
    if policy(&state) != BudgetPolicy::ApprovalRequired {
        return Err("unbounded approval requires approval_required policy".into());
    }
    Ok(UnboundedSpendQuote {
        currency: cost::compute_ledger(&state).currency,
        request_hash: request_hash.into(),
        ledger_revision: revision(&state)?,
        disclosure:
            "No verified maximum exists for this request; its charge may exceed your budget limit."
                .into(),
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
    if policy(&state) != BudgetPolicy::ApprovalRequired || revision(&state)? != shown_revision {
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
        currency: cost::compute_ledger(&state).currency,
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
    capability: &str,
    model: &str,
    confirmation: Option<&str>,
    retry_of: Option<&str>,
    request: Option<SpendRequestMetadata>,
    maximum: Option<VerifiedMaximum>,
    dispatch: F,
) -> Result<String, String>
where
    F: FnOnce(&str, &dyn Fn(&str) -> Result<(), String>) -> Result<String, String>,
{
    valid_scope(category, operation, model)?;
    if retry_of.is_some() {
        return Err("retry unsupported: provider outcome is not independently verified and original request is not stored".into());
    }
    let estimate = cost::resolve_price(provider, capability);
    let _lock = import::lock_project(root)?;
    let mut state = persisted(root)?;
    let ledger = cost::compute_ledger(&state);
    let mode = policy(&state);
    if mode == BudgetPolicy::StrictHardStop && ledger.unknown_exposure_jobs != 0 {
        return Err("unknown outstanding spend; generation blocked".into());
    }
    let currency = ledger.currency.clone();
    let minor = maximum.as_ref().map_or(0, |max| max.minor);
    if maximum
        .as_ref()
        .is_some_and(|max| max.minor < 0 || !max.currency.eq_ignore_ascii_case(&currency))
    {
        return Err("invalid verified maximum or currency mismatch".into());
    }
    if mode == BudgetPolicy::StrictHardStop && maximum.is_none() {
        return Err("strict hard stop requires an adapter-verified request maximum".into());
    }
    if request
        .as_ref()
        .is_some_and(|r| !valid_hash(&r.request_hash) || r.prediction_id.is_some())
    {
        return Err("invalid request fingerprint or pre-dispatch prediction id".into());
    }
    // Identical payloads can be intentional, but an unresolved earlier attempt
    // could already have been accepted. Refuse any ambiguous replay by default.
    if request.as_ref().is_some_and(|r| {
        state.jobs.iter().any(|j| {
            j.request_metadata
                .as_ref()
                .is_some_and(|old| old.request_hash == r.request_hash)
                && j.reconciliation.is_none()
        })
    }) {
        return Err("ambiguous previous provider outcome for identical request; recover by job id, do not resubmit".into());
    }
    if mode == BudgetPolicy::WarnOnly && maximum.is_none() && request.is_none() {
        return Err(
            "warn-only unbounded requests require a fingerprint and persisted disclosure".into(),
        );
    }
    if !ledger.currency.eq_ignore_ascii_case(&currency) {
        return Err(format!(
            "budget currency {} differs from price currency {currency}",
            ledger.currency
        ));
    }
    if mode == BudgetPolicy::ApprovalRequired && maximum.is_none() {
        let hash = request
            .as_ref()
            .ok_or("unbounded approval needs an exact request fingerprint")?
            .request_hash
            .as_str();
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
                    && item.amount_minor == 0
                    && item.request_hash == hash
                    && item.ledger_revision == current
            })
            .ok_or("unbounded request approval required")?;
        state.spend_confirmations.remove(index);
    }
    match cost::gate_spend(&state, minor) {
        SpendGate::Blocked => return Err("budget hard stop; generation blocked".into()),
        SpendGate::Confirm if maximum.is_none() && mode == BudgetPolicy::ApprovalRequired => {}
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
                        && item.request_hash.is_empty()
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
    job.currency = Some(currency.clone());
    job.estimated_cost_minor = match estimate {
        Price::Known {
            currency: estimate_currency,
            minor: estimate_minor,
        } if estimate_currency.eq_ignore_ascii_case(&currency) && estimate_minor >= 0 => {
            Some(estimate_minor)
        }
        _ => None,
    };
    job.reserved_max_minor = maximum.as_ref().map(|max| max.minor);
    job.unbounded_exposure = maximum.is_none();
    job.request_metadata = request.map(|mut metadata| {
        metadata.disclosure = maximum.is_none().then(||
            "Unbounded provider charge may exceed the project budget; no hard stop is guaranteed.".into()
        );
        metadata
    });
    job.retry_of = retry_of.map(str::to_string);
    job.status = JobStatus::Running;
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
        Ok(_) => job.finish(None, None),
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
    fn submission_id_is_durable_before_status_and_duplicate_unknown_outcome_is_blocked() {
        let dir = setup(100, false);
        let mut state = persisted(dir.path()).unwrap();
        state.budget.as_mut().unwrap().policy = Some(BudgetPolicy::WarnOnly);
        import::persist_state(dir.path(), &state).unwrap();
        let req = request(&fingerprint('d'));
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
                None,
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
        assert!(job.unbounded_exposure);
        assert!(job.request_metadata.as_ref().unwrap().disclosure.is_some());
    }

    #[test]
    fn warn_only_unbounded_persists_disclosure_and_requires_fingerprint() {
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
        run_scoped_with_request(
            dir.path(),
            "atlas-cloud",
            "media",
            "atlas_generate_image",
            "image.text_to_image",
            "model",
            None,
            None,
            Some(request(&fingerprint('c'))),
            None,
            || Ok("done".into()),
        )
        .unwrap();
        let state = persisted(dir.path()).unwrap();
        assert!(state.jobs[0]
            .request_metadata
            .as_ref()
            .unwrap()
            .disclosure
            .as_ref()
            .unwrap()
            .contains("budget"));
    }

    #[test]
    fn strict_rejects_unknown_maximum_mismatched_currency_and_overflow() {
        let dir = setup(i64::MAX, true);
        let attempted = AtomicUsize::new(0);
        let invoke = |maximum| {
            run_scoped_with_request(
                dir.path(),
                "atlas-cloud",
                "media",
                "atlas_generate_image",
                "image.text_to_image",
                "model",
                None,
                None,
                None,
                maximum,
                || {
                    attempted.fetch_add(1, Ordering::Relaxed);
                    Ok("sent".into())
                },
            )
        };
        assert!(invoke(None).is_err());
        assert!(invoke(Some(VerifiedMaximum::synthetic_for_test("EUR", 1))).is_err());
        invoke(Some(VerifiedMaximum::synthetic_for_test("USD", i64::MAX))).unwrap();
        assert!(invoke(Some(VerifiedMaximum::synthetic_for_test("USD", 1))).is_err());
        assert_eq!(attempted.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn verified_completed_unsettled_allows_sequential_requests_up_to_ceiling() {
        let dir = setup(100, true);
        let mut state = persisted(dir.path()).unwrap();
        state.budget.as_mut().unwrap().policy = Some(BudgetPolicy::StrictHardStop);
        import::persist_state(dir.path(), &state).unwrap();
        let maximum = VerifiedMaximum::synthetic_for_test("USD", 40);
        for _ in 0..2 {
            run_scoped_with_request(
                dir.path(),
                "atlas-cloud",
                "media",
                "atlas_generate_image",
                "image.text_to_image",
                "model",
                None,
                None,
                None,
                Some(maximum.clone()),
                || Ok("ok".into()),
            )
            .unwrap();
        }
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
            None,
            Some(maximum),
            || {
                attempted.fetch_add(1, Ordering::Relaxed);
                Ok("no".into())
            }
        )
        .is_err());
        assert_eq!(attempted.load(Ordering::Relaxed), 0);
        assert_eq!(
            cost::compute_ledger(&persisted(dir.path()).unwrap()).unsettled_estimated_minor,
            80
        );
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
    fn image(
        root: &Path,
        token: Option<&str>,
        retry: Option<&str>,
        dispatch: impl FnOnce() -> Result<String, String>,
    ) -> Result<String, String> {
        run_scoped_with_request(
            root,
            "atlas-cloud",
            "media",
            "atlas_generate_image",
            "image.text_to_image",
            "model",
            token,
            retry,
            None,
            Some(VerifiedMaximum::synthetic_for_test("USD", 7)),
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
        // Manual evidence does not verify the outcome, but the adapter's bounded
        // maximum still makes a fresh, independent request safe under the cap.
        image(dir.path(), None, None, || Ok("new request".into())).unwrap();
        assert_eq!(persisted(dir.path()).unwrap().jobs.len(), 2);
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
        image(dir.path(), None, None, || {
            Ok("new request dispatched".into())
        })
        .unwrap();
        assert_eq!(persisted(dir.path()).unwrap().jobs.len(), 2);
    }
    #[test]
    fn fabricated_reference_and_zero_never_verify_outcome_or_authorize_retry() {
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
        // The fabricated evidence remains untrusted, while the independently
        // verified per-request ceiling permits a different bounded dispatch.
        image(dir.path(), None, None, || Ok("paid request sent".into())).unwrap();
        assert_eq!(persisted(dir.path()).unwrap().jobs.len(), 2);
    }

    #[test]
    fn unknown_price_without_verified_maximum_and_missing_project_never_dispatch() {
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
            .contains("adapter-verified request maximum"));
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
