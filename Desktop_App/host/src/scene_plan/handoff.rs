//! Typed schema for one generated cut. This value is the single source of
//! truth: the master block in `prod.md` and the sidecar are both rendered
//! from it, never hand-assembled from text.
use serde::{Deserialize, Serialize};
use std::fmt;

/// Dual identity of a handoff: which *source* scene of the script it depicts
/// and which generated *cut* of that scene it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CutId {
    pub source_scene: u32,
    pub cut: u32,
}

impl CutId {
    pub fn new(source_scene: u32, cut: u32) -> Self {
        CutId { source_scene, cut }
    }
    /// `S01C02` — used in cue ids and output file names.
    pub fn code(&self) -> String {
        format!("S{:02}C{:02}", self.source_scene, self.cut)
    }
    pub fn cue_prefix(&self) -> String {
        format!("{}_", self.code())
    }
    pub fn next_cut(&self) -> CutId {
        CutId::new(self.source_scene, self.cut + 1)
    }
}

impl fmt::Display for CutId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Scene {} — Cut {}", self.source_scene, self.cut)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneHandoff {
    /// Script scene this cut depicts.
    pub source_scene: u32,
    /// Generated cut within that source scene (1-based, consecutive).
    pub cut: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Host-owned. Ignored on input; always `atlascloud` after validation.
    #[serde(default = "default_provider")]
    pub provider: String,
    /// Host-owned. Ignored on input; always `base_video` after validation.
    #[serde(default)]
    pub stage: Stage,
    pub scene_type: SceneType,
    pub continuity: Continuity,
    pub editing: Editing,
    #[serde(default)]
    pub speech: Speech,
    /// Host-owned. Ignored on input, recomputed on every validation.
    #[serde(default)]
    pub generation_requirements: GenerationRequirements,
    pub prompt: String,
    /// Host-owned. Ignored on input when a registry is present; attached from the speaker.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reference_assets: Vec<ReferenceAsset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub negative_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_notes: Option<String>,
    /// Host-owned: speaker → discovered character / reference / voice.
    /// Ignored on input, recomputed on every validation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cast: Vec<CastBinding>,
    /// Host-owned timing. Ignored on input, recomputed on every validation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived: Option<DerivedTiming>,
}

