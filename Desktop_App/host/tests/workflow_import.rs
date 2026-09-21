use std::fs;
use std::path::{Path, PathBuf};
use workbench_host::scene_plan::fixtures::{
    broll_cut_yaml, cut_markdown, lipsync_cut_yaml, voiceover_cut_yaml,
};
use workbench_host::scene_plan::handoff::SpeechMode;
use workbench_host::workflow::state::{AssetRole, STATE_REL};
use workbench_host::workflow::{import_project, ImportOptions};

const SCRIPT: &str = r#"# The Lantern at Willowby Halt

**Format:** Vertical 9:16

## Scenes

### Scene 1 — The last train

- **Location:** A two-carriage night train approaching Willowby Halt.
- **Action:** Rain streams down the carriage windows. Tom guards a brown paper parcel.
- **Dialogue:**
- **Tom:** We shall be there before supper.
- **Pip:** Will there be cocoa?
- **Tom:** Aunt Marian promised clean sheets and hot cocoa.
- **Pip:** And jam?
- **Nell:** If Aunt Marian has any.
- **Pip:** Then we must get there quickly.
- **Sound:** Train wheels clatter. Rain strikes the glass.
- **Action:** The train slows. The children gather the parcel.
- **Dialogue:**
- **Guard, outside:** Willowby Halt!
- **Transition:** Match cut from the warm window to the red lantern.

### Scene 2 — An empty platform

- **Location:** Willowby Halt platform, night.
- **Dialogue:**
- **Pip:** Has Aunt Marian forgotten us?
"#;

fn write_cut(dir: &Path, n: u32, yaml: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join(format!("Cut{n}.txt")), cut_markdown(1, n, yaml)).unwrap();
}

fn write_character(root: &Path, name: &str, file_stem: &str) {
    let dir = root.join("Characters").join(name).join("model_sheet");
    fs::create_dir_all(&dir).unwrap();
    let img = dir.join(format!("{file_stem}.png"));
    fs::write(&img, b"fake-image").unwrap();
    let rel = format!("model_sheet/{file_stem}.png");
    fs::write(
        dir.join("model_sheet_manifest.yaml"),
        format!(
            "character: \"{name}\"\nidentity_source:\n  approval_shot_path: {rel}\nshots:\n  - shot_id: closeup_front\n    path: {rel}\n    status: saved\n"
        ),
    )
    .unwrap();
}

fn write_lite_shaped(root: &Path) {
    fs::create_dir_all(root.join("Scripts")).unwrap();
    fs::write(
        root.join("Scripts/Lantern_at_Willowby_Halt_8min_9x16.md"),
        SCRIPT,
    )
    .unwrap();
    write_character(root, "Tom Vale", "tom_closeup_front");
    write_character(root, "Pip Vale", "pip_closeup_front");
    write_character(root, "Nell Vale", "nell_closeup_front");

    let tom = root
        .join("Characters/Tom Vale/model_sheet/tom_closeup_front.png")
        .to_string_lossy()
        .to_string();
    let pip = root
        .join("Characters/Pip Vale/model_sheet/pip_closeup_front.png")
        .to_string_lossy()
        .to_string();
    let nell = root
        .join("Characters/Nell Vale/model_sheet/nell_closeup_front.png")
        .to_string_lossy()
        .to_string();

    let lines: [(&str, u32, String); 9] = [
        (
            "Tom",
            1,
            lipsync_cut_yaml(
                1,
                1,
                &[("a", "Tom", "We shall be there before supper.", true)],
                &tom,
            ),
        ),
        (
            "Pip",
            2,
            lipsync_cut_yaml(1, 2, &[("a", "Pip", "Will there be cocoa?", true)], &pip),
        ),
        (
            "Tom",
            3,
            lipsync_cut_yaml(
                1,
                3,
                &[(
                    "a",
                    "Tom",
                    "Aunt Marian promised clean sheets and hot cocoa.",
                    true,
                )],
                &tom,
            ),
        ),
        (
            "Pip",
            4,
            lipsync_cut_yaml(1, 4, &[("a", "Pip", "And jam?", true)], &pip),
        ),
        (
            "Nell",
            5,
            lipsync_cut_yaml(
                1,
                5,
                &[("a", "Nell", "If Aunt Marian has any.", true)],
                &nell,
            ),
        ),
        (
            "Pip",
            6,
            lipsync_cut_yaml(
                1,
                6,
                &[("a", "Pip", "Then we must get there quickly.", true)],
                &pip,
            ),
        ),
        ("silent", 7, broll_cut_yaml(1, 7)),
        (
            "Guard",
            8,
            voiceover_cut_yaml(1, 8, &[("a", "Guard", "Willowby Halt!", false)]),
        ),
        ("silent", 9, broll_cut_yaml(1, 9)),
    ];

    let sidecar = root.join("Scene_Prompts/Scene1");
    let mut prod = String::new();
    for (_, n, yaml) in &lines {
        write_cut(&sidecar, *n, yaml);
        prod.push_str(&cut_markdown(1, *n, yaml));
        prod.push('\n');
    }
    fs::write(root.join("prod.md"), prod).unwrap();
}

