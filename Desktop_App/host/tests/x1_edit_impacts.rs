use std::fs;
use serde_json::json;
use workbench_host::workflow::{import::{import_project, persist_state, ImportOptions}, state::{AnimaticShot, ArtifactVersion, AssemblyShotSnapshot, AssetStatus, DependencyState, JobRecord, JobScope, JobStatus, ProjectState, RenderedSequenceReview, SceneAnimatic, TakeRecord, TakeStatus, STATE_REL}};
use workbench_host::scene_plan::handoff::CutId;

fn fixture(root: &std::path::Path) -> ProjectState {
    let mut state = ProjectState::new();
    for n in 1..=2 {
        let scene = state.scene_mut(n, "Scene");
        scene.stable_id = format!("scene_{n}");
        scene.source_path = Some(format!("Scripts/{n}.md"));
        let cut = scene.cut_mut(1);
        cut.stable_id = format!("shot_{n}");
        cut.selected_take = Some(format!("take_{n}"));
        cut.takes.push(TakeRecord { id:format!("take_{n}"), path:format!("Media/{n}.mp4"), status:TakeStatus::Selected, notes:None });
        cut.selected_performance_asset_id = Some(format!("wav_{n}"));
        cut.approved_start_frame = Some(format!("Media/{n}.png"));
        scene.animatic = Some(SceneAnimatic { id:format!("animatic_{n}"), approved:true, warnings:vec![], shots:vec![AnimaticShot { id:format!("shot_{n}"), cut_id:CutId::new(n,1), duration_seconds:2.0, auto_duration_seed_seconds:None, scratch_audio_path:None, trim_in_seconds:None, trim_out_seconds:None, trim_source_take_id:Some(format!("take_{n}")), transition:None }] });
        let mut pending = JobRecord::open(format!("pending_{n}"), "render");
        pending.scope = Some(JobScope { kind:"shot".into(), id:format!("shot_{n}") });
        state.jobs.push(pending);
    }
    let mut master = JobRecord::open("master_1", "assemble");
    master.scope = Some(JobScope {kind:"project".into(), id:"project".into()});
    master.raw_provider_id = Some("project".into());
    master.output_path = Some("Final/master.mp4".into());
    master.finish(Some("Final/master.mp4"), None);
    master.assembly_snapshot = Some((1..=2).map(|n| AssemblyShotSnapshot { shot_id:format!("shot_{n}"), cut_label:1, take_id:format!("take_{n}"), path:format!("Media/{n}.mp4"), start_seconds:0.0, duration_seconds:2.0, transition:"cut".into() }).collect());
    state.jobs.push(master);
    let mut prepared = JobRecord::open("prepared_project", "assemble");
    prepared.scope = Some(JobScope {kind:"project".into(), id:"project".into()});
    state.jobs.push(prepared);
    state.rendered_review = Some(RenderedSequenceReview {job_id:"master_1".into()});
    state.versions.push(ArtifactVersion { id:"ver_1".into(), asset_id:"take_1".into(), version:1, path:"Media/1.mp4".into(), status:AssetStatus::Approved });
    fs::create_dir_all(root.join("Media")).unwrap();
    fs::write(root.join("Media/1.mp4"), b"paid-original").unwrap();
    persist_state(root, &state).unwrap();
    read(root)
}
fn read(root:&std::path::Path) -> ProjectState {
    ProjectState::from_json(&fs::read_to_string(root.join(STATE_REL)).unwrap()).unwrap()
}
#[test]
fn four_edits_in_separate_projects_invalidate_only_their_edges() {
    for edit in ["script", "image", "audio", "video"] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = fixture(root);
        assert!(state.rendered_review.is_some(), "fixture review must be current: {edit}");
        assert_eq!(state.jobs.iter().find(|j|j.id=="pending_2").unwrap().status, JobStatus::Pending);
        match edit {
            "script" => state.scene_mut(1, "Scene").action.push("Changed cue".into()),
            "image" => state.scene_mut(1, "Scene").cut_mut(1).approved_start_frame = Some("Media/new.png".into()),
            "audio" => state.scene_mut(1, "Scene").cut_mut(1).selected_performance_asset_id = Some("wav_new".into()),
            "video" => state.scene_mut(1, "Scene").animatic.as_mut().unwrap().shots[0].trim_in_seconds = Some(0.25),
            _ => unreachable!(),
        }
        persist_state(root, &state).unwrap();
        let changed = read(root);
        assert_eq!(changed.scenes[0].cuts[0].dependency_state, Some(DependencyState::NeedsReview), "{edit}");
        assert!(changed.scenes[0].cuts[0].package_needs_review, "{edit}");
        assert_eq!(changed.scenes[1].cuts[0].dependency_state, None, "{edit}");
        assert!(changed.scenes[1].animatic.as_ref().unwrap().approved, "sibling approval: {edit}");
        assert!(!changed.scenes[1].cuts[0].package_needs_review, "{edit}");
        assert_eq!(changed.jobs.iter().find(|j|j.id=="pending_1").unwrap().status, JobStatus::Cancelled, "{edit}");
        assert_eq!(changed.jobs.iter().find(|j|j.id=="pending_2").unwrap().status, JobStatus::Pending, "{edit}");
        assert_eq!(changed.jobs.iter().find(|j|j.id=="prepared_project").unwrap().status, JobStatus::Cancelled, "{edit}");
        assert_eq!(changed.jobs.iter().find(|j|j.id=="master_1").unwrap().status, JobStatus::Completed, "{edit}");
        assert!(changed.rendered_review.is_none(), "{edit}");
        assert!(changed.change_impacts.iter().any(|edge|edge.source.starts_with(edit) && edge.dependent=="shot:shot_1" && edge.disposition=="needs_review"), "{edit}");
        assert!(changed.change_impacts.iter().any(|edge|edge.dependent=="master:master_1" && edge.disposition=="invalidated"), "{edit}");
        assert!(workbench_host::workflow::cmd_approve_rendered_sequence(&json!({"projectRoot":root,"jobId":"master_1"})).is_err(), "old master cannot be reapproved: {edit}");
        assert!(!changed.change_impacts.iter().any(|edge|edge.dependent=="shot:shot_2" || edge.dependent=="job:pending_2"), "{edit}");
        assert_eq!(changed.versions, state.versions, "{edit}");
        assert_eq!(fs::read(root.join("Media/1.mp4")).unwrap(), b"paid-original", "{edit}");
        persist_state(root, &changed).unwrap();
        assert_eq!(read(root).change_impacts, changed.change_impacts, "no duplicate edges on reopen: {edit}");
    }
}

