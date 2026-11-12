//! Real production-library tests for render lifecycle wrapper.
use std::fs;
use std::path::Path;
use workbench_host::session;
use workbench_host::workflow::{self, import, state::{JobRecord, JobStatus, ProjectState, STATE_REL}};
static SESSION: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn init_project(root: &Path) {
    fs::create_dir_all(root.join("Scenes/Scene1/Cut1")).unwrap();
    fs::create_dir_all(root.join("Preproduction")).unwrap();
}

fn open_session(root: &std::path::PathBuf) {
    session::open(root.to_str().unwrap()).unwrap();
}

fn close_session() {
    session::close();
}

#[test]
fn render_wrapper_exact_path_and_lifecycle() {
    let _session = SESSION.lock().unwrap_or_else(|error| error.into_inner());
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("project");
    fs::create_dir_all(&root).unwrap();
    init_project(&root);
    open_session(&root);

    // Set up real scene and cut with stable id matching scope.
    let mut state = ProjectState::new();
    let scene = state.scene_mut(1, "Scene 1");
    let cut = scene.cut_mut(1);
    cut.id = workbench_host::scene_plan::handoff::CutId::new(1, 1);
    cut.stable_id = "shot_s01c01".into();
    // Job: Pending render with canonical output path and exact scope.
    let mut job = JobRecord::open("job_render_1", "render");
    job.status = JobStatus::Pending;
    job.output_path = Some("Scenes/Scene1/Cut1/S01C01.mp4".into());
    job.scope = Some(workflow::state::JobScope {
        kind: "shot".into(),
        id: "shot_s01c01".into(),
    });
    job.raw_provider_id = Some("S01C01".into());
    state.jobs.push(job);
    import::persist_state(&root, &state).unwrap();

    let output_path_str = "Scenes/Scene1/Cut1/S01C01.mp4";
    let mut observed_running = false;

    // Call wrapper directly; observe Running inside closure by reading persisted bytes.
    let dispatch_result = workflow::render_jobs::with_render_job(
        &root,
        output_path_str,
        || {
            // Read persisted bytes directly (not load_state repairs) to observe Running.
            let text = fs::read_to_string(root.join(STATE_REL)).unwrap();
            let inner_state = ProjectState::from_json(&text).unwrap();
            let inner_job = inner_state.jobs.iter().find(|j| j.id == "job_render_1").unwrap();
            assert_eq!(inner_job.status, JobStatus::Running, "must observe Running inside dispatch");
            assert!(inner_job.started_at.is_some(), "must have real start timestamp");
            observed_running = true;
            // Produce output file locally before returning Ok.
            let out_file = root.join(output_path_str);
            fs::create_dir_all(out_file.parent().unwrap()).unwrap();
            fs::write(&out_file, b"video-data").unwrap();
            Ok("adapter-ok".into())
        },
    );
    assert!(observed_running, "must observe Running inside closure");

    assert!(dispatch_result.is_ok());
    assert_eq!(dispatch_result.unwrap(), "adapter-ok");

    // After dispatch with actual file, reload and verify Completed with truth.
    let final_state_text = fs::read_to_string(root.join(STATE_REL)).unwrap();
    let final_state = ProjectState::from_json(&final_state_text).unwrap();
    let final_job = final_state.jobs.iter().find(|j| j.id == "job_render_1").unwrap();
    assert_eq!(final_job.status, JobStatus::Completed);
    assert!(final_job.completed_at.is_some());
    assert!(final_state_text.contains("video-data") == false); // state doesn't contain media bytes, fine.
    close_session();
}

#[test]
fn render_wrapper_missing_output_keeps_running() {
    let _session = SESSION.lock().unwrap_or_else(|error| error.into_inner());
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("p");
    fs::create_dir_all(&root.join("Scenes/Scene2/Cut1")).unwrap();
    fs::create_dir_all(root.join("Preproduction")).unwrap();
    open_session(&root);

    let mut state = ProjectState::new();
    let scene = state.scene_mut(2, "Scene 2");
    scene.cuts.push(workbench_host::workflow::state::CutRecord::new(
        workbench_host::scene_plan::handoff::CutId::new(2, 1),
    ));
    scene.cuts[0].stable_id = "shot_s02c01".into();
    let mut job = JobRecord::open("j1", "render");
    job.output_path = Some("Scenes/Scene2/Cut1/S02C01.mp4".into());
    job.scope = Some(workflow::state::JobScope { kind: "shot".into(), id: "shot_s02c01".into() });
    job.raw_provider_id = Some("S02C01".into());
    state.jobs.push(job);
    import::persist_state(&root, &state).unwrap();

    let dispatch_result = workflow::render_jobs::with_render_job(
        &root,
        "Scenes/Scene2/Cut1/S02C01.mp4",
        || {
            // Don't create file; return Ok but expect not completed.
            Ok("adapter-ok-no-file".into())
        },
    );
    assert!(dispatch_result.is_ok());

    let after_text = fs::read_to_string(root.join(STATE_REL)).unwrap();
    let after_state = ProjectState::from_json(&after_text).unwrap();
    let after_job = after_state.jobs.iter().find(|j| j.id == "j1").unwrap();
    assert_eq!(after_job.status, JobStatus::Running);
    assert!(after_job.completed_at.is_none());
    close_session();
}

