use std::fs;
use std::path::Path;
use workbench_host::scene_plan::extract::load_handoff_document;
use workbench_host::scene_plan::fixtures::{
    broll_scene_yaml, cut_markdown, emit, lipsync_cut_yaml, lipsync_scene_yaml, plan_state,
    scene_markdown, voiceover_cut_yaml, write_fixture_project, SAM_REF,
};
use workbench_host::scene_plan::{
    commit_model_chunk, AssetRegistry, CommitError, CutId, HostConfig, IssueKind, LaneMix,
    PlanPaths, PlanState, PlanStatus, RenderLane, RenderRequest, SectionStatus,
};

const SOURCE: &str = "This is a twelve word sentence that needs a reliable duration estimate. Then Sam turns and leaves the room.";
const S1: &str = "This is a twelve word sentence that needs a reliable duration estimate.";
const S2: &str = "Then Sam turns and leaves the room.";

fn paths(root: &Path) -> PlanPaths {
    PlanPaths {
        prod_md: root.join("prod.md"),
        sidecar_root: root.join("scene_prompts"),
        state: root.join(".plan/state.json"),
    }
}

fn state_for(paths: &PlanPaths, sections: usize) -> PlanState {
    let mut state = plan_state(sections);
    state.sidecar_root = paths.sidecar_root.to_string_lossy().to_string();
    state
}

fn rejected(err: CommitError) -> Vec<workbench_host::scene_plan::Issue> {
    match err {
        CommitError::Rejected(issues) => issues,
        other => panic!("expected Rejected, got {other}"),
    }
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

fn no_staging_files(root: &Path) {
    for entry in walk(root) {
        assert!(
            !entry.to_string_lossy().contains(".staging-"),
            "leftover staging file {}",
            entry.display()
        );
    }
}

#[test]
fn first_chunk_commits_cut_paths_master_and_state() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(dir.path());
    let mut state = state_for(&p, 1);
    let e = emit(&[("prod.md", scene_markdown(1, &lipsync_scene_yaml(1, S1)))]);
    let report =
        commit_model_chunk(&p, &HostConfig::default(), &mut state, 0, SOURCE, None, &e).unwrap();
    assert_eq!(report.cuts, vec![CutId::new(1, 1)]);
    assert_eq!(
        report.sidecars,
        vec![p.sidecar_root.join("Scene1/Cut1.txt")]
    );
    assert_eq!(
        (report.covered_word_count, report.total_word_count),
        (12, 19)
    );

    let prod = fs::read_to_string(&p.prod_md).unwrap();
    assert!(prod.starts_with("### Scene 1 — Cut 1 — Sam addresses the camera\n"));
    assert!(prod.contains("aspect_ratio: \"9:16\""));
    let sidecar = fs::read_to_string(p.sidecar_root.join("Scene1/Cut1.txt")).unwrap();
    assert_eq!(
        load_handoff_document(&sidecar).unwrap(),
        load_handoff_document(&prod).unwrap()
    );
    assert_eq!(
        RenderRequest::from_document(&sidecar)
            .unwrap()
            .duration_seconds,
        10
    );

    let on_disk = PlanState::from_json(&fs::read_to_string(&p.state).unwrap()).unwrap();
    assert_eq!(on_disk, state);
    assert_eq!(on_disk.sidecar_root, p.sidecar_root.to_string_lossy());
    assert_eq!(on_disk.active_source_scene, Some(1));
    let s = &on_disk.sections[0];
    assert_eq!(
        s.production_path.as_deref(),
        Some(p.prod_md.to_string_lossy().as_ref())
    );
    assert_eq!(
        s.sidecar_dir.as_deref(),
        Some(p.sidecar_root.join("Scene1").to_string_lossy().as_ref())
    );
    assert_eq!(
        (s.next_cut, s.cuts.clone(), s.status),
        (2, vec![1], SectionStatus::InProgress)
    );
    no_staging_files(dir.path());
}

