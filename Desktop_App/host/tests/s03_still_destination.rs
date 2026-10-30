//! Real production-library destination checks; no cfg(test) authorization bypass.
use serde_json::json;
use std::{fs, os::unix::fs::symlink};
use workbench_host::{project_path::PathIntent, scene_plan::handoff::CutId, session};
use workbench_host::workflow::{self, import, state::{CutRecord, ProjectState}};

#[test]
fn still_destinations_reject_symlink_buckets() {
    for case in ["directory", "file", "other_scene"] {
        let tmp=tempfile::tempdir().unwrap();
        let root=tmp.path().join("project");fs::create_dir(&root).unwrap();
        let outside=tmp.path().join("outside");fs::create_dir(&outside).unwrap();
        let marker=outside.join("marker.png");fs::write(&marker,b"untouched").unwrap();
        let source=tmp.path().join("source.png");fs::write(&source,b"RIFF-s02-destination").unwrap();
        let mut state=ProjectState::new();
        state.scene_mut(1,"S02").cuts.push(CutRecord::new(CutId::new(1,1)));
        
        import::save_project(&root,state).unwrap();
        session::open(root.to_str().unwrap()).unwrap();
        import::load_state(&root).unwrap();
        let state_path=root.join("Preproduction/workflow_state.json");
        let before=fs::read(&state_path).unwrap();
        let bucket=root.join("Preproduction/Scene1/start_frames");
        fs::create_dir_all(bucket.parent().unwrap()).unwrap();
        let other=root.join("Preproduction/Scene2/start_frames");
        match case {
            "directory" => symlink(&outside,&bucket).unwrap(),
            "file" => {fs::create_dir(&bucket).unwrap();symlink(&marker,bucket.join("cut_1_v1.png")).unwrap();},
            "other_scene" => {fs::create_dir_all(&other).unwrap();symlink(&other,&bucket).unwrap();},
            _ => (),
        }
        session::grant(source.to_str().unwrap(),PathIntent::Read).unwrap();
        let args=json!({"projectRoot":root,"sceneId":1,"cut":1,"sourcePath":source});
        let result=workflow::cmd_attach_start_frame(&args);
        assert!(result.is_err(),"{case}: unsafe destination accepted: {result:?}");
        assert_eq!(fs::read(&marker).unwrap(),b"untouched","{case}: outside bytes changed");
        assert_eq!(fs::read_dir(&outside).unwrap().count(),1,"{case}: outside file created");
        if other.exists(){assert_eq!(fs::read_dir(&other).unwrap().count(),0,"wrong scene bucket written");}
        assert_eq!(fs::read(&state_path).unwrap(),before,"{case}: persisted state changed");
        assert_eq!(fs::read(&source).unwrap(),b"RIFF-s02-destination");
        session::close();
    }
}
