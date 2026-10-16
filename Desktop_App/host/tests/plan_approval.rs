//! Phase 4 §8.6 chain heads + §8.3 identity hop at the approval boundary.
//!
//! The pre-existing suite imported and rendered, but never drove the two
//! approval commands that gate production: `workflow_approve_intent` and
//! `workflow_approve_animatic`. These tests call the real command surface
//! (the same JSON args the host RPC receives) and read state back from disk
//! to prove what persisted. No network, no billable dispatch.

use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use workbench_host::scene_plan::fixtures::{broll_cut_yaml, cut_markdown};
use workbench_host::workflow::import::{import_project, load_state, ImportOptions};

const SCRIPT: &str = r#"# The Lantern at Willowby Halt

**Format:** Vertical 9:16

## Scenes

### Scene 1 — The last train

- **Location:** A two-carriage night train approaching Willowby Halt.
- **Action:** Rain streams down the carriage windows. Tom guards a parcel.
- **Sound:** Train wheels clatter. Rain strikes the glass.

### Scene 2 — An empty platform

- **Location:** Willowby Halt platform, night.
"#;

/// Two silent cuts for Scene 1 and none for Scene 2, so the animatic tests
/// have a scene WITH shots and a scene WITHOUT one.
fn write_project(root: &Path) {
    fs::create_dir_all(root.join("Scripts")).unwrap();
    fs::write(root.join("Scripts/Lantern.md"), SCRIPT).unwrap();
    let sidecar = root.join("Scene_Prompts/Scene1");
    let mut prod = String::new();
    for n in 1..=2u32 {
        let yaml = broll_cut_yaml(1, n);
        fs::create_dir_all(&sidecar).unwrap();
        fs::write(
            sidecar.join(format!("Cut{n}.txt")),
            cut_markdown(1, n, &yaml),
        )
        .unwrap();
        prod.push_str(&cut_markdown(1, n, &yaml));
        prod.push('\n');
    }
    fs::write(root.join("prod.md"), prod).unwrap();
}

/// The host commands operate on the globally-open project session (same
/// guard pattern as tests/vertical_slice.rs). Integration tests share one
/// process-global session, so each test locks, opens, runs, and closes.
static SESSION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct SessionGuard;

impl Drop for SessionGuard {
    fn drop(&mut self) {
        workbench_host::session::close();
    }
}

struct Session(
    tempfile::TempDir,
    std::path::PathBuf,
    SessionGuard,
    std::sync::MutexGuard<'static, ()>,
);

fn session() -> Session {
    let lock = SESSION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("show");
    write_project(&root);
    // Persisting the first import is what lays down workflow_state.json;
    // later reads go through load_state so they see exactly what is on disk.
    import_project(&root, ImportOptions { persist: true }).unwrap();
    workbench_host::session::open(root.to_str().unwrap()).unwrap();
    Session(dir, root, SessionGuard, lock)
}

fn on_disk(root: &Path) -> workbench_host::workflow::state::ProjectState {
    load_state(root).unwrap_or_else(|err| panic!("state unreadable at {root:?}: {err}"))
}

fn intent_saved(root: &Path, purpose: &str) {
    workbench_host::workflow::cmd_save_intent(&json!({
        "projectRoot": root.to_string_lossy(),
        "sceneId": 1,
        "intent": { "narrative_purpose": purpose },
    }))
    .unwrap();
}

#[test]
fn approve_intent_without_a_draft_is_rejected() {
    let _s = session();
    let root = &_s.1;
    let args = json!({ "projectRoot": root.to_string_lossy(), "sceneId": 1 });
    let err = workbench_host::workflow::cmd_approve_intent(&args)
        .expect_err("approving an empty intent must fail");
    assert!(
        err.contains("Draft scene intent"),
        "unexpected rejection: {err}"
    );
    assert!(
        !on_disk(&root).scene(1).unwrap().intent_approved,
        "a rejected approval must not touch disk"
    );
}

#[test]
fn saved_intent_stays_a_draft_until_approved() {
    let _s = session();
    let root = &_s.1;
    intent_saved(&root, "children leave warmth for danger");
    let scene = on_disk(&root).scene(1).unwrap().clone();
    assert_eq!(
        scene
            .intent
            .as_ref()
            .and_then(|i| i.narrative_purpose.as_deref()),
        Some("children leave warmth for danger")
    );
    assert!(!scene.intent_approved, "a save alone must not approve");
}

#[test]
fn approving_an_intent_persists_the_approval() {
    let _s = session();
    let root = &_s.1;
    intent_saved(&root, "children leave warmth for danger");
    let approved = workbench_host::workflow::cmd_approve_intent(&json!({
        "projectRoot": root.to_string_lossy(),
        "sceneId": 1,
    }))
    .expect("approve after a draft");
    assert!(approved["note"]
        .as_str()
        .unwrap_or_default()
        .contains("approved"));

    let scene = on_disk(&root).scene(1).unwrap().clone();
    assert!(scene.intent_approved);
    assert_eq!(
        scene
            .intent
            .as_ref()
            .and_then(|i| i.narrative_purpose.as_deref()),
        Some("children leave warmth for danger"),
        "approval must not lose the approved text"
    );
}

#[test]
fn resaving_an_approved_intent_drops_the_approval() {
    // The user approves the exact card on screen; a later edit must return
    // it to draft rather than silently keeping the old approval.
    let _s = session();
    let root = &_s.1;
    intent_saved(&root, "first version");
    workbench_host::workflow::cmd_approve_intent(&json!({
        "projectRoot": root.to_string_lossy(),
        "sceneId": 1,
    }))
    .unwrap();
    intent_saved(&root, "edited after approval");

    let scene = on_disk(&root).scene(1).unwrap().clone();
    assert!(!scene.intent_approved, "edited intent must return to draft");
    assert_eq!(
        scene
            .intent
            .as_ref()
            .and_then(|i| i.narrative_purpose.as_deref()),
        Some("edited after approval")
    );
}

