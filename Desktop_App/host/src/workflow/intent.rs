//! Scene-intent draft parse and host-owned save/approve.
use crate::workflow::state::SceneIntent;
use serde_json::Value;

pub fn intent_from_json(value: &Value) -> Result<SceneIntent, String> {
    let obj = value.as_object().ok_or_else(|| {
        "scene intent JSON must be an object with narrative_purpose, emotional_purpose, …"
            .to_string()
    })?;
    let intent = SceneIntent {
        narrative_purpose: string_field(obj.get("narrative_purpose")),
        emotional_purpose: string_field(obj.get("emotional_purpose")),
        character_focus: string_list(obj.get("character_focus")),
        visual_contrast: string_field(obj.get("visual_contrast")),
        important_props: string_list(obj.get("important_props")),
        continuity_constraints: string_list(obj.get("continuity_constraints")),
        sound_and_transition: string_list(
            obj.get("sound_and_transition").or_else(|| obj.get("sound")),
        ),
        audience_must_know: string_field(
            obj.get("audience_must_know")
                .or_else(|| obj.get("audience")),
        ),
    };
    if intent.is_empty() {
        return Err("scene intent JSON had no usable fields".into());
    }
    Ok(intent)
}

pub fn merge_intent(base: SceneIntent, patch: &SceneIntent) -> SceneIntent {
    SceneIntent {
        narrative_purpose: patch.narrative_purpose.clone().or(base.narrative_purpose),
        emotional_purpose: patch.emotional_purpose.clone().or(base.emotional_purpose),
        character_focus: if patch.character_focus.is_empty() {
            base.character_focus
        } else {
            patch.character_focus.clone()
        },
        visual_contrast: patch.visual_contrast.clone().or(base.visual_contrast),
        important_props: if patch.important_props.is_empty() {
            base.important_props
        } else {
            patch.important_props.clone()
        },
        continuity_constraints: if patch.continuity_constraints.is_empty() {
            base.continuity_constraints
        } else {
            patch.continuity_constraints.clone()
        },
        sound_and_transition: if patch.sound_and_transition.is_empty() {
            base.sound_and_transition
        } else {
            patch.sound_and_transition.clone()
        },
        audience_must_know: patch.audience_must_know.clone().or(base.audience_must_know),
    }
}

fn string_field(value: Option<&Value>) -> Option<String> {
    value
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
        Some(Value::String(s)) => s
            .split(|c| c == ',' || c == ';' || c == '\n')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
        _ => Vec::new(),
    }
}

pub fn intent_system_prompt() -> &'static str {
    "You draft a typed scene-intent card for a film/video production desk.\n\
Return ONLY JSON (no markdown) with these keys:\n\
narrative_purpose (string),\n\
emotional_purpose (string),\n\
character_focus (array of character names),\n\
visual_contrast (string),\n\
important_props (array of strings),\n\
continuity_constraints (array of strings),\n\
sound_and_transition (array of strings),\n\
audience_must_know (string: what must be clear by the end of the scene).\n\
Do not invent file paths, cut numbers, shot lists, or production notes.\n\
Stay faithful to the script. Be specific and concrete."
}

pub fn intent_user_prompt(scene_title: &str, excerpt: &str, brief: &str) -> String {
    let mut s = format!("Scene: {scene_title}\n\nScript / beats:\n{excerpt}\n");
    if !brief.trim().is_empty() {
        s.push_str("\nUser direction:\n");
        s.push_str(brief.trim());
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_lists_from_strings() {
        let intent = intent_from_json(&json!({
            "narrative_purpose": "Reach the halt before supper.",
            "character_focus": "Tom, Pip, Nell",
            "important_props": ["brown paper parcel"]
        }))
        .unwrap();
        assert_eq!(intent.character_focus, ["Tom", "Pip", "Nell"]);
        assert_eq!(intent.important_props, ["brown paper parcel"]);
    }
}
