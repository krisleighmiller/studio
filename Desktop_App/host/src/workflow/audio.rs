//! Audio as first-class production state. Classify audio assets by role and
//! derive a scene's audio requirements deterministically — never ask an LLM
//! what a scene's asset table already records.

use serde::{Deserialize, Serialize};

use crate::workflow::state::{AssetRecord, AssetRole, AssetStatus, ProjectState};

/// Per-role presence/approval for one scene.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioRoleState {
    pub role: AssetRole,
    pub present: bool,
    pub approved: bool,
    pub asset_count: usize,
}

/// A scene's audio picture: what is required, what exists, what is missing,
/// and whether that blocks Finish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioStatus {
    pub required: Vec<AssetRole>,
    pub roles: Vec<AudioRoleState>,
    pub missing: Vec<AssetRole>,
    pub all_approved: bool,
    pub blocks_finish: bool,
}

/// Scan an audio-plan string for role keywords. Deterministic — substring match
/// against a fixed keyword table, no LLM.
pub fn parse_audio_roles(plan: &str) -> Vec<AssetRole> {
    let text = plan.to_ascii_lowercase();
    let table: &[(&[&str], AssetRole)] = &[
        (&["dialogue", "dialog", "speech"], AssetRole::Dialogue),
        (
            &[
                "voice over",
                "voiceover",
                "voice-over",
                "narration",
                "narrator",
            ],
            AssetRole::VoiceOver,
        ),
        (&["music", "score", "soundtrack", "theme"], AssetRole::Music),
        (&["ambience", "ambient", "atmosphere"], AssetRole::Ambience),
        (&["room tone", "roomtone"], AssetRole::RoomTone),
        (
            &["sound effect", "soundeffect", "sfx"],
            AssetRole::SoundEffect,
        ),
        (&["foley"], AssetRole::Foley),
        (&["source audio", "sourceaudio"], AssetRole::SourceAudio),
        (&["scratch audio", "scratch"], AssetRole::ScratchAudio),
        (&["final mix", "mixdown", "master"], AssetRole::Mix),
    ];
    table
        .iter()
        .filter(|(keywords, _)| keywords.iter().any(|keyword| text.contains(keyword)))
        .map(|(_, role)| *role)
        .collect()
}

/// Audio assets attached to a scene (by `scene_id`), sorted by role label.
pub fn scene_audio_assets<'a>(state: &'a ProjectState, scene_id: u32) -> Vec<&'a AssetRecord> {
    let mut assets: Vec<&AssetRecord> = state
        .assets
        .iter()
        .filter(|asset| asset.scene_id == Some(scene_id) && asset.role.is_audio())
        .collect();
    assets.sort_by_key(|asset| asset.role.label());
    assets
}

/// Whether any cut in the scene carries dialogue (a non-empty line).
fn scene_has_dialogue(state: &ProjectState, scene_id: u32) -> bool {
    state
        .scene(scene_id)
        .map(|scene| scene.cuts.iter().any(|cut| !cut.dialogue.is_empty()))
        .unwrap_or(false)
}

