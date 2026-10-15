//! Unknown-exposure direct voice dispatch shares the project ledger and reservation.
//! No provider usage header or user-entered amount is a verified USD maximum.
#[cfg(test)]
use crate::workflow::{
    cost, import,
    state::{JobRecord, JobStatus, SpendConfirmation},
};
use crate::workflow::{
    spend,
    state::{BudgetPolicy, ProjectState, SpendRequestMetadata},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
#[cfg(test)]
use std::{
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
        request_hash: String::new(),
    });
    import::persist_state_locked(root, &state)?;
    Ok(token)
}
fn fingerprint(operation: &str, model: &str, input: &str) -> Result<String, String> {
    scope(operation, model, input)?;
    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|_| "invalid voice request JSON".to_string())?;
    let fields = value.as_object().ok_or("voice request must be an object")?;
    let keys: &[&str] = match operation {
        "elevenlabs_tts_preview" => &["text", "voice_id"],
        "elevenlabs_design_voice" => &["text", "voice_description"],
        "elevenlabs_create_designed_voice" => {
            &["voice_name", "voice_description", "generated_voice_id"]
        }
        _ => return Err("unknown direct voice operation".into()),
    };
    if fields.len() != keys.len()
        || keys.iter().any(|key| {
            fields
                .get(*key)
                .and_then(serde_json::Value::as_str)
                .is_none_or(|text| text.is_empty())
        })
    {
        return Err("voice request fields do not match the operation".into());
    }
    let canonical = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
    Ok(hash(
        format!("voice:v1:{operation}:{model}:")
            .as_bytes()
            .iter()
            .copied()
            .chain(canonical)
            .collect::<Vec<_>>()
            .as_slice(),
    ))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceQuote {
    pub policy: &'static str,
    pub request_hash: String,
    pub ledger_revision: Option<String>,
    pub disclosure: String,
}

/// Host derives the fingerprint from the exact submitted input, not a renderer hash.
pub fn quote(root: &Path, operation: &str, model: &str, input: &str) -> Result<VoiceQuote, String> {
    let request_hash = fingerprint(operation, model, input)?;
    let _lock = crate::workflow::import::lock_project(root)?;
    let text = fs::read_to_string(root.join(crate::workflow::state::STATE_REL))
        .map_err(|_| "persisted project state required for paid voice request".to_string())?;
    let state = ProjectState::from_json(&text)?;
    let policy = state
        .budget
        .as_ref()
        .and_then(|budget| budget.policy)
        .unwrap_or_else(|| {
            if state.budget.as_ref().is_some_and(|budget| budget.hard_stop) {
                BudgetPolicy::StrictHardStop
            } else {
                BudgetPolicy::ApprovalRequired
            }
        });
    drop(_lock);
    match policy {
        BudgetPolicy::StrictHardStop => Err("strict hard stop requires an adapter-verified request maximum; voice request blocked".into()),
        BudgetPolicy::ApprovalRequired => {
            let shared = spend::quote_unbounded_scoped(root, &request_hash)?;
            Ok(VoiceQuote { policy: "approval_required", request_hash: shared.request_hash,
                ledger_revision: Some(shared.ledger_revision), disclosure: shared.disclosure })
        }
        BudgetPolicy::WarnOnly => Ok(VoiceQuote { policy: "warn_only", request_hash,
            ledger_revision: None, disclosure: "No verified maximum exists for this voice request. The charge may exceed your budget; warn-only policy does not enforce a hard stop.".into() }),
    }
}

pub fn confirm_unbounded(
    root: &Path,
    operation: &str,
    model: &str,
    input: &str,
    shown_hash: &str,
    shown_revision: &str,
) -> Result<String, String> {
    let actual_hash = fingerprint(operation, model, input)?;
    if shown_hash != actual_hash {
        return Err("voice request changed since quote".into());
    }
    spend::confirm_unbounded_scoped(
        root,
        "elevenlabs",
        "voice",
        operation,
        model,
        &actual_hash,
        shown_revision,
    )
}

/// The shared host boundary reserves before dispatch and leaves actual USD unknown.
pub fn run(
    root: &Path,
    operation: &str,
    model: &str,
    input: &str,
    token: Option<&str>,
    dispatch: impl FnOnce() -> Result<String, String>,
) -> Result<String, String> {
    let request_hash = fingerprint(operation, model, input)?;
    let capability = match operation {
        "elevenlabs_tts_preview" => "audio.tts_profile",
        "elevenlabs_design_voice" => "audio.voice_design",
        "elevenlabs_create_designed_voice" => "audio.voice_clone",
        _ => return Err("unknown direct voice operation".into()),
    };
    spend::run_scoped_with_request(root, "elevenlabs", "voice", operation, capability,
        model, token, None, Some(SpendRequestMetadata {
            request_hash, disclosure: Some("No verified maximum exists for this voice request; charge may exceed the project budget.".into()), duration_seconds: None, width: None,
            height: None, prediction_id: None,
        }), None, dispatch)
}