#[test]
fn approve_animatic_without_cut_sketches_is_rejected() {
    let _s = session();
    let root = &_s.1;
    let args = json!({ "projectRoot": root.to_string_lossy(), "sceneId": 2 });
    let err = workbench_host::workflow::cmd_approve_animatic(&args)
        .expect_err("scene 2 has no cuts and must not approve");
    assert!(
        err.contains("no animatic") || err.contains("Add cut sketches"),
        "unexpected rejection: {err}"
    );
}

#[test]
fn animatic_approval_requires_durations_and_persists() {
    let _s = session();
    let root = &_s.1;
    let root_arg = root.to_string_lossy().to_string();

    let animatic = on_disk(&root)
        .scene(1)
        .unwrap()
        .animatic
        .clone()
        .expect("imported cuts carry an animatic");
    assert_eq!(animatic.shots.len(), 2);
    assert!(!animatic.approved, "imported animatic starts unapproved");

    // Out-of-range durations are refused, not clamped — at both bounds
    // (host/src/workflow/animatic.rs: MIN_DURATION 0.4, MAX_DURATION 120).
    assert!(workbench_host::workflow::cmd_set_animatic_duration(&json!({
        "projectRoot": root_arg,
        "sceneId": 1,
        "shotId": animatic.shots[0].id,
        "durationSeconds": 0.1,
    }))
    .is_err());
    assert!(workbench_host::workflow::cmd_set_animatic_duration(&json!({
        "projectRoot": root_arg,
        "sceneId": 1,
        "shotId": animatic.shots[0].id,
        "durationSeconds": 2000.0,
    }))
    .is_err());

    for shot in &animatic.shots {
        workbench_host::workflow::cmd_set_animatic_duration(&json!({
            "projectRoot": root_arg,
            "sceneId": 1,
            "shotId": shot.id,
            "durationSeconds": 2.0,
        }))
        .unwrap();
    }
    workbench_host::workflow::cmd_approve_animatic(&json!({
        "projectRoot": root_arg,
        "sceneId": 1,
    }))
    .expect("rough cut approves once every shot is timed");
    let approved = on_disk(&root);
    assert!(
        approved
            .scene(1)
            .unwrap()
            .animatic
            .as_ref()
            .unwrap()
            .approved,
        "approval must survive the round trip to disk"
    );

    // Timing edits after approval demote the rough cut back to unapproved.
    let shot_id = approved.scene(1).unwrap().animatic.as_ref().unwrap().shots[0]
        .id
        .clone();
    workbench_host::workflow::cmd_set_animatic_duration(&json!({
        "projectRoot": root_arg,
        "sceneId": 1,
        "shotId": shot_id,
        "durationSeconds": 3.5,
    }))
    .unwrap();
    let animatic = on_disk(&root).scene(1).unwrap().animatic.clone().unwrap();
    assert!(
        !animatic.approved,
        "an approved rough cut must not silently absorb a new duration"
    );
    assert_eq!(animatic.shots[0].duration_seconds, 3.5);
}

#[test]
fn reorder_cuts_renumbers_positionally_but_identity_survives() {
    // §8.3: the numeric CutId is positional (permute reassigns it), so the
    // identity contract lives in stable_id — and the animatic shots must be
    // relinked to follow the same cut across the renumbering.
    let _s = session();
    let root = &_s.1;
    let before = on_disk(&root);
    let scene = before.scene(1).unwrap().clone();
    let stable_of: HashMap<_, String> = scene
        .cuts
        .iter()
        .map(|cut| (cut.id, cut.stable_id.clone()))
        .collect();
    let anim_before: Vec<(String, String)> = scene
        .animatic
        .as_ref()
        .unwrap()
        .shots
        .iter()
        .map(|shot| (shot.id.clone(), stable_of[&shot.cut_id].clone()))
        .collect();

    let mut wanted: Vec<String> = scene.cuts.iter().map(|cut| cut.stable_id.clone()).collect();
    wanted.reverse();
    workbench_host::workflow::cmd_reorder_cuts(&json!({
        "projectRoot": root.to_string_lossy(),
        "sceneId": 1,
        "shotIds": wanted,
    }))
    .unwrap();

    let scene = on_disk(root).scene(1).unwrap().clone();
    assert_eq!(
        scene
            .cuts
            .iter()
            .map(|cut| cut.stable_id.clone())
            .collect::<Vec<_>>(),
        wanted,
        "reorder must change the order and nothing else"
    );
    assert!(
        scene
            .cuts
            .iter()
            .enumerate()
            .all(|(index, cut)| cut.id.cut == index as u32 + 1 && cut.id.source_scene == 1),
        "numeric ids must stay dense in display order"
    );
    let stable_of: HashMap<_, String> = scene
        .cuts
        .iter()
        .map(|cut| (cut.id, cut.stable_id.clone()))
        .collect();
    let anim_after: Vec<(String, String)> = scene
        .animatic
        .as_ref()
        .unwrap()
        .shots
        .iter()
        .map(|shot| (shot.id.clone(), stable_of[&shot.cut_id].clone()))
        .collect();
    let mut before_set = anim_before;
    let mut after_set = anim_after;
    before_set.sort();
    after_set.sort();
    assert_eq!(
        before_set, after_set,
        "animatic shot -> stable cut identity must survive renumbering"
    );
}
