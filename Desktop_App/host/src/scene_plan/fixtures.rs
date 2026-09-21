//! Canonical example documents and a throwaway project tree for tests.
use crate::scene_plan::commit::ModelEmit;
use crate::scene_plan::state::PlanState;
use std::fs;
use std::path::{Path, PathBuf};

pub const SAM_REF: &str = "/project/Characters/Sam_9x16.png";

/// Cues: (id suffix, speaker, text, on_camera).
pub fn lipsync_cut_yaml(
    source_scene: u32,
    cut: u32,
    cues: &[(&str, &str, &str, bool)],
    asset_path: &str,
) -> String {
    let id = format!("S{source_scene:02}C{cut:02}");
    let mut cue_block = String::new();
    for (suffix, speaker, text, on_camera) in cues {
        cue_block.push_str(&format!(
            "    - id: {id}_{suffix}\n      speaker: {speaker}\n      text: \"{text}\"\n      on_camera: {on_camera}\n      delivery: calm, conversational\n      estimated_duration_seconds: 2.8\n"
        ));
    }
    format!(
        r#"source_scene: {source_scene}
cut: {cut}
title: Sam addresses the camera
provider: atlascloud
stage: base_video
scene_type: character_performance
continuity:
  group: standalone
  relation_to_previous: new_setup
  opening_state: "Sam faces the camera in a medium shot."
  closing_state: "Sam settles in the same position."
  carry_forward: []
editing:
  estimated_screen_duration_seconds: 6.0
  head_handle_seconds: 1.0
  tail_handle_seconds: 1.0
  transition_in: cut
  transition_out: cut
  continuity_frame_strategy: none
  cut_guidance: null
speech:
  mode: source_video_lipsync
  language: en
  cues:
{cue_block}generation_requirements:
  source_mode: reference_to_video
  generation_path: audio_driven_i2v
  aspect_ratio: "9:16"
  required_capabilities:
    - character_reference
  priorities:
    - prompt_adherence
    - visual_consistency
prompt: |-
  Sam faces the camera and speaks naturally. Hold settled before speaking
  and settle again afterward. Locked-off camera with no zoom.
reference_assets:
  - path: "{asset_path}"
    role: character_reference
negative_prompt: "sideways scene, rotated horizon, Dutch angle"
"#
    )
}

pub fn lipsync_scene_yaml_with_cues(cut: u32, cues: &[(&str, &str, &str, bool)]) -> String {
    lipsync_cut_yaml(1, cut, cues, SAM_REF)
}

pub fn lipsync_scene_yaml(cut: u32, text: &str) -> String {
    lipsync_scene_yaml_with_cues(cut, &[("a", "Sam", text, true)])
}

pub fn voiceover_cut_yaml(
    source_scene: u32,
    cut: u32,
    cues: &[(&str, &str, &str, bool)],
) -> String {
    let id = format!("S{source_scene:02}C{cut:02}");
    let mut cue_block = String::new();
    for (suffix, speaker, text, on_camera) in cues {
        cue_block.push_str(&format!("    - id: {id}_{suffix}\n      speaker: {speaker}\n      text: \"{text}\"\n      on_camera: {on_camera}\n"));
    }
    format!(
        r#"source_scene: {source_scene}
cut: {cut}
title: Rain on the window
provider: atlascloud
stage: base_video
scene_type: dialogue
continuity:
  group: standalone
  relation_to_previous: new_setup
editing:
  estimated_screen_duration_seconds: 8.0
  head_handle_seconds: 1.0
  tail_handle_seconds: 1.0
  transition_in: cut
  transition_out: cut
speech:
  mode: voice_over
  language: en
  cues:
{cue_block}generation_requirements:
  source_mode: text_to_video
  generation_path: prompt_driven
  aspect_ratio: "9:16"
prompt: |-
  Rain streams down a night-train window; warm lamp light swings overhead.
"#
    )
}

