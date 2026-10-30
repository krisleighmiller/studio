//! Production-library continuity writes: real media, session/root and saved bytes.
use serde_json::json;
use std::{fs, os::unix::fs::symlink, process::Command};
use workbench_host::{scene_plan::handoff::CutId, session};
use workbench_host::workflow::{self, import, sources, state::{CutRecord, ProjectState, TakeRecord, TakeStatus, VisualSourceStrategy}};
#[test]
fn continuity_destinations_reject_redirected_buckets() {
 for case in ["outside_directory", "other_scene", "dangling_file"] {
  let tmp=tempfile::tempdir().unwrap();let root=tmp.path().join("project");fs::create_dir(&root).unwrap();
  let outside=tmp.path().join("outside");fs::create_dir(&outside).unwrap();fs::write(outside.join("marker"),b"untouched").unwrap();
  let video=root.join("take.mp4");let output=Command::new("ffmpeg").args(["-v","error","-f","lavfi","-i","color=c=blue:s=160x90:d=1","-threads","1","-pix_fmt","yuv420p",video.to_str().unwrap()]).output().expect("ffmpeg required for this acceptance test");assert!(output.status.success(),"{:?}",output);
  let mut state=ProjectState::new();let mut first=CutRecord::new(CutId::new(1,1));first.takes.push(TakeRecord{id:"s03-first".into(),path:"take.mp4".into(),status:TakeStatus::Selected,notes:None});first.selected_take=Some("s03-first".into());state.scene_mut(1,"S03").cuts.push(first);state.scene_mut(1,"S03").cuts.push(CutRecord::new(CutId::new(1,2)));
  sources::set_strategy(&mut state,1,2,VisualSourceStrategy::PreviousSelectedTakeExitFrame,Some(CutId::new(1,1)),None).unwrap();import::save_project(&root,state).unwrap();session::open(root.to_str().unwrap()).unwrap();import::load_state(&root).unwrap();let sp=root.join("Preproduction/workflow_state.json");let before=fs::read(&sp).unwrap();let original_video=fs::read(&video).unwrap();
  let bucket=root.join("Preproduction/Scene1/continuity_frames");fs::create_dir_all(bucket.parent().unwrap()).unwrap();let other=root.join("Preproduction/Scene2/continuity_frames");
  match case {"outside_directory"=>symlink(&outside,&bucket).unwrap(),"other_scene"=>{fs::create_dir_all(&other).unwrap();symlink(&other,&bucket).unwrap();},_=>{fs::create_dir(&bucket).unwrap();symlink(outside.join("new.png"),bucket.join("cut_2_from_cut_1_v1.png")).unwrap();}}
  let result=workflow::cmd_bind_continuity_frame(&json!({"projectRoot":root,"sceneId":1,"cut":2,"frameTimeSeconds":0.4}));assert!(result.is_err(),"{case}: redirected output accepted: {result:?}");
  assert_eq!(fs::read(&sp).unwrap(),before,"{case}: state changed");assert_eq!(fs::read(&video).unwrap(),original_video);assert_eq!(fs::read(outside.join("marker")).unwrap(),b"untouched");assert_eq!(fs::read_dir(&outside).unwrap().count(),1,"{case}: outside file created");if other.exists(){assert_eq!(fs::read_dir(&other).unwrap().count(),0,"other scene written");}session::close();
 }
}
