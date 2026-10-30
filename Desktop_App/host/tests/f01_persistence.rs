//! F01: exercise existing production persistence seams on disposable fixtures.
use std::{fs, sync::{Arc, Barrier}};
use workbench_host::workflow::{import, state::ProjectState};
use workbench_host::scene_plan::{commit::{write_atomic, commit_model_chunk, PlanPaths}, fixtures::{emit, scene_markdown, slim_broll_yaml}, HostConfig, PlanState, CommitError};

#[test]
fn stale_and_concurrent_writers_preserve_exact_winner() {
    let d=tempfile::tempdir().unwrap();
    import::save_project(d.path(),ProjectState::new()).unwrap();
    let base=import::load_state(d.path()).unwrap(); // Include existing profile enrichment before byte assertions.
    let gate=Arc::new(Barrier::new(3));
    std::thread::scope(|scope| {
        let mut handles=vec![];
        for name in ["writer-a","writer-b"] {
            let mut s=base.clone(); s.project_name=Some(name.into());
            let g=gate.clone(); let root=d.path();
            handles.push(scope.spawn(move||{g.wait();import::save_project(root,s)}));
        }
        gate.wait();
        let outcomes:Vec<_>=handles.into_iter().map(|h|h.join().unwrap()).collect();
        assert_eq!(outcomes.iter().filter(|x|x.is_ok()).count(),1);
        assert!(outcomes.iter().find_map(|x|x.as_ref().err()).unwrap().contains("changed since load"));
        let path=d.path().join("Preproduction/workflow_state.json");
        let bytes=fs::read(&path).unwrap();
        let winner=outcomes.into_iter().find_map(Result::ok).unwrap().state;
        assert_eq!(import::load_state(d.path()).unwrap().project_name,winner.project_name);
        assert!(import::save_project(d.path(),base).unwrap_err().contains("changed since load"));
        assert_eq!(fs::read(path).unwrap(),bytes);
    });
}

#[test]
fn orphan_partial_staging_and_corrupt_final_fail_closed_then_exact_restore() {
    let d=tempfile::tempdir().unwrap();
    import::save_project(d.path(),ProjectState::new()).unwrap();
    // Normalize existing supported load-time enrichment, not an import replacement.
    import::load_state(d.path()).unwrap();
    let path=d.path().join("Preproduction/workflow_state.json");
    let bytes=fs::read(&path).unwrap();
    let orphan=path.with_file_name("workflow_state.json.staging-f01-interrupted");
    fs::write(&orphan,b"{partial").unwrap();
    import::load_state(d.path()).unwrap();
    assert_eq!(fs::read(&path).unwrap(),bytes);
    assert_eq!(fs::read(&orphan).unwrap(),b"{partial");
    fs::write(&path,b"{broken-final").unwrap();
    assert!(import::load_state(d.path()).unwrap_err().contains("not valid JSON"));
    assert_eq!(fs::read(&path).unwrap(),b"{broken-final");
    // Operator snapshot restore using existing atomic primitive, not a backup UI.
    write_atomic(&path,&bytes).unwrap();
    import::load_state(d.path()).unwrap();
    assert_eq!(fs::read(&path).unwrap(),bytes);
}

#[test]
fn atomic_rename_failure_keeps_destination_and_cleans_its_staging() {
    let d=tempfile::tempdir().unwrap(); let dest=d.path().join("occupied");
    fs::create_dir(&dest).unwrap();fs::write(dest.join("sentinel"),b"old").unwrap();
    assert!(write_atomic(&dest,b"new").unwrap_err().contains("rename"));
    assert_eq!(fs::read(dest.join("sentinel")).unwrap(),b"old");
    assert_eq!(fs::read_dir(d.path()).unwrap().count(),1);
    let file=d.path().join("state.json");write_atomic(&file,b"old").unwrap();write_atomic(&file,b"new").unwrap();
    assert_eq!(fs::read(file).unwrap(),b"new");
}

#[test]
fn scene_commit_last_rename_failure_rolls_back_prior_files_and_in_memory_state() {
    let d=tempfile::tempdir().unwrap();
    let side=d.path().join("Scene_Prompts");fs::create_dir_all(side.join("Scene1")).unwrap();
    let paths=PlanPaths{prod_md:d.path().join("prod.md"),sidecar_root:side.clone(),state:d.path().join("occupied-state")};
    fs::write(&paths.prod_md,b"# Existing production notes\n").unwrap();
    fs::write(side.join("Scene1/Cut1.txt"),b"previous sidecar").unwrap();
    fs::create_dir(&paths.state).unwrap();fs::write(paths.state.join("sentinel"),b"state untouched").unwrap();
    let mut state=PlanState::new(&side,Some("9:16".into()));state.ensure_section(1,"Scene 1","Scene 1");
    let before=state.to_json().unwrap();
    let e=emit(&[("prod.md",scene_markdown(1,&slim_broll_yaml(1)))]);
    let result=commit_model_chunk(&paths,&HostConfig::default(),&mut state,0,"",None,&e);
    assert!(matches!(result,Err(CommitError::RolledBack(_))),"{result:?}");
    assert_eq!(state.to_json().unwrap(),before);
    assert_eq!(fs::read(&paths.prod_md).unwrap(),b"# Existing production notes\n");
    assert_eq!(fs::read(side.join("Scene1/Cut1.txt")).unwrap(),b"previous sidecar");
    assert_eq!(fs::read(paths.state.join("sentinel")).unwrap(),b"state untouched");
    for dir in [d.path().to_path_buf(),side.clone(),side.join("Scene1")] {
        assert!(!fs::read_dir(dir).unwrap().any(|x|x.unwrap().file_name().to_string_lossy().contains("staging-")));
    }
}
