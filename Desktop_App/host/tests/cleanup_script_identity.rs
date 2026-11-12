//! FX02: real retained reviewer files through native parsing/import/persistence.
use std::fs;
use workbench_host::workflow::{import::{import_project, persist_state, ImportOptions}, state::ProjectState};

const REVIEWER: &str = include_str!("support/fx02/reviewer.md");
const FINAL: &str = include_str!("support/fx02/final.md");
const SAVED: &str = include_str!("support/fx02/workflow_state.json");

fn scan(root: &std::path::Path) -> ProjectState {
    import_project(root, ImportOptions { persist: true }).unwrap().state
}
fn ownership(state: &ProjectState) -> Vec<(u32, String, Option<String>, String)> {
    state.scenes.iter().map(|s| (s.id, s.stable_id.clone(), s.source_path.clone(), s.source_excerpt.clone().unwrap_or_default())).collect()
}
fn assert_sources(state: &ProjectState) {
    assert_eq!(state.scenes.len(), 4, "both real scripts must own their Scene 1/2 records");
    for (path, text) in [("Scripts/Reviewer saved.md", REVIEWER), ("Scripts/Final authored dialogue.md", FINAL)] {
        let scenes: Vec<_> = state.scenes.iter().filter(|s| s.source_path.as_deref() == Some(path)).collect();
        let parsed = workbench_host::script::extract_scenes(text);
        assert_eq!(scenes.len(), parsed.len(), "source {path}");
        for (scene, expected) in scenes.iter().zip(parsed) {
            assert_eq!(scene.source_scene_number, Some(expected.id));
            assert_eq!(scene.source_excerpt.as_deref(), Some(expected.source_text.as_str()));
            assert_eq!(scene.dialogue, expected.dialogue);
            assert_eq!(scene.beats, expected.beats);
        }
    }
}
#[test]
fn fx02_retained_failed_second_script_recovers_without_reassigning_first() {
    let dir = tempfile::tempdir().unwrap(); let root = dir.path();
    fs::create_dir(root.join("Scripts")).unwrap();
    fs::create_dir(root.join("Preproduction")).unwrap();
    fs::write(root.join("Scripts/Reviewer saved.md"), REVIEWER).unwrap();
    fs::write(root.join("Scripts/Final authored dialogue.md"), FINAL).unwrap();
    fs::write(root.join("Preproduction/workflow_state.json"), SAVED).unwrap();
    let old = ProjectState::from_json(SAVED).unwrap();
    let state = scan(root);
    assert_sources(&state);
    if let Ok(path) = std::env::var("FX02_CAPTURE") {
        let result = serde_json::json!({"state": state, "report": workbench_host::workflow::report::project_report(&state)});
        fs::write(path, serde_json::to_string_pretty(&result).unwrap()).unwrap();
    }
    for original in old.scenes {
        let kept = state.scene(original.id).unwrap();
        assert_eq!(kept.source_path, original.source_path);
        assert_eq!(kept.stable_id, original.stable_id);
        assert_eq!(kept.dialogue, original.dialogue);
    }
    let expected = ownership(&state);
    for _ in 0..3 { assert_eq!(ownership(&scan(root)), expected); }
    assert_eq!(fs::read_to_string(root.join("Scripts/Reviewer saved.md")).unwrap(), REVIEWER);
    assert_eq!(fs::read_to_string(root.join("Scripts/Final authored dialogue.md")).unwrap(), FINAL);
}
#[test]
fn fx02_lower_sorting_source_preserves_existing_production_and_reload_identity() {
    let dir = tempfile::tempdir().unwrap(); let root = dir.path();
    fs::create_dir(root.join("Scripts")).unwrap();
    fs::write(root.join("Scripts/Reviewer saved.md"), REVIEWER).unwrap();
    fs::create_dir_all(root.join("Scene_Prompts/Scene1")).unwrap();
    fs::write(root.join("Scene_Prompts/Scene1/Cut1.txt"),
        workbench_host::scene_plan::fixtures::cut_markdown(1, 1,
            &workbench_host::scene_plan::fixtures::broll_cut_yaml(1, 1))).unwrap();
    let mut first = scan(root);
    assert_eq!(first.scenes.iter().map(|s| s.id).collect::<Vec<_>>(), vec![1,2]);
    first.scene_mut(1, "").intent_approved = true;
    workbench_host::workflow::identity::add_alternate_shot(&mut first, 1).unwrap();
    persist_state(root, &first).unwrap();
    let original = first.scene(1).unwrap().clone();
    fs::write(root.join("Scripts/Final authored dialogue.md"), FINAL).unwrap();
    let state = scan(root);
    assert_sources(&state);
    let kept = state.scene(1).unwrap();
    assert_eq!(kept.source_path, original.source_path);
    assert_eq!(kept.cuts, original.cuts);
    assert!(kept.intent_approved);
    let expected = ownership(&state);
    for _ in 0..3 { assert_eq!(ownership(&scan(root)), expected); }
    // Temporarily absent source must not free or erase its persisted identity.
    fs::rename(root.join("Scripts/Final authored dialogue.md"), root.join("Final.offline")).unwrap();
    assert_eq!(ownership(&scan(root)), expected);
    fs::rename(root.join("Final.offline"), root.join("Scripts/Final authored dialogue.md")).unwrap();
    assert_eq!(ownership(&scan(root)), expected);
}
