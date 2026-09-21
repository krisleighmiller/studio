//! Invert the old rejection matrix: given creative choices the host already
//! has, fill every mechanically derivable handoff field.
use crate::scene_plan::assets::AssetRegistry;
use crate::scene_plan::config::HostConfig;
use crate::scene_plan::handoff::{
    AssetRole, CastBinding, CutId, GenerationPath, GenerationRequirements, Pace, ReferenceAsset,
    RenderLane, SceneHandoff, SourceMode, SpeechMode, Stage,
};
use crate::scene_plan::timing::round2;
use regex::Regex;
use std::collections::BTreeSet;
use std::sync::OnceLock;

const NATURAL_AIR_SECONDS: f64 = 1.0;
const DEFAULT_ASPECT: &str = "9:16";

fn cue_suffix_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"_([A-Za-z0-9]+)$").unwrap())
}

/// `a`, `b`, … `z`, then `c27`, `c28`, …
pub fn letter_suffix(index: usize) -> String {
    if index < 26 {
        ((b'a' + index as u8) as char).to_string()
    } else {
        format!("c{}", index + 1)
    }
}

pub fn assign_cue_ids(id: CutId, cues: &mut [crate::scene_plan::handoff::Cue]) {
    for (index, cue) in cues.iter_mut().enumerate() {
        let suffix = cue_suffix_re()
            .captures(cue.id.trim())
            .map(|c| c[1].to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| letter_suffix(index));
        cue.id = format!("{}{suffix}", id.cue_prefix());
    }
}

pub fn apply_host_defaults(
    handoff: &mut SceneHandoff,
    config: &HostConfig,
    expected_aspect_ratio: Option<&str>,
) {
    handoff.provider = "atlascloud".into();
    handoff.stage = Stage::BaseVideo;
    let aspect = expected_aspect_ratio
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            let existing = handoff.generation_requirements.aspect_ratio.trim();
            (!existing.is_empty()).then_some(existing)
        })
        .unwrap_or(DEFAULT_ASPECT);
    handoff.generation_requirements.aspect_ratio = aspect.to_string();
    if handoff.speech.cues.is_empty() {
        handoff.speech.language = None;
    } else if handoff
        .speech
        .language
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty()
    {
        handoff.speech.language = Some(config.default_language.clone());
    }
}