#[test]
fn per_scene_masters_share_one_root_and_state_records_each() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Prod_Notes");
    let cfg = HostConfig::default();
    let p1 = PlanPaths {
        prod_md: root.join("Scene1.md"),
        sidecar_root: root.clone(),
        state: root.join("scene_plan_state.json"),
    };
    let mut state = state_for(&p1, 1);
    commit_model_chunk(
        &p1,
        &cfg,
        &mut state,
        0,
        S1,
        None,
        &emit(&[("prod.md", scene_markdown(1, &lipsync_scene_yaml(1, S1)))]),
    )
    .unwrap();

    let idx = state.ensure_section(2, "Scene 2", "Scene 2");
    let p2 = PlanPaths {
        prod_md: root.join("Scene2.md"),
        sidecar_root: root.clone(),
        state: root.join("scene_plan_state.json"),
    };
    let s2 = emit(&[(
        "prod.md",
        cut_markdown(
            2,
            1,
            &lipsync_cut_yaml(2, 1, &[("a", "Sam", "Here we are.", true)], SAM_REF),
        ),
    )]);
    let report = commit_model_chunk(&p2, &cfg, &mut state, idx, "Here we are.", None, &s2).unwrap();
    assert_eq!(report.sidecars, vec![root.join("Scene2/Cut1.txt")]);
    assert!(root.join("Scene1/Cut1.txt").is_file() && root.join("Scene2/Cut1.txt").is_file());
    assert!(
        !root.join("Scene2/Scene2").exists(),
        "scene folder must not be doubled"
    );

    let on_disk = PlanState::from_json(&fs::read_to_string(&p2.state).unwrap()).unwrap();
    assert_eq!(on_disk.sidecar_root, root.to_string_lossy());
    assert_eq!(
        on_disk.sections[0].production_path.as_deref(),
        Some(root.join("Scene1.md").to_string_lossy().as_ref())
    );
    assert_eq!(
        on_disk.sections[1].production_path.as_deref(),
        Some(root.join("Scene2.md").to_string_lossy().as_ref())
    );
    assert_eq!(on_disk.active_source_scene, Some(2));
    assert_eq!(on_disk.status, PlanStatus::Done);

    // Continuing Scene 1 into a different master is refused before anything is written.
    let wrong = PlanPaths {
        prod_md: root.join("Other.md"),
        sidecar_root: root.clone(),
        state: p1.state.clone(),
    };
    let err = commit_model_chunk(
        &wrong,
        &cfg,
        &mut state,
        0,
        S1,
        None,
        &emit(&[("prod.md", scene_markdown(2, &broll_scene_yaml(2)))]),
    )
    .unwrap_err();
    assert!(
        matches!(err, CommitError::Host(ref m) if m.contains("Scene 1 is recorded in")),
        "{err}"
    );
    assert!(!root.join("Other.md").exists());
}

#[test]
fn second_chunk_appends_and_scene_two_restarts_cut_numbering() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(dir.path());
    let cfg = HostConfig::default();
    let mut state = state_for(&p, 1);
    commit_model_chunk(
        &p,
        &cfg,
        &mut state,
        0,
        SOURCE,
        None,
        &emit(&[("prod.md", scene_markdown(1, &lipsync_scene_yaml(1, S1)))]),
    )
    .unwrap();
    let e = emit(&[(
        "prod.md",
        format!(
            "{}{}",
            scene_markdown(2, &lipsync_scene_yaml(2, S2)),
            scene_markdown(3, &broll_scene_yaml(3))
        ),
    )]);
    let report = commit_model_chunk(&p, &cfg, &mut state, 0, SOURCE, None, &e).unwrap();
    assert_eq!(report.cuts, vec![CutId::new(1, 2), CutId::new(1, 3)]);
    assert_eq!(report.section_cuts, vec![1, 2, 3]);
    assert!(report.section_done && report.plan_done);
    let idx = state.ensure_section(2, "Scene 2", "Scene 2");
    let s2 = emit(&[(
        "prod.md",
        cut_markdown(
            2,
            1,
            &lipsync_cut_yaml(2, 1, &[("a", "Sam", "Here we are.", true)], SAM_REF),
        ),
    )]);
    let report = commit_model_chunk(&p, &cfg, &mut state, idx, "Here we are.", None, &s2).unwrap();
    assert_eq!(report.cuts, vec![CutId::new(2, 1)]);
    assert!(p.sidecar_root.join("Scene2/Cut1.txt").is_file());
    assert_eq!(
        PlanState::from_json(&fs::read_to_string(&p.state).unwrap())
            .unwrap()
            .committed_cuts()
            .len(),
        4
    );
}

