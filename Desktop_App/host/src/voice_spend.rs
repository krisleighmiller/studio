//! Direct voice operations require a machine-verified account-specific USD ceiling.
//! User attestations and arbitrary positive caps cannot establish a hard spend bound.
//! The same project lock and ledger as media spend serialize quote and reservation.
#[cfg(test)]
use crate::workflow::{
    cost, import,
    state::{JobRecord, JobStatus, ProjectState, SpendConfirmation},
};
#[cfg(test)]
use sha2::{Digest, Sha256};
use std::path::Path;
#[cfg(test)]
use std::{
    fs,
    io::Read,
    sync::atomic::{AtomicU64, Ordering},
};
#[cfg(test)]
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
fn lock(root: &Path) -> Result<fs::File, String> {
    import::lock_project(root)
}
#[cfg(test)]
fn persisted(root: &Path) -> Result<ProjectState, String> {
    let text = fs::read_to_string(root.join(crate::workflow::state::STATE_REL))
        .map_err(|_| "persisted project state required for paid voice request".to_string())?;
    ProjectState::from_json(&text)
}
#[cfg(test)]
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
#[cfg(test)]
fn revision(state: &ProjectState) -> Result<String, String> {
    let bytes = serde_json::to_vec(&(&state.budget, &state.jobs)).map_err(|e| e.to_string())?;
    Ok(hash(&bytes))
}
#[cfg(test)]
fn scope(operation: &str, model: &str, input: &str) -> Result<String, String> {
    let expected = match operation {
        "elevenlabs_tts_preview" => "eleven_multilingual_v2",
        "elevenlabs_design_voice" => "eleven_multilingual_ttv_v2",
        "elevenlabs_create_designed_voice" => "text-to-voice/create",
        _ => return Err("unknown direct voice operation".into()),
    };
    if model != expected || input.trim().is_empty() {
        return Err("invalid voice model or request input".into());
    }
    Ok(format!("{model}:{}", hash(input.as_bytes())))
}
#[cfg(test)]
fn preflight(state: &ProjectState, amount: i64) -> Result<(), String> {
    if amount <= 0
        || state
            .budget
            .as_ref()
            .is_none_or(|b| !b.hard_stop || b.currency != "USD" || b.limit_minor.is_none())
    {
        return Err("USD hard-stop project budget and positive verified ceiling required".into());
    }
    let ledger = cost::compute_ledger(state);
    if ledger.unknown_cost_jobs != 0 || ledger.currency != "USD" {
        return Err("unknown outstanding spend or currency mismatch".into());
    }
    if cost::gate_spend(state, amount) == cost::SpendGate::Blocked {
        return Err("budget hard stop; voice request blocked".into());
    }
    Ok(())
}
/// No provider-backed verifier is available for this account-specific USD ceiling.
/// A free-text invoice, checkbox and arbitrary amount are NOT proof of a maximum.
/// Preserve this fail-closed API until a trusted provider-backed verifier can mint
/// a `VerifiedCeiling` for the exact operation, model and payload.
pub fn confirm(
    _root: &Path,
    _operation: &str,
    _model: &str,
    _input: &str,
    _amount: i64,
    _evidence: &str,
    _attested: bool,
) -> Result<String, String> {
    Err("Direct voice is blocked: no machine-verifiable account-specific USD upper bound is available; a user-entered ceiling cannot guarantee a hard stop".into())
}

/// Only a provider-backed verifier may construct this in production; the test
/// harness injects a synthetic verified bound to exercise the downstream path.
#[cfg(test)]
struct VerifiedCeiling {
    upper_minor: i64,
}

#[cfg(test)]
fn confirm_verified(
    root: &Path,
    operation: &str,
    model: &str,
    input: &str,
    ceiling: VerifiedCeiling,
    requested_cap: i64,
) -> Result<String, String> {
    if requested_cap < ceiling.upper_minor || ceiling.upper_minor <= 0 {
        return Err("cap falls below verified provider upper bound".into());
    }
    let route = scope(operation, model, input)?;
    let _lock = lock(root)?;
    let mut state = persisted(root)?;
    preflight(&state, requested_cap)?;
    let mut random = [0u8; 32];
    fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut random))
        .map_err(|e| e.to_string())?;
    let token = random
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    state.spend_confirmations.push(SpendConfirmation {
        token: token.clone(),
        provider: "elevenlabs".into(),
        category: "voice".into(),
        operation: operation.into(),
        model: format!("verified:v1:{route}"),
        currency: "USD".into(),
        amount_minor: requested_cap,
        ledger_revision: revision(&state)?,
    });
    import::persist_state_locked(root, &state)?;
    Ok(token)
}
#[cfg(not(test))]
pub fn run(
    _root: &Path,
    _operation: &str,
    _model: &str,
    _input: &str,
    _token: Option<&str>,
    _dispatch: impl FnOnce() -> Result<String, String>,
) -> Result<String, String> {
    Err("Direct voice is blocked: no machine-verifiable account-specific USD upper bound is available".into())
}

