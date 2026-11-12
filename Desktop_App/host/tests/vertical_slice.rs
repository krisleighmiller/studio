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
    cmd_bind_continuity_frame, cmd_set_take_status, cmd_set_visual_source,
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
fn selecting_talking_take_reconciles_measured_audio_without_changing_source() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let _session = workbench_host::session::open(root.to_str().unwrap()).unwrap();
    let _close = SessionGuard;
    let cut_dir = root.join("Scenes/Scene2/Cut1");
    fs::create_dir_all(&cut_dir).unwrap();
    let clip = cut_dir.join("S02C01.mp4");
    let video = Command::new("ffmpeg").args(["-y", "-f", "lavfi", "-i", "color=c=black:s=16x16:r=25", "-t", "10", clip.to_str().unwrap()]).output().unwrap();
    assert!(video.status.success(), "{}", String::from_utf8_lossy(&video.stderr));
    for (name, seconds) in [("S02C01.wav", "5.746"), ("S02C01_pad_h0.5_t0.5.wav", "6.896")] {
        let output = Command::new("ffmpeg").args(["-y", "-f", "lavfi", "-i", "anullsrc=r=44100:cl=mono", "-t", seconds, cut_dir.join(name).to_str().unwrap()]).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    }
    let mut state = import_project(root, ImportOptions { persist: true }).unwrap().state;
    let cut = state.scene_mut(2, "Scene 2").cut_mut(1);
    cut.speech_mode = Some(workbench_host::scene_plan::handoff::SpeechMode::SourceVideoLipsync);
    cut.sketch = Some(workbench_host::workflow::state::CutSketch { pace: Some(workbench_host::scene_plan::handoff::Pace::Held), ..Default::default() });
    cut.dialogue.push(workbench_host::script::DialogueLine { speaker: "Kris".into(), text: "Nope. It followed the same life cycle as every other system before it.".into() });
    state.scene_mut(2, "Scene 2").animatic = None; // Seed after the scripted dialogue exists.
    workbench_host::workflow::animatic::sync(&mut state);
    let seed = state.scene(2).unwrap().animatic.as_ref().unwrap().shots[0].duration_seconds;
    assert_eq!(seed, 5.73);
    let take_id = state.scene(2).unwrap().cuts[0].takes[0].id.clone();
    workbench_host::workflow::import::persist_state(root, &state).unwrap();
    cmd_set_take_status(&project_args(root, json!({ "sceneId": 2, "cut": 1, "takeId": take_id, "status": "selected" }))).unwrap();
    let saved = load_state(root).unwrap();
    let shot = &saved.scene(2).unwrap().animatic.as_ref().unwrap().shots[0];
    assert!((shot.duration_seconds - 7.4).abs() < 0.03, "measured speech + lead + deliberate beat: {}", shot.duration_seconds);
    assert!(clip.is_file(), "raw paid source must stay put");
}