#[cfg(test)]
/// Synthetic verified ceiling only: exercise reservation without live dispatch.
/// Actual cost remains unknown until independently reconciled, including failed requests.
fn run_verified(
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
            policy: None,
        });
        import::persist_state(root.path(), &state).unwrap();
        root
    }
    #[test]
    fn production_boundary_blocks_every_direct_voice_route_even_with_a_token() {
        let root = setup();
        let before = fs::read(root.path().join(crate::workflow::state::STATE_REL)).unwrap();
        let routes = [
            ("elevenlabs_tts_preview", "eleven_multilingual_v2"),
            ("elevenlabs_design_voice", "eleven_multilingual_ttv_v2"),
            ("elevenlabs_create_designed_voice", "text-to-voice/create"),
        ];
        for (operation, model) in routes {
            let input = match operation {
                "elevenlabs_tts_preview" => r#"{"text":"hello","voice_id":"voice"}"#,
                "elevenlabs_design_voice" => r#"{"text":"hello","voice_description":"sample"}"#,
                _ => {
                    r#"{"voice_name":"name","voice_description":"sample","generated_voice_id":"id"}"#
                }
            };
            let calls = Cell::new(0);
            let error = run(
                root.path(),
                operation,
                model,
                input,
                Some("forged-token"),
                || {
                    calls.set(calls.get() + 1);
                    Ok("sent".into())
                },
            )
            .unwrap_err();
            assert!(
                error.contains("strict hard stop requires an adapter-verified request maximum"),
                "{error}"
            );
            assert_eq!(
                calls.get(),
                0,
                "{operation} dispatched without authorization"
            );
            assert!(confirm(
                root.path(),
                operation,
                model,
                "input",
                100,
                "user-entered invoice",
                true
            )
            .is_err());
        }
        assert_eq!(
            fs::read(root.path().join(crate::workflow::state::STATE_REL)).unwrap(),
            before
        );
    }

    #[test]
    fn no_quote_never_reaches_provider() {
        let root = setup();
        let calls = Cell::new(0);
        assert!(run_verified(
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
        assert!(run_verified(
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
            run_verified(
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
        assert!(run_verified(
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
        assert!(run_verified(
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
        assert!(run_verified(
            root.path(),
            "elevenlabs_create_designed_voice",
            "text-to-voice/create",
            "id",
            Some(&token),
            || panic!("wrong route sent")
        )
        .is_err());
    }
    #[test]
    fn unknown_approval_binds_request_and_is_one_use() {
        let root = setup();
        let mut state = persisted(root.path()).unwrap();
        state.budget.as_mut().unwrap().policy = Some(BudgetPolicy::ApprovalRequired);
        import::persist_state(root.path(), &state).unwrap();
        let op = "elevenlabs_tts_preview";
        let model = "eleven_multilingual_v2";
        let input = r#"{"text":"hello","voice_id":"voice"}"#;
        let quote = quote(root.path(), op, model, input).unwrap();
        assert_eq!(quote.policy, "approval_required");
        let altered = r#"{"text":"different","voice_id":"voice"}"#;
        assert!(confirm_unbounded(
            root.path(),
            op,
            model,
            altered,
            &quote.request_hash,
            quote.ledger_revision.as_deref().unwrap()
        )
        .is_err());
        let token = confirm_unbounded(
            root.path(),
            op,
            model,
            input,
            &quote.request_hash,
            quote.ledger_revision.as_deref().unwrap(),
        )
        .unwrap();
        let calls = Cell::new(0);
        assert!(run(root.path(), op, model, altered, Some(&token), || {
            calls.set(1);
            Ok("sent".into())
        })
        .is_err());
        assert_eq!(calls.get(), 0);
        assert_eq!(
            run(root.path(), op, model, input, Some(&token), || {
                calls.set(1);
                Ok("sent".into())
            })
            .unwrap(),
            "sent"
        );
        assert!(run(root.path(), op, model, input, Some(&token), || panic!(
            "replay sent"
        ))
        .is_err());
        let state = persisted(root.path()).unwrap();
        assert_eq!(state.jobs[0].cost_minor, None);
        assert_eq!(state.jobs[0].estimated_cost_minor, None);
    }
    #[test]
    fn strict_blocks_quote_warn_only_discloses_unknown_exposure() {
        let root = setup();
        let op = "elevenlabs_design_voice";
        let model = "eleven_multilingual_ttv_v2";
        let input = r#"{"text":"hello","voice_description":"sample"}"#;
        assert!(quote(root.path(), op, model, input).is_err());
        let mut state = persisted(root.path()).unwrap();
        state.budget.as_mut().unwrap().policy = Some(BudgetPolicy::WarnOnly);
        import::persist_state(root.path(), &state).unwrap();
        let quote = quote(root.path(), op, model, input).unwrap();
        assert_eq!(quote.policy, "warn_only");
        assert!(quote.disclosure.contains("verified maximum"));
        assert_eq!(
            run(root.path(), op, model, input, None, || Ok("sent".into())).unwrap(),
            "sent"
        );
    }
}
