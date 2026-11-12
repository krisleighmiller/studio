use workbench_host::workflow::{look, sources, state::*};
use workbench_host::scene_plan::handoff::CutId;

#[test]
fn revised_look_keeps_bound_cuts_and_revokes_prepared_video_until_rebind() {
    let mut state = ProjectState::new();
    let old = state.push_asset(AssetRecord {
        id: String::new(), path: "old.png".into(), role: AssetRole::InteriorReference,
        status: AssetStatus::Approved, scene_id: Some(1), cut_id: None,
        character_id: None, source_job_id: None, parent_asset_id: None, notes: None,
    });
    let new = state.push_asset(AssetRecord {
        id: String::new(), path: "new.png".into(), role: AssetRole::InteriorReference,
        status: AssetStatus::Draft, scene_id: Some(1), cut_id: None,
        character_id: None, source_job_id: None, parent_asset_id: Some(old.clone()), notes: None,
    });
    let other = state.push_asset(AssetRecord {
        id: String::new(), path: "other.png".into(), role: AssetRole::InteriorReference,
        status: AssetStatus::Approved, scene_id: Some(1), cut_id: None,
        character_id: None, source_job_id: None, parent_asset_id: None, notes: None,
    });
    let scene = state.scene_mut(1, "Scene");
    let mut slot = LookSlot::new("look", AssetRole::InteriorReference, "Room");
    slot.asset_ids = vec![old.clone(), new.clone()];
    slot.approved_asset_id = Some(old.clone());
    scene.look_board = Some(LookBoard { slots: vec![slot], ..LookBoard::default() });
    for (number, asset_id) in [(1, &old), (2, &other)] {
        let cut = scene.cut_mut(number);
        cut.visual_source = Some(VisualSourceSpec {
            asset_id: Some(asset_id.clone()), ..VisualSourceSpec::new(VisualSourceStrategy::SharedReference)
        });
        cut.production_note_path = Some(format!("note_{number}.md"));
    }
    let mut pending = JobRecord::open("pending", "render");
    pending.input_asset_ids = vec![old.clone()];
    state.jobs.push(pending);
    let mut unrelated = JobRecord::open("unrelated", "render");
    unrelated.input_asset_ids = vec![other.clone()];
    state.jobs.push(unrelated);
    assert_eq!(sources::resolve_render_source(&state, CutId::new(1, 1)).unwrap(), sources::RenderSourceResolution::Bound("old.png".into()));
    look::set_asset_status(&mut state, 1, "look", Some(&new), AssetStatus::Approved).unwrap();
    let revised = &state.scene(1).unwrap().cuts[0];
    assert_eq!(revised.visual_source.as_ref().unwrap().asset_id.as_deref(), Some(old.as_str()));
    assert_eq!(revised.production_note_path.as_deref(), Some("note_1.md"));
    assert_eq!(revised.dependency_state, Some(DependencyState::NeedsReview));
    assert_eq!(state.jobs[0].status, JobStatus::Failed);
    assert_eq!(state.jobs[1].status, JobStatus::Pending);
    assert!(sources::resolve_render_source(&state, CutId::new(1, 1)).unwrap_err().contains("superseded image"));
    assert!(sources::set_strategy(&mut state, 1, 1, VisualSourceStrategy::SharedReference, None, Some(&old)).is_err());
    assert!(sources::resolve_render_source(&state, CutId::new(1, 1)).is_err());
    sources::set_strategy(&mut state, 1, 1, VisualSourceStrategy::SharedReference, None, Some(&new)).unwrap();
    assert_eq!(state.scene(1).unwrap().cuts[0].dependency_state, None);
    assert_eq!(sources::resolve_render_source(&state, CutId::new(1, 1)).unwrap(), sources::RenderSourceResolution::Bound("new.png".into()));
    assert_eq!(sources::resolve_render_source(&state, CutId::new(1, 2)).unwrap(), sources::RenderSourceResolution::Bound("other.png".into()));
    let dir = tempfile::tempdir().unwrap();
    for path in ["old.png", "new.png", "other.png"] {
        std::fs::write(dir.path().join(path), b"fixture still").unwrap();
    }
    workbench_host::workflow::import::persist_state(dir.path(), &state).unwrap();
    let reopened = workbench_host::workflow::import::load_state(dir.path()).unwrap();
    assert_eq!(reopened.scene(1).unwrap().cuts[0].visual_source.as_ref().unwrap().asset_id.as_deref(), Some(new.as_str()));
    assert_eq!(reopened.jobs[0].status, JobStatus::Failed);
}
