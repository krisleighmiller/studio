//! Unknown-exposure direct voice dispatch shares the project ledger and reservation.
//! No provider usage header or user-entered amount is a verified USD maximum.
#[cfg(test)]
use crate::workflow::{
    import,
    state::ProjectState,
};
use crate::workflow::{spend, state::SpendRequestMetadata};
#[cfg(test)]
use crate::workflow::state::BudgetPolicy;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::Path;
#[cfg(test)]
use std::fs;
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
pub fn confirm(_root: &Path, _operation: &str, _model: &str, _input: &str, _amount: i64, _evidence: &str, _attested: bool) -> Result<String, String> { Err("Project accounting has been retired.".into()) }

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
    let shared = spend::quote_unbounded_scoped(root, &request_hash)?;
    Ok(VoiceQuote {
        policy: "approval_required",
        request_hash: shared.request_hash,
        ledger_revision: Some(shared.ledger_revision),
        disclosure: shared.disclosure,
    })
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
            request_hash, disclosure: None, duration_seconds: None, width: None,
            height: None, prediction_id: None,
        }), None, dispatch)
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
    fn no_budget_voice_needs_approval_but_not_a_price_or_ceiling() {
        for budget in [
            None,
            Some(ProjectBudget {
                limit_minor: None,
                hard_stop: true,
                policy: Some(BudgetPolicy::StrictHardStop),
                ..Default::default()
            }),
        ] {
            let root = tempfile::tempdir().unwrap();
            let mut state = ProjectState::new();
            state.budget = budget;
            import::persist_state(root.path(), &state).unwrap();
            for (op, model, input) in [
                (
                    "elevenlabs_tts_preview",
                    "eleven_multilingual_v2",
                    r#"{"text":"hello","voice_id":"voice"}"#,
                ),
                (
                    "elevenlabs_design_voice",
                    "eleven_multilingual_ttv_v2",
                    r#"{"text":"hello","voice_description":"sample"}"#,
                ),
                (
                    "elevenlabs_create_designed_voice",
                    "text-to-voice/create",
                    r#"{"voice_name":"name","voice_description":"sample","generated_voice_id":"id"}"#,
                ),
            ] {
                let quote = quote(root.path(), op, model, input).unwrap();
                assert_eq!(quote.policy, "approval_required");
                assert!(run(root.path(), op, model, input, None, || panic!(
                    "unapproved request"
                ))
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
                assert_eq!(
                    run(root.path(), op, model, input, Some(&token), || Ok(
                        "sent".into()
                    ))
                    .unwrap(),
                    "sent"
                );
                assert!(run(root.path(), op, model, input, Some(&token), || panic!(
                    "duplicate"
                ))
                .is_err());
            }
            let state = persisted(root.path()).unwrap();
            assert_eq!(state.jobs.len(), 3);
            assert!(state
                .jobs
                .iter()
                .all(|job| job.cost_minor.is_none() && job.estimated_cost_minor.is_none()));
        }
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
    fn strict_blocks_quote_and_warn_only_still_requires_bound_authorization() {
        let root = setup();
        let op = "elevenlabs_design_voice";
        let model = "eleven_multilingual_ttv_v2";
        let input = r#"{"text":"hello","voice_description":"sample"}"#;
        assert!(quote(root.path(), op, model, input).is_ok());
        let mut state = persisted(root.path()).unwrap();
        state.budget.as_mut().unwrap().policy = Some(BudgetPolicy::WarnOnly);
        import::persist_state(root.path(), &state).unwrap();
        let quote = quote(root.path(), op, model, input).unwrap();
        assert_eq!(quote.policy, "approval_required");
        assert!(quote.disclosure.contains("Authorize"));
        assert!(quote.ledger_revision.is_some());
        let calls = Cell::new(0);
        assert!(run(root.path(), op, model, input, None, || {
            calls.set(calls.get() + 1);
            Ok("sent".into())
        })
        .is_err());
        assert_eq!(calls.get(), 0);
        let token = confirm_unbounded(
            root.path(),
            op,
            model,
            input,
            &quote.request_hash,
            quote.ledger_revision.as_deref().unwrap(),
        )
        .unwrap();
        assert_eq!(
            run(root.path(), op, model, input, Some(&token), || {
                calls.set(calls.get() + 1);
                Ok("sent".into())
            })
            .unwrap(),
            "sent"
        );
        assert_eq!(calls.get(), 1);
    }
}
