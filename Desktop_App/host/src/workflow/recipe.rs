//! Recipe graphs. The host reads stages, dependencies, required roles, and
//! readiness from this data. It does not branch on the recipe name.
use crate::scene_plan::handoff::SpeechMode;
use crate::workflow::post::CutVersionStage;
use crate::workflow::state::{
    CutRecord, ProductionRecipe, ProjectState, SceneRecord, StageStatus, WorkflowStage,
};
use crate::workflow::validate::has_dialogue;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkArea {
    Plan,
    Produce,
    Finish,
}

impl WorkArea {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Produce => "produce",
            Self::Finish => "finish",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "plan" => Some(Self::Plan),
            "produce" => Some(Self::Produce),
            "finish" => Some(Self::Finish),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeStageReport {
    pub id: String,
    pub label: String,
    pub work_area: String,
    pub depends_on: Vec<String>,
    pub required_roles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageNav {
    pub id: String,
    pub label: String,
    pub work_area: String,
    pub status: String,
    pub available: bool,
    pub disabled_reason: Option<String>,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeReport {
    pub id: String,
    pub label: String,
    pub stages: Vec<RecipeStageReport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeChoice {
    pub id: String,
    pub label: String,
}

const RECIPE_SCHEMA_VERSION: u64 = 1;

fn registry() -> &'static [LoadedRecipe] {
    static REGISTRY: std::sync::OnceLock<Vec<LoadedRecipe>> = std::sync::OnceLock::new();
    REGISTRY.get_or_init(|| {
        let raw: serde_json::Value = serde_json::from_str(include_str!("recipes.json"))
            .expect("built-in recipe registry is not valid json");
        let items = raw
            .as_array()
            .expect("built-in recipe registry must be a list");
        items
            .iter()
            .map(|item| {
                parse_recipe_record(item)
                    .unwrap_or_else(|error| panic!("built-in recipe registry: {error}"))
            })
            .collect()
    })
}

pub fn choices() -> Vec<RecipeChoice> {
    registry()
        .iter()
        .map(|recipe| RecipeChoice {
            id: recipe.id.clone(),
            label: recipe.label.clone(),
        })
        .collect()
}

/// Map an alias or registry id onto a recipe id in the host registry file.
pub fn canonical_id(value: &str) -> Option<String> {
    let key = recipe_key(value);
    let key = match key.as_str() {
        "narrative" => "narrative_dialogue".to_string(),
        "music_video" => "montage".to_string(),
        other => other.to_string(),
    };
    registry()
        .iter()
        .find(|recipe| recipe.id == key)
        .map(|recipe| recipe.id.clone())
}

/// Look up a built-in recipe. Unknown ids use the first registry record.
pub fn graph(recipe: ProductionRecipe) -> &'static LoadedRecipe {
    registry()
        .iter()
        .find(|item| item.id == recipe.as_str())
        .unwrap_or(&registry()[0])
}

/// One validated recipe. Built-ins and project documents use this same record.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedRecipe {
    pub id: String,
    pub label: String,
    pub stages: Vec<LoadedStage>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadedStage {
    pub id: String,
    pub label: String,
    pub work_area: WorkArea,
    pub depends_on: Vec<String>,
    pub required_roles: Vec<String>,
}

impl LoadedRecipe {
    pub fn includes(&self, stage: &str) -> bool {
        self.stages.iter().any(|item| item.id == stage)
    }

    pub fn requires_role(&self, role: &str) -> bool {
        self.stages
            .iter()
            .any(|stage| stage.required_roles.iter().any(|item| item == role))
    }

    pub fn report(&self) -> RecipeReport {
        RecipeReport {
            id: self.id.clone(),
            label: self.label.clone(),
            stages: self
                .stages
                .iter()
                .map(|stage| RecipeStageReport {
                    id: stage.id.clone(),
                    label: stage.label.clone(),
                    work_area: stage.work_area.as_str().to_string(),
                    depends_on: stage.depends_on.clone(),
                    required_roles: stage.required_roles.clone(),
                })
                .collect(),
        }
    }

    pub fn navigation(&self, scene: &SceneRecord) -> Vec<StageNav> {
        let rows: Vec<NavRow<'_>> = self
            .stages
            .iter()
            .map(|stage| NavRow {
                id: stage.id.as_str(),
                label: stage.label.as_str(),
                work_area: stage.work_area.as_str(),
                depends_on: &stage.depends_on,
            })
            .collect();
        navigation_rows(&rows, scene)
    }
}

struct NavRow<'a> {
    id: &'a str,
    label: &'a str,
    work_area: &'a str,
    depends_on: &'a [String],
}

fn navigation_rows(stages: &[NavRow<'_>], scene: &SceneRecord) -> Vec<StageNav> {
    stages
        .iter()
        .map(|stage| {
            let status = stage_status(scene, stage.id);
            let blocking: Vec<&str> = stage
                .depends_on
                .iter()
                .map(String::as_str)
                .filter(|dependency| stage_status(scene, dependency) == StageStatus::Missing)
                .collect();
            let available = blocking.is_empty() || status != StageStatus::Missing;
            let disabled_reason = if available {
                None
            } else {
                let labels: Vec<&str> = blocking
                    .iter()
                    .map(|dependency| {
                        stages
                            .iter()
                            .find(|item| item.id == *dependency)
                            .map(|item| item.label)
                            .unwrap_or(dependency)
                    })
                    .collect();
                Some(format!("Waiting on {}.", labels.join(", ")))
            };
            StageNav {
                id: stage.id.to_string(),
                label: stage.label.to_string(),
                work_area: stage.work_area.to_string(),
                status: status.as_str().to_string(),
                available,
                disabled_reason,
                depends_on: stage.depends_on.to_vec(),
            }
        })
        .collect()
}

pub fn recipe_key(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace(['-', ' '], "_")
}

fn recipe_slug(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
}

/// A slug the built-in table does not already own. Project load uses this for document-backed recipes.
pub fn custom_recipe_id(value: &str) -> Option<String> {
    let key = recipe_key(value);
    if !recipe_slug(&key) || canonical_id(&key).is_some() {
        return None;
    }
    Some(key)
}

pub fn parse_recipe_document(value: &serde_json::Value) -> Result<LoadedRecipe, String> {
    let document = parse_recipe_record(value)?;
    if canonical_id(&document.id).is_some() {
        return Err("recipe document needs an id that is not a built-in recipe".to_string());
    }
    Ok(document)
}

/// Validate a recipe record. Built-in registry entries and project documents share this.
pub fn parse_recipe_record(value: &serde_json::Value) -> Result<LoadedRecipe, String> {
    let version = match value.get("version") {
        None => 1,
        Some(item) => item
            .as_u64()
            .ok_or_else(|| "recipe schema version must be a number".to_string())?,
    };
    if version == 0 || version > RECIPE_SCHEMA_VERSION {
        return Err(format!(
            "recipe schema version {version} is not supported (expected {RECIPE_SCHEMA_VERSION})"
        ));
    }
    let id = recipe_key(value.get("id").and_then(|item| item.as_str()).unwrap_or(""));
    if !recipe_slug(&id) {
        return Err("recipe document needs an id".to_string());
    }
    let label = value
        .get("label")
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .trim();
    if label.is_empty() {
        return Err(format!("recipe `{id}` needs a label"));
    }
    let raw_stages = value
        .get("stages")
        .and_then(|item| item.as_array())
        .ok_or_else(|| format!("recipe `{id}` needs stages"))?;
    if raw_stages.is_empty() {
        return Err(format!("recipe `{id}` needs stages"));
    }
    let mut stages = Vec::new();
    for item in raw_stages {
        let stage_id = recipe_key(item.get("id").and_then(|part| part.as_str()).unwrap_or(""));
        if !recipe_slug(&stage_id) {
            return Err(format!("recipe `{id}` has an empty stage id"));
        }
        let stage_label = item
            .get("label")
            .and_then(|part| part.as_str())
            .unwrap_or("")
            .trim();
        if stage_label.is_empty() {
            return Err(format!("recipe `{id}` stage `{stage_id}` needs a label"));
        }
        let work_area = item
            .get("work_area")
            .and_then(|part| part.as_str())
            .and_then(WorkArea::parse)
            .ok_or_else(|| format!("recipe `{id}` stage `{stage_id}` needs a work area"))?;
        let depends_on = string_list(item.get("depends_on"))?;
        let required_roles = string_list(item.get("required_roles"))?;
        stages.push(LoadedStage {
            id: stage_id,
            label: stage_label.to_string(),
            work_area,
            depends_on,
            required_roles,
        });
    }
    let document = LoadedRecipe {
        id,
        label: label.to_string(),
        stages,
    };
    validate_document(&document)?;
    Ok(document)
}

fn string_list(value: Option<&serde_json::Value>) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let items = value
        .as_array()
        .ok_or_else(|| "recipe stage lists must be arrays".to_string())?;
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(recipe_key)
                .filter(|text| recipe_slug(text))
                .ok_or_else(|| "recipe stage lists need stage or role ids".to_string())
        })
        .collect()
}