/// Speech floor plus policy handles, then pace, then the smallest legal bucket.
pub fn resolve_duration(
    speech_total: f64,
    pace: Pace,
    lane: RenderLane,
    config: &HostConfig,
) -> Result<ResolvedDuration, String> {
    let head = if config.head_handle_seconds.is_finite() && config.head_handle_seconds >= 0.0 {
        config.head_handle_seconds
    } else {
        0.5
    };
    let tail = if config.tail_handle_seconds.is_finite() && config.tail_handle_seconds >= 0.0 {
        config.tail_handle_seconds
    } else {
        0.5
    };
    let floor = match pace {
        Pace::Tight => speech_total + head + tail,
        Pace::Natural => speech_total + head + tail + NATURAL_AIR_SECONDS,
        Pace::Held => speech_total + head + tail,
    };
    let tight = config.choose_render_duration(lane, floor.max(speech_total + head + tail))?;
    let render = match pace {
        Pace::Tight | Pace::Natural => tight,
        Pace::Held => config.policy_for(lane).next_above(tight).unwrap_or(tight),
    };
    let screen = (render - head - tail).max(speech_total);
    Ok(ResolvedDuration {
        screen_seconds: round2(screen),
        head_handle_seconds: head,
        tail_handle_seconds: tail,
        minimum_source_seconds: round2(screen + head + tail),
        render_duration_seconds: render,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedDuration {
    pub screen_seconds: f64,
    pub head_handle_seconds: f64,
    pub tail_handle_seconds: f64,
    pub minimum_source_seconds: f64,
    pub render_duration_seconds: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedGeneration {
    pub source_mode: SourceMode,
    pub generation_path: GenerationPath,
    pub required_capabilities: Vec<String>,
    pub reference_assets: Vec<ReferenceAsset>,
}

/// `speech.mode` + on-camera speaker + discovered reference → the legal combo.
pub fn resolve_generation(
    mode: Option<SpeechMode>,
    on_camera_speakers: &BTreeSet<String>,
    cast: &[CastBinding],
    registry: Option<&AssetRegistry>,
    existing_assets: &[ReferenceAsset],
) -> ResolvedGeneration {
    let lipsync = mode == Some(SpeechMode::SourceVideoLipsync);
    let generation_path = if lipsync {
        GenerationPath::AudioDrivenI2v
    } else {
        GenerationPath::PromptDriven
    };

    let speaker_ref = |speaker: &str| -> Option<String> {
        cast.iter()
            .find(|c| c.speaker == speaker)
            .and_then(|c| c.reference_image.clone())
            .filter(|p| !p.trim().is_empty())
    };

    let attach_character = |speaker: &str| -> Vec<ReferenceAsset> {
        match speaker_ref(speaker) {
            Some(path) => vec![ReferenceAsset {
                path,
                role: AssetRole::CharacterReference,
            }],
            None => Vec::new(),
        }
    };

    if lipsync {
        if let Some(speaker) = on_camera_speakers.iter().next() {
            let assets = if registry.is_some() {
                attach_character(speaker)
            } else {
                usable_existing(existing_assets)
            };
            let source_mode = if assets.is_empty() {
                SourceMode::TextToVideo
            } else if assets.iter().any(|a| a.role == AssetRole::StartFrame) {
                SourceMode::ImageToVideo
            } else {
                SourceMode::ReferenceToVideo
            };
            return ResolvedGeneration {
                source_mode,
                generation_path,
                required_capabilities: capabilities_for(&assets),
                reference_assets: assets,
            };
        }
        return ResolvedGeneration {
            source_mode: SourceMode::TextToVideo,
            generation_path,
            required_capabilities: Vec::new(),
            reference_assets: Vec::new(),
        };
    }

    if registry.is_some() {
        if let Some(speaker) = on_camera_speakers.iter().next() {
            let assets = attach_character(speaker);
            if !assets.is_empty() {
                return ResolvedGeneration {
                    source_mode: SourceMode::ReferenceToVideo,
                    generation_path,
                    required_capabilities: capabilities_for(&assets),
                    reference_assets: assets,
                };
            }
        }
        return ResolvedGeneration {
            source_mode: SourceMode::TextToVideo,
            generation_path,
            required_capabilities: Vec::new(),
            reference_assets: Vec::new(),
        };
    }

    let assets = existing_assets.to_vec();
    let source_mode = if assets.iter().any(|a| a.role == AssetRole::StartFrame) {
        SourceMode::ImageToVideo
    } else if !assets.is_empty() {
        SourceMode::ReferenceToVideo
    } else {
        SourceMode::TextToVideo
    };
    ResolvedGeneration {
        source_mode,
        generation_path,
        required_capabilities: capabilities_for(&assets),
        reference_assets: assets,
    }
}

fn usable_existing(assets: &[ReferenceAsset]) -> Vec<ReferenceAsset> {
    assets
        .iter()
        .filter(|a| {
            matches!(
                a.role,
                AssetRole::StartFrame | AssetRole::CharacterReference
            ) && !a.path.trim().is_empty()
        })
        .cloned()
        .collect()
}

fn capabilities_for(assets: &[ReferenceAsset]) -> Vec<String> {
    if assets
        .iter()
        .any(|a| a.role == AssetRole::CharacterReference)
    {
        vec!["character_reference".into()]
    } else {
        Vec::new()
    }
}

pub fn apply_generation(handoff: &mut SceneHandoff, resolved: ResolvedGeneration) {
    let priorities = std::mem::take(&mut handoff.generation_requirements.priorities);
    let aspect = handoff.generation_requirements.aspect_ratio.clone();
    handoff.generation_requirements = GenerationRequirements {
        source_mode: resolved.source_mode,
        generation_path: Some(resolved.generation_path),
        aspect_ratio: aspect,
        required_capabilities: resolved.required_capabilities,
        priorities,
    };
    handoff.reference_assets = resolved.reference_assets;
}

pub fn apply_duration(handoff: &mut SceneHandoff, timing: &ResolvedDuration) {
    handoff.editing.estimated_screen_duration_seconds = timing.screen_seconds;
    handoff.editing.head_handle_seconds = timing.head_handle_seconds;
    handoff.editing.tail_handle_seconds = timing.tail_handle_seconds;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_plan::handoff::Cue;

    #[test]
    fn cue_ids_are_rewritten_to_the_cut() {
        let mut cues = vec![
            Cue {
                id: String::new(),
                speaker: "Sam".into(),
                text: "Hi.".into(),
                on_camera: true,
                delivery: None,
                pause_before_seconds: 0.0,
                pause_after_seconds: 0.0,
                estimated_duration_seconds: None,
            },
            Cue {
                id: "S99C99_b".into(),
                speaker: "Alex".into(),
                text: "Hey.".into(),
                on_camera: false,
                delivery: None,
                pause_before_seconds: 0.0,
                pause_after_seconds: 0.0,
                estimated_duration_seconds: None,
            },
        ];
        assign_cue_ids(CutId::new(1, 3), &mut cues);
        assert_eq!(cues[0].id, "S01C03_a");
        assert_eq!(cues[1].id, "S01C03_b");
    }

    #[test]
    fn tight_snaps_to_smallest_bucket() {
        let cfg = HostConfig::default();
        let d = resolve_duration(4.36, Pace::Tight, RenderLane::AudioDrivenI2v, &cfg).unwrap();
        assert_eq!(d.render_duration_seconds, 10.0);
        assert_eq!(d.head_handle_seconds, 0.5);
        assert_eq!(d.tail_handle_seconds, 0.5);
        assert!((d.screen_seconds - 9.0).abs() < 1e-9);
    }

    #[test]
    fn held_takes_the_next_bucket_when_one_exists() {
        let cfg = HostConfig::default();
        let d = resolve_duration(0.0, Pace::Held, RenderLane::TextToVideo, &cfg).unwrap();
        assert_eq!(d.render_duration_seconds, 10.0);
        let tight = resolve_duration(0.0, Pace::Tight, RenderLane::TextToVideo, &cfg).unwrap();
        assert_eq!(tight.render_duration_seconds, 5.0);
    }
}