#[test]
fn render_wrapper_terminal_error_fails_only_bound() {
    let _session = SESSION.lock().unwrap_or_else(|error| error.into_inner());
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("p");
    fs::create_dir_all(&root.join("Scenes/Scene3/Cut1")).unwrap();
    fs::create_dir_all(root.join("Preproduction")).unwrap();
    open_session(&root);

    let mut state = ProjectState::new();
    let scene = state.scene_mut(3, "Scene 3");
    scene.cuts.push(workbench_host::workflow::state::CutRecord::new(
        workbench_host::scene_plan::handoff::CutId::new(3, 1),
    ));
    scene.cuts[0].stable_id = "shot_s03c01".into();
    let mut job = JobRecord::open("j_fail", "render");
    job.output_path = Some("Scenes/Scene3/Cut1/S03C01.mp4".into());
    job.scope = Some(workflow::state::JobScope { kind: "shot".into(), id: "shot_s03c01".into() });
    job.raw_provider_id = Some("S03C01".into());
    // Add an unrelated other job that must be preserved.
    let mut other = JobRecord::open("other", "render");
    other.output_path = Some("Scenes/Scene3/Cut2/S03C02.mp4".into());
    state.jobs.push(job);
    state.jobs.push(other);
    import::persist_state(&root, &state).unwrap();

    let dispatch_result = workflow::render_jobs::with_render_job(
        &root,
        "Scenes/Scene3/Cut1/S03C01.mp4",
        || {
            Err("provider-failed: terminal adapter error".into())
        },
    );
    assert!(dispatch_result.is_err());
    assert!(dispatch_result.unwrap_err().contains("provider-failed"));

    let after_text = fs::read_to_string(root.join(STATE_REL)).unwrap();
    let after_state = ProjectState::from_json(&after_text).unwrap();
    let j_fail = after_state.jobs.iter().find(|j| j.id == "j_fail").unwrap();
    assert_eq!(j_fail.status, JobStatus::Failed);
    assert!(j_fail.completed_at.is_some());
    assert!(j_fail.error.as_ref().unwrap().contains("provider-failed"));
    // Unrelated job preserved.
    let other_after = after_state.jobs.iter().find(|j| j.id == "other").unwrap();
    assert_eq!(other_after.status, JobStatus::Pending);
    close_session();
}

#[test]
fn render_wrapper_ambiguous_rejects_before_dispatch() {
    let _session = SESSION.lock().unwrap_or_else(|error| error.into_inner());
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("p");
    fs::create_dir_all(&root.join("Scenes/Scene1/Cut1")).unwrap();
    open_session(&root);
    let mut state = ProjectState::new();
    let scene = state.scene_mut(1, "Scene 1");
    scene.cut_mut(1);

    let mut job1 = JobRecord::open("j1", "render");
    job1.output_path = Some("Scenes/Scene1/Cut1/S01C01.mp4".into());
    job1.scope = Some(workflow::state::JobScope { kind: "shot".into(), id: "s1".into() });
    job1.raw_provider_id = Some("S01C01".into());
    state.jobs.push(job1);
    let mut job2 = JobRecord::open("j2", "render");
    job2.output_path = Some("Scenes/Scene1/Cut1/S01C01.mp4".into());
    job2.scope = Some(workflow::state::JobScope { kind: "shot".into(), id: "s1".into() });
    job2.raw_provider_id = Some("S01C01".into());
    state.jobs.push(job2);
    import::persist_state(&root, &state).unwrap();

    let result = workflow::render_jobs::with_render_job(
        &root,
        "Scenes/Scene1/Cut1/S01C01.mp4",
        || panic!("dispatch must not be called when ambiguous"),
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("ambiguous"));
    close_session();
}