fn validate_document(document: &LoadedRecipe) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for stage in &document.stages {
        if !seen.insert(stage.id.as_str()) {
            return Err(format!(
                "recipe {} has a duplicate or empty stage id",
                document.id
            ));
        }
        for dependency in &stage.depends_on {
            if !document.stages.iter().any(|item| item.id == *dependency) {
                return Err(format!(
                    "recipe {} stage {} depends on unknown stage {dependency}",
                    document.id, stage.id
                ));
            }
        }
    }
    Ok(())
}

/// Built-in registry record, or the project's validated recipe document when its id matches.
pub fn bound(state: &ProjectState) -> LoadedRecipe {
    let recipe = state
        .profile
        .as_ref()
        .map(|profile| profile.recipe.clone())
        .unwrap_or_default();
    if let Some(raw) = state
        .profile
        .as_ref()
        .and_then(|profile| profile.recipe_document.as_ref())
    {
        if let Ok(document) = parse_recipe_document(raw) {
            if document.id == recipe.as_str() {
                return document;
            }
        }
    }
    graph(recipe).clone()
}

pub fn recipe_requires_role(state: &ProjectState, role: &str) -> bool {
    bound(state).requires_role(role)
}

pub fn strip_mouths(scene: &mut SceneRecord) {
    for cut in &mut scene.cuts {
        clear_mouth(cut);
    }
}

