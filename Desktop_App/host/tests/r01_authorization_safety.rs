//! Independent safety controls retained after budgeting retirement (local callbacks only).
use workbench_host::workflow::{import,spend,state::{ProjectState,SpendRequestMetadata}};
use std::path::Path;
use std::sync::{Arc,Barrier,atomic::{AtomicUsize,Ordering}};
fn req()->SpendRequestMetadata{SpendRequestMetadata{request_hash:"a".repeat(64),disclosure:None,duration_seconds:None,width:None,height:None,prediction_id:None}}
fn permission(root:&Path)->String{let q=spend::quote_unbounded_scoped(root,&req().request_hash).unwrap();spend::confirm_unbounded_scoped(root,"fixture","media","generate","model",&q.request_hash,&q.ledger_revision).unwrap()}
fn invoke(root:&Path,t:&str,dispatch:impl FnOnce()->Result<String,String>)->Result<String,String>{spend::run_scoped_with_request(root,"fixture","media","generate","unknown","model",Some(t),None,Some(req()),None,dispatch)}
#[test]
fn retired_accounting_commands_do_not_mutate_or_issue_authorization(){let d=tempfile::tempdir().unwrap();let app=workbench_host::ctx::HostCtx::new(d.path().join("settings"));let recorder=workbench_host::voice::RecorderState::new();for cmd in ["workflow_set_budget","workflow_unsettled_spend","workflow_reconcile_spend","confirm_spend","elevenlabs_confirm_voice_spend"]{let error=workbench_host::dispatch(&app,&recorder,cmd,&serde_json::json!({})).unwrap_err();assert!(error.contains("retired"),"{cmd}: {error}");}}
#[test]
fn stale_saves_cannot_erase_permission_or_inflight_attempt(){let d=tempfile::tempdir().unwrap();import::persist_state(d.path(),&ProjectState::new()).unwrap();let stale=import::load_state(d.path()).unwrap();let t=permission(d.path());assert!(import::save_project(d.path(),stale).unwrap_err().contains("changed"));let before=import::load_state(d.path()).unwrap();invoke(d.path(),&t,||{let running=import::load_state(d.path()).unwrap();assert_eq!(running.jobs.len(),1);assert!(import::save_project(d.path(),before.clone()).unwrap_err().contains("changed"));Ok("done".into())}).unwrap();let after=import::load_state(d.path()).unwrap();assert_eq!(after.jobs.len(),1);assert!(after.spend_confirmations.is_empty());assert!(after.jobs[0].started_at.is_some()&&after.jobs[0].completed_at.is_some());}
#[test]
fn concurrent_identical_authorizations_submit_at_most_once(){let d=tempfile::tempdir().unwrap();import::persist_state(d.path(),&ProjectState::new()).unwrap();let tokens=[permission(d.path()),permission(d.path())];let b=Arc::new(Barrier::new(3));let calls=AtomicUsize::new(0);std::thread::scope(|s|{let hs:Vec<_>=tokens.iter().map(|t|{let b=b.clone();let calls=&calls;let root=d.path();s.spawn(move||{b.wait();invoke(root,t,||{calls.fetch_add(1,Ordering::SeqCst);Ok("done".into())})})}).collect();b.wait();assert_eq!(hs.into_iter().map(|h|h.join().unwrap()).filter(Result::is_ok).count(),1);});assert_eq!(calls.load(Ordering::SeqCst),1);assert_eq!(import::load_state(d.path()).unwrap().jobs.len(),1);}
#[test]
fn unsafe_prediction_id_never_persists_and_uncertain_request_stays_nonreplayable(){let d=tempfile::tempdir().unwrap();import::persist_state(d.path(),&ProjectState::new()).unwrap();let t=permission(d.path());let result=spend::run_scoped_with_submission(d.path(),"fixture","media","generate","unknown","model",Some(&t),None,Some(req()),None,|_,submitted|{submitted("https://unsafe.invalid/result?api_key=not-a-real-secret")?;Ok("done".into())});assert!(result.unwrap_err().contains("unsafe prediction"));let after=import::load_state(d.path()).unwrap();assert!(after.jobs[0].request_metadata.as_ref().unwrap().prediction_id.is_none());let t=permission(d.path());assert!(invoke(d.path(),&t,||panic!("uncertain request replay")).is_err());}

#[test]
fn every_supported_direct_voice_route_keeps_exact_one_use_permission() {
    use workbench_host::{voice_spend, workflow::state::{ProjectBudget, BudgetPolicy}};
    for (op, model, input) in [
        ("elevenlabs_tts_preview", "eleven_multilingual_v2", r#"{"text":"fixture hello","voice_id":"fixture-id"}"#),
        ("elevenlabs_design_voice", "eleven_multilingual_ttv_v2", r#"{"text":"fixture hello","voice_description":"fixture voice"}"#),
        ("elevenlabs_create_designed_voice", "text-to-voice/create", r#"{"voice_name":"fixture name","voice_description":"fixture voice","generated_voice_id":"fixture-id"}"#),
    ] {
        let d = tempfile::tempdir().unwrap();
        let mut state = ProjectState::new();
        state.budget = Some(ProjectBudget { currency: "EUR".into(), limit_minor: Some(0), hard_stop: true, policy: Some(BudgetPolicy::StrictHardStop), ..Default::default() });
        import::persist_state(d.path(), &state).unwrap();
        let q = voice_spend::quote(d.path(), op, model, input).unwrap();
        assert!(voice_spend::run(d.path(), op, model, input, Some("forged"), || panic!("forged permission dispatched")).is_err());
        assert!(voice_spend::confirm_unbounded(d.path(), op, model, &input.replace("fixture", "changed"), &q.request_hash, q.ledger_revision.as_deref().unwrap()).is_err());
        let t = voice_spend::confirm_unbounded(d.path(), op, model, input, &q.request_hash, q.ledger_revision.as_deref().unwrap()).unwrap();
        assert_eq!(voice_spend::run(d.path(), op, model, input, Some(&t), || Ok("fixture result".into())).unwrap(), "fixture result");
        assert!(voice_spend::run(d.path(), op, model, input, Some(&t), || panic!("voice permission replay")).is_err());
    }
}