#[test]
fn silent_cut_serializes_positive_zero_speech_duration() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(dir.path());
    let mut state = state_for(&p, 1);
    commit_model_chunk(
        &p,
        &HostConfig::default(),
        &mut state,
        0,
        "",
        None,
        &emit(&[("prod.md", scene_markdown(1, &broll_scene_yaml(1)))]),
    )
    .unwrap();
    let sidecar = fs::read_to_string(p.sidecar_root.join("Scene1/Cut1.txt")).unwrap();
    assert!(
        sidecar.contains("speech_duration_seconds: 0.0\n"),
        "{sidecar}"
    );
    assert!(!sidecar.contains("-0.0"));
}

#[test]
fn real_character_assets_make_lipsync_possible_without_forcing_it() {
    let dir = tempfile::tempdir().unwrap();
    let fx = write_fixture_project(dir.path());
    let registry = AssetRegistry::discover(dir.path());
    let p = paths(dir.path());
    let cfg = HostConfig::default();
    let mut state = state_for(&p, 1);
    let dialogue = "We shall be there before supper. Will there be cocoa? Willowby Halt!";
    let hidden = emit(&[(
        "prod.md",
        scene_markdown(
            1,
            &voiceover_cut_yaml(
                1,
                1,
                &[("a", "Tom", "We shall be there before supper.", true)],
            ),
        ),
    )]);
    let report =
        commit_model_chunk(&p, &cfg, &mut state, 0, dialogue, Some(&registry), &hidden).unwrap();
    assert!(report
        .warnings
        .iter()
        .any(|w| w.contains("is on camera in a voice_over cut and has a reference image")));
    assert_eq!(
        report.mix,
        LaneMix {
            lipsync: 0,
            voice_over: 1,
            silent: 0
        }
    );
    let good = emit(&[(
        "prod.md",
        format!(
            "{}{}",
            scene_markdown(
                2,
                &lipsync_cut_yaml(
                    1,
                    2,
                    &[("a", "Pip", "Will there be cocoa?", true)],
                    &fx.pip_closeup.to_string_lossy()
                )
            ),
            scene_markdown(
                3,
                &voiceover_cut_yaml(1, 3, &[("a", "Guard, outside", "Willowby Halt!", false)])
            )
        ),
    )]);
    let report =
        commit_model_chunk(&p, &cfg, &mut state, 0, dialogue, Some(&registry), &good).unwrap();
    assert!(report.section_done);
    let pip = fs::read_to_string(p.sidecar_root.join("Scene1/Cut2.txt")).unwrap();
    assert!(pip.contains("Voice: Pip Vale (elevenlabs MlxAbUenU34aJO4OFnwY)"));
    assert_eq!(
        RenderRequest::from_document(&pip).unwrap().lane,
        RenderLane::AudioDrivenI2v
    );
}

#[test]
fn rejected_chunk_leaves_everything_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(dir.path());
    fs::write(&p.prod_md, "# Production\n").unwrap();
    let mut state = state_for(&p, 1);
    let before = state.clone();
    let bad = lipsync_scene_yaml(1, S1).replace("  generation_path: audio_driven_i2v\n", "");
    let issues = rejected(
        commit_model_chunk(
            &p,
            &HostConfig::default(),
            &mut state,
            0,
            SOURCE,
            None,
            &emit(&[("prod.md", scene_markdown(1, &bad))]),
        )
        .unwrap_err(),
    );
    assert!(issues.iter().any(|i| i.kind == IssueKind::NotReady));
    assert_eq!(fs::read_to_string(&p.prod_md).unwrap(), "# Production\n");
    assert!(!p.sidecar_root.exists() && !p.state.exists());
    assert_eq!(state, before);
}