pub fn broll_cut_yaml(source_scene: u32, cut: u32) -> String {
    format!(
        r#"source_scene: {source_scene}
cut: {cut}
title: City at dusk
provider: atlascloud
stage: base_video
scene_type: b_roll
continuity:
  group: standalone
  relation_to_previous: new_setup
editing:
  estimated_screen_duration_seconds: 4.0
  head_handle_seconds: 0.5
  tail_handle_seconds: 0.5
  transition_in: cut
  transition_out: dissolve
speech:
  mode: none
generation_requirements:
  source_mode: text_to_video
  generation_path: prompt_driven
  aspect_ratio: "9:16"
prompt: |-
  Slow aerial drift over a city skyline at dusk, warm practicals switching on.
"#
    )
}

pub fn broll_scene_yaml(cut: u32) -> String {
    broll_cut_yaml(1, cut)
}

/// Creative-only YAML: no provider, stage, cue ids, generation_requirements, or durations.
pub fn slim_lipsync_yaml(cut: u32, text: &str) -> String {
    format!(
        r#"source_scene: 1
cut: {cut}
title: Sam addresses the camera
scene_type: character_performance
continuity:
  group: standalone
  relation_to_previous: new_setup
  opening_state: "Sam faces the camera in a medium shot."
  closing_state: "Sam settles in the same position."
editing:
  pace: natural
  transition_in: cut
  transition_out: cut
speech:
  mode: source_video_lipsync
  cues:
    - speaker: Sam
      text: "{text}"
      on_camera: true
      delivery: calm, conversational
prompt: |-
  Sam faces the camera and speaks naturally. Locked-off camera with no zoom.
negative_prompt: "sideways scene, rotated horizon"
"#
    )
}

pub fn slim_broll_yaml(cut: u32) -> String {
    format!(
        r#"source_scene: 1
cut: {cut}
title: City at dusk
scene_type: b_roll
continuity:
  group: standalone
  relation_to_previous: new_setup
editing:
  transition_in: cut
  transition_out: dissolve
speech:
  mode: none
prompt: |-
  Slow aerial drift over a city skyline at dusk, warm practicals switching on.
"#
    )
}

pub fn cut_markdown(source_scene: u32, cut: u32, yaml: &str) -> String {
    format!("### Scene {source_scene} — Cut {cut}\n\n#### Atlas Generation Handoff\n\n```yaml\n{}\n```\n", yaml.trim_end())
}

pub fn scene_markdown(cut: u32, yaml: &str) -> String {
    cut_markdown(1, cut, yaml)
}

pub fn emit(files: &[(&str, String)]) -> ModelEmit {
    ModelEmit {
        files: files
            .iter()
            .map(|(p, c)| (p.to_string(), c.clone()))
            .collect(),
    }
}

/// `sections` source scenes 1..=n; section 0 (Scene 1) is current.
pub fn plan_state(sections: usize) -> PlanState {
    let mut state = PlanState::new(Path::new("scene_prompts"), Some("9:16".into()));
    state.script_path = "script.md".into();
    for n in 1..=sections as u32 {
        state.ensure_section(n, &format!("Scene {n}"), &format!("Scene {n}"));
    }
    state
}

pub struct FixtureProject {
    pub root: PathBuf,
    pub tom_closeup: PathBuf,
    pub tom_standing: PathBuf,
    pub nell_closeup: PathBuf,
    pub pip_closeup: PathBuf,
}

fn manifest(name: &str, approval: &Path, shots: &[(&str, &Path, &str)]) -> String {
    let mut out = format!(
        "pack_version: 1\ncharacter: \"{name}\"\naspect_ratio: \"3:4\"\nidentity_source:\n  character_sheet: \"\"\n  reference_images: []\n  approval_shot: closeup_front\n  approval_shot_path: \"{}\"\nshots:\n",
        approval.display()
    );
    for (id, path, status) in shots {
        out.push_str(&format!("  - shot_id: {id}\n    path: \"{}\"\n    prediction_id: \"\"\n    model_id: x/y\n    status: {status}\n", path.display()));
    }
    out
}