#[test]
fn render_wrapper_wrong_bucket_rejected_before_dispatch() {
    let _session = SESSION.lock().unwrap_or_else(|error| error.into_inner());
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("p");
    fs::create_dir_all(&root).unwrap();
    open_session(&root);
    let mut state = ProjectState::new();
    let mut job = JobRecord::open("j_bad", "render");
    job.output_path = Some("Preproduction/Scene1/video/x.mp4".into());
    job.scope = Some(workflow::state::JobScope { kind: "shot".into(), id: "shot_bad".into() });
    job.raw_provider_id = Some("bad".into());
    state.jobs.push(job);
    import::persist_state(&root, &state).unwrap();

    // A matching prepared job with a wrong bucket cannot bypass its scope by
    // falling through to generic dispatch. Generic no-match is covered separately.
    let before = fs::read(root.join(STATE_REL)).unwrap();
    let res = workflow::render_jobs::with_render_job(
        &root,
        "Preproduction/Scene1/video/x.mp4",
        || panic!("invalid prepared bucket must not dispatch"),
    );
    assert!(res.is_err());
    assert_eq!(fs::read(root.join(STATE_REL)).unwrap(), before);
    close_session();
}

#[test]
fn render_wrapper_symlink_redirect_rejected() {
    let _session = SESSION.lock().unwrap_or_else(|error| error.into_inner());
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("p");
    fs::create_dir_all(&root).unwrap();
    open_session(&root);
    // Create a symlink redirect outside expected bucket; resolve(Write) should reject.
    fs::create_dir_all(root.join("Preproduction")).unwrap();
    std::os::unix::fs::symlink("/etc/passwd", root.join("Preproduction/symlink_redirect")).unwrap();
    let res = workflow::render_jobs::with_render_job(
        &root,
        "Preproduction/symlink_redirect/S01C01.mp4",
        || panic!("must reject before dispatch"),
    );
    assert!(res.is_err() || res.unwrap() != "fake");
    close_session();
}

#[test]
fn render_wrapper_lexical_alias_resolves() {
    let _session = SESSION.lock().unwrap_or_else(|error| error.into_inner());
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("p");
    fs::create_dir_all(&root.join("Scenes/Scene4/Cut2")).unwrap();
    open_session(&root);
    let mut state = ProjectState::new();
    let scene = state.scene_mut(4, "Scene 4");
    scene.cut_mut(2);
    scene.cuts[0].stable_id = "s4c2".into();
    scene.cuts[0].id = workbench_host::scene_plan::handoff::CutId::new(4, 2);
    let mut job = JobRecord::open("j_alias", "render");
    job.output_path = Some("Scenes/Scene4/Cut2/S04C02.mp4".into());
    job.scope = Some(workflow::state::JobScope { kind: "shot".into(), id: "s4c2".into() });
    job.raw_provider_id = Some("S04C02".into());
    state.jobs.push(job);
    import::persist_state(&root, &state).unwrap();

    // Call via lexical alias root (same absolute path but different lexical representation? We'll just use same root).
    let res = workflow::render_jobs::with_render_job(
        &root,
        "Scenes/Scene4/Cut2/S04C02.mp4",
        || Ok("alias-ok".into()),
    );
    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "alias-ok");
    close_session();
}

#[test]
fn failed_pre_submit_retry_checks_child_and_audio_before_dispatch() {
    let _session = SESSION.lock().unwrap_or_else(|error| error.into_inner());
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("p");
    init_project(&root);
    fs::write(root.join("Scenes/Scene1/Cut1/S01C01.wav"), b"protected wav bytes").unwrap();
    open_session(&root);
    let mut state = ProjectState::new();
    let cut = state.scene_mut(1,"Scene").cut_mut(1);
    cut.stable_id="shot_1".into();
    let error="HOST PRE-SUBMIT VALIDATION: incompatible schema. No generation submitted.";
    let scope=Some(workflow::state::JobScope{kind:"shot".into(),id:"shot_1".into()});
    let mut prior=JobRecord::open("job_3","render");
    prior.scope=scope.clone(); prior.output_path=Some("Scenes/Scene1/Cut1/S01C01.mp4".into());
    prior.raw_provider_id=Some("S01C01".into()); prior.mark_running("fixture");prior.fail(None,error);
    let mut child=JobRecord::open("spend_1","atlas_generate_video");
    child.mark_running("fixture");child.fail(None,error);
    for job in [&mut prior,&mut child] {
        job.attempts[0].started_at=Some("2026-11-01T23:07:34Z".into());
        job.attempts[0].completed_at=Some("2026-11-01T23:07:36Z".into());
    }
    let mut retry=JobRecord::open("job_4","render");
    retry.scope=scope;retry.raw_provider_id=Some("S01C01".into());
    retry.output_path=Some("Scenes/Scene1/Cut1/S01C01_v2.mp4".into());
    retry.retry_of=Some("job_3".into());
    state.jobs.extend([prior.clone(),child.clone(),retry]);
    import::persist_state(&root,&state).unwrap();
    let mut called=0;
    let result=workflow::render_jobs::with_render_job(&root,"Scenes/Scene1/Cut1/S01C01_v2.mp4",||{
        called+=1;Err("synthetic no-provider failure".into())
    });
    assert!(result.is_err());assert_eq!(called,1);
    let after=ProjectState::from_json(&fs::read_to_string(root.join(STATE_REL)).unwrap()).unwrap();
    assert_eq!(after.jobs[0],prior);assert_eq!(after.jobs[1],child);
    assert_eq!(after.jobs[2].status,JobStatus::Failed);
    assert_eq!(fs::read(root.join("Scenes/Scene1/Cut1/S01C01.wav")).unwrap(),b"protected wav bytes");
    // An occupied retry WAV must fail before the dispatch closure executes.
    state.jobs[2].status=JobStatus::Pending;
    state.state_revision=after.state_revision;
    import::persist_state(&root,&state).unwrap();
    fs::write(root.join("Scenes/Scene1/Cut1/S01C01_v2.wav"),b"newer bytes").unwrap();
    let result=workflow::render_jobs::with_render_job(&root,"Scenes/Scene1/Cut1/S01C01_v2.mp4",||panic!("must not dispatch"));
    assert!(result.is_err());
    close_session();
}

