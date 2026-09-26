//! Scene 1 moves from imported clips through Finish using host commands.
//! The test never writes `workflow_state.json` itself.
use std::fs;
use std::process::Command;

use serde_json::json;
use workbench_host::scene_plan::handoff::CutId;
use workbench_host::workflow::import::{import_project, load_state, ImportOptions};
use workbench_host::workflow::sources::{resolve_render_source, RenderSourceResolution};
use workbench_host::workflow::state::{AssetRole, AssetStatus, StageStatus, TakeStatus};
use workbench_host::workflow::{
    cmd_bind_continuity_frame, cmd_prepare_assemble, cmd_set_take_status, cmd_set_visual_source,
};

fn write_clip(path: &std::path::Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=16x16:r=25",
            "-t",
            "2",
            path.to_str().unwrap(),
        ])
        .output()
        .expect("ffmpeg is required for the production slice");
    assert!(
        status.status.success(),
        "ffmpeg could not write a test clip: {}",
        String::from_utf8_lossy(&status.stderr)
    );
}

struct SessionGuard;

impl Drop for SessionGuard {
    fn drop(&mut self) {
        workbench_host::session::close();
    }
}

fn project_args(root: &std::path::Path, mut extra: serde_json::Value) -> serde_json::Value {
    extra["projectRoot"] = json!(root.to_str().unwrap());
    extra
}