#[cfg(test)]
/// The closure is the sole provider dispatch; reservation is persisted before invocation.
/// Actual cost remains unknown until independently reconciled, including failed requests.
pub fn run(
    root: &Path,
    operation: &str,
    model: &str,
    input: &str,
    token: Option<&str>,
    dispatch: impl FnOnce() -> Result<String, String>,
) -> Result<String, String> {
    let route = scope(operation, model, input)?;
    let token = token
        .filter(|s| !s.is_empty())
        .ok_or("one-use voice spend confirmation required")?;
    let _lock = lock(root)?;
    let mut state = persisted(root)?;
    let current = revision(&state)?;
    let index = state
        .spend_confirmations
        .iter()
        .position(|item| {
            item.token == token
                && item.provider == "elevenlabs"
                && item.category == "voice"
                && item.operation == operation
                && item.model == format!("verified:v1:{route}")
                && item.currency == "USD"
                && item.ledger_revision == current
                && item.amount_minor > 0
        })
        .ok_or("voice spend confirmation missing, stale, or for a different request")?;
    let minor = state.spend_confirmations[index].amount_minor;
    preflight(&state, minor)?;
    state.spend_confirmations.remove(index);
    let id = format!(
        "voice-spend-{}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    );
    let mut job = JobRecord::open(id.clone(), operation);
    job.provider = Some("elevenlabs".into());
    job.spend_category = Some("voice".into());
    job.model = Some(route);
    job.currency = Some("USD".into());
    job.estimated_cost_minor = Some(minor);
    job.status = JobStatus::Running;
    state.jobs.push(job);
    import::persist_state_locked(root, &state)?;
    let result = dispatch();
    let mut state = persisted(root)
        .map_err(|e| format!("voice request dispatched; manually reconcile {id}: {e}"))?;
    let job = state
        .jobs
        .iter_mut()
        .find(|job| job.id == id)
        .ok_or_else(|| {
            format!("voice request dispatched; reservation lost; manually reconcile {id}")
        })?;
    match &result {
        Ok(_) => job.finish(None, None),
        Err(error) => job.fail(None, error.clone()),
    };
    import::persist_state_locked(root, &state)
        .map_err(|e| format!("voice request dispatched; manually reconcile {id}: {e}"))?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::state::ProjectBudget;
    use std::cell::Cell;
    fn setup() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let mut state = ProjectState::new();
        state.budget = Some(ProjectBudget {
            currency: "USD".into(),
            limit_minor: Some(100),
            warning_at_minor: None,
            hard_stop: true,
        });
        import::persist_state(root.path(), &state).unwrap();
        root
    }
    #[test]
    fn no_quote_never_reaches_provider() {
        let root = setup();
        let calls = Cell::new(0);
        assert!(run(
            root.path(),
            "elevenlabs_tts_preview",
            "eleven_multilingual_v2",
            "hello",
            None,
            || {
                calls.set(calls.get() + 1);
                Ok("sent".into())
            }
        )
        .is_err());
        assert_eq!(calls.get(), 0);
    }
    #[test]
    fn verified_ceiling_reserves_once_and_leaves_actual_unknown() {
        let root = setup();
        let token = confirm_verified(
            root.path(),
            "elevenlabs_tts_preview",
            "eleven_multilingual_v2",
            "hello",
            VerifiedCeiling { upper_minor: 25 },
            25,
        )
        .unwrap();
        let calls = Cell::new(0);
        assert!(run(
            root.path(),
            "elevenlabs_tts_preview",
            "eleven_multilingual_v2",
            "different",
            Some(&token),
            || {
                calls.set(1);
                Ok("sent".into())
            }
        )
        .is_err());
        assert_eq!(calls.get(), 0);
        assert_eq!(
            run(
                root.path(),
                "elevenlabs_tts_preview",
                "eleven_multilingual_v2",
                "hello",
                Some(&token),
                || {
                    calls.set(1);
                    Ok("sent".into())
                }
            )
            .unwrap(),
            "sent"
        );
        assert_eq!(calls.get(), 1);
        assert!(run(
            root.path(),
            "elevenlabs_tts_preview",
            "eleven_multilingual_v2",
            "hello",
            Some(&token),
            || panic!("sent twice")
        )
        .is_err());
        let state = persisted(root.path()).unwrap();
        assert_eq!(state.jobs[0].estimated_cost_minor, Some(25));
        assert_eq!(state.jobs[0].cost_minor, None);
    }
    #[test]
    fn ceiling_is_not_inferred_from_registry_or_generic_rate() {
        let root = setup();
        assert!(confirm(
            root.path(),
            "elevenlabs_design_voice",
            "eleven_multilingual_ttv_v2",
            "x".repeat(100).as_str(),
            1,
            "user-supplied arbitrary invoice text",
            true
        )
        .unwrap_err()
        .contains("blocked"));
        let calls = Cell::new(0);
        assert!(run(
            root.path(),
            "elevenlabs_design_voice",
            "eleven_multilingual_ttv_v2",
            "x",
            Some("arbitrary"),
            || {
                calls.set(1);
                Ok("sent".into())
            }
        )
        .is_err());
        assert_eq!(calls.get(), 0);
        assert!(confirm_verified(
            root.path(),
            "elevenlabs_design_voice",
            "eleven_multilingual_ttv_v2",
            "x".repeat(100).as_str(),
            VerifiedCeiling { upper_minor: 1000 },
            1,
        )
        .unwrap_err()
        .contains("below verified"));
        let token = confirm_verified(
            root.path(),
            "elevenlabs_design_voice",
            "eleven_multilingual_ttv_v2",
            "x".repeat(100).as_str(),
            VerifiedCeiling { upper_minor: 25 },
            25,
        )
        .unwrap();
        assert!(run(
            root.path(),
            "elevenlabs_create_designed_voice",
            "text-to-voice/create",
            "id",
            Some(&token),
            || panic!("wrong route sent")
        )
        .is_err());
    }
}