#[test]
fn reimport_changed_script_reopens_only_its_scene_and_retains_history() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("Scripts")).unwrap();
    let source = root.join("Scripts/one.md");
    fs::write(&source, "Scene 1 — Room\n- **Dialogue:**\n  - **MARA:** Original cue.\n\nScene 2 — Hall\n- **Dialogue:**\n  - **MARA:** Unchanged.").unwrap();
    let mut state = import_project(root, ImportOptions {persist:true}).unwrap().state;
    state.scene_mut(1, "Room").cut_mut(1);
    state.scene_mut(2, "Hall").cut_mut(1);
    workbench_host::workflow::identity::assign(&mut state);
    let shot = state.scene(1).unwrap().cuts[0].stable_id.clone();
    let sibling = state.scene(2).unwrap().cuts[0].stable_id.clone();
    let mut pending = JobRecord::open("script_pending", "render");
    pending.scope = Some(JobScope {kind:"shot".into(),id:shot.clone()});
    state.jobs.push(pending);
    state.versions.push(ArtifactVersion {id:"ver_7".into(),asset_id:"asset_7".into(),version:7,path:"Media/paid.mp4".into(),status:AssetStatus::Approved});
    fs::create_dir_all(root.join("Media")).unwrap();
    fs::write(root.join("Media/paid.mp4"), b"retained").unwrap();
    persist_state(root, &state).unwrap();
    fs::write(&source, "Scene 1 — Room\n- **Dialogue:**\n  - **MARA:** Revised cue.\n\nScene 2 — Hall\n- **Dialogue:**\n  - **MARA:** Unchanged.").unwrap();
    let imported = import_project(root, ImportOptions {persist:true}).unwrap().state;
    assert_eq!(imported.scene(1).unwrap().cuts[0].stable_id, shot);
    assert_eq!(imported.scene(2).unwrap().cuts[0].stable_id, sibling);
    assert_eq!(imported.scene(1).unwrap().cuts[0].dependency_state, Some(DependencyState::NeedsReview));
    assert_eq!(imported.scene(2).unwrap().cuts[0].dependency_state, None);
    assert_eq!(imported.jobs.iter().find(|j|j.id=="script_pending").unwrap().status, JobStatus::Cancelled);
    assert_eq!(imported.versions, state.versions);
    assert_eq!(fs::read(root.join("Media/paid.mp4")).unwrap(), b"retained");
    assert!(imported.change_impacts.iter().any(|edge|edge.source=="script:Scripts/one.md" && edge.dependent==format!("shot:{shot}")));
    let reopened = import_project(root, ImportOptions {persist:true}).unwrap().state;
    assert_eq!(reopened.change_impacts, imported.change_impacts);
    assert_eq!(reopened.jobs.iter().find(|j|j.id=="script_pending").unwrap().status, JobStatus::Cancelled);
}