/// Stages the recipe actually offers, in recipe order. Omitted stages are absent.
/// A stage with missing dependencies stays visible and disabled instead of becoming a dead control.
pub fn navigation(recipe: &LoadedRecipe, scene: &SceneRecord) -> Vec<StageNav> {
    recipe.navigation(scene)
}

fn stage_status(scene: &SceneRecord, id: &str) -> StageStatus {
    scene.stages.status_of(id)
}

pub fn apply_stages(state: &mut ProjectState) {
    let active = bound(state);
    let any_characters = !state.characters.is_empty();
    for scene in &mut state.scenes {
        for stage in WorkflowStage::ALL {
            let status = if active.includes(stage.as_str()) {
                readiness(stage, scene, any_characters)
            } else {
                StageStatus::Missing
            };
            scene.stages.set(stage, status);
        }
    }
}

/// Drop mouth policy and lip-sync when the active graph does not require them.
pub fn clear_unrequired_mouths(recipe: &LoadedRecipe, scene: &mut SceneRecord) {
    if recipe.requires_role("mouth_policy") {
        return;
    }
    for cut in &mut scene.cuts {
        clear_mouth(cut);
    }
}

pub fn clear_mouth(cut: &mut CutRecord) {
    if let Some(sketch) = cut.sketch.as_mut() {
        sketch.mouth_policy = None;
    }
    if cut.sketch.as_ref().is_some_and(|sketch| !sketch.is_empty()) {
        cut.speech_mode = Some(if has_dialogue(cut) {
            SpeechMode::VoiceOver
        } else {
            SpeechMode::Silent
        });
    }
}

fn assemble_readiness(scene: &SceneRecord) -> StageStatus {
    let Some(post) = scene.post.as_ref() else {
        return StageStatus::Missing;
    };
    if post.picture_locked_id.is_some() {
        return StageStatus::Approved;
    }
    if post.versions.iter().any(|version| {
        version
            .assembly_path
            .as_ref()
            .is_some_and(|path| !path.is_empty())
            || matches!(
                version.stage,
                CutVersionStage::Assembly | CutVersionStage::FineCut | CutVersionStage::PictureLock
            )
    }) {
        StageStatus::Present
    } else {
        StageStatus::Missing
    }
}