#[test]
fn imported_takes_reach_finish_without_hand_edited_workflow_state() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let _session = workbench_host::session::open(root.to_str().unwrap()).unwrap();
    let _close = SessionGuard;
    write_clip(&root.join("Scenes/Scene1/Cut1/take.mp4"));
    write_clip(&root.join("Scenes/Scene1/Cut2/take.mp4"));
    fs::write(root.join("prod.md"), "# Scene 1\n\nLocal assembly map.\n").unwrap();
    assert!(!root.join("Preproduction/workflow_state.json").exists());

    let imported = import_project(root, ImportOptions { persist: true }).unwrap();
    assert!(root.join("Preproduction/workflow_state.json").is_file());
    let scene = imported.state.scene(1).unwrap();
    assert_eq!(scene.cuts.len(), 2);
    assert!(scene.cuts.iter().all(|cut| cut.takes.len() == 1));
    assert!(scene.cuts.iter().all(|cut| cut.selected_take.is_none()));
    assert_eq!(scene.stages.assemble, StageStatus::Missing);
    let first_take = scene
        .cuts
        .iter()
        .find(|cut| cut.id.cut == 1)
        .unwrap()
        .takes[0]
        .id
        .clone();
    let second_take = scene
        .cuts
        .iter()
        .find(|cut| cut.id.cut == 2)
        .unwrap()
        .takes[0]
        .id
        .clone();

    for (cut, take_id) in [(1, &first_take), (2, &second_take)] {
        cmd_set_take_status(&project_args(
            root,
            json!({ "sceneId": 1, "cut": cut, "takeId": take_id, "status": "selected" }),
        ))
        .unwrap();
    }
    let planned = cmd_set_visual_source(&project_args(
        root,
        json!({
            "sceneId": 1,
            "cut": 2,
            "strategy": "previous_selected_take_exit_frame",
            "predecessorCut": 1
        }),
    ))
    .unwrap();
    assert!(planned["note"].as_str().unwrap().contains("Bind a reviewed continuity frame"));
    let blocked = load_state(root).unwrap();
    let successor = blocked
        .scene(1)
        .unwrap()
        .cuts
        .iter()
        .find(|cut| cut.id.cut == 2)
        .unwrap();
    assert!(!successor.visual_source_ready());
    let blocked_render = resolve_render_source(&blocked, CutId::new(1, 2)).unwrap_err();
    assert!(blocked_render.contains("not render ready"));

    cmd_bind_continuity_frame(&project_args(
        root,
        json!({ "sceneId": 1, "cut": 2, "frameTimeSeconds": 0.4 }),
    ))
    .unwrap();
    let bound = load_state(root).unwrap();
    let successor = bound
        .scene(1)
        .unwrap()
        .cuts
        .iter()
        .find(|cut| cut.id.cut == 2)
        .unwrap();
    let source = successor.visual_source.as_ref().unwrap();
    assert!(successor.visual_source_ready());
    assert!((source.frame_time_seconds.unwrap() - 0.4).abs() < 0.05);
    assert_eq!(source.source_take_id.as_deref(), Some(first_take.as_str()));
    let frame = bound.asset(source.asset_id.as_deref().unwrap()).unwrap();
    assert_eq!(frame.role, AssetRole::ContinuityFrame);
    assert_eq!(frame.status, AssetStatus::Approved);
    assert_eq!(frame.parent_asset_id.as_deref(), Some(first_take.as_str()));
    assert!(bound.versions.iter().any(|version| version.asset_id == frame.id));
    assert!(matches!(
        resolve_render_source(&bound, CutId::new(1, 2)).unwrap(),
        RenderSourceResolution::Bound(_)
    ));

    let prepared = cmd_prepare_assemble(&project_args(root, json!({ "sceneId": 1 }))).unwrap();
    assert!(prepared["pendingAction"].as_str().unwrap().contains("Assemble Scene 1"));
    let pending = load_state(root).unwrap();
    let job = pending
        .jobs
        .iter()
        .find(|job| job.kind == "assemble")
        .unwrap();
    assert_eq!(job.status, workbench_host::workflow::state::JobStatus::Pending);
    let output = job.output_path.clone().unwrap();
    assert_eq!(output, "Final/Scene1_master.mp4");
    fs::create_dir_all(root.join("Final")).unwrap();
    fs::write(root.join(&output), b"master").unwrap();

    let refreshed = import_project(root, ImportOptions { persist: true }).unwrap();
    let scene = refreshed.state.scene(1).unwrap();
    assert_eq!(scene.stages.assemble, StageStatus::Present);
    let version = &scene.post.as_ref().unwrap().versions[0];
    assert_eq!(version.assembly_path.as_deref(), Some(output.as_str()));
    let finished = refreshed
        .state
        .jobs
        .iter()
        .find(|job| job.kind == "assemble")
        .unwrap();
    assert_eq!(finished.status, workbench_host::workflow::state::JobStatus::Completed);
    assert_eq!(finished.output_path.as_deref(), Some(output.as_str()));
    for cut in &scene.cuts {
        let take = &cut.takes[0];
        assert_eq!(take.status, TakeStatus::UsedInAssembly);
        assert_eq!(cut.selected_take.as_deref(), Some(take.id.as_str()));
    }
    let assemble = refreshed
        .report
        .scenes
        .iter()
        .find(|item| item.id == 1)
        .unwrap()
        .navigation
        .iter()
        .find(|item| item.id == "assemble")
        .unwrap();
    assert_eq!(assemble.status, "present");
    assert!(assemble.available);

    let again = cmd_prepare_assemble(&project_args(root, json!({ "sceneId": 1 }))).unwrap();
    assert!(
        again["pendingAction"]
            .as_str()
            .unwrap()
            .contains("Final/Scene1_master_v2.mp4"),
        "{}",
        again["pendingAction"]
    );
    let second = "Final/Scene1_master_v2.mp4";
    fs::write(root.join(second), b"master-2").unwrap();
    let reassembled = import_project(root, ImportOptions { persist: true }).unwrap();
    let versions = &reassembled.state.scene(1).unwrap().post.as_ref().unwrap().versions;
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].assembly_path.as_deref(), Some("Final/Scene1_master.mp4"));
    assert_eq!(versions[1].assembly_path.as_deref(), Some(second));
    assert_ne!(versions[0].id, versions[1].id);
    assert_eq!(
        reassembled.state.scene(1).unwrap().stages.assemble,
        StageStatus::Present
    );
}