#[test]
fn imports_lite_challenge_shaped_scene1_mix_and_missing_preproduction() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Lite_Challenge");
    write_lite_shaped(&root);

    let imported = import_project(&root, ImportOptions { persist: true }).unwrap();
    let scene = imported
        .report
        .scenes
        .iter()
        .find(|s| s.id == 1)
        .expect("Scene 1");

    assert_eq!(scene.cuts_found, 9);
    assert_eq!(
        (scene.mix.lipsync, scene.mix.voice_over, scene.mix.silent),
        (6, 1, 2)
    );
    assert_eq!(scene.approved_start_frames, 0);
    assert!(scene.production_notes);
    assert!(!scene.scene_look);
    assert!(!scene.scene_intent);
    assert!(!scene.cut_sketches);
    assert!(scene.can_backfill_sketches);
    assert!(scene.effectively_performance_heavy);
    assert!(scene.summary.contains("9 cuts found"));
    assert!(scene
        .summary
        .contains("6 lip-sync / 1 voice-over / 2 silent"));
    assert!(scene
        .summary
        .contains("0 approved cut-specific start frames"));
    assert!(scene.summary.contains("production notes exist"));
    assert!(scene.summary.contains("scene look board missing"));
    assert!(scene
        .summary
        .contains("cut sketches can be backfilled from production notes"));

    let state = &imported.state;
    assert_eq!(state.scripts.len(), 1);
    assert_eq!(state.settings.aspect_ratio.as_deref(), Some("9:16"));
    assert_eq!(state.scenes.len(), 2);
    let s1 = state.scene(1).unwrap();
    assert_eq!(s1.dialogue.len(), 7);
    assert_eq!(s1.action.len(), 2);
    assert!(!s1.sound.is_empty());
    assert!(!s1.transitions.is_empty());
    assert_eq!(s1.cuts.len(), 9);
    assert_eq!(s1.cuts[0].speech_mode, Some(SpeechMode::SourceVideoLipsync));
    assert_eq!(s1.cuts[6].speech_mode, Some(SpeechMode::Silent));
    assert_eq!(s1.cuts[7].speech_mode, Some(SpeechMode::VoiceOver));
    assert!(s1.cuts.iter().all(|c| c.approved_start_frame.is_none()));
    assert!(s1.cuts.iter().any(|c| !c.identity_ref_paths.is_empty()));
    assert!(state
        .assets
        .iter()
        .any(|a| a.role == AssetRole::IdentityReference));
    assert!(!state
        .assets
        .iter()
        .any(|a| a.role == AssetRole::CutStartFrame));
    assert!(state
        .characters
        .iter()
        .any(|c| c.name == "Tom Vale" && c.identity_ready));

    let persisted = imported.persisted_path.expect("wrote workflow state");
    assert_eq!(persisted, root.join(STATE_REL));
    assert!(persisted.is_file());
}

