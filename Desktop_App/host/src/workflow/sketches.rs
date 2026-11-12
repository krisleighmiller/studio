//! Cut-sketch backfill and LLM apply. Host owns IDs, numbering, speech, mix.
use crate::scene_plan::handoff::SpeechMode;
use crate::workflow::state::{
    AssetRole, CutRecord, CutSketch, MouthPolicy, SceneRecord, ShotRole, SketchDialogue,
};
use crate::workflow::validate::{
    apply_derived_speech, default_mouth, has_dialogue, validate_scene_mix,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize, Default)]
struct LlmSketchFile {
    #[serde(default)]
    cuts: Vec<LlmCut>,
}

#[derive(Debug, Deserialize, Default)]
struct LlmCut {
    #[serde(default)]
    cut: Option<u32>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    shot_role: Option<String>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    speaker: Option<String>,
    #[serde(default)]
    mouth_policy: Option<String>,
    #[serde(default)]
    pace: Option<String>,
    #[serde(default)]
    visual_prompt: Option<String>,
    #[serde(default)]
    negative_prompt: Option<String>,
    #[serde(default)]
    reference_roles: Vec<String>,
    #[serde(default)]
    transition_idea: Option<String>,
    #[serde(default)]
    dialogue: Vec<LlmDialogue>,
    #[serde(default)]
    on_camera: Vec<String>,
    /// Ignored — host derives speech from role/mouth.
    #[serde(default)]
    speech_mode: Option<String>,
    /// Ignored — host owns paths.
    #[serde(default)]
    output_path: Option<String>,
    #[serde(default)]
    production_note_path: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct LlmDialogue {
    #[serde(default)]
    speaker: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    heard_as: Option<String>,
}

pub fn backfill_scene(scene: &mut SceneRecord, replace: bool) -> usize {
    let mut n = 0;
    for cut in &mut scene.cuts {
        if !replace && cut.sketch.as_ref().is_some_and(|s| !s.is_empty()) {
            continue;
        }
        if !cut.has_production_note() && cut.dialogue.is_empty() && cut.prompt.is_none() {
            continue;
        }
        cut.sketch = Some(sketch_from_note(cut));
        apply_derived_speech(cut);
        n += 1;
    }
    n
}

pub fn sketch_from_note(cut: &CutRecord) -> CutSketch {
    let has_d = !cut.dialogue.is_empty();
    let (role, mouth, heard) = match cut.speech_mode {
        Some(SpeechMode::SourceVideoLipsync) => (
            ShotRole::Performance,
            MouthPolicy::SpeakingVisible,
            Some("lip_sync"),
        ),
        Some(SpeechMode::VoiceOver) => (
            ShotRole::Reaction,
            MouthPolicy::OffCamera,
            Some("voice_over"),
        ),
        _ if has_d => (
            ShotRole::Atmosphere,
            MouthPolicy::ClosedOrNotVisible,
            Some("voice_over"),
        ),
        _ => (ShotRole::Insert, MouthPolicy::NoPeople, None),
    };
    let dialogue = cut
        .dialogue
        .iter()
        .map(|line| SketchDialogue {
            speaker: line.speaker.clone(),
            text: line.text.clone(),
            heard_as: heard.map(|s| s.to_string()),
        })
        .collect();
    CutSketch {
        title: cut.title.clone(),
        shot_role: Some(role),
        subject: cut.subject.clone(),
        speaker: cut.dialogue.first().map(|l| l.speaker.clone()),
        mouth_policy: Some(mouth),
        pace: None,
        visual_prompt: cut.prompt.clone(),
        negative_prompt: cut.negative_prompt.clone(),
        reference_roles: Vec::new(),
        transition_idea: None,
        dialogue,
        on_camera: Vec::new(),
    }
}

pub fn apply_llm_sketches(scene: &mut SceneRecord, value: &Value) -> Result<usize, String> {
    let file: LlmSketchFile = if value.get("cuts").is_some() {
        serde_json::from_value(value.clone()).map_err(|e| format!("cut sketch JSON: {e}"))?
    } else if value.as_array().is_some() {
        LlmSketchFile {
            cuts: serde_json::from_value(value.clone())
                .map_err(|e| format!("cut sketch JSON: {e}"))?,
        }
    } else {
        return Err("cut sketch JSON must be {\"cuts\":[...]}".into());
    };
    if file.cuts.is_empty() {
        return Err("cut sketch JSON had no cuts".into());
    }
    let max_cuts = scene.max_cuts.unwrap_or(12);
    if file.cuts.len() > max_cuts as usize {
        return Err(format!(
            "Model returned {} cuts, above this scene's maximum of {max_cuts}; no sketches were saved.",
            file.cuts.len()
        ));
    }

    let existing = !scene.cuts.is_empty();
    if existing {
        for (i, proposed) in file.cuts.iter().enumerate() {
            let n = proposed.cut.unwrap_or((i as u32) + 1);
            let cut = scene.cut_mut(n);
            apply_llm_cut(cut, proposed);
            let _ = proposed.speech_mode.as_ref();
            let _ = proposed.output_path.as_ref();
            let _ = proposed.production_note_path.as_ref();
        }
    } else {
        for (i, proposed) in file.cuts.iter().enumerate() {
            let n = i as u32 + 1;
            let cut = scene.cut_mut(n);
            apply_llm_cut(cut, proposed);
        }
    }
    Ok(scene
        .cuts
        .iter()
        .filter(|c| c.sketch.as_ref().is_some_and(|s| !s.is_empty()))
        .count())
}

fn apply_llm_cut(cut: &mut CutRecord, proposed: &LlmCut) {
    let mut sketch = cut.sketch.clone().unwrap_or_default();
    if let Some(title) = nonempty(&proposed.title) {
        sketch.title = Some(title.clone());
        if cut.title.is_none() {
            cut.title = Some(title);
        }
    }
    if let Some(role) = proposed.shot_role.as_deref().and_then(ShotRole::parse) {
        sketch.shot_role = Some(role);
    }
    if let Some(subject) = nonempty(&proposed.subject) {
        sketch.subject = Some(subject.clone());
        cut.subject = Some(subject);
    }
    if let Some(speaker) = nonempty(&proposed.speaker) {
        sketch.speaker = Some(speaker);
    }
    let has_d = !proposed.dialogue.is_empty() || has_dialogue(cut);
    if let Some(mouth) = proposed
        .mouth_policy
        .as_deref()
        .and_then(MouthPolicy::parse)
    {
        sketch.mouth_policy = Some(mouth);
    } else if sketch.mouth_policy.is_none() {
        if let Some(role) = sketch.shot_role {
            sketch.mouth_policy = Some(default_mouth(role, has_d));
        }
    }
    if let Some(pace) = proposed.pace.as_deref() {
        sketch.pace = serde_json::from_value(Value::String(pace.to_ascii_lowercase())).ok();
    }
    if let Some(prompt) = nonempty(&proposed.visual_prompt) {
        sketch.visual_prompt = Some(prompt);
    }
    if let Some(neg) = nonempty(&proposed.negative_prompt) {
        sketch.negative_prompt = Some(neg);
    }
    if !proposed.reference_roles.is_empty() {
        sketch.reference_roles = proposed
            .reference_roles
            .iter()
            .filter_map(|s| parse_look_role(s))
            .collect();
    }
    if let Some(t) = nonempty(&proposed.transition_idea) {
        sketch.transition_idea = Some(t);
    }
    if !proposed.dialogue.is_empty() {
        sketch.dialogue = proposed
            .dialogue
            .iter()
            .filter(|d| !d.text.trim().is_empty())
            .map(|d| SketchDialogue {
                speaker: d.speaker.trim().to_string(),
                text: d.text.trim().to_string(),
                heard_as: d.heard_as.clone(),
            })
            .collect();
        if cut.dialogue.is_empty() {
            cut.dialogue = sketch
                .dialogue
                .iter()
                .map(|d| crate::script::DialogueLine {
                    speaker: d.speaker.clone(),
                    text: d.text.clone(),
                })
                .collect();
        }
    }
    if !proposed.on_camera.is_empty() {
        sketch.on_camera = proposed
            .on_camera
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    cut.sketch = Some(sketch);
    apply_derived_speech(cut);
}

pub fn patch_sketch(
    cut: &mut CutRecord,
    shot_role: Option<ShotRole>,
    mouth_policy: Option<MouthPolicy>,
    title: Option<String>,
    visual_prompt: Option<String>,
    subject: Option<String>,
) {
    let mut sketch = cut.sketch.clone().unwrap_or_else(|| sketch_from_note(cut));
    if let Some(role) = shot_role {
        sketch.shot_role = Some(role);
        let has_d = has_dialogue(cut);
        if mouth_policy.is_none() {
            sketch.mouth_policy = Some(default_mouth(role, has_d));
        }
    }
    if let Some(mouth) = mouth_policy {
        sketch.mouth_policy = Some(mouth);
    }
    if let Some(title) = nonempty(&title) {
        sketch.title = Some(title.clone());
        cut.title = Some(title);
    }
    if let Some(prompt) = nonempty(&visual_prompt) {
        sketch.visual_prompt = Some(prompt);
    }
    if let Some(subject) = nonempty(&subject) {
        sketch.subject = Some(subject.clone());
        cut.subject = Some(subject);
    }
    cut.sketch = Some(sketch);
    apply_derived_speech(cut);
}

pub fn replace_sketch(cut: &mut CutRecord, value: &Value) -> Result<(), String> {
    let mut sketch: CutSketch = serde_json::from_value(value.clone())
        .map_err(|error| format!("invalid cut sketch: {error}"))?;
    if sketch
        .reference_roles
        .iter()
        .any(|role| !role.is_scene_look())
    {
        return Err("cut sketch references must be scene-look roles".into());
    }
    sketch.title = clean_option(sketch.title);
    sketch.subject = clean_option(sketch.subject);
    sketch.speaker = clean_option(sketch.speaker);
    sketch.visual_prompt = clean_option(sketch.visual_prompt);
    sketch.negative_prompt = clean_option(sketch.negative_prompt);
    sketch.transition_idea = clean_option(sketch.transition_idea);
    sketch.on_camera = clean_list(sketch.on_camera);
    sketch.reference_roles.dedup();
    sketch.dialogue = sketch
        .dialogue
        .into_iter()
        .filter_map(|line| {
            let speaker = line.speaker.trim().to_string();
            let text = line.text.trim().to_string();
            if text.is_empty() {
                return None;
            }
            Some(SketchDialogue {
                speaker,
                text,
                heard_as: clean_option(line.heard_as),
            })
        })
        .collect();
    if sketch.is_empty() {
        return Err("cut sketch cannot be empty".into());
    }
    cut.title = sketch.title.clone();
    cut.subject = sketch.subject.clone();
    cut.dialogue = sketch
        .dialogue
        .iter()
        .map(|line| crate::script::DialogueLine {
            speaker: line.speaker.clone(),
            text: line.text.clone(),
        })
        .collect();
    cut.sketch = Some(sketch);
    apply_derived_speech(cut);
    Ok(())
}

fn clean_option(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn clean_list(values: Vec<String>) -> Vec<String> {
    let mut cleaned = Vec::new();
    for value in values {
        let value = value.trim().to_string();
        if !value.is_empty() && !cleaned.contains(&value) {
            cleaned.push(value);
        }
    }
    cleaned
}

fn nonempty(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn parse_look_role(raw: &str) -> Option<AssetRole> {
    let v = raw.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    serde_json::from_value(Value::String(v))
        .ok()
        .filter(|role| {
            !matches!(
                role,
                AssetRole::IdentityReference
                    | AssetRole::BodyReference
                    | AssetRole::ExpressionReference
                    | AssetRole::CutStartFrame
                    | AssetRole::VideoTake
                    | AssetRole::AudioTake
                    | AssetRole::SelectedTake
            )
        })
}

pub fn sketches_system_prompt(existing_cuts: usize, max_cuts: u32) -> String {
    let numbering = if existing_cuts > 0 {
        format!(
            "There are already {existing_cuts} cuts. Emit exactly those cut numbers 1..{existing_cuts}. Do not add or remove cuts."
        )
    } else {
        format!("Propose between 1 and {max_cuts} cuts. Use only the cuts needed for the scene's pace; cover dialogue and essential action without inventing extra angles. The host will number them 1..N in order.")
    };
    format!(
        "You draft typed cut sketches for a production desk. Return ONLY JSON:\n\
{{\"cuts\":[{{\n  \"cut\":1,\n  \"title\":\"...\",\n  \"shot_role\":\"performance|reaction|insert|atmosphere|establishing|transition|action|detail\",\n  \"subject\":\"...\",\n  \"speaker\":\"optional\",\n  \"mouth_policy\":\"speaking_visible|closed_or_not_visible|off_camera|no_people\",\n  \"pace\":\"tight|natural|held\",\n  \"visual_prompt\":\"...\",\n  \"negative_prompt\":\"...\",\n  \"reference_roles\":[\"location_reference\"],\n  \"transition_idea\":\"optional\",\n  \"dialogue\":[{{\"speaker\":\"Tom\",\"text\":\"...\",\"heard_as\":\"voice_over\"}}],\n  \"on_camera\":[\"Pip\"]\n}}]}}\n\
{numbering}\n\
Do not emit file paths, speech_mode, production notes, or model IDs. The host derives those.\n\
Follow the requested strategy. Performance + speaking_visible is lip-sync. Reaction/insert/atmosphere with closed or off_camera mouths carry dialogue without lip-sync."
    )
}

pub fn sketches_user_prompt(
    scene: &SceneRecord,
    strategy_label: &str,
    strategy_guidance: &str,
    brief: &str,
) -> String {
    let mut s = format!(
        "Strategy: {strategy_label}\nHost rule: {strategy_guidance}\n\nScene: {}\n",
        scene.title
    );
    if let Some(intent) = &scene.intent {
        s.push_str("\nApproved/draft scene intent:\n");
        if let Some(v) = &intent.narrative_purpose {
            s.push_str(&format!("- narrative: {v}\n"));
        }
        if let Some(v) = &intent.emotional_purpose {
            s.push_str(&format!("- emotional: {v}\n"));
        }
        if !intent.character_focus.is_empty() {
            s.push_str(&format!("- focus: {}\n", intent.character_focus.join(", ")));
        }
        if let Some(v) = &intent.audience_must_know {
            s.push_str(&format!("- audience must know: {v}\n"));
        }
        if !intent.important_props.is_empty() {
            s.push_str(&format!("- props: {}\n", intent.important_props.join(", ")));
        }
    }
    s.push('\n');
    s.push_str(&crate::workflow::look::look_summary(scene));
    s.push('\n');
    s.push_str("\nScript beats:\n");
    s.push_str(scene.source_excerpt.as_deref().unwrap_or("(no excerpt)"));
    s.push('\n');
    if !scene.dialogue.is_empty() {
        s.push_str("\nDialogue to cover:\n");
        for line in &scene.dialogue {
            s.push_str(&format!("- {}: {}\n", line.speaker, line.text));
        }
    }
    if !scene.cuts.is_empty() {
        s.push_str("\nExisting cuts (keep these numbers; change roles/mouths/prompts):\n");
        for cut in &scene.cuts {
            s.push_str(&format!(
                "- Cut {}: {} | mode={:?} | {}\n",
                cut.id.cut,
                cut.title.clone().unwrap_or_default(),
                cut.speech_mode,
                cut.dialogue
                    .iter()
                    .map(|d| format!("{}: {}", d.speaker, d.text))
                    .collect::<Vec<_>>()
                    .join(" / ")
            ));
        }
    }
    if !brief.trim().is_empty() {
        s.push_str("\nUser direction:\n");
        s.push_str(brief.trim());
        s.push('\n');
    }
    s
}

pub fn mix_correction(scene: &SceneRecord) -> String {
    let v = validate_scene_mix(scene);
    format!(
        "Workbench host rejected that mix for {}.\n{}\n{}\nRe-emit the full JSON. Change enough cuts to reaction/insert/atmosphere with closed_or_not_visible or off_camera mouths. Do not emit paths or speech_mode.",
        v.strategy.label(),
        v.mix.display(),
        v.messages.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_plan::handoff::{CutId, Pace};
    use crate::script::DialogueLine;
    use crate::workflow::validate::is_performance_dialogue;
    use serde_json::json;

    #[test]
    fn scene_cut_limit_is_backward_compatible_and_round_trips() {
        let legacy = json!({"id": 1, "title": "Scene 1"});
        let mut scene: SceneRecord = serde_json::from_value(legacy).unwrap();
        assert_eq!(scene.max_cuts, None);
        scene.max_cuts = Some(2);
        let restored: SceneRecord = serde_json::from_value(serde_json::to_value(scene).unwrap()).unwrap();
        assert_eq!(restored.max_cuts, Some(2));
        assert!(sketches_system_prompt(0, 2).contains("between 1 and 2 cuts"));
        assert!(!sketches_system_prompt(0, 2).contains("5–12"));
    }

    #[test]
    fn over_limit_model_response_never_mutates_new_or_existing_cuts() {
        let mut scene = SceneRecord::new(1, "Short scene");
        scene.max_cuts = Some(2);
        let oversized = json!({"cuts": [{"title":"A"}, {"title":"B"}, {"title":"C"}]});
        assert!(apply_llm_sketches(&mut scene, &oversized).unwrap_err().contains("maximum of 2"));
        assert!(scene.cuts.is_empty());
        assert_eq!(apply_llm_sketches(&mut scene, &json!({"cuts": [{"title":"A"}]})).unwrap(), 1);
        let prior = scene.clone();
        assert!(apply_llm_sketches(&mut scene, &oversized).is_err());
        assert_eq!(scene, prior);
    }

    #[test]
    fn backfill_maps_lipsync_to_performance() {
        let mut cut = CutRecord::new(CutId::new(1, 1));
        cut.speech_mode = Some(SpeechMode::SourceVideoLipsync);
        cut.dialogue = vec![DialogueLine {
            speaker: "Tom".into(),
            text: "Supper.".into(),
        }];
        cut.prompt = Some("Tom speaks".into());
        let sketch = sketch_from_note(&cut);
        assert_eq!(sketch.shot_role, Some(ShotRole::Performance));
        assert_eq!(sketch.mouth_policy, Some(MouthPolicy::SpeakingVisible));
    }

    #[test]
    fn llm_cannot_set_speech_or_paths() {
        let mut scene = SceneRecord::new(1, "Scene 1");
        let mut existing = CutRecord::new(CutId::new(1, 1));
        existing.dialogue = vec![DialogueLine {
            speaker: "Tom".into(),
            text: "Supper.".into(),
        }];
        existing.production_note_path = Some("Scene_Prompts/Scene1/Cut1.txt".into());
        scene.cuts.push(existing);
        apply_llm_sketches(
            &mut scene,
            &json!({
                "cuts": [{
                    "cut": 1,
                    "shot_role": "reaction",
                    "mouth_policy": "closed_or_not_visible",
                    "speech_mode": "source_video_lipsync",
                    "output_path": "/tmp/hack.mp4",
                    "production_note_path": "evil.md",
                    "visual_prompt": "Pip watches the parcel"
                }]
            }),
        )
        .unwrap();
        let cut = &scene.cuts[0];
        assert_eq!(
            cut.production_note_path.as_deref(),
            Some("Scene_Prompts/Scene1/Cut1.txt")
        );
        assert_eq!(cut.speech_mode, Some(SpeechMode::VoiceOver));
        assert!(!is_performance_dialogue(cut));
        assert_eq!(
            cut.sketch.as_ref().unwrap().shot_role,
            Some(ShotRole::Reaction)
        );
    }

    #[test]
    fn full_manual_edit_replaces_sketch_and_derives_speech() {
        let mut cut = CutRecord::new(CutId::new(1, 2));
        replace_sketch(
            &mut cut,
            &json!({
                "title": " Pip watches ",
                "shot_role": "reaction",
                "subject": " Pip Vale ",
                "speaker": "Tom Vale",
                "mouth_policy": "closed_or_not_visible",
                "pace": "held",
                "visual_prompt": " Pip studies the parcel. ",
                "negative_prompt": " lip movement ",
                "reference_roles": ["interior_reference", "wardrobe_reference"],
                "transition_idea": " Cut to the parcel. ",
                "dialogue": [{
                    "speaker": " Tom Vale ",
                    "text": " Supper. ",
                    "heard_as": " voice_over "
                }],
                "on_camera": [" Pip Vale ", "Pip Vale", ""]
            }),
        )
        .unwrap();

        let sketch = cut.sketch.as_ref().unwrap();
        assert_eq!(cut.title.as_deref(), Some("Pip watches"));
        assert_eq!(cut.subject.as_deref(), Some("Pip Vale"));
        assert_eq!(sketch.pace, Some(Pace::Held));
        assert_eq!(
            sketch.reference_roles,
            vec![AssetRole::InteriorReference, AssetRole::WardrobeReference]
        );
        assert_eq!(sketch.on_camera, vec!["Pip Vale"]);
        assert_eq!(sketch.dialogue[0].speaker, "Tom Vale");
        assert_eq!(sketch.dialogue[0].heard_as.as_deref(), Some("voice_over"));
        assert_eq!(cut.dialogue[0].text, "Supper.");
        assert_eq!(cut.speech_mode, Some(SpeechMode::VoiceOver));
    }

    #[test]
    fn manual_edit_rejects_non_scene_reference_roles() {
        let mut cut = CutRecord::new(CutId::new(1, 1));
        let error = replace_sketch(
            &mut cut,
            &json!({
                "title": "Bad reference",
                "reference_roles": ["cut_start_frame"]
            }),
        )
        .unwrap_err();
        assert!(error.contains("scene-look roles"));
        assert!(cut.sketch.is_none());
    }

    #[test]
    fn removing_sketch_dialogue_makes_the_cut_silent() {
        let mut cut = CutRecord::new(CutId::new(1, 3));
        cut.dialogue.push(DialogueLine {
            speaker: "Tom".into(),
            text: "Old coverage".into(),
        });
        replace_sketch(
            &mut cut,
            &json!({
                "title": "Silent parcel insert",
                "shot_role": "insert",
                "mouth_policy": "no_people",
                "dialogue": []
            }),
        )
        .unwrap();
        assert!(cut.dialogue.is_empty());
        assert_eq!(cut.speech_mode, Some(SpeechMode::Silent));
    }
}
