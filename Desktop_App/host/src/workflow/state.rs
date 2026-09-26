//! V2 workflow state: project / scene / cut / asset / job records.
//!
//! The project folder remains the source of truth. This file is the typed
//! mapping the UI will read. Mechanical fields are host-owned; creative
//! fields stay optional until a later stage fills them.
use crate::output_paths::slugify;
use crate::scene_plan::handoff::{CutId, Pace, RenderLane, SceneType, SpeechMode};
use crate::scene_plan::strategy::SceneStrategy;
use crate::script::{DialogueLine, ScriptBeat};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;

pub const WORKFLOW_VERSION: u32 = 2;
pub const STATE_REL: &str = "Preproduction/workflow_state.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStage {
    Script,
    Characters,
    SceneIntent,
    SceneLook,
    CutSketches,
    ReferenceImages,
    ProductionNotes,
    Render,
    TakeReview,
    Assemble,
}

impl WorkflowStage {
    pub const ALL: [Self; 10] = [
        Self::Script,
        Self::Characters,
        Self::SceneIntent,
        Self::SceneLook,
        Self::CutSketches,
        Self::ReferenceImages,
        Self::ProductionNotes,
        Self::Render,
        Self::TakeReview,
        Self::Assemble,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Script => "script",
            Self::Characters => "characters",
            Self::SceneIntent => "scene_intent",
            Self::SceneLook => "scene_look",
            Self::CutSketches => "cut_sketches",
            Self::ReferenceImages => "reference_images",
            Self::ProductionNotes => "production_notes",
            Self::Render => "render",
            Self::TakeReview => "take_review",
            Self::Assemble => "assemble",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|stage| stage.as_str() == value)
    }
}

impl StageStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Present => "present",
            Self::Draft => "draft",
            Self::Approved => "approved",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    #[default]
    Missing,
    Present,
    Draft,
    Approved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShotRole {
    Performance,
    Reaction,
    Insert,
    Atmosphere,
    Establishing,
    Transition,
    Action,
    Detail,
}

impl ShotRole {
    pub fn parse(value: &str) -> Option<Self> {
        let v = value.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        match v.as_str() {
            "performance" | "lipsync" | "lip_sync" | "talking" => Some(Self::Performance),
            "reaction" => Some(Self::Reaction),
            "insert" => Some(Self::Insert),
            "atmosphere" | "atmospheric" | "b_roll" | "broll" => Some(Self::Atmosphere),
            "establishing" | "establish" => Some(Self::Establishing),
            "transition" => Some(Self::Transition),
            "action" => Some(Self::Action),
            "detail" => Some(Self::Detail),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Performance => "performance",
            Self::Reaction => "reaction",
            Self::Insert => "insert",
            Self::Atmosphere => "atmosphere",
            Self::Establishing => "establishing",
            Self::Transition => "transition",
            Self::Action => "action",
            Self::Detail => "detail",
        }
    }

