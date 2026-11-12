//! LP01 exercises the real dispatcher with fail-closed legacy dependency spies.
use super::*;
use std::cell::Cell;

thread_local! { static FORBID_LEGACY_IO: Cell<bool> = const { Cell::new(false) }; }

pub(super) fn dependency(name: &str) {
    FORBID_LEGACY_IO.with(|forbidden| assert!(!forbidden.get(), "LP01 forbidden legacy dependency: {name}"));
}

struct NoLegacyIo;
impl NoLegacyIo {
    fn new() -> Self {
        FORBID_LEGACY_IO.with(|f| assert!(!f.replace(true)));
        Self
    }
}
impl Drop for NoLegacyIo {
    fn drop(&mut self) { FORBID_LEGACY_IO.with(|f| f.set(false)); }
}

fn action(specialist: &str, inputs: Value) -> ValidatedAction {
    ValidatedAction {
        action: serde_json::from_value(json!({"action":"run_specialist", "specialist":specialist, "inputs":inputs})).unwrap(),
        resolved_paths: vec![],
    }
}
fn api() -> ApiEntry {
    ApiEntry { id:"fake".into(), name:"Fail-closed fake".into(), kind:"openai".into(),
        base_url:"http://127.0.0.1:1".into(), api_key:"fake".into(), secret_key:String::new(), enabled:true }
}

fn retired(name: &str, surface: &str, required_guidance: &[&str]) {
    let dir = tempfile::tempdir().unwrap();
    let app = HostCtx::new(dir.path().join("settings"));
    let api = api();
    let llm = SpecialistLlm { api:&api, api_key:"fake", model:"fake-only", user_question:"Create a new draft" };
    let validated = action(name, json!({"brief":"Create a new draft", "paths":["never-read.md"], "output_path":"never-written.md"}));
    let _guard = NoLegacyIo::new();
    let mut previous = None;
    // Must be independent of both project state and whether a chat model exists.
    for root in [None, dir.path().to_str()] {
        for model in [Some(&llm), None] {
            let result = run_specialist(&app, &DirectorContext::default(), &validated, root, model).unwrap();
            assert!(!result.continue_suggested, "retired actions must terminate");
            let body: Value = serde_json::from_str(&result.message).expect("structured retired result");
            assert_eq!(body["status"], "unsupported");
            assert_eq!(body["reason"], "retired_action");
            assert_eq!(body["specialist"], name);
            assert_eq!(body["recovery"]["surface"], surface);
            let guidance = body["recovery"]["guidance"].as_str().unwrap();
            for text in required_guidance { assert!(guidance.contains(text), "{guidance}"); }
            if let Some(old) = &previous { assert_eq!(old, &result.message); }
            previous = Some(result.message);
        }
    }
    assert!(!dir.path().join("never-written.md").exists());
}

#[test]
fn lp01_script_is_retired_without_contract_file_or_model_access() {
    retired("script", "Script", &["Draft", "Notes", "Save", "local", "not"]);
}
#[test]
fn lp01_character_creation_is_retired_without_contract_file_or_model_access() {
    retired("character_creation", "Characters", &["profile", "reference", "model sheet", "Save"]);
}
#[test]
fn lp01_production_helper_is_retired_without_contract_file_or_model_access() {
    retired("production_helper", "Files / Assets", &["documents", "scene", "unavailable"]);
}

#[test]
fn lp01_scene_plan_still_reaches_host_owned_planning_runner() {
    let dir = tempfile::tempdir().unwrap();
    let app = HostCtx::new(dir.path().join("settings"));
    let api = api();
    let llm = SpecialistLlm { api:&api, api_key:"fake", model:"fake-only", user_question:"scene intent" };
    let validated = action("scene_plan", json!({"brief":"scene intent"}));
    let _guard = NoLegacyIo::new();
    let dispatched = run_specialist(&app, &DirectorContext::default(), &validated, None, Some(&llm)).unwrap_err();
    let direct = run_scene_planning(&app, &DirectorContext::default(), &validated, None, &llm, "scene_plan").unwrap_err();
    assert_eq!(dispatched, direct);
    assert_eq!(dispatched, "Open a project before drafting scene intent or cut sketches.");
}

#[test]
fn lp01_model_sheet_still_reaches_host_owned_manifest_preparation() {
    let dir = tempfile::tempdir().unwrap();
    let app = HostCtx::new(dir.path().join("settings"));
    let validated = action("model_sheet", json!({"output_path":"Characters/Alice/model_sheet", "character_slug":"Alice", "identity_text":"Alice", "reference_images":[]}));
    let _guard = NoLegacyIo::new();
    // No media model configured: the real runner prepares the typed manifest then
    // stops at its existing settings guard, before any provider can be called.
    let dispatched = run_specialist(&app, &DirectorContext::default(), &validated, dir.path().to_str(), None).unwrap_err();
    assert_eq!(dispatched, "Set Settings → Image → Text to image for the model-sheet identity plate.");
    let path = dir.path().join("Characters/Alice/model_sheet/model_sheet_manifest.yaml");
    let manifest: Value = serde_yaml_ng::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(manifest["shots"][0]["status"], "pending");
    assert_eq!(manifest["model_id"], "");
}