#[test]
fn sequence_and_identity_rules_at_the_commit_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(dir.path());
    let cfg = HostConfig::default();
    let mut state = state_for(&p, 1);
    let gap = emit(&[
        ("a.md", scene_markdown(1, &lipsync_scene_yaml(1, S1))),
        ("b.md", scene_markdown(3, &lipsync_scene_yaml(3, S2))),
    ]);
    assert!(
        rejected(commit_model_chunk(&p, &cfg, &mut state, 0, SOURCE, None, &gap).unwrap_err())
            .iter()
            .any(|i| i
                .message
                .contains("Scene 1 — Cut 3 follows Scene 1 — Cut 1"))
    );
    let other_scene = emit(&[(
        "prod.md",
        cut_markdown(
            2,
            1,
            &lipsync_cut_yaml(2, 1, &[("a", "Sam", S1, true)], SAM_REF),
        ),
    )]);
    assert!(rejected(
        commit_model_chunk(&p, &cfg, &mut state, 0, SOURCE, None, &other_scene).unwrap_err()
    )
    .iter()
    .any(|i| i.message.contains("this turn plans source Scene 1")));
    let legacy = emit(&[(
        "prod.md",
        format!(
            "### Scene 1\n\n```yaml\n{}\n```\n",
            lipsync_scene_yaml(1, S1)
        ),
    )]);
    assert!(rejected(
        commit_model_chunk(&p, &cfg, &mut state, 0, SOURCE, None, &legacy).unwrap_err()
    )
    .iter()
    .any(|i| i.message.contains("has no cut number")));
    assert!(!p.prod_md.exists());
}

#[test]
fn incomplete_coverage_never_becomes_done() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(dir.path());
    let mut state = state_for(&p, 1);
    state.sections[0].attempts = 99;
    let report = commit_model_chunk(
        &p,
        &HostConfig::default(),
        &mut state,
        0,
        SOURCE,
        None,
        &emit(&[("prod.md", scene_markdown(1, &lipsync_scene_yaml(1, S1)))]),
    )
    .unwrap();
    assert!(!report.section_done);
    assert_eq!(state.sections[0].status, SectionStatus::InProgress);
    assert_eq!(report.remaining_source_speech, S2);
}

#[test]
fn v3_state_on_disk_loads_with_migration_notes_and_v2_is_refused() {
    let v3 = r#"{"version":3,"status":"done","script_path":"","production_path":"/p/Prod_Notes/Scene1.md","sidecar_dir":"/p/Prod_Notes/Scene1","aspect_ratio":"9:16","section_index":1,
      "sections":[{"source_scene":1,"title":"a","heading":"a","status":"done","covered_word_count":33,"next_cut":10,"cuts":[1,2,3]},
                  {"source_scene":2,"title":"b","heading":"b","status":"done","covered_word_count":55,"next_cut":8,"cuts":[1,2]}]}"#;
    let (state, notes) = PlanState::load(v3).unwrap();
    assert_eq!(state.sidecar_root, "/p/Prod_Notes");
    assert_eq!(state.sections[1].production_path, None);
    assert!(notes
        .iter()
        .any(|n| n.contains("repairs that layout on load")));
    let v2 = r#"{"version":2,"status":"done","script_path":"","production_path":"p","sidecar_dir":"s","next_scene_number":4,"sections":[]}"#;
    assert!(PlanState::from_json(v2)
        .unwrap_err()
        .contains("state version 2 is not supported"));
}

#[test]
fn failed_state_rename_rolls_back_master_and_nested_sidecars() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(dir.path());
    fs::write(&p.prod_md, "# Production\n").unwrap();
    fs::create_dir_all(p.state.join("blocker")).unwrap();
    fs::write(p.state.join("blocker/x"), "x").unwrap();
    let mut state = state_for(&p, 1);
    let before = state.clone();
    let err = commit_model_chunk(
        &p,
        &HostConfig::default(),
        &mut state,
        0,
        SOURCE,
        None,
        &emit(&[("prod.md", scene_markdown(1, &lipsync_scene_yaml(1, S1)))]),
    )
    .unwrap_err();
    assert!(matches!(err, CommitError::RolledBack(_)), "got {err}");
    assert_eq!(fs::read_to_string(&p.prod_md).unwrap(), "# Production\n");
    assert!(!p.sidecar_root.join("Scene1/Cut1.txt").exists());
    assert_eq!(state, before);
    no_staging_files(dir.path());
}