fn readiness(stage: WorkflowStage, scene: &SceneRecord, any_characters: bool) -> StageStatus {
    match stage {
        WorkflowStage::Script => {
            if scene.source_excerpt.is_some() || !scene.dialogue.is_empty() {
                StageStatus::Present
            } else {
                StageStatus::Missing
            }
        }
        WorkflowStage::Characters => {
            if any_characters {
                StageStatus::Present
            } else {
                StageStatus::Missing
            }
        }
        WorkflowStage::SceneIntent => {
            if scene
                .intent
                .as_ref()
                .is_some_and(|intent| !intent.is_empty())
            {
                if scene.intent_approved {
                    StageStatus::Approved
                } else {
                    StageStatus::Draft
                }
            } else {
                StageStatus::Missing
            }
        }
        WorkflowStage::SceneLook => {
            let board = scene.look_board.as_ref();
            if board.map(|board| board.is_empty()).unwrap_or(true) {
                StageStatus::Missing
            } else if board.is_some_and(|board| board.approved_count() > 0) {
                StageStatus::Approved
            } else {
                StageStatus::Draft
            }
        }
        WorkflowStage::CutSketches => {
            let sketches = scene
                .cuts
                .iter()
                .filter(|cut| cut.sketch.as_ref().is_some_and(|sketch| !sketch.is_empty()))
                .count();
            if sketches == 0 {
                StageStatus::Missing
            } else {
                StageStatus::Draft
            }
        }
        WorkflowStage::ReferenceImages => {
            let planned = scene
                .cuts
                .iter()
                .filter(|cut| cut.visual_source_planned())
                .count();
            if planned == 0 {
                StageStatus::Missing
            } else if planned == scene.cuts.len() {
                StageStatus::Approved
            } else {
                StageStatus::Draft
            }
        }
        WorkflowStage::ProductionNotes => {
            if scene.cuts.iter().any(|cut| cut.has_production_note()) {
                StageStatus::Present
            } else {
                StageStatus::Missing
            }
        }
        WorkflowStage::Render => {
            if scene.cuts.iter().any(|cut| !cut.takes.is_empty()) {
                StageStatus::Present
            } else {
                StageStatus::Missing
            }
        }
        WorkflowStage::TakeReview => {
            if scene.cuts.iter().any(|cut| cut.selected_take.is_some()) {
                StageStatus::Present
            } else {
                StageStatus::Missing
            }
        }
        WorkflowStage::Assemble => assemble_readiness(scene),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_plan::handoff::CutId;
    use crate::scene_plan::handoff::SpeechMode;
    use crate::workflow::state::{
        CharacterProfile, CharacterRecord, CutRecord, CutSketch, MouthPolicy, ProductionProfile,
        ShotRole, TakeRecord, TakeStatus,
    };

    #[test]
    fn narrative_graph_keeps_the_slice_and_requires_cast_and_mouth() {
        let narrative = graph(ProductionRecipe::narrative());
        let _montage = graph(ProductionRecipe::montage());
        let ids: Vec<_> = narrative
            .stages
            .iter()
            .map(|stage| stage.id.as_str())
            .collect();
        let expected: Vec<_> = WorkflowStage::ALL
            .iter()
            .map(|stage| stage.as_str())
            .collect();
        assert_eq!(ids, expected);
        assert!(narrative.requires_role("cast"));
        assert!(narrative.requires_role("voice"));
        assert!(narrative.requires_role("mouth_policy"));
        assert!(narrative.stages.iter().any(|stage| {
            stage.id == "render"
                && stage
                    .depends_on
                    .iter()
                    .any(|item| item == "production_notes")
        }));
    }

    #[test]
    fn montage_skips_characters_and_lipsync_without_a_dead_stage() {
        let graph = graph(ProductionRecipe::montage());
        assert!(!graph.includes("characters"));
        assert!(!graph.requires_role("cast"));
        assert!(!graph.requires_role("voice"));
        assert!(!graph.requires_role("mouth_policy"));
        assert!(graph.stages.iter().all(|stage| {
            stage.id != "characters"
                && !stage
                    .required_roles
                    .iter()
                    .any(|role| role == "mouth_policy")
                && !stage.required_roles.iter().any(|role| role == "cast")
        }));

        let mut state = ProjectState::new();
        state.profile = Some(ProductionProfile {
            recipe: ProductionRecipe::montage(),
            ..ProductionProfile::default()
        });
        state.characters.push(CharacterRecord {
            id: "tom_vale".into(),
            name: "Tom Vale".into(),
            dir: "Characters/Tom Vale".into(),
            identity_ready: true,
            voice_ready: false,
            profile_path: None,
            model_sheet_manifest_path: None,
            profile: CharacterProfile::default(),
            asset_ids: Vec::new(),
            notes: Vec::new(),
        });
        let scene = state.scene_mut(1, "Night passing");
        scene.source_excerpt = Some("Rails and rain. No dialogue.".into());
        let mut cut = CutRecord::new(CutId::new(1, 1));
        cut.sketch = Some(CutSketch {
            title: Some("Window".into()),
            shot_role: Some(ShotRole::Atmosphere),
            subject: None,
            speaker: None,
            mouth_policy: Some(MouthPolicy::SpeakingVisible),
            pace: None,
            visual_prompt: Some("Rain on the glass.".into()),
            negative_prompt: None,
            reference_roles: Vec::new(),
            transition_idea: None,
            dialogue: Vec::new(),
            on_camera: Vec::new(),
        });
        cut.speech_mode = Some(SpeechMode::SourceVideoLipsync);
        cut.takes.push(TakeRecord {
            id: "take_1".into(),
            path: "Renders/window.mp4".into(),
            status: TakeStatus::Selected,
            notes: None,
        });
        cut.selected_take = Some("take_1".into());
        scene.cuts = vec![cut];

        apply_stages(&mut state);
        clear_unrequired_mouths(graph, state.scene_mut(1, "Night passing"));

        let scene = state.scene(1).unwrap();
        assert_eq!(scene.stages.characters, StageStatus::Missing);
        assert_eq!(scene.stages.script, StageStatus::Present);
        assert_eq!(scene.stages.cut_sketches, StageStatus::Draft);
        assert_eq!(scene.stages.render, StageStatus::Present);
        assert_eq!(scene.stages.take_review, StageStatus::Present);
        assert!(scene.cuts[0]
            .sketch
            .as_ref()
            .unwrap()
            .mouth_policy
            .is_none());
        assert_eq!(scene.cuts[0].speech_mode, Some(SpeechMode::Silent));

        let narrative = ProjectState::new();
        let mut narrative = narrative;
        narrative.profile = Some(ProductionProfile::default());
        narrative.characters.push(CharacterRecord {
            id: "tom_vale".into(),
            name: "Tom Vale".into(),
            dir: "Characters/Tom Vale".into(),
            identity_ready: false,
            voice_ready: false,
            profile_path: None,
            model_sheet_manifest_path: None,
            profile: CharacterProfile::default(),
            asset_ids: Vec::new(),
            notes: Vec::new(),
        });
        let scene = narrative.scene_mut(1, "The last train");
        scene.source_excerpt = Some("Tom speaks.".into());
        apply_stages(&mut narrative);
        assert_eq!(
            narrative.scene(1).unwrap().stages.characters,
            StageStatus::Present
        );
    }

    #[test]
    fn montage_breakdown_skips_the_character_registry() {
        use crate::script::DialogueLine;
        use crate::workflow::state::RequirementKind;

        let mut state = ProjectState::new();
        state.profile = Some(ProductionProfile {
            recipe: ProductionRecipe::montage(),
            ..ProductionProfile::default()
        });
        state.characters.push(CharacterRecord {
            id: "tom_vale".into(),
            name: "Tom Vale".into(),
            dir: "Characters/Tom Vale".into(),
            identity_ready: true,
            voice_ready: true,
            profile_path: None,
            model_sheet_manifest_path: None,
            profile: CharacterProfile::default(),
            asset_ids: Vec::new(),
            notes: Vec::new(),
        });
        let scene = state.scene_mut(1, "Passing");
        scene.dialogue.push(DialogueLine {
            speaker: "Tom".into(),
            text: "Not used.".into(),
        });
        crate::workflow::breakdown::apply(&mut state, &[]);
        assert_eq!(
            state.profile.as_ref().unwrap().recipe,
            ProductionRecipe::montage()
        );
        assert!(state
            .requirements
            .iter()
            .all(|item| item.kind != RequirementKind::Cast && item.kind != RequirementKind::Voice));
    }

    #[test]
    fn assemble_readiness_follows_a_review_export() {
        let mut state = ProjectState::new();
        let mut cut = CutRecord::new(CutId::new(1, 1));
        cut.stable_id = "shot_1".into();
        cut.takes.push(TakeRecord {
            id: "take_1".into(),
            path: "a.mp4".into(),
            status: TakeStatus::Selected,
            notes: None,
        });
        cut.selected_take = Some("take_1".into());
        state.scene_mut(1, "Scene 1").cuts = vec![cut];
        apply_stages(&mut state);
        assert_eq!(
            state.scene(1).unwrap().stages.assemble,
            StageStatus::Missing
        );
        crate::workflow::post::attach_review_export(&mut state, 1, "Final/Scene1_master.mp4")
            .unwrap();
        assert_eq!(
            state.scene(1).unwrap().cuts[0].selected_take.as_deref(),
            Some("take_1")
        );
        assert_eq!(
            state.scene(1).unwrap().cuts[0].takes[0].status,
            TakeStatus::UsedInAssembly
        );
        apply_stages(&mut state);
        assert_eq!(
            state.scene(1).unwrap().stages.assemble,
            StageStatus::Present
        );
        let version = state.scene(1).unwrap().post.as_ref().unwrap().versions[0]
            .id
            .clone();
        crate::workflow::post::lock_picture(&mut state, 1, &version).unwrap();
        apply_stages(&mut state);
        assert_eq!(
            state.scene(1).unwrap().stages.assemble,
            StageStatus::Approved
        );
    }

    #[test]
    fn navigation_follows_recipe_order_and_work_areas() {
        let mut state = ProjectState::new();
        state.profile = Some(ProductionProfile::default());
        let scene = state.scene_mut(1, "Open");
        scene.source_excerpt = Some("A room.".into());
        apply_stages(&mut state);
        let scene = state.scene(1).unwrap();
        let nav = navigation(graph(ProductionRecipe::narrative()), scene);
        let ids: Vec<_> = nav.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(
            ids,
            WorkflowStage::ALL
                .iter()
                .map(|stage| stage.as_str())
                .collect::<Vec<_>>()
        );
        assert!(nav.iter().take(7).all(|item| item.work_area == "plan"));
        assert!(nav
            .iter()
            .any(|item| item.id == "render" && item.work_area == "produce"));
        assert!(nav
            .iter()
            .any(|item| item.id == "assemble" && item.work_area == "finish"));
        let intent = nav.iter().find(|item| item.id == "scene_intent").unwrap();
        assert!(!intent.available);
        assert_eq!(
            intent.disabled_reason.as_deref(),
            Some("Waiting on Characters.")
        );
        assert!(nav.iter().any(|item| item.id == "script" && item.available));
    }

    #[test]
    fn montage_navigation_omits_characters_and_lipsync() {
        let mut state = ProjectState::new();
        let scene = state.scene_mut(1, "Passing");
        scene.source_excerpt = Some("Rails.".into());
        let nav = navigation(graph(ProductionRecipe::montage()), scene);
        assert!(nav.iter().all(|item| item.id != "characters"));
        assert!(nav.iter().all(|item| {
            graph(ProductionRecipe::montage())
                .stages
                .iter()
                .find(|stage| stage.id == item.id)
                .is_some_and(|stage| {
                    !stage
                        .required_roles
                        .iter()
                        .any(|role| role == "mouth_policy")
                })
        }));
        assert_eq!(nav[0].id, "script");
        assert_eq!(nav[1].id, "scene_intent");
        assert_eq!(nav[1].work_area, "plan");
    }

    #[test]
    fn an_unlisted_stage_can_appear_from_recipe_data() {
        let graph = parse_recipe_record(&serde_json::json!({
            "version": 1,
            "id": "test_color",
            "label": "Color",
            "stages": [
                {"id": "script", "label": "Script", "work_area": "plan"},
                {"id": "color_script", "label": "Color script", "work_area": "plan", "depends_on": ["script"]}
            ]
        }))
        .unwrap();
        let mut state = ProjectState::new();
        let scene = state.scene_mut(1, "Dawn");
        scene.source_excerpt = Some("Light.".into());
        apply_stages(&mut state);
        let scene = state.scene(1).unwrap();
        let nav = navigation(&graph, scene);
        assert_eq!(nav.len(), 2);
        assert_eq!(nav[1].id, "color_script");
        assert_eq!(nav[1].label, "Color script");
        assert_eq!(nav[1].work_area, "plan");
        assert!(nav[1].available);
        assert!(WorkflowStage::parse("color_script").is_none());
    }

    #[test]
    fn unknown_recipe_and_version_one_files_load() {
        let unknown = ProjectState::from_json(
            r#"{"version":1,"profile":{"recipe":"opera","deliverables":[]}}"#,
        )
        .unwrap();
        assert_eq!(unknown.version, 2);
        assert_eq!(
            unknown.profile.as_ref().unwrap().recipe,
            ProductionRecipe::narrative()
        );
        assert!(unknown
            .notes
            .iter()
            .any(|note| note.contains("Unknown recipe `opera`")));

        let old = ProjectState::from_json(r#"{"version":1,"project_name":"Lite"}"#).unwrap();
        assert_eq!(old.version, 2);
        assert_eq!(old.project_name.as_deref(), Some("Lite"));

        let alias = ProjectState::from_json(
            r#"{"version":1,"profile":{"recipe":"narrative","deliverables":[]}}"#,
        )
        .unwrap();
        assert_eq!(
            alias.profile.as_ref().unwrap().recipe.as_str(),
            "narrative_dialogue"
        );
        assert_eq!(
            alias.profile.as_ref().unwrap().recipe.label(),
            "Narrative dialogue"
        );

        let future = ProjectState::from_json(r#"{"version":99}"#);
        assert!(future.is_err());
    }

    #[test]
    fn resolving_the_builtin_registry_runs_validation() {
        let raw = include_str!("recipes.json");
        assert!(raw.contains("\"narrative_dialogue\""));
        assert!(raw.contains("\"montage\""));
        assert!(!raw.contains("color_grade"));
        for choice in choices() {
            let recipe = ProductionRecipe::parse(&choice.id).unwrap();
            let loaded = graph(recipe);
            assert_eq!(loaded.id, choice.id);
            assert_eq!(loaded.label, choice.label);
        }
        assert_eq!(choices().len(), 2);
        assert!(ProductionRecipe::parse("opera").is_none());
        assert!(choices().iter().all(|item| item.id != "opera"));
    }

    #[test]
    fn a_recipe_record_resolves_from_data_without_a_recipe_enum() {
        let loaded = parse_recipe_record(&serde_json::json!({
            "version": 1,
            "id": "color_grade",
            "label": "Color grade",
            "stages": [
                {"id": "script", "label": "Track", "work_area": "plan"},
                {"id": "color_script", "label": "Color script", "work_area": "plan", "depends_on": ["script"]}
            ]
        }))
        .unwrap();
        let broken = parse_recipe_record(&serde_json::json!({
            "version": 1,
            "id": "broken",
            "label": "Broken",
            "stages": [
                {"id": "grade", "label": "Grade", "work_area": "finish", "depends_on": ["missing"]}
            ]
        }));
        assert!(broken.is_err());
        assert!(ProductionRecipe::parse("color_grade").is_none());
        assert_eq!(loaded.label, "Color grade");
        assert_eq!(loaded.stages[1].id, "color_script");
        assert!(choices().iter().all(|item| item.id != "color_grade"));
        assert!(parse_recipe_record(&serde_json::json!({
            "version": 9,
            "id": "color_grade",
            "label": "Color grade",
            "stages": [{"id": "script", "label": "Track", "work_area": "plan"}]
        }))
        .unwrap_err()
        .contains("schema version 9"));
    }

    #[test]
    fn unknown_stage_readiness_survives_a_workflow_round_trip() {
        let record = parse_recipe_record(&serde_json::json!({
            "version": 1,
            "id": "color_grade",
            "label": "Color grade",
            "stages": [
                {"id": "script", "label": "Script", "work_area": "plan"},
                {"id": "color_script", "label": "Color script", "work_area": "plan", "depends_on": ["script"]}
            ]
        }))
        .unwrap();
        let mut state = ProjectState::new();
        let scene = state.scene_mut(1, "Dawn");
        scene.source_excerpt = Some("Light.".into());
        scene
            .stages
            .set_extension("color_script", StageStatus::Present);
        apply_stages(&mut state);
        let loaded = ProjectState::from_json(&state.to_json().unwrap()).unwrap();
        let scene = loaded.scene(1).unwrap();
        assert_eq!(scene.stages.status_of("color_script"), StageStatus::Present);
        assert_eq!(scene.stages.status_of("script"), StageStatus::Present);
        let nav = navigation(&record, scene);
        let color = nav.iter().find(|item| item.id == "color_script").unwrap();
        assert_eq!(color.status, "present");
        assert!(color.available);
        assert!(color.disabled_reason.is_none());
    }

    #[test]
    fn a_new_recipe_loads_from_a_document_the_builtin_table_does_not_contain() {
        use crate::workflow::report::project_report;

        let json = r#"{
          "version": 1,
          "project_name": "Color reel",
          "profile": {
            "recipe": "color_grade",
            "recipe_document": {
              "id": "color_grade",
              "label": "Color grade",
              "stages": [
                {"id": "script", "label": "Track", "work_area": "plan", "depends_on": [], "required_roles": []},
                {"id": "color_script", "label": "Color script", "work_area": "produce", "depends_on": ["script"], "required_roles": []},
                {"id": "assemble", "label": "Edit", "work_area": "finish", "depends_on": ["color_script"], "required_roles": []}
              ]
            },
            "deliverables": []
          },
          "characters": [{"id": "ada", "name": "Ada", "dir": "Characters/Ada"}],
          "scenes": [{
            "id": 1,
            "title": "Dawn",
            "source_excerpt": "Light.",
            "stages": {"extensions": {"color_script": "present"}}
          }]
        }"#;
        assert!(ProductionRecipe::parse("color_grade").is_none());
        let mut state = ProjectState::from_json(json).unwrap();
        assert_eq!(
            state.profile.as_ref().unwrap().recipe.as_str(),
            "color_grade"
        );
        assert!(state
            .notes
            .iter()
            .all(|note| !note.contains("Unknown recipe")));
        apply_stages(&mut state);
        let scene = state.scene(1).unwrap();
        assert_eq!(scene.stages.characters, StageStatus::Missing);
        assert_eq!(scene.stages.script, StageStatus::Present);
        assert_eq!(scene.stages.status_of("color_script"), StageStatus::Present);
        let report = project_report(&state);
        assert_eq!(report.recipe.id, "color_grade");
        assert_eq!(report.recipe.label, "Color grade");
        let ids: Vec<_> = report
            .recipe
            .stages
            .iter()
            .map(|stage| stage.id.as_str())
            .collect();
        assert_eq!(ids, ["script", "color_script", "assemble"]);
        assert!(!ids.contains(&"characters"));
        assert!(report.recipe.stages.iter().all(|stage| {
            stage
                .required_roles
                .iter()
                .all(|role| role != "mouth_policy" && role != "cast")
        }));
        assert_eq!(
            report
                .recipe
                .stages
                .iter()
                .map(|stage| stage.work_area.as_str())
                .collect::<Vec<_>>(),
            ["plan", "produce", "finish"]
        );
        let nav_ids: Vec<_> = report.scenes[0]
            .navigation
            .iter()
            .map(|item| item.id.as_str())
            .collect();
        assert_eq!(nav_ids, ids);
        let color = report.scenes[0]
            .navigation
            .iter()
            .find(|item| item.id == "color_script")
            .unwrap();
        assert_eq!(color.label, "Color script");
        assert_eq!(color.work_area, "produce");
        assert!(color.available);
        assert!(color.disabled_reason.is_none());
        let again = ProjectState::from_json(&state.to_json().unwrap()).unwrap();
        assert_eq!(
            again.profile.as_ref().unwrap().recipe.as_str(),
            "color_grade"
        );

        let blocked = ProjectState::from_json(
            r#"{
              "version": 2,
              "profile": {
                "recipe": "color_grade",
                "recipe_document": {
                  "id": "color_grade",
                  "label": "Color grade",
                  "stages": [
                    {"id": "script", "label": "Track", "work_area": "plan"},
                    {"id": "color_script", "label": "Color script", "work_area": "produce", "depends_on": ["script"]}
                  ]
                },
                "deliverables": []
              },
              "scenes": [{"id": 1, "title": "Dawn", "stages": {}}]
            }"#,
        )
        .unwrap();
        let blocked_report = project_report(&blocked);
        let waiting = blocked_report.scenes[0]
            .navigation
            .iter()
            .find(|item| item.id == "color_script")
            .unwrap();
        assert!(!waiting.available);
        assert_eq!(
            waiting.disabled_reason.as_deref(),
            Some("Waiting on Track.")
        );

        let future_schema = ProjectState::from_json(
            r#"{"version":1,"profile":{"recipe":"opera","recipe_document":{"version":9,"id":"opera","label":"Opera","stages":[{"id":"grade","label":"Grade","work_area":"finish"}]},"deliverables":[]}}"#,
        )
        .unwrap();
        assert_eq!(
            future_schema.profile.as_ref().unwrap().recipe,
            ProductionRecipe::narrative()
        );
        assert!(future_schema
            .notes
            .iter()
            .any(|note| note.contains("schema version 9")));

        let broken = ProjectState::from_json(
            r#"{"version":1,"profile":{"recipe":"opera","recipe_document":{"id":"opera","label":"Opera","stages":[{"id":"grade","label":"Grade","work_area":"finish","depends_on":["missing"]}]},"deliverables":[]}}"#,
        )
        .unwrap();
        assert_eq!(
            broken.profile.as_ref().unwrap().recipe,
            ProductionRecipe::narrative()
        );
        assert!(broken
            .notes
            .iter()
            .any(|note| note.contains("Unknown recipe `opera`")));
        assert!(broken.profile.as_ref().unwrap().recipe_document.is_none());
    }
}