#[test]
fn measured_wav_does_not_override_deliberate_screen_duration() {
    let dir = tempfile::tempdir().unwrap(); let root = dir.path();
    let _session = workbench_host::session::open(root.to_str().unwrap()).unwrap(); let _close = SessionGuard;
    let cut_dir = root.join("Scenes/Scene2/Cut1"); fs::create_dir_all(&cut_dir).unwrap();
    for (file, input, seconds) in [("S02C01.mp4","color=c=black:s=16x16:r=25","10"),("S02C01.wav","anullsrc=r=44100:cl=mono","5.746"),("S02C01_pad_h0.5_t0.5.wav","anullsrc=r=44100:cl=mono","6.896")] {
        let output=Command::new("ffmpeg").args(["-y","-v","error","-f","lavfi","-i",input,"-t",seconds,cut_dir.join(file).to_str().unwrap()]).output().unwrap();
        assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
    }
    let mut state=import_project(root,ImportOptions{persist:true}).unwrap().state;
    let cut=state.scene_mut(2,"Scene 2").cut_mut(1);
    cut.speech_mode=Some(workbench_host::scene_plan::handoff::SpeechMode::SourceVideoLipsync);
    cut.sketch=Some(workbench_host::workflow::state::CutSketch{pace:Some(workbench_host::scene_plan::handoff::Pace::Held),..Default::default()});
    cut.dialogue.push(workbench_host::script::DialogueLine{speaker:"Test".into(),text:"Nope. It followed the same life cycle as every other system before it.".into()});
    state.scene_mut(2,"Scene 2").animatic=None;
    workbench_host::workflow::animatic::sync(&mut state);
    let shot=state.scene(2).unwrap().animatic.as_ref().unwrap().shots[0].clone();
    assert_eq!(shot.duration_seconds,5.73);
    assert_eq!(shot.auto_duration_seed_seconds,Some(5.73));
    let take=state.scene(2).unwrap().cuts[0].takes[0].id.clone();
    workbench_host::workflow::import::persist_state(root,&state).unwrap();
    workbench_host::workflow::cmd_set_animatic_duration(&project_args(root,json!({"sceneId":2,"shotId":shot.id,"durationSeconds":8.25}))).unwrap();
    let manual=load_state(root).unwrap();
    assert_eq!(manual.scene(2).unwrap().animatic.as_ref().unwrap().shots[0].auto_duration_seed_seconds,None);
    cmd_set_take_status(&project_args(root,json!({"sceneId":2,"cut":1,"takeId":take,"status":"selected"}))).unwrap();
    let saved=load_state(root).unwrap();
    assert_eq!(saved.scene(2).unwrap().animatic.as_ref().unwrap().shots[0].duration_seconds,8.25);
    assert!(cut_dir.join("S02C01.wav").is_file());
    println!("P2 synthetic decodable shot WAV measured, deliberate 8.25s manual screen duration retained; no transcript-completeness claim");
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
    let first_take = scene.cuts.iter().find(|cut| cut.id.cut == 1).unwrap().takes[0]
        .id
        .clone();
    let second_take = scene.cuts.iter().find(|cut| cut.id.cut == 2).unwrap().takes[0]
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
    assert!(planned["note"]
        .as_str()
        .unwrap()
        .contains("Bind a reviewed continuity frame"));
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
    assert!(bound
        .versions
        .iter()
        .any(|version| version.asset_id == frame.id));
    assert!(matches!(
        resolve_render_source(&bound, CutId::new(1, 2)).unwrap(),
        RenderSourceResolution::Bound(_)
    ));

    for shot in load_state(root).unwrap().scene(1).unwrap().animatic.as_ref().unwrap().shots.clone() {
        workbench_host::workflow::cmd_set_animatic_duration(&project_args(root,json!({"sceneId":1,"shotId":shot.id,"durationSeconds":2.0}))).unwrap();
    }
    let prepared = workbench_host::workflow::cmd_prepare_assemble_with_host(&workbench_host::ctx::HostCtx::new(dir.path().join("settings")), &project_args(root, json!({ "sceneId": 1 }))).unwrap();
    assert!(prepared["pendingAction"]
        .as_str()
        .unwrap()
        .contains("Assemble Scene 1"));
    let pending = load_state(root).unwrap();
    let job = pending
        .jobs
        .iter()
        .find(|job| job.kind == "assemble")
        .unwrap();
    assert_eq!(
        job.status,
        workbench_host::workflow::state::JobStatus::Pending
    );
    let output = job.output_path.clone().unwrap();
    assert_eq!(output, "Final/Scene1_master.mp4");
    // Minimum job metadata: the assemble job records creation and scene scope.
    assert!(job.created_at.is_some());
    assert_eq!(
        job.scope.as_ref().map(|scope| scope.kind.as_str()),
        Some("scene")
    );
    let token=prepared["pendingAction"].as_str().unwrap().split(workbench_host::pending::APPROVE_TOKEN_PREFIX).nth(1).unwrap().split("]]").next().unwrap().to_string();
    let result=workbench_host::director::director_approve(workbench_host::ctx::HostCtx::new(dir.path().join("settings")),token).unwrap();
    assert!(result.tool_events.iter().any(|e|e=="approve:assemble"),"{:?}",result);
    let finished_state = workbench_host::workflow::import_project(root, workbench_host::workflow::ImportOptions { persist: true }).unwrap();
    let scene = finished_state.state.scene(1).unwrap();
    assert_eq!(scene.stages.assemble, StageStatus::Present);
    let version = &scene.post.as_ref().unwrap().versions[0];
    assert_eq!(version.assembly_path.as_deref(), Some(output.as_str()));
    let finished = finished_state
        .state
        .jobs
        .iter()
        .find(|job| job.kind == "assemble")
        .unwrap();
    assert_eq!(
        finished.status,
        workbench_host::workflow::state::JobStatus::Completed
    );
    assert_eq!(finished.output_path.as_deref(), Some(output.as_str()));
    for cut in &scene.cuts {
        let take = &cut.takes[0];
        assert_eq!(take.status, TakeStatus::UsedInAssembly);
        assert_eq!(cut.selected_take.as_deref(), Some(take.id.as_str()));
    }
    let assemble = finished_state
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

    let again = workbench_host::workflow::cmd_prepare_assemble_with_host(&workbench_host::ctx::HostCtx::new(dir.path().join("settings")), &project_args(root, json!({ "sceneId": 1 }))).unwrap();
    assert!(
        again["pendingAction"]
            .as_str()
            .unwrap()
            .contains("Final/Scene1_master_v2.mp4"),
        "{}",
        again["pendingAction"]
    );
    let second = "Final/Scene1_master_v2.mp4";
    let token=again["pendingAction"].as_str().unwrap().split(workbench_host::pending::APPROVE_TOKEN_PREFIX).nth(1).unwrap().split("]]").next().unwrap().to_string();
    let second_result=workbench_host::director::director_approve(workbench_host::ctx::HostCtx::new(dir.path().join("settings")),token).unwrap();
    assert!(second_result.tool_events.iter().any(|e|e=="approve:assemble"),"{:?}",second_result);
    let reassembled = workbench_host::workflow::import_project(root, workbench_host::workflow::ImportOptions { persist: true }).unwrap();
    let versions = &reassembled
        .state
        .scene(1)
        .unwrap()
        .post
        .as_ref()
        .unwrap()
        .versions;
    assert_eq!(versions.len(), 2);
    assert_eq!(
        versions[0].assembly_path.as_deref(),
        Some("Final/Scene1_master.mp4")
    );
    assert_eq!(versions[1].assembly_path.as_deref(), Some(second));
    assert_ne!(versions[0].id, versions[1].id);
    assert_eq!(
        reassembled.state.scene(1).unwrap().stages.assemble,
        StageStatus::Present
    );
}
