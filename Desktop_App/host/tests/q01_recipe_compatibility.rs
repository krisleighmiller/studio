use std::fs;
use std::path::Path;
use workbench_host::workflow::state::{ProjectState, StageStatus, STATE_REL};
use workbench_host::workflow::{import_project, ImportOptions};

const SCENE_SCRIPT: &str = "# Scene 1 — Sunrise study\n\n- **Action:** Blue light becomes gold.\n";

fn fixture(root: &Path, recipe: &str) -> serde_json::Value {
    fs::create_dir_all(root.join("Scripts")).unwrap();
    fs::write(root.join("Scripts/script.md"), SCENE_SCRIPT).unwrap();
    fs::create_dir_all(root.join("Preproduction")).unwrap();
    serde_json::json!({
        "version": 1,
        "project_name": "directory name is authoritative",
        "profile": {"recipe":recipe,"audience":"Quiet viewers","style_notes":"Blue to gold","deliverables":[]},
        "scenes":[{
            "id":1,"title":"Sunrise study","stable_id":"q01-sunrise",
            "intent":{"narrative_purpose":"Keep this exact saved purpose"},"intent_approved":true,
            "stages":{"script":"missing","render":"approved","characters":"approved",
                "extensions":{"color_script":"present","external_review":"approved"}}
        }],
        "notes":["stale scan diagnostic to discard"]
    })
}

fn write_fixture(root: &Path, value: &serde_json::Value) {
    fs::write(root.join(STATE_REL), serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn assert_retained(state: &ProjectState) {
    let profile = state.profile.as_ref().unwrap();
    assert_eq!(profile.audience.as_deref(), Some("Quiet viewers"));
    assert_eq!(profile.style_notes.as_deref(), Some("Blue to gold"));
    let scene = state.scene(1).unwrap();
    assert_eq!(scene.stable_id, "q01-sunrise");
    assert_eq!(scene.intent.as_ref().unwrap().narrative_purpose.as_deref(), Some("Keep this exact saved purpose"));
    assert!(scene.intent_approved);
    assert_eq!(scene.stages.extensions.len(), 2);
    assert_eq!(scene.stages.status_of("color_script"), StageStatus::Present);
    assert_eq!(scene.stages.status_of("external_review"), StageStatus::Approved);
    // Native readiness follows current scanned inputs, never the saved stale map.
    assert_eq!(scene.stages.script, StageStatus::Present);
    assert_eq!(scene.stages.render, StageStatus::Missing);
    assert_eq!(scene.stages.characters, StageStatus::Missing);
    assert!(!state.notes.iter().any(|note| note == "stale scan diagnostic to discard"));
}

#[test]
fn q01_unknown_recipe_warning_and_extension_values_survive_repeated_persisted_import() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("SunriseProject");
    let value = fixture(&root, "opera");
    write_fixture(&root, &value);
    let warning = "Unknown recipe `opera` loaded as narrative dialogue. no recipe document was included.";
    for _ in 0..2 {
        let imported = import_project(&root, ImportOptions { persist: true }).unwrap();
        assert_eq!(imported.state.profile.as_ref().unwrap().recipe.as_str(), "narrative_dialogue");
        assert_eq!(imported.state.project_name.as_deref(), Some("SunriseProject"));
        assert_retained(&imported.state);
        assert_eq!(imported.state.notes.iter().filter(|note| note.as_str() == warning).count(), 1);
        assert_eq!(imported.report.warnings.iter().filter(|note| note.as_str() == warning).count(), 1);
        assert!(imported.state.notes.iter().any(|note| note.contains("no Characters/")), "fresh scan diagnostics remain");
        let disk = ProjectState::from_json(&fs::read_to_string(root.join(STATE_REL)).unwrap()).unwrap();
        assert_retained(&disk);
        assert_eq!(disk.notes.iter().filter(|note| note.as_str() == warning).count(), 1);
    }
}

#[test]
fn q01_invalid_recipe_documents_report_specific_reason_and_retain_creative_state() {
    let cases = [
        (serde_json::json!({"id":"opera","label":"Opera","stages":[{"id":"grade","label":"Grade","work_area":"finish","depends_on":["missing"]}]}), "depends on unknown stage missing"),
        (serde_json::json!({"id":"other","label":"Other","stages":[{"id":"script","label":"Track","work_area":"plan"}]}), "recipe document id does not match the profile"),
        (serde_json::json!({"version":9,"id":"opera","label":"Opera","stages":[{"id":"script","label":"Track","work_area":"plan"}]}), "recipe schema version 9 is not supported"),
    ];
    for (document, reason) in cases {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("InvalidDocument");
        let mut value = fixture(&root, "opera");
        value["profile"]["recipe_document"] = document;
        write_fixture(&root, &value);
        for _ in 0..2 {
            let imported = import_project(&root, ImportOptions { persist: true }).unwrap();
            assert_retained(&imported.state);
            assert_eq!(imported.state.profile.as_ref().unwrap().recipe.as_str(), "narrative_dialogue");
            assert!(imported.state.notes.iter().any(|note| note.contains("Unknown recipe `opera`") && note.contains(reason)));
            assert!(imported.report.warnings.iter().any(|note| note.contains(reason)));
            let disk = ProjectState::from_json(&fs::read_to_string(root.join(STATE_REL)).unwrap()).unwrap();
            assert_retained(&disk);
        }
    }
}

#[test]
fn q01_legacy_alias_is_montage_without_false_unknown_warning() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("AliasProject");
    let value = fixture(&root, "music_video");
    write_fixture(&root, &value);
    for _ in 0..2 {
        let imported = import_project(&root, ImportOptions { persist: true }).unwrap();
        assert_eq!(imported.state.profile.as_ref().unwrap().recipe.as_str(), "montage");
        assert_retained(&imported.state);
        assert!(!imported.report.warnings.iter().any(|note| note.starts_with("Unknown recipe")));
        assert!(!imported.report.recipe.stages.iter().any(|stage| stage.id == "characters"));
    }
}

#[test]
fn q01_missing_recipe_refuses_without_replacing_saved_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("MissingRecipe");
    let mut value = fixture(&root, "montage");
    value["profile"].as_object_mut().unwrap().remove("recipe");
    write_fixture(&root, &value);
    let before = fs::read(root.join(STATE_REL)).unwrap();
    let error = import_project(&root, ImportOptions { persist: true }).unwrap_err();
    assert!(error.contains("missing field `recipe`"), "{error}");
    assert_eq!(fs::read(root.join(STATE_REL)).unwrap(), before);
}

#[test]
fn q01_future_workflow_refuses_without_replacing_saved_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("FutureWorkflow");
    let mut value = fixture(&root, "montage");
    value["version"] = 99.into();
    write_fixture(&root, &value);
    let before = fs::read(root.join(STATE_REL)).unwrap();
    let error = import_project(&root, ImportOptions { persist: true }).unwrap_err();
    assert!(error.contains("workflow state version 99 is not supported"), "{error}");
    assert_eq!(fs::read(root.join(STATE_REL)).unwrap(), before);
}
