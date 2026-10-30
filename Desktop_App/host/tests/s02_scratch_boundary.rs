//! Production-library boundary regression; no cfg(test) path bypass.
use serde_json::json;
use std::fs;
use workbench_host::project_path::PathIntent;
use workbench_host::session;
use workbench_host::scene_plan::handoff::CutId;
use workbench_host::workflow::{self, import, state::{CutRecord, ProjectState}};

#[test]
fn scratch_source_uses_one_time_read_grant_not_project_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let mut state = ProjectState::new();
    state.scene_mut(1, "S02").cuts.push(CutRecord::new(CutId::new(1,1)));
    workflow::animatic::sync(&mut state);
    let shot = state.scenes[0].animatic.as_ref().unwrap().shots[0].id.clone();
    import::persist_state(&root, &state).unwrap();
    session::open(root.to_str().unwrap()).unwrap();
    let source = temp.path().join("scratch.wav");
    // Minimal local bytes prove exact copy/permission; no audio-playback claim.
    let bytes = b"RIFF-local-S02-boundary-fixture";
    fs::write(&source, bytes).unwrap();
    let args = json!({"projectRoot":root,"sceneId":1,"shotId":shot,"sourcePath":source});
    let state_path=root.join("Preproduction/workflow_state.json");
    let before=fs::read(&state_path).unwrap();
    assert!(workflow::cmd_attach_scratch_audio(&args).is_err(), "ungranted external source denied");
    assert_eq!(fs::read(&state_path).unwrap(),before);
    session::grant(source.to_str().unwrap(),PathIntent::Read).unwrap();
    workflow::cmd_attach_scratch_audio(&args).expect("dialog-granted audio source must not be treated as projectRoot");
    let loaded=import::load_state(&root).unwrap();
    let rel=loaded.scenes[0].animatic.as_ref().unwrap().shots[0].scratch_audio_path.as_ref().unwrap();
    let dest=root.join(rel);assert_eq!(fs::read(&dest).unwrap(),bytes);
    let attached=fs::read(&state_path).unwrap();
    assert!(workflow::cmd_attach_scratch_audio(&args).is_err(),"external grant consumed exactly once");
    assert_eq!(fs::read(&state_path).unwrap(),attached);
    let other=temp.path().join("other");fs::create_dir(&other).unwrap();
    session::grant(source.to_str().unwrap(),PathIntent::Read).unwrap();
    let mut wrong=args.clone();wrong["projectRoot"]=json!(other);
    assert!(workflow::cmd_attach_scratch_audio(&wrong).unwrap_err().contains("not the open project"));
    assert_eq!(fs::read(&state_path).unwrap(),attached);
    // Root rejection must not consume the read grant.
    workflow::cmd_attach_scratch_audio(&args).unwrap();
    let internal=root.join("inside.wav");fs::write(&internal,bytes).unwrap();
    let mut inside=args.clone();inside["sourcePath"]=json!(internal);
    workflow::cmd_attach_scratch_audio(&inside).unwrap();
    let clear=json!({"projectRoot":root,"sceneId":1,"shotId":shot});
    workflow::cmd_clear_scratch_audio(&clear).unwrap();
    assert!(import::load_state(&root).unwrap().scenes[0].animatic.as_ref().unwrap().shots[0].scratch_audio_path.is_none());
    assert!(!dest.exists());assert_eq!(fs::read(&source).unwrap(),bytes);
    session::close();
}