impl SceneHandoff {
    pub fn id(&self) -> CutId {
        CutId::new(self.source_scene, self.cut)
    }
    pub fn asset_with_role(&self, role: AssetRole) -> Option<&ReferenceAsset> {
        self.reference_assets.iter().find(|a| a.role == role)
    }
    /// Distinct speakers in cue order.
    pub fn distinct_speakers(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for cue in &self.speech.cues {
            let s = cue.speaker.trim().to_string();
            if !s.is_empty() && !out.contains(&s) {
                out.push(s);
            }
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    #[default]
    BaseVideo,
    LipSync,
    Post,
}

fn default_provider() -> String {
    "atlascloud".into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneType {
    CharacterPerformance,
    Dialogue,
    BRoll,
    Establishing,
    Insert,
    TitleCard,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Continuity {
    pub group: String,
    pub relation_to_previous: RelationToPrevious,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opening_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closing_state: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub carry_forward: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationToPrevious {
    NewSetup,
    Continuous,
    SameSetupNewAngle,
    TimeJump,
    MatchCut,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Editing {
    /// Host-owned. Ignored on input; snapped from speech + pace + lane buckets.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub estimated_screen_duration_seconds: f64,
    /// Host-owned policy constant. Ignored on input.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub head_handle_seconds: f64,
    /// Host-owned policy constant. Ignored on input.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub tail_handle_seconds: f64,
    /// Creative pace hint. The host snaps it to a provider bucket.
    #[serde(default, skip_serializing_if = "Pace::is_natural")]
    pub pace: Pace,
    pub transition_in: Transition,
    pub transition_out: Transition,
    #[serde(default)]
    pub continuity_frame_strategy: ContinuityFrameStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut_guidance: Option<String>,
}

/// How much air the host should leave around the spoken floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Pace {
    Tight,
    #[default]
    Natural,
    Held,
}

impl Pace {
    pub fn is_natural(&self) -> bool {
        matches!(self, Pace::Natural)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transition {
    Cut,
    Dissolve,
    FadeIn,
    FadeOut,
    Wipe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContinuityFrameStrategy {
    #[default]
    None,
    LastFrameAsStart,
    SharedReference,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Speech {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<SpeechMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cues: Vec<Cue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechMode {
    #[serde(rename = "none")]
    Silent,
    VoiceOver,
    SourceVideoLipsync,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cue {
    /// Host-owned `S{source:02}C{cut:02}_{suffix}`. Optional on input.
    #[serde(default)]
    pub id: String,
    pub speaker: String,
    pub text: String,
    /// Lip-sync cuts: this speaker's face performs the line. Voice-over cuts:
    /// the speaker is visible while the line plays (not mouthing it).
    #[serde(default = "default_true")]
    pub on_camera: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub pause_before_seconds: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub pause_after_seconds: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_duration_seconds: Option<f64>,
}

fn default_true() -> bool {
    true
}
fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct GenerationRequirements {
    /// Host-owned. Ignored on input.
    #[serde(default)]
    pub source_mode: SourceMode,
    /// Host-owned. Ignored on input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_path: Option<GenerationPath>,
    /// Host-owned. Copied from PlanState, never echoed by the model.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub aspect_ratio: String,
    /// Host-owned. Ignored on input.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub priorities: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SourceMode {
    #[default]
    TextToVideo,
    ImageToVideo,
    ReferenceToVideo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationPath {
    PromptDriven,
    AudioDrivenI2v,
}

/// Which provider lane a cut renders on; each lane has its own duration buckets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderLane {
    TextToVideo,
    ImageToVideo,
    ReferenceToVideo,
    AudioDrivenI2v,
}

impl RenderLane {
    pub fn for_request(path: GenerationPath, source_mode: SourceMode) -> RenderLane {
        match (path, source_mode) {
            (GenerationPath::AudioDrivenI2v, _) => RenderLane::AudioDrivenI2v,
            (_, SourceMode::TextToVideo) => RenderLane::TextToVideo,
            (_, SourceMode::ImageToVideo) => RenderLane::ImageToVideo,
            (_, SourceMode::ReferenceToVideo) => RenderLane::ReferenceToVideo,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceAsset {
    pub path: String,
    pub role: AssetRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetRole {
    CharacterReference,
    StartFrame,
    EndFrame,
    StyleReference,
}

/// Host-owned speaker binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastBinding {
    pub speaker: String,
    pub character: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_name: Option<String>,
    /// Local clip used when the character has no provider voice id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_sample: Option<String>,
}

/// Host-owned, recomputed on every validation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedTiming {
    pub speaking_rate_wpm: f64,
    pub speech_duration_seconds: f64,
    pub minimum_source_duration_seconds: f64,
    pub render_lane: RenderLane,
    pub render_duration_seconds: f64,
}

pub fn label<T: Serialize>(value: &T) -> String {
    serde_yaml_ng::to_string(value)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "?".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_id_codes_and_display() {
        let id = CutId::new(1, 2);
        assert_eq!(id.code(), "S01C02");
        assert_eq!(id.cue_prefix(), "S01C02_");
        assert_eq!(id.to_string(), "Scene 1 — Cut 2");
        assert_eq!(id.next_cut(), CutId::new(1, 3));
        assert_eq!(CutId::new(12, 105).code(), "S12C105");
    }

    #[test]
    fn enum_labels_match_yaml_spelling() {
        assert_eq!(label(&SpeechMode::Silent), "none");
        assert_eq!(label(&GenerationPath::AudioDrivenI2v), "audio_driven_i2v");
        assert_eq!(label(&RenderLane::ReferenceToVideo), "reference_to_video");
    }

    #[test]
    fn render_lane_resolution() {
        assert_eq!(
            RenderLane::for_request(GenerationPath::AudioDrivenI2v, SourceMode::TextToVideo),
            RenderLane::AudioDrivenI2v
        );
        assert_eq!(
            RenderLane::for_request(GenerationPath::PromptDriven, SourceMode::ImageToVideo),
            RenderLane::ImageToVideo
        );
        assert_eq!(
            RenderLane::for_request(GenerationPath::PromptDriven, SourceMode::TextToVideo),
            RenderLane::TextToVideo
        );
    }
}