fn voice_profile(name: &str, voice_id: &str, voice_name: &str) -> String {
    format!(
        "profile_version: 1\ncharacter: \"{name}\"\nsource_mode: designed_voice\nprovider: elevenlabs\nmodel_id: eleven_multilingual_ttv_v2\nvoice_id: \"{voice_id}\"\nvoice_name: \"{voice_name}\"\nlanguage: null\napproved_audio:\n- path: \"voice/sample.mp3\"\npurpose: canonical_sample\ncreated_at: \"SystemTime {{ tv_sec: 1, tv_nsec: 2 }}\"\n"
    )
}

/// Writes a `Characters/` tree shaped like the real project: Tom (sheet, no
/// voice, one missing shot), Nell (sheet + voice, pending shots), Pip (sheet
/// + voice), Aunt Marian (manifest but no files), Mr Harker (voice only).
pub fn write_fixture_project(root: &Path) -> FixtureProject {
    let chars = root.join("Characters");
    let png = b"\x89PNG fixture";

    let tom = chars.join("Tom Vale").join("model_sheet");
    fs::create_dir_all(&tom).expect("fixture dir");
    let tom_closeup = tom.join("tom_closeup_front.png");
    let tom_standing = tom.join("tom_standing_front.png");
    fs::write(&tom_closeup, png).unwrap();
    fs::write(&tom_standing, png).unwrap();
    fs::write(
        tom.join("model_sheet_manifest.yaml"),
        manifest(
            "Tom Vale",
            &tom_closeup,
            &[
                ("closeup_front", &tom_closeup, "saved"),
                ("standing_front", &tom_standing, "saved"),
                (
                    "expression_sad",
                    &tom.join("tom_expression_sad.png"),
                    "saved",
                ),
            ],
        ),
    )
    .unwrap();

    let nell = chars.join("Nell Vale").join("model_sheet");
    fs::create_dir_all(&nell).unwrap();
    let nell_closeup = nell.join("nell_closeup_front.jpg");
    fs::write(&nell_closeup, png).unwrap();
    fs::write(
        nell.join("model_sheet_manifest.yaml"),
        manifest(
            "Nell Vale",
            &nell_closeup,
            &[
                ("closeup_front", &nell_closeup, "saved"),
                (
                    "expression_excited",
                    &nell.join("nell_expression_excited.png"),
                    "pending",
                ),
            ],
        ),
    )
    .unwrap();
    let nell_voice = chars.join("Nell Vale").join("voice");
    fs::create_dir_all(&nell_voice).unwrap();
    fs::write(
        nell_voice.join("Nell_voice_profile.yaml"),
        voice_profile("Nell Vale", "NellVoice001", "Ivy"),
    )
    .unwrap();

    let pip = chars.join("Pip Vale").join("model_sheet");
    fs::create_dir_all(&pip).unwrap();
    let pip_closeup = pip.join("pip_closeup_front.jpg");
    fs::write(&pip_closeup, png).unwrap();
    fs::write(
        pip.join("model_sheet_manifest.yaml"),
        manifest(
            "Pip Vale",
            &pip_closeup,
            &[("closeup_front", &pip_closeup, "saved")],
        ),
    )
    .unwrap();
    let pip_voice = chars.join("Pip Vale").join("voice");
    fs::create_dir_all(&pip_voice).unwrap();
    fs::write(
        pip_voice.join("Pip_voice_profile.yaml"),
        voice_profile("Pip Vale", "MlxAbUenU34aJO4OFnwY", "Benny"),
    )
    .unwrap();

    let marian = chars.join("Aunt Marian");
    fs::create_dir_all(&marian).unwrap();
    fs::write(
        marian.join("model_sheet_manifest.yaml"),
        manifest(
            "Aunt Marian",
            &marian.join("marian_closeup_front.png"),
            &[(
                "closeup_front",
                &marian.join("marian_closeup_front.png"),
                "saved",
            )],
        ),
    )
    .unwrap();

    let harker_voice = chars.join("Mr Harker").join("voice");
    fs::create_dir_all(&harker_voice).unwrap();
    fs::write(
        harker_voice.join("Harker_voice_profile.yaml"),
        voice_profile("Mr Harker", "HarkerVoice001", "Gruff"),
    )
    .unwrap();

    FixtureProject {
        root: root.to_path_buf(),
        tom_closeup,
        tom_standing,
        nell_closeup,
        pip_closeup,
    }
}
