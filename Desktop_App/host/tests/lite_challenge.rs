//! Fixture acceptance for the Lite_Challenge production, through Phase 7.
use workbench_host::workflow::report::project_report;
use workbench_host::workflow::state::ProjectState;

const WORKFLOW: &str =
    "/home/kris/Content_Creation/Projects/Lite_Challenge/Preproduction/workflow_state.json";

#[test]
fn lite_challenge_scene_one_follows_the_recipe() {
    let text = std::fs::read_to_string(WORKFLOW)
        .unwrap_or_else(|error| panic!("Lite_Challenge workflow at {WORKFLOW}: {error}"));
    let state = ProjectState::from_json(&text).unwrap();
    assert_eq!(state.version, 2);
    assert_eq!(state.project_name.as_deref(), Some("Lite_Challenge"));
    let report = project_report(&state);
    let scene = report
        .scenes
        .iter()
        .find(|scene| scene.id == 1)
        .expect("scene 1");
    assert_eq!(scene.title, "Scene 1 — The last train");
    let recipe_ids: Vec<_> = report
        .recipe
        .stages
        .iter()
        .map(|stage| stage.id.as_str())
        .collect();
    let nav_ids: Vec<_> = scene
        .navigation
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    assert_eq!(nav_ids, recipe_ids);
    assert!(nav_ids.windows(2).all(|pair| {
        let left = report
            .recipe
            .stages
            .iter()
            .position(|stage| stage.id == pair[0]);
        let right = report
            .recipe
            .stages
            .iter()
            .position(|stage| stage.id == pair[1]);
        left < right
    }));
    for item in &scene.navigation {
        let stage = report
            .recipe
            .stages
            .iter()
            .find(|stage| stage.id == item.id)
            .unwrap();
        assert_eq!(item.work_area, stage.work_area);
        if item.available {
            assert!(item.disabled_reason.is_none());
        } else {
            let reason = item.disabled_reason.as_deref().unwrap_or("");
            assert!(reason.starts_with("Waiting on "), "{reason}");
        }
    }
    assert!(nav_ids.contains(&"characters"));
    assert!(report.recipe.stages.iter().any(|stage| stage
        .required_roles
        .iter()
        .any(|role| role == "mouth_policy")));
}