#[test]
fn preserves_v2_intent_on_reimport() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("show");
    write_lite_shaped(&root);
    let mut first = import_project(&root, ImportOptions { persist: true }).unwrap();
    first.state.scene_mut(1, "Scene 1 — The last train").intent =
        Some(workbench_host::workflow::state::SceneIntent {
            narrative_purpose: Some("children leave warmth for danger".into()),
            ..Default::default()
        });
    workbench_host::scene_plan::commit::write_atomic(
        &root.join(STATE_REL),
        first.state.to_json().unwrap().as_bytes(),
    )
    .unwrap();

    let again = import_project(&root, ImportOptions { persist: false }).unwrap();
    assert_eq!(
        again
            .state
            .scene(1)
            .unwrap()
            .intent
            .as_ref()
            .unwrap()
            .narrative_purpose
            .as_deref(),
        Some("children leave warmth for danger")
    );
    assert!(again.report.scenes[0].scene_intent);
}

#[test]
fn backfill_sketches_from_notes_are_rejected_for_balanced() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Lite_Challenge");
    write_lite_shaped(&root);
    let mut imported = import_project(&root, ImportOptions { persist: true }).unwrap();
    let scene = imported.state.scene_mut(1, "Scene 1 — The last train");
    scene.strategy = Some(workbench_host::scene_plan::strategy::SceneStrategy::CinematicBalanced);
    let n = workbench_host::workflow::sketches::backfill_scene(scene, true);
    assert_eq!(n, 9);
    let verdict = workbench_host::workflow::validate::validate_scene_mix(scene);
    assert_eq!(
        verdict.status,
        workbench_host::workflow::validate::VerdictStatus::Warn,
        "{:?}",
        verdict.messages
    );
    assert!(verdict.mix.performance >= 6);
    assert!(verdict.messages.iter().any(|m| m.contains("60%")));
    let reaction = scene.cuts.iter_mut().find(|c| c.id.cut == 2).unwrap();
    workbench_host::workflow::sketches::patch_sketch(
        reaction,
        Some(workbench_host::workflow::state::ShotRole::Reaction),
        Some(workbench_host::workflow::state::MouthPolicy::ClosedOrNotVisible),
        None,
        None,
        None,
    );
    assert_eq!(
        reaction.speech_mode,
        Some(workbench_host::scene_plan::handoff::SpeechMode::VoiceOver)
    );
}

#[test]
fn character_model_sheet_is_identity_not_a_start_frame() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("show");
    write_lite_shaped(&root);
    let imported = import_project(&root, ImportOptions { persist: false }).unwrap();
    let tom = imported
        .state
        .characters
        .iter()
        .find(|c| c.name == "Tom Vale")
        .unwrap();
    let roles: Vec<_> = imported
        .state
        .assets
        .iter()
        .filter(|a| a.character_id.as_deref() == Some(tom.id.as_str()))
        .map(|a| a.role)
        .collect();
    assert!(roles.contains(&AssetRole::IdentityReference));
    assert!(!roles.contains(&AssetRole::CutStartFrame));
    assert_eq!(
        tom.model_sheet_manifest_path.as_deref(),
        Some("Characters/Tom Vale/model_sheet/model_sheet_manifest.yaml")
    );
}

#[test]
fn real_lite_challenge_reports_six_one_two_if_present() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../Projects/Lite_Challenge");
    if !root.is_dir() {
        return;
    }
    let imported = import_project(&root, ImportOptions { persist: false }).unwrap();
    let scene = imported
        .report
        .scenes
        .iter()
        .find(|s| s.id == 1)
        .expect("Scene 1");
    assert_eq!(scene.cuts_found, 9, "{}", scene.summary);
    assert_eq!(
        (scene.mix.lipsync, scene.mix.voice_over, scene.mix.silent),
        (6, 1, 2),
        "{}",
        scene.summary
    );
    assert_eq!(scene.approved_start_frames, 0);
    assert!(scene.production_notes);
    assert!(scene.scene_look);
    assert_eq!(scene.can_backfill_sketches, !scene.cut_sketches);
    assert!(scene.effectively_performance_heavy);
    assert!(imported.state.characters.len() >= 5);
    assert_eq!(imported.state.scenes.len(), 9);
}
