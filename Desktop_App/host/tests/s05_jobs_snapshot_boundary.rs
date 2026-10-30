//! Pure Jobs observation must authorize the persisted state file, not just its parent root.
use std::fs;
use serde_json::json;
use workbench_host::{session,workflow::{self,state::{ProjectState,JobRecord,STATE_REL}}};
#[test]
fn jobs_snapshot_refuses_outside_state_symlink_without_readback_or_writes(){
 let t=tempfile::tempdir().unwrap();let root=t.path().join("project");fs::create_dir_all(root.join("Preproduction")).unwrap();let outside=t.path().join("outside-state.json");let mut state=ProjectState::new();state.jobs.push(JobRecord::open("OUTSIDE-DO-NOT-DISCLOSE","render"));let bytes=serde_json::to_vec(&state).unwrap();fs::write(&outside,&bytes).unwrap();std::os::unix::fs::symlink(&outside,root.join(STATE_REL)).unwrap();session::open(root.to_str().unwrap()).unwrap();let result=workflow::cmd_status(&json!({"projectRoot":root,"jobsOnly":true,"refresh":false}));assert!(result.is_err(),"outside state must not be returned through the pure Jobs reader: {result:?}");assert_eq!(fs::read(&outside).unwrap(),bytes);session::close();
}