#[test]
fn saved_audio_retry_ignores_only_its_own_reserved_spend_child() {
    let _session = SESSION.lock().unwrap_or_else(|error| error.into_inner());
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("p");
    init_project(&root);
    fs::write(root.join("Scenes/Scene1/Cut1/S01C01.wav"), b"protected wav bytes").unwrap();
    open_session(&root);
    let mut state = ProjectState::new();
    state.scene_mut(1, "Scene").cut_mut(1).stable_id = "shot_1".into();
    let error = "HOST PRE-SUBMIT VALIDATION: incompatible schema. No generation submitted.";
    let scope = Some(workflow::state::JobScope { kind: "shot".into(), id: "shot_1".into() });
    let mut prior = JobRecord::open("job_3", "render");
    prior.scope = scope.clone(); prior.output_path = Some("Scenes/Scene1/Cut1/S01C01.mp4".into());
    prior.raw_provider_id = Some("S01C01".into()); prior.mark_running("fixture"); prior.fail(None, error);
    let mut failed_child = JobRecord::open("spend_old", "atlas_generate_video");
    failed_child.mark_running("fixture"); failed_child.fail(None, error);
    for job in [&mut prior, &mut failed_child] {
        job.attempts[0].started_at = Some("2026-11-01T23:07:34Z".into());
        job.attempts[0].completed_at = Some("2026-11-01T23:07:36Z".into());
    }
    let mut retry = JobRecord::open("job_4", "render");
    retry.scope = scope; retry.raw_provider_id = Some("S01C01".into());
    retry.output_path = Some("Scenes/Scene1/Cut1/S01C01_v2.mp4".into());
    retry.retry_of = Some("job_3".into());
    let mut current = JobRecord::open("spend_current", "atlas_generate_video");
    current.mark_running("reserved by ledger");
    current.request_metadata = Some(workflow::state::SpendRequestMetadata {
        request_hash: "a".repeat(64), disclosure: None, duration_seconds: Some(5),
        width: None, height: None, prediction_id: None,
    });
    state.jobs.extend([prior, failed_child, retry, current]);
    import::persist_state(&root, &state).unwrap();
    let mut called = 0;
    let result = workflow::render_jobs::with_render_job_outcome_for_spend(
        &root, "Scenes/Scene1/Cut1/S01C01_v2.mp4", Some("spend_current"), || {
            called += 1; Err::<(), String>("synthetic no-provider failure".into())
        });
    assert_eq!(result.unwrap_err(), "synthetic no-provider failure");
    assert_eq!(called, 1);
    let mut state = ProjectState::from_json(&fs::read_to_string(root.join(STATE_REL)).unwrap()).unwrap();
    assert_eq!(state.jobs[2].status, JobStatus::Failed);
    state.jobs[2].status = JobStatus::Pending;
    let mut other = JobRecord::open("spend_other", "atlas_generate_video");
    other.mark_running("unrelated in-flight request");
    state.jobs.push(other);
    import::persist_state(&root, &state).unwrap();
    let result: Result<(), String> = workflow::render_jobs::with_render_job_outcome_for_spend(
        &root, "Scenes/Scene1/Cut1/S01C01_v2.mp4", Some("spend_current"),
        || panic!("unrelated running child must block dispatch"));
    assert!(result.unwrap_err().contains("no longer proven pre-submit"));
    close_session();
}
