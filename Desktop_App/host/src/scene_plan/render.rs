//! Everything written to disk is rendered from a `SceneHandoff`.
use crate::scene_plan::handoff::{AssetRole, CutId, SceneHandoff};
use regex::Regex;
use serde_yaml_ng::Value;
use std::sync::OnceLock;

const SENTINEL: &str = "__QSX__";

fn sexagesimal_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\d{1,4}(?::\d{1,4})+$").unwrap())
}

fn unprotect_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(&format!("{SENTINEL}([0-9:]+){SENTINEL}")).unwrap())
}

/// Mark whole string scalars that YAML 1.1 consumers would read as
/// sexagesimal integers (`9:16`, `0:05`). Done on the value tree, so text
/// *inside* block scalars (e.g. a prompt line `Timing: 0:05`) is untouched.
fn protect(value: &mut Value) {
    match value {
        Value::String(s) if sexagesimal_re().is_match(s) => *s = format!("{SENTINEL}{s}{SENTINEL}"),
        Value::Mapping(m) => {
            for (_, v) in m.iter_mut() {
                protect(v);
            }
        }
        Value::Sequence(seq) => {
            for v in seq {
                protect(v);
            }
        }
        _ => {}
    }
}

pub fn canonical_yaml(handoff: &SceneHandoff) -> Result<String, String> {
    let mut value = serde_yaml_ng::to_value(handoff).map_err(|e| e.to_string())?;
    protect(&mut value);
    let text = serde_yaml_ng::to_string(&value).map_err(|e| e.to_string())?;
    Ok(unprotect_re()
        .replace_all(text.trim_end(), "\"${1}\"")
        .into_owned())
}

pub fn master_block(handoff: &SceneHandoff, canonical_yaml: &str) -> String {
    let title = handoff
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| format!(" — {t}"))
        .unwrap_or_default();
    format!(
        "### {}{}\n\n#### Atlas Generation Handoff\n\n```yaml\n{}\n```\n",
        handoff.id(),
        title,
        canonical_yaml.trim_end()
    )
}

/// `Scene1/Cut2.txt`, relative to the sidecar directory.
pub fn sidecar_relative_path(id: CutId) -> String {
    format!("Scene{}/Cut{}.txt", id.source_scene, id.cut)
}

pub fn sidecar_document(handoff: &SceneHandoff, canonical_yaml: &str) -> String {
    let id = handoff.id();
    let character_reference = handoff
        .asset_with_role(AssetRole::CharacterReference)
        .or_else(|| handoff.asset_with_role(AssetRole::StartFrame))
        .map(|a| a.path.as_str())
        .unwrap_or("none");
    let voice = if handoff.speech.cues.is_empty() {
        "none".to_string()
    } else if handoff.cast.is_empty() {
        handoff.distinct_speakers().join(", ")
    } else if let [only] = handoff.cast.as_slice() {
        match &only.voice_id {
            Some(v) => format!(
                "{} ({} {v})",
                only.character,
                only.voice_provider.as_deref().unwrap_or("voice")
            ),
            None => only
                .voice_sample
                .clone()
                .unwrap_or_else(|| format!("{} (no voice on file)", only.character)),
        }
    } else {
        handoff
            .cast
            .iter()
            .map(|c| match &c.voice_id {
                Some(v) => format!(
                    "{} ({} {v})",
                    c.character,
                    c.voice_provider.as_deref().unwrap_or("voice")
                ),
                None => format!("{} (no voice on file)", c.character),
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let code = id.code();
    format!(
        "Source_Scene: {}\nCut: {}\nCharacter_Reference: {character_reference}\nVoice: {voice}\nOutput: Scenes/Scene{}/Cut{}\nFilenames: \"{code}.mp4\" and \"{code}.wav\"\n\nPrompt:\n\n```yaml\n{}\n```\n",
        id.source_scene,
        id.cut,
        id.source_scene,
        id.cut,
        canonical_yaml.trim_end()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_plan::config::HostConfig;
    use crate::scene_plan::extract::{load_handoff_document, parse_handoff_yaml};
    use crate::scene_plan::fixtures::{lipsync_scene_yaml, SAM_REF};
    use crate::scene_plan::validate::validate_scene;

    #[test]
    fn master_and_sidecar_share_the_same_yaml_and_reload_identically() {
        let h = parse_handoff_yaml(&lipsync_scene_yaml(4, "Hello there everyone.")).unwrap();
        let v = validate_scene(h, &HostConfig::default(), None, None).unwrap();
        let master = master_block(&v.handoff, &v.canonical_yaml);
        let sidecar = sidecar_document(&v.handoff, &v.canonical_yaml);
        assert!(master.starts_with("### Scene 1 — Cut 4 — Sam addresses the camera\n\n#### Atlas Generation Handoff\n\n```yaml\n"));
        assert!(sidecar.starts_with(&format!(
            "Source_Scene: 1\nCut: 4\nCharacter_Reference: {SAM_REF}\nVoice: Sam (no voice on file)\nOutput: Scenes/Scene1/Cut4\nFilenames: \"S01C04.mp4\" and \"S01C04.wav\"\n"
        )));
        assert_eq!(load_handoff_document(&master).unwrap(), v.handoff);
        assert_eq!(load_handoff_document(&sidecar).unwrap(), v.handoff);
        assert_eq!(sidecar_relative_path(CutId::new(1, 4)), "Scene1/Cut4.txt");
    }

    #[test]
    fn aspect_ratio_is_quoted_and_block_scalars_are_not_touched() {
        let yaml = lipsync_scene_yaml(1, "Hello there.").replace(
            "  and settle again afterward. Locked-off camera with no zoom.",
            "  and settle again afterward.\n  Timing: 0:05\n  Locked-off camera.",
        );
        let h = parse_handoff_yaml(&yaml).unwrap();
        let v = validate_scene(h, &HostConfig::default(), Some("9:16"), None).unwrap();
        assert!(
            v.canonical_yaml.contains("aspect_ratio: \"9:16\""),
            "{}",
            v.canonical_yaml
        );
        assert!(
            v.canonical_yaml.contains("\n  Timing: 0:05\n"),
            "{}",
            v.canonical_yaml
        );
        assert!(!v.canonical_yaml.contains(SENTINEL));
        let again = parse_handoff_yaml(&v.canonical_yaml).unwrap();
        assert_eq!(again, v.handoff);
        assert_eq!(again.generation_requirements.aspect_ratio, "9:16");
    }
}