/// Compute a scene's audio status: required roles (from the plan + dialogue),
/// per-role presence/approval, missing roles, and whether Finish is blocked.
pub fn scene_audio_status(state: &ProjectState, scene_id: u32) -> Option<AudioStatus> {
    state.scene(scene_id)?;

    let plan = state
        .profile
        .as_ref()
        .and_then(|profile| profile.audio_plan.as_deref())
        .unwrap_or("");
    let mut required = parse_audio_roles(plan);
    if !required.contains(&AssetRole::Dialogue) && scene_has_dialogue(state, scene_id) {
        required.push(AssetRole::Dialogue);
    }
    required.sort_by_key(|role| role.label());
    required.dedup();

    let assets = scene_audio_assets(state, scene_id);
    let mut present: Vec<AssetRole> = assets.iter().map(|asset| asset.role).collect();
    present.sort_by_key(|role| role.label());
    present.dedup();

    let mut all_roles = present.clone();
    for role in &required {
        if !all_roles.contains(role) {
            all_roles.push(*role);
        }
    }
    all_roles.sort_by_key(|role| role.label());

    let roles: Vec<AudioRoleState> = all_roles
        .iter()
        .map(|&role| {
            let matching: Vec<_> = assets.iter().filter(|asset| asset.role == role).collect();
            AudioRoleState {
                role,
                present: !matching.is_empty(),
                approved: !matching.is_empty()
                    && matching
                        .iter()
                        .all(|asset| asset.status == AssetStatus::Approved),
                asset_count: matching.len(),
            }
        })
        .collect();

    let missing: Vec<AssetRole> = required
        .iter()
        .copied()
        .filter(|role| !present.contains(role))
        .collect();
    let all_approved = roles.iter().all(|entry| !entry.present || entry.approved);
    let blocks_finish = !missing.is_empty() || !all_approved;

    Some(AudioStatus {
        required,
        roles,
        missing,
        all_approved,
        blocks_finish,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::state::{AssetRecord, AssetRole, AssetStatus, ProductionProfile};

    fn scene_with(state: &mut ProjectState, scene_id: u32, dialogue: bool) {
        let scene = state.scene_mut(scene_id, "Scene");
        let mut cut =
            crate::workflow::state::CutRecord::new(crate::scene_plan::handoff::CutId::new(1, 1));
        if dialogue {
            cut.dialogue.push(crate::script::DialogueLine {
                speaker: "A".into(),
                text: "hello".into(),
            });
        }
        scene.cuts = vec![cut];
    }

    fn audio_asset(
        state: &mut ProjectState,
        id: &str,
        role: AssetRole,
        scene: u32,
        status: AssetStatus,
    ) {
        let id = state.push_asset(AssetRecord {
            id: id.to_string(),
            path: format!("{id}.mp3"),
            role,
            status,
            scene_id: Some(scene),
            cut_id: None,
            character_id: None,
            source_job_id: None,
            parent_asset_id: None,
            notes: None,
        });
        let _ = id;
    }

    #[test]
    fn parse_audio_roles_finds_keywords_in_a_plan() {
        let plan = "A quiet room tone, light ambience, a music cue, and foley footsteps.";
        let roles = parse_audio_roles(plan);
        assert!(roles.contains(&AssetRole::RoomTone));
        assert!(roles.contains(&AssetRole::Ambience));
        assert!(roles.contains(&AssetRole::Music));
        assert!(roles.contains(&AssetRole::Foley));
        assert!(!roles.contains(&AssetRole::Dialogue));
    }

    #[test]
    fn parse_audio_roles_returns_empty_for_an_unrelated_plan() {
        assert!(parse_audio_roles("establishing exterior at dusk").is_empty());
    }

    #[test]
    fn asset_role_single_token_parse_round_trips() {
        assert_eq!(
            AssetRole::parse_audio("voice-over"),
            Some(AssetRole::VoiceOver)
        );
        assert_eq!(AssetRole::parse_audio("SFX"), Some(AssetRole::SoundEffect));
        assert_eq!(
            AssetRole::parse_audio("dialogue"),
            Some(AssetRole::Dialogue)
        );
        assert_eq!(AssetRole::parse_audio("wardrobe"), None);
    }

    #[test]
    fn scene_audio_status_reports_required_present_and_missing() {
        let mut state = ProjectState::default();
        state.profile = Some(ProductionProfile {
            audio_plan: Some("music and ambience".into()),
            ..ProductionProfile::default()
        });
        scene_with(&mut state, 1, false);
        audio_asset(&mut state, "m", AssetRole::Music, 1, AssetStatus::Approved);

        let status = scene_audio_status(&state, 1).unwrap();
        assert_eq!(status.required, vec![AssetRole::Ambience, AssetRole::Music]);
        assert_eq!(status.missing, vec![AssetRole::Ambience]);
        assert!(status.blocks_finish);
    }

    #[test]
    fn scene_audio_status_derives_dialogue_from_cuts() {
        let mut state = ProjectState::default();
        scene_with(&mut state, 1, true);
        let status = scene_audio_status(&state, 1).unwrap();
        assert_eq!(status.required, vec![AssetRole::Dialogue]);
        assert_eq!(status.missing, vec![AssetRole::Dialogue]);
        assert!(status.blocks_finish);
    }

    #[test]
    fn scene_audio_status_passes_when_required_audio_is_approved() {
        let mut state = ProjectState::default();
        state.profile = Some(ProductionProfile {
            audio_plan: Some("music".into()),
            ..ProductionProfile::default()
        });
        scene_with(&mut state, 1, false);
        audio_asset(&mut state, "m", AssetRole::Music, 1, AssetStatus::Approved);

        let status = scene_audio_status(&state, 1).unwrap();
        assert!(status.missing.is_empty());
        assert!(status.all_approved);
        assert!(!status.blocks_finish);
    }

    #[test]
    fn scene_audio_status_blocks_on_unapproved_audio() {
        let mut state = ProjectState::default();
        state.profile = Some(ProductionProfile {
            audio_plan: Some("music".into()),
            ..ProductionProfile::default()
        });
        scene_with(&mut state, 1, false);
        audio_asset(&mut state, "m", AssetRole::Music, 1, AssetStatus::Draft);

        let status = scene_audio_status(&state, 1).unwrap();
        assert!(status.missing.is_empty());
        assert!(!status.all_approved);
        assert!(status.blocks_finish);
    }
}