    pub fn all() -> [Self; 8] {
        [
            Self::Performance,
            Self::Reaction,
            Self::Insert,
            Self::Atmosphere,
            Self::Establishing,
            Self::Transition,
            Self::Action,
            Self::Detail,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouthPolicy {
    SpeakingVisible,
    ClosedOrNotVisible,
    OffCamera,
    NoPeople,
}

impl MouthPolicy {
    pub fn parse(value: &str) -> Option<Self> {
        let v = value.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        match v.as_str() {
            "speaking_visible" | "speaking" | "lipsync" | "lip_sync" => Some(Self::SpeakingVisible),
            "closed_or_not_visible" | "closed" | "not_visible" => Some(Self::ClosedOrNotVisible),
            "off_camera" | "offcamera" | "voice_over" | "vo" => Some(Self::OffCamera),
            "no_people" | "empty" | "nobody" => Some(Self::NoPeople),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::SpeakingVisible => "speaking_visible",
            Self::ClosedOrNotVisible => "closed_or_not_visible",
            Self::OffCamera => "off_camera",
            Self::NoPeople => "no_people",
        }
    }

    pub fn all() -> [Self; 4] {
        [
            Self::SpeakingVisible,
            Self::ClosedOrNotVisible,
            Self::OffCamera,
            Self::NoPeople,
        ]
    }

    pub fn default_for(role: ShotRole, has_dialogue: bool) -> Self {
        match role {
            ShotRole::Performance => Self::SpeakingVisible,
            ShotRole::Reaction => Self::ClosedOrNotVisible,
            ShotRole::Insert | ShotRole::Detail => {
                if has_dialogue {
                    Self::ClosedOrNotVisible
                } else {
                    Self::NoPeople
                }
            }
            ShotRole::Atmosphere
            | ShotRole::Establishing
            | ShotRole::Transition
            | ShotRole::Action => {
                if has_dialogue {
                    Self::OffCamera
                } else {
                    Self::NoPeople
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetRole {
    IdentityReference,
    BodyReference,
    ExpressionReference,
    VoiceSample,
    VoiceProfile,
    LocationReference,
    WardrobeReference,
    /// Scene-specific close-up or standing plate wearing the approved wardrobe.
    WardrobePlate,
    PropReference,
    MoodReference,
    LightingReference,
    CompositionReference,
    GroupReference,
    ExteriorReference,
    InteriorReference,
    CutStartFrame,
    PerformanceSource,
    ReactionPlate,
    InsertPlate,
    ContinuityFrame,
    LastFrame,
    EditSource,
    VideoTake,
    AudioTake,
    SelectedTake,
    AssemblyDraft,
    FinalExport,
}

impl AssetRole {
    pub fn parse_look(value: &str) -> Option<Self> {
        let v = value.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        let mapped = match v.as_str() {
            "interior" | "interior_reference" | "inside" | "carriage" | "cabin" => {
                Some(Self::InteriorReference)
            }
            "exterior" | "exterior_reference" | "outside" | "train_exterior" => {
                Some(Self::ExteriorReference)
            }
            "wardrobe" | "wardrobe_reference" | "costume" | "clothes" | "clothing" => {
                Some(Self::WardrobeReference)
            }
            "prop" | "prop_reference" | "props" | "parcel" => Some(Self::PropReference),
            "mood" | "mood_reference" | "atmosphere" | "tone" => Some(Self::MoodReference),
            "location" | "location_reference" | "set" | "setting" => Some(Self::LocationReference),
            "lighting" | "lighting_reference" | "light" | "lamp" => Some(Self::LightingReference),
            "composition" | "composition_reference" | "framing" => Some(Self::CompositionReference),
            "group" | "group_reference" | "ensemble" | "cast" => Some(Self::GroupReference),
            other => serde_json::from_value(serde_json::Value::String(other.to_string())).ok(),
        };
        mapped.filter(|role| role.is_scene_look())
    }

    pub fn is_scene_look(self) -> bool {
        matches!(
            self,
            Self::LocationReference
                | Self::WardrobeReference
                | Self::PropReference
                | Self::MoodReference
                | Self::LightingReference
                | Self::CompositionReference
                | Self::GroupReference
                | Self::ExteriorReference
                | Self::InteriorReference
        )
    }

    /// Interior / group / wardrobe / location / composition stills usually show people.
    pub fn look_usually_shows_people(self) -> bool {
        matches!(
            self,
            Self::InteriorReference
                | Self::GroupReference
                | Self::WardrobeReference
                | Self::LocationReference
                | Self::CompositionReference
        )
    }

    pub fn is_cut_image(self) -> bool {
        matches!(
            self,
            Self::CutStartFrame
                | Self::PerformanceSource
                | Self::ReactionPlate
                | Self::InsertPlate
                | Self::ContinuityFrame
                | Self::LastFrame
                | Self::EditSource
        )
    }

    pub fn core_look_roles() -> [Self; 5] {
        [
            Self::InteriorReference,
            Self::ExteriorReference,
            Self::WardrobeReference,
            Self::PropReference,
            Self::MoodReference,
        ]
    }

    pub fn look_roles() -> [Self; 9] {
        [
            Self::InteriorReference,
            Self::ExteriorReference,
            Self::LocationReference,
            Self::WardrobeReference,
            Self::PropReference,
            Self::MoodReference,
            Self::LightingReference,
            Self::CompositionReference,
            Self::GroupReference,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::IdentityReference => "identity_reference",
            Self::BodyReference => "body_reference",
            Self::ExpressionReference => "expression_reference",
            Self::VoiceSample => "voice_sample",
            Self::VoiceProfile => "voice_profile",
            Self::LocationReference => "location_reference",
            Self::WardrobeReference => "wardrobe_reference",
            Self::WardrobePlate => "wardrobe_plate",
            Self::PropReference => "prop_reference",
            Self::MoodReference => "mood_reference",
            Self::LightingReference => "lighting_reference",
            Self::CompositionReference => "composition_reference",
            Self::GroupReference => "group_reference",
            Self::ExteriorReference => "exterior_reference",
            Self::InteriorReference => "interior_reference",
            Self::CutStartFrame => "cut_start_frame",
            Self::PerformanceSource => "performance_source",
            Self::ReactionPlate => "reaction_plate",
            Self::InsertPlate => "insert_plate",
            Self::ContinuityFrame => "continuity_frame",
            Self::LastFrame => "last_frame",
            Self::EditSource => "edit_source",
            Self::VideoTake => "video_take",
            Self::AudioTake => "audio_take",
            Self::SelectedTake => "selected_take",
            Self::AssemblyDraft => "assembly_draft",
            Self::FinalExport => "final_export",
        }
    }

    pub fn display_label(self) -> &'static str {
        match self {
            Self::InteriorReference => "Interior",
            Self::ExteriorReference => "Exterior",
            Self::LocationReference => "Location",
            Self::WardrobeReference => "Wardrobe",
            Self::WardrobePlate => "Wardrobe plate",
            Self::PropReference => "Prop",
            Self::MoodReference => "Mood",
            Self::LightingReference => "Lighting",
            Self::CompositionReference => "Composition",
            Self::GroupReference => "Group",
            other => other.label(),
        }
    }

    pub fn file_stem(self) -> &'static str {
        match self {
            Self::InteriorReference => "interior",
            Self::ExteriorReference => "exterior",
            Self::LocationReference => "location",
            Self::WardrobeReference => "wardrobe",
            Self::PropReference => "prop",
            Self::MoodReference => "mood",
            Self::LightingReference => "lighting",
            Self::CompositionReference => "composition",
            Self::GroupReference => "group",
            other => other.label(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AssetStatus {
    #[default]
    Present,
    Draft,
    Approved,
    Rejected,
    /// Kept on disk. A newer approval replaced it; the file is not rewritten.
    Superseded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TakeStatus {
    Unreviewed,
    Selected,
    Alternate,
    Rejected,
    NeedsEdit,
    NeedsRerender,
    UsedInAssembly,
}

impl TakeStatus {
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        match value.as_str() {
            "unreviewed" => Some(Self::Unreviewed),
            "selected" => Some(Self::Selected),
            "alternate" => Some(Self::Alternate),
            "rejected" => Some(Self::Rejected),
            "needs_edit" => Some(Self::NeedsEdit),
            "needs_rerender" => Some(Self::NeedsRerender),
            "used_in_assembly" => Some(Self::UsedInAssembly),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Unreviewed => "unreviewed",
            Self::Selected => "selected",
            Self::Alternate => "alternate",
            Self::Rejected => "rejected",
            Self::NeedsEdit => "needs_edit",
            Self::NeedsRerender => "needs_rerender",
            Self::UsedInAssembly => "used_in_assembly",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualSourceStrategy {
    PreproductionStartFrame,
    PreviousSelectedTakeExitFrame,
    SharedReference,
    NoStartFrame,
    DeferredManual,
}

impl VisualSourceStrategy {
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        match value.as_str() {
            "preproduction_start_frame" | "start_frame" | "generated_start_frame" => {
                Some(Self::PreproductionStartFrame)
            }
            "previous_selected_take_exit_frame"
            | "previous_selected_take"
            | "previous_exit_frame"
            | "continuity_frame" => Some(Self::PreviousSelectedTakeExitFrame),
            "shared_reference" | "shared" => Some(Self::SharedReference),
            "no_start_frame" | "none" | "text_only" => Some(Self::NoStartFrame),
            "deferred_manual" | "manual" | "deferred" => Some(Self::DeferredManual),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::PreproductionStartFrame => "preproduction_start_frame",
            Self::PreviousSelectedTakeExitFrame => "previous_selected_take_exit_frame",
            Self::SharedReference => "shared_reference",
            Self::NoStartFrame => "no_start_frame",
            Self::DeferredManual => "deferred_manual",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualSourceSpec {
    pub strategy: VisualSourceStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor: Option<CutId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_take_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_time_seconds: Option<f64>,
    /// Stable shot id of the predecessor. The cut number in `predecessor` is only a label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_shot_id: Option<String>,
}

impl VisualSourceSpec {
    pub fn new(strategy: VisualSourceStrategy) -> Self {
        Self {
            strategy,
            predecessor: None,
            asset_id: None,
            source_take_id: None,
            frame_time_seconds: None,
            predecessor_shot_id: None,
        }
    }

    pub fn is_bound(&self) -> bool {
        match self.strategy {
            VisualSourceStrategy::NoStartFrame => true,
            VisualSourceStrategy::PreproductionStartFrame
            | VisualSourceStrategy::PreviousSelectedTakeExitFrame
            | VisualSourceStrategy::SharedReference
            | VisualSourceStrategy::DeferredManual => self.asset_id.is_some(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StageMap {
    #[serde(default)]
    pub script: StageStatus,
    #[serde(default)]
    pub characters: StageStatus,
    #[serde(default)]
    pub scene_intent: StageStatus,
    #[serde(default)]
    pub scene_look: StageStatus,
    #[serde(default)]
    pub cut_sketches: StageStatus,
    #[serde(default)]
    pub reference_images: StageStatus,
    #[serde(default)]
    pub production_notes: StageStatus,
    #[serde(default)]
    pub render: StageStatus,
    #[serde(default)]
    pub take_review: StageStatus,
    #[serde(default)]
    pub assemble: StageStatus,
    /// Readiness for stage ids the recipe names that are not one of the known fields.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, StageStatus>,
}

impl StageMap {
    pub fn set(&mut self, stage: WorkflowStage, status: StageStatus) {
        *self.get_mut(stage) = status;
    }

    pub fn status_of(&self, id: &str) -> StageStatus {
        if let Some(stage) = WorkflowStage::parse(id) {
            return self.get(stage);
        }
        self.extensions
            .get(id)
            .copied()
            .unwrap_or(StageStatus::Missing)
    }

    /// Known stages stay on their typed fields. Other ids persist in `extensions`.
    pub fn set_extension(&mut self, id: &str, status: StageStatus) {
        if let Some(stage) = WorkflowStage::parse(id) {
            self.set(stage, status);
            return;
        }
        if status == StageStatus::Missing {
            self.extensions.remove(id);
        } else {
            self.extensions.insert(id.to_string(), status);
        }
    }

    pub fn get(&self, stage: WorkflowStage) -> StageStatus {
        match stage {
            WorkflowStage::Script => self.script,
            WorkflowStage::Characters => self.characters,
            WorkflowStage::SceneIntent => self.scene_intent,
            WorkflowStage::SceneLook => self.scene_look,
            WorkflowStage::CutSketches => self.cut_sketches,
            WorkflowStage::ReferenceImages => self.reference_images,
            WorkflowStage::ProductionNotes => self.production_notes,
            WorkflowStage::Render => self.render,
            WorkflowStage::TakeReview => self.take_review,
            WorkflowStage::Assemble => self.assemble,
        }
    }

    fn get_mut(&mut self, stage: WorkflowStage) -> &mut StageStatus {
        match stage {
            WorkflowStage::Script => &mut self.script,
            WorkflowStage::Characters => &mut self.characters,
            WorkflowStage::SceneIntent => &mut self.scene_intent,
            WorkflowStage::SceneLook => &mut self.scene_look,
            WorkflowStage::CutSketches => &mut self.cut_sketches,
            WorkflowStage::ReferenceImages => &mut self.reference_images,
            WorkflowStage::ProductionNotes => &mut self.production_notes,
            WorkflowStage::Render => &mut self.render,
            WorkflowStage::TakeReview => &mut self.take_review,
            WorkflowStage::Assemble => &mut self.assemble,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScriptRecord {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CharacterProfile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub appearance: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wardrobe: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub personality: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub continuity_notes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_prompt: Option<String>,
}

impl Default for CharacterProfile {
    fn default() -> Self {
        Self {
            description: None,
            appearance: Vec::new(),
            wardrobe: Vec::new(),
            personality: Vec::new(),
            continuity_notes: Vec::new(),
            identity_prompt: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CharacterRecord {
    pub id: String,
    pub name: String,
    pub dir: String,
    #[serde(default)]
    pub identity_ready: bool,
    #[serde(default)]
    pub voice_ready: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_sheet_manifest_path: Option<String>,
    #[serde(default)]
    pub profile: CharacterProfile,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asset_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SceneIntent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub narrative_purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emotional_purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub character_focus: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visual_contrast: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub important_props: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub continuity_constraints: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sound_and_transition: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience_must_know: Option<String>,
}

impl SceneIntent {
    pub fn is_empty(&self) -> bool {
        self.narrative_purpose.is_none()
            && self.emotional_purpose.is_none()
            && self.character_focus.is_empty()
            && self.visual_contrast.is_none()
            && self.important_props.is_empty()
            && self.continuity_constraints.is_empty()
            && self.sound_and_transition.is_empty()
            && self.audience_must_know.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LookSlot {
    pub id: String,
    pub role: AssetRole,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub negative_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asset_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_asset_id: Option<String>,
}

impl LookSlot {
    pub fn new(id: impl Into<String>, role: AssetRole, title: impl Into<String>) -> Self {
        LookSlot {
            id: id.into(),
            role,
            title: title.into(),
            prompt: None,
            negative_prompt: None,
            asset_ids: Vec::new(),
            approved_asset_id: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.title.trim().is_empty()
            && self.prompt.is_none()
            && self.negative_prompt.is_none()
            && self.asset_ids.is_empty()
            && self.approved_asset_id.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LookBoard {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asset_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slots: Vec<LookSlot>,
}

impl LookBoard {
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
            && self.asset_ids.is_empty()
            && (self.slots.is_empty() || self.slots.iter().all(LookSlot::is_empty))
    }

    pub fn approved_count(&self) -> usize {
        self.slots
            .iter()
            .filter(|s| s.approved_asset_id.is_some())
            .count()
    }

    pub fn slot_mut(&mut self, slot_id: &str) -> Option<&mut LookSlot> {
        self.slots.iter_mut().find(|s| s.id == slot_id)
    }

    pub fn sync_asset_ids(&mut self) {
        let mut ids = Vec::new();
        for slot in &self.slots {
            for id in &slot.asset_ids {
                if !ids.contains(id) {
                    ids.push(id.clone());
                }
            }
        }
        self.asset_ids = ids;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SketchDialogue {
    pub speaker: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heard_as: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CutSketch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shot_role: Option<ShotRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mouth_policy: Option<MouthPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pace: Option<Pace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visual_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub negative_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reference_roles: Vec<AssetRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition_idea: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dialogue: Vec<SketchDialogue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_camera: Vec<String>,
}

impl CutSketch {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.shot_role.is_none()
            && self.subject.is_none()
            && self.speaker.is_none()
            && self.mouth_policy.is_none()
            && self.pace.is_none()
            && self.visual_prompt.is_none()
            && self.negative_prompt.is_none()
            && self.reference_roles.is_empty()
            && self.transition_idea.is_none()
            && self.dialogue.is_empty()
            && self.on_camera.is_empty()
    }
}

/// What to do with work that depended on an artifact that just changed.
/// Downstream files stay on disk either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyState {
    Stale,
    NeedsReview,
    Invalid,
    Keep,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TakeRecord {
    pub id: String,
    pub path: String,
    pub status: TakeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutRecord {
    pub id: CutId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dialogue: Vec<DialogueLine>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speech_mode: Option<SpeechMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_type: Option<SceneType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render_lane: Option<RenderLane>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub negative_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub production_note_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub identity_ref_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_start_frame: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_frame_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_frame_negative_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sketch: Option<CutSketch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visual_source: Option<VisualSourceSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub takes: Vec<TakeRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_take: Option<String>,
    /// Durable shot id. Cut numbers in `id` are display labels.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stable_id: String,
    /// Set when an approved upstream artifact changes. Downstream files stay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependency_state: Option<DependencyState>,
    /// Set when animatic timing or order changes. Does not delete takes.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub package_needs_review: bool,
}

impl CutRecord {
    pub fn new(id: CutId) -> Self {
        CutRecord {
            id,
            title: None,
            dialogue: Vec::new(),
            speech_mode: None,
            scene_type: None,
            subject: None,
            render_lane: None,
            prompt: None,
            negative_prompt: None,
            production_note_path: None,
            identity_ref_paths: Vec::new(),
            approved_start_frame: None,
            start_frame_prompt: None,
            start_frame_negative_prompt: None,
            sketch: None,
            visual_source: None,
            takes: Vec::new(),
            selected_take: None,
            stable_id: String::new(),
            dependency_state: None,
            package_needs_review: false,
        }
    }

    pub fn has_production_note(&self) -> bool {
        self.production_note_path.is_some() || self.prompt.is_some()
    }

    pub fn visual_source_strategy(&self) -> Option<VisualSourceStrategy> {
        self.visual_source
            .as_ref()
            .map(|source| source.strategy)
            .or_else(|| {
                self.approved_start_frame
                    .as_ref()
                    .map(|_| VisualSourceStrategy::PreproductionStartFrame)
            })
    }

    pub fn visual_source_planned(&self) -> bool {
        self.visual_source_strategy().is_some()
    }

    pub fn visual_source_ready(&self) -> bool {
        match &self.visual_source {
            Some(source) if source.strategy == VisualSourceStrategy::PreproductionStartFrame => {
                source.asset_id.is_some() || self.approved_start_frame.is_some()
            }
            Some(source) => source.is_bound(),
            None => self.approved_start_frame.is_some(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageWarningKind {
    MissingReaction,
    MissingInsert,
    MissingEstablishing,
    UnplannedBeat,
}

impl CoverageWarningKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingReaction => "missing_reaction",
            Self::MissingInsert => "missing_insert",
            Self::MissingEstablishing => "missing_establishing",
            Self::UnplannedBeat => "unplanned_beat",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoverageWarning {
    pub id: String,
    pub kind: CoverageWarningKind,
    pub message: String,
}

/// Timing and order for one cut sketch. The id is allocated once and is not
/// the cut number or the position in the rough cut.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimaticShot {
    pub id: String,
    pub cut_id: CutId,
    pub duration_seconds: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scratch_audio_path: Option<String>,
}

/// Rough-cut timing laid over cut sketches. Not a second copy of the prompts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneAnimatic {
    pub id: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub approved: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shots: Vec<AnimaticShot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<CoverageWarning>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneRecord {
    pub id: u32,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_excerpt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub action: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sound: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transitions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dialogue: Vec<DialogueLine>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub beats: Vec<ScriptBeat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<SceneIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look_board: Option<LookBoard>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<SceneStrategy>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub strategy_override: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub intent_approved: bool,
    #[serde(default)]
    pub cuts: Vec<CutRecord>,
    #[serde(default)]
    pub stages: StageMap,
    /// Durable scene id. `id` is the display number.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stable_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animatic: Option<SceneAnimatic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<crate::workflow::post::ScenePost>,
}

impl SceneRecord {
    pub fn new(id: u32, title: impl Into<String>) -> Self {
        SceneRecord {
            id,
            title: title.into(),
            source_path: None,
            source_excerpt: None,
            location: None,
            action: Vec::new(),
            sound: Vec::new(),
            transitions: Vec::new(),
            dialogue: Vec::new(),
            beats: Vec::new(),
            intent: None,
            look_board: None,
            strategy: None,
            strategy_override: false,
            intent_approved: false,
            cuts: Vec::new(),
            stages: StageMap::default(),
            stable_id: String::new(),
            animatic: None,
            post: None,
        }
    }

    pub fn cut_mut(&mut self, cut: u32) -> &mut CutRecord {
        if let Some(i) = self.cuts.iter().position(|c| c.id.cut == cut) {
            return &mut self.cuts[i];
        }
        self.cuts.push(CutRecord::new(CutId::new(self.id, cut)));
        self.cuts.last_mut().expect("just pushed")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetRecord {
    pub id: String,
    pub path: String,
    pub role: AssetRole,
    pub status: AssetStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut_id: Option<CutId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_asset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Where a job applies. `kind` is `project`, `scene`, `shot`, `asset`, or `deliverable`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobScope {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobRecord {
    pub id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_asset_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_asset_ids: Vec<String>,
    #[serde(default)]
    pub status: JobStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attempts: Vec<JobAttempt>,
    /// RFC3339 UTC. Absent on older state; never treat absence as zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<JobScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Job id this attempt retries. The previous job stays in the ledger.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_of: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobAttempt {
    pub id: String,
    #[serde(default)]
    pub status: JobStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_asset_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

/// A frozen path for one asset. Approving a replacement does not rewrite this path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactVersion {
    pub id: String,
    pub asset_id: String,
    pub version: u32,
    pub path: String,
    #[serde(default)]
    pub status: AssetStatus,
}

impl JobRecord {
    /// One pending attempt. Look generation, renders, stills, and assembly all use this.
    pub fn open(id: impl Into<String>, kind: impl Into<String>) -> Self {
        let id = id.into();
        let attempt_id = format!("{id}_attempt_1");
        Self {
            id,
            kind: kind.into(),
            provider: None,
            model: None,
            input_asset_ids: Vec::new(),
            output_asset_ids: Vec::new(),
            status: JobStatus::Pending,
            output_path: None,
            raw_provider_id: None,
            error: None,
            created_at: Some(utc_now()),
            started_at: None,
            completed_at: None,
            scope: None,
            progress: None,
            cost: None,
            estimated_cost: None,
            currency: None,
            retry_of: None,
            attempts: vec![JobAttempt {
                id: attempt_id,
                status: JobStatus::Pending,
                error: None,
                output_asset_ids: Vec::new(),
                started_at: None,
                completed_at: None,
            }],
        }
    }

    pub fn finish(&mut self, path: Option<&str>, asset_id: Option<&str>) {
        self.status = JobStatus::Completed;
        if self.completed_at.is_none() {
            self.completed_at = Some(utc_now());
        }
        if let Some(path) = path {
            self.output_path = Some(path.to_string());
        }
        if let Some(asset_id) = asset_id {
            if !self.output_asset_ids.iter().any(|id| id == asset_id) {
                self.output_asset_ids.push(asset_id.to_string());
            }
        }
        if let Some(attempt) = self.attempts.last_mut() {
            attempt.status = JobStatus::Completed;
            if attempt.completed_at.is_none() {
                attempt.completed_at = self.completed_at.clone();
            }
            if let Some(asset_id) = asset_id {
                if !attempt.output_asset_ids.iter().any(|id| id == asset_id) {
                    attempt.output_asset_ids.push(asset_id.to_string());
                }
            }
        }
    }

    /// Elapsed time from a real start timestamp to a real completion timestamp.
    /// Missing timestamps are unknown, not zero.
    pub fn elapsed_label(&self) -> String {
        let start = self
            .started_at
            .as_deref()
            .or(self.created_at.as_deref())
            .and_then(parse_utc);
        let end = self.completed_at.as_deref().and_then(parse_utc);
        match (start, end) {
            (Some(start), Some(end)) if end >= start => format_elapsed_seconds(end - start),
            _ => "unknown".into(),
        }
    }
}

/// UTC timestamp `YYYY-MM-DDTHH:MM:SSZ`.
pub fn utc_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format_unix_utc(secs)
}

pub fn format_unix_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

pub fn parse_utc(value: &str) -> Option<u64> {
    let value = value.trim().trim_end_matches('Z');
    let (date, time) = value.split_once('T')?;
    let mut date = date.split('-');
    let year: i32 = date.next()?.parse().ok()?;
    let month: u32 = date.next()?.parse().ok()?;
    let day: u32 = date.next()?.parse().ok()?;
    if date.next().is_some() {
        return None;
    }
    let mut time = time.split(':');
    let hour: u64 = time.next()?.parse().ok()?;
    let minute: u64 = time.next()?.parse().ok()?;
    let second: u64 = time.next()?.parse().ok()?;
    if time.next().is_some() || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let days = days_from_civil(year, month, day)?;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second)
}

fn format_elapsed_seconds(seconds: u64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {}m {}s", seconds / 3600, (seconds % 3600) / 60, seconds % 60)
    }
}

fn civil_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (year as i32, month as u32, day as u32)
}

fn days_from_civil(year: i32, month: u32, day: u32) -> Option<u64> {
    if !(1..=12).contains(&month) || day == 0 || day > 31 {
        return None;
    }
    let year = year as i64;
    let month = month as i64;
    let day = day as i64;
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = (year - era * 400) as u64;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp as u64 + 2) / 5 + day as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe as i64 - 719_468;
    u64::try_from(days).ok()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SettingsSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<String>,
}

/// Registry id for a recipe. Built-in ids come from the host table.
/// A project can also name a recipe that arrived as a validated document.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProductionRecipe(String);

impl Default for ProductionRecipe {
    fn default() -> Self {
        Self("narrative_dialogue".to_string())
    }
}

impl ProductionRecipe {
    pub fn narrative() -> Self {
        Self("narrative_dialogue".to_string())
    }

    pub fn montage() -> Self {
        Self("montage".to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Accept an id only when the built-in recipe table has that record.
    pub fn parse(value: &str) -> Option<Self> {
        crate::workflow::recipe::canonical_id(value).map(Self)
    }

    pub fn label(&self) -> String {
        crate::workflow::recipe::graph(self.clone()).label.clone()
    }
}

impl Serialize for ProductionRecipe {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ProductionRecipe {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        if let Some(parsed) = Self::parse(&value) {
            return Ok(parsed);
        }
        crate::workflow::recipe::custom_recipe_id(&value)
            .map(Self)
            .ok_or_else(|| D::Error::custom(format!("unknown recipe `{value}`")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementKind {
    Cast,
    Location,
    Prop,
    Wardrobe,
    Sound,
    Graphic,
    RiskyShot,
    Deferred,
    Voice,
    Plate,
}

impl RequirementKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cast => "cast",
            Self::Location => "location",
            Self::Prop => "prop",
            Self::Wardrobe => "wardrobe",
            Self::Sound => "sound",
            Self::Graphic => "graphic",
            Self::RiskyShot => "risky_shot",
            Self::Deferred => "deferred",
            Self::Voice => "voice",
            Self::Plate => "plate",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Cast => "Cast",
            Self::Location => "Location",
            Self::Prop => "Prop",
            Self::Wardrobe => "Wardrobe",
            Self::Sound => "Sound",
            Self::Graphic => "Graphic",
            Self::RiskyShot => "Risky shot",
            Self::Deferred => "Deferred",
            Self::Voice => "Voice",
            Self::Plate => "Plate",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        let v = value.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        match v.as_str() {
            "cast" | "character" => Some(Self::Cast),
            "location" => Some(Self::Location),
            "prop" => Some(Self::Prop),
            "wardrobe" => Some(Self::Wardrobe),
            "sound" => Some(Self::Sound),
            "graphic" | "graphics" => Some(Self::Graphic),
            "risky_shot" | "risk" => Some(Self::RiskyShot),
            "deferred" => Some(Self::Deferred),
            "voice" => Some(Self::Voice),
            "plate" => Some(Self::Plate),
            _ => None,
        }
    }

    pub fn sort_key(self) -> u8 {
        match self {
            Self::Cast => 0,
            Self::Voice => 1,
            Self::Location => 2,
            Self::Prop => 3,
            Self::Wardrobe => 4,
            Self::Plate => 5,
            Self::Sound => 6,
            Self::Graphic => 7,
            Self::RiskyShot => 8,
            Self::Deferred => 9,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RequirementStatus {
    #[default]
    Missing,
    Draft,
    Present,
    Approved,
    Deferred,
}

impl RequirementStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Draft => "draft",
            Self::Present => "present",
            Self::Approved => "approved",
            Self::Deferred => "deferred",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        let v = value.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        match v.as_str() {
            "missing" => Some(Self::Missing),
            "draft" => Some(Self::Draft),
            "present" => Some(Self::Present),
            "approved" => Some(Self::Approved),
            "deferred" => Some(Self::Deferred),
            _ => None,
        }
    }

    pub fn rank(self) -> u8 {
        match self {
            Self::Missing => 0,
            Self::Draft => 1,
            Self::Deferred => 2,
            Self::Present => 3,
            Self::Approved => 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeliverableRecord {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_rate: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductionProfile {
    pub recipe: ProductionRecipe,
    /// Validated recipe data for an id the built-in table does not contain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe_document: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deliverables: Vec<DeliverableRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aspect_ratios: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_rate: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medium: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_plan: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub continuity_rules: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub safety_constraints: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rights_notes: Option<String>,
}

impl Default for ProductionProfile {
    fn default() -> Self {
        Self {
            recipe: ProductionRecipe::narrative(),
            recipe_document: None,
            deliverables: Vec::new(),
            audience: None,
            platforms: Vec::new(),
            runtime: None,
            aspect_ratios: Vec::new(),
            resolution: None,
            frame_rate: None,
            language: None,
            medium: None,
            style_notes: None,
            audio_plan: None,
            continuity_rules: Vec::new(),
            safety_constraints: Vec::new(),
            budget: None,
            schedule: None,
            rights_notes: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequirementRecord {
    pub id: String,
    pub kind: RequirementKind,
    pub name: String,
    pub status: RequirementStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scene_ids: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// When set, reimport keeps this status instead of recomputing it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub status_override: bool,
    /// Kept across reimport even when the script scan does not derive it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub user_added: bool,
}

impl RequirementRecord {
    /// A scene can be blocked on a location, prop, voice, or plate.
    pub fn blocks_scene(&self) -> bool {
        matches!(
            self.kind,
            RequirementKind::Location
                | RequirementKind::Prop
                | RequirementKind::Voice
                | RequirementKind::Plate
        ) && matches!(
            self.status,
            RequirementStatus::Missing | RequirementStatus::Draft
        )
    }
}

pub fn deliverable_id(name: &str) -> String {
    let slug = slugify(name);
    if slug.is_empty() {
        "del_deliverable".into()
    } else {
        format!("del_{slug}")
    }
}

/// Stable registry id from kind plus a content key. Not a display index.
pub fn requirement_id(kind: RequirementKind, key: &str) -> String {
    let slug = slugify(key);
    let slug = if slug.is_empty() {
        "item".to_string()
    } else {
        slug
    };
    format!("req_{}_{slug}", kind.as_str())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectState {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    #[serde(default)]
    pub scripts: Vec<ScriptRecord>,
    #[serde(default)]
    pub characters: Vec<CharacterRecord>,
    #[serde(default)]
    pub scenes: Vec<SceneRecord>,
    #[serde(default)]
    pub assets: Vec<AssetRecord>,
    #[serde(default)]
    pub jobs: Vec<JobRecord>,
    #[serde(default)]
    pub settings: SettingsSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<ProductionProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<RequirementRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<ArtifactVersion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// Accept workflow files from the previous schema and unknown recipe ids.
/// Newer files than this build are refused. Recipe work areas stay derived.
fn migrate_workflow_json(value: &mut serde_json::Value) -> Result<Vec<String>, String> {
    let version = value.get("version").and_then(|item| item.as_u64()).unwrap_or(1);
    if version > u64::from(WORKFLOW_VERSION) {
        return Err(format!(
            "workflow state version {version} is not supported (expected {WORKFLOW_VERSION})"
        ));
    }
    let mut warnings = Vec::new();
    let recipe_id = value
        .pointer("/profile/recipe")
        .and_then(|item| item.as_str())
        .map(str::to_string);
    if let Some(recipe) = recipe_id {
        if ProductionRecipe::parse(&recipe).is_none() {
            let normalized = crate::workflow::recipe::recipe_key(&recipe);
            let document = value.pointer("/profile/recipe_document").cloned();
            let accepted = document
                .as_ref()
                .and_then(|item| crate::workflow::recipe::parse_recipe_document(item).ok())
                .is_some_and(|item| item.id == normalized);
            if accepted {
                if let Some(profile) = value.get_mut("profile").and_then(|item| item.as_object_mut()) {
                    profile.insert(
                        "recipe".into(),
                        serde_json::Value::String(normalized),
                    );
                }
            } else {
                let reason = document
                    .as_ref()
                    .map(|item| {
                        crate::workflow::recipe::parse_recipe_document(item)
                            .err()
                            .unwrap_or_else(|| "recipe document id does not match the profile".into())
                    })
                    .unwrap_or_else(|| "no recipe document was included".into());
                warnings.push(format!(
                    "Unknown recipe `{recipe}` loaded as narrative dialogue. {reason}."
                ));
                if let Some(profile) = value.get_mut("profile").and_then(|item| item.as_object_mut()) {
                    profile.insert(
                        "recipe".into(),
                        serde_json::Value::String("narrative_dialogue".into()),
                    );
                    profile.remove("recipe_document");
                }
            }
        }
    }
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "version".into(),
            serde_json::Value::from(WORKFLOW_VERSION),
        );
    }
    Ok(warnings)
}

impl ProjectState {
    pub fn new() -> Self {
        ProjectState {
            version: WORKFLOW_VERSION,
            project_name: None,
            scripts: Vec::new(),
            characters: Vec::new(),
            scenes: Vec::new(),
            assets: Vec::new(),
            jobs: Vec::new(),
            settings: SettingsSnapshot::default(),
            profile: None,
            requirements: Vec::new(),
            versions: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let mut value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("workflow state json: {e}"))?;
        let warnings = migrate_workflow_json(&mut value)?;
        let mut state: Self = serde_json::from_value(value)
            .map_err(|e| format!("workflow state json: {e}"))?;
        for warning in warnings {
            if !state.notes.iter().any(|note| note == &warning) {
                state.notes.push(warning);
            }
        }
        state.version = WORKFLOW_VERSION;
        Ok(state)
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self)
            .map(|s| s + "\n")
            .map_err(|e| format!("workflow state json: {e}"))
    }

    pub fn scene_mut(&mut self, id: u32, title: &str) -> &mut SceneRecord {
        if let Some(i) = self.scenes.iter().position(|s| s.id == id) {
            return &mut self.scenes[i];
        }
        self.scenes.push(SceneRecord::new(id, title));
        self.scenes.last_mut().expect("just pushed")
    }

    pub fn scene(&self, id: u32) -> Option<&SceneRecord> {
        self.scenes.iter().find(|s| s.id == id)
    }

    pub fn next_asset_id(&self) -> String {
        let next = self
            .assets
            .iter()
            .filter_map(|asset| asset.id.strip_prefix("asset_")?.parse::<u64>().ok())
            .max()
            .unwrap_or(0)
            + 1;
        format!("asset_{next}")
    }

    pub fn push_asset(&mut self, mut asset: AssetRecord) -> String {
        if asset.id.is_empty() {
            asset.id = self.next_asset_id();
        }
        let id = asset.id.clone();
        self.assets.push(asset);
        id
    }

    pub fn next_job_id(&self) -> String {
        let next = self
            .jobs
            .iter()
            .filter_map(|job| job.id.strip_prefix("job_")?.parse::<u64>().ok())
            .max()
            .unwrap_or(0)
            + 1;
        format!("job_{next}")
    }

    pub fn asset(&self, id: &str) -> Option<&AssetRecord> {
        self.assets.iter().find(|a| a.id == id)
    }

    pub fn asset_mut(&mut self, id: &str) -> Option<&mut AssetRecord> {
        self.assets.iter_mut().find(|a| a.id == id)
    }

    pub fn blockers_for_scene(&self, scene_id: u32) -> Vec<&RequirementRecord> {
        self.requirements
            .iter()
            .filter(|requirement| {
                requirement.scene_ids.contains(&scene_id) && requirement.blocks_scene()
            })
            .collect()
    }

    pub fn requirement_mut(&mut self, id: &str) -> Option<&mut RequirementRecord> {
        self.requirements
            .iter_mut()
            .find(|requirement| requirement.id == id)
    }
}

impl Default for ProjectState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn character_id(name: &str) -> String {
    let slug = slugify(name);
    if slug.is_empty() {
        "character".into()
    } else {
        slug
    }
}

pub fn role_for_shot_id(shot_id: &str) -> AssetRole {
    let id = shot_id.to_ascii_lowercase();
    if id.starts_with("expression") {
        AssetRole::ExpressionReference
    } else if id.starts_with("standing") || id.starts_with("t_pose") || id.contains("body") {
        AssetRole::BodyReference
    } else {
        AssetRole::IdentityReference
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_assigns_stable_ids() {
        let mut s = ProjectState::new();
        s.project_name = Some("Lite_Challenge".into());
        let scene = s.scene_mut(1, "Scene 1 — The last train");
        scene.cut_mut(2).title = Some("Cocoa".into());
        let json = s.to_json().unwrap();
        let back = ProjectState::from_json(&json).unwrap();
        assert_eq!(back.scenes[0].cuts[0].id, CutId::new(1, 2));
        assert_eq!(character_id("Tom Vale"), "tom_vale");
        assert_eq!(
            role_for_shot_id("closeup_front"),
            AssetRole::IdentityReference
        );
        assert_eq!(role_for_shot_id("standing_left"), AssetRole::BodyReference);
        assert_eq!(
            role_for_shot_id("expression_sad"),
            AssetRole::ExpressionReference
        );
    }

    #[test]
    fn ids_do_not_reuse_deleted_numeric_slots() {
        let mut state = ProjectState::new();
        for id in ["asset_1", "asset_3"] {
            state.assets.push(AssetRecord {
                id: id.into(),
                path: format!("{id}.png"),
                role: AssetRole::MoodReference,
                status: AssetStatus::Draft,
                scene_id: Some(1),
                cut_id: None,
                character_id: None,
                source_job_id: None,
                parent_asset_id: None,
                notes: None,
            });
        }
        let mut job = JobRecord::open("job_4", "scene_look");
        job.status = JobStatus::Completed;
        state.jobs.push(job);
        assert_eq!(state.next_asset_id(), "asset_4");
        assert_eq!(state.next_job_id(), "job_5");
    }

    #[test]
    fn older_jobs_load_without_timing_or_cost() {
        let job: JobRecord = serde_json::from_str(
            r#"{"id":"job_1","kind":"render","status":"completed","attempts":[{"id":"job_1_attempt_1","status":"completed"}]}"#,
        )
        .unwrap();
        assert!(job.created_at.is_none());
        assert!(job.started_at.is_none());
        assert!(job.completed_at.is_none());
        assert!(job.cost.is_none());
        assert!(job.estimated_cost.is_none());
        assert!(job.currency.is_none());
        assert!(job.scope.is_none());
        assert_eq!(job.elapsed_label(), "unknown");
        assert!(job.attempts[0].completed_at.is_none());
    }

    #[test]
    fn elapsed_uses_real_timestamps_and_does_not_invent_zero() {
        assert_eq!(format_unix_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(parse_utc("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_utc("1970-01-01T00:01:30Z"), Some(90));
        let mut job = JobRecord::open("job_1", "render");
        job.created_at = Some("1970-01-01T00:00:00Z".into());
        job.completed_at = None;
        assert_eq!(job.elapsed_label(), "unknown");
        job.completed_at = Some("1970-01-01T00:01:30Z".into());
        assert_eq!(job.elapsed_label(), "1m 30s");
        job.started_at = Some("1970-01-01T00:01:00Z".into());
        assert_eq!(job.elapsed_label(), "30s");
    }
}
