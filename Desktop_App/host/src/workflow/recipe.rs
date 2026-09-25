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
}

pub struct RecipeStage {
    pub id: &'static str,
    pub label: &'static str,
    pub work_area: WorkArea,
    pub depends_on: &'static [&'static str],
    pub required_roles: &'static [&'static str],
}

const fn stage(
    id: &'static str,
    label: &'static str,
    work_area: WorkArea,
    depends_on: &'static [&'static str],
    required_roles: &'static [&'static str],
) -> RecipeStage {
    RecipeStage {
        id,
        label,
        work_area,
        depends_on,
        required_roles,
    }
}

pub struct RecipeGraph {
    pub id: &'static str,
    pub label: &'static str,
    pub stages: &'static [RecipeStage],
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

const NARRATIVE_STAGES: &[RecipeStage] = &[
    stage("script", "Script", WorkArea::Plan, &[], &[]),
    stage("characters", "Characters", WorkArea::Plan, &["script"], &["cast", "voice"]),
    stage("scene_intent", "Scene Intent", WorkArea::Plan, &["characters"], &[]),
    stage("scene_look", "Scene Look", WorkArea::Plan, &["scene_intent"], &[]),
    stage("cut_sketches", "Cut Sketches", WorkArea::Plan, &["scene_look"], &["mouth_policy"]),
    stage("reference_images", "Reference Images", WorkArea::Plan, &["cut_sketches"], &[]),
    stage("production_notes", "Production Notes", WorkArea::Plan, &["reference_images"], &[]),
    stage("render", "Render", WorkArea::Produce, &["production_notes"], &[]),
    stage("take_review", "Take Review", WorkArea::Produce, &["render"], &[]),
    stage("assemble", "Assemble", WorkArea::Finish, &["take_review"], &[]),
];

const MONTAGE_STAGES: &[RecipeStage] = &[
    stage("script", "Track", WorkArea::Plan, &[], &[]),
    stage("scene_intent", "Timing", WorkArea::Plan, &["script"], &[]),
    stage("scene_look", "Visual treatment", WorkArea::Plan, &["scene_intent"], &[]),
    stage("cut_sketches", "Boards", WorkArea::Plan, &["scene_look"], &[]),
    stage("reference_images", "Shot sources", WorkArea::Plan, &["cut_sketches"], &[]),
    stage("production_notes", "Shot specs", WorkArea::Plan, &["reference_images"], &[]),
    stage("render", "Takes", WorkArea::Produce, &["production_notes"], &[]),
    stage("take_review", "Selects", WorkArea::Produce, &["render"], &[]),
    stage("assemble", "Edit", WorkArea::Finish, &["take_review"], &[]),
];

const NARRATIVE: RecipeGraph = RecipeGraph {
    id: "narrative_dialogue",
    label: "Narrative dialogue",
    stages: NARRATIVE_STAGES,
};

const MONTAGE: RecipeGraph = RecipeGraph {
    id: "montage",
    label: "Montage",
    stages: MONTAGE_STAGES,
};

const GRAPHS: &[&RecipeGraph] = &[&NARRATIVE, &MONTAGE];

impl RecipeGraph {
    pub fn includes(&self, stage: &str) -> bool {
        self.stages.iter().any(|item| item.id == stage)
    }

    pub fn requires_role(&self, role: &str) -> bool {
        self.stages
            .iter()
            .any(|item| item.required_roles.contains(&role))
    }

    pub fn report(&self) -> RecipeReport {
        RecipeReport {
            id: self.id.to_string(),
            label: self.label.to_string(),
            stages: self
                .stages
                .iter()
                .map(|stage| RecipeStageReport {
                    id: stage.id.to_string(),
                    label: stage.label.to_string(),
                    work_area: stage.work_area.as_str().to_string(),
                    depends_on: stage.depends_on.iter().map(|item| (*item).to_string()).collect(),
                    required_roles: stage
                        .required_roles
                        .iter()
                        .map(|role| (*role).to_string())
                        .collect(),
                })
                .collect(),
        }
    }
}

pub fn graphs() -> &'static [&'static RecipeGraph] {
    GRAPHS
}

pub fn choices() -> Vec<RecipeChoice> {
    GRAPHS
        .iter()
        .map(|graph| RecipeChoice {
            id: graph.id.to_string(),
            label: graph.label.to_string(),
        })
        .collect()
}

/// Look up the graph for a recipe id. Unknown ids use narrative dialogue.
pub fn graph(recipe: ProductionRecipe) -> &'static RecipeGraph {
    GRAPHS
        .iter()
        .copied()
        .find(|graph| graph.id == recipe.as_str())
        .unwrap_or(&NARRATIVE)
}

pub fn validate_graph(graph: &RecipeGraph) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for stage in graph.stages {
        if stage.id.is_empty() || !seen.insert(stage.id) {
            return Err(format!("recipe {} has a duplicate or empty stage id", graph.id));
        }
        for dependency in stage.depends_on {
            if !graph.stages.iter().any(|item| item.id == *dependency) {
                return Err(format!(
                    "recipe {} stage {} depends on unknown stage {dependency}",
                    graph.id, stage.id
                ));
            }
        }
    }
    Ok(())
}

/// Stages the recipe actually offers, in recipe order. Omitted stages are absent.
/// A stage with missing dependencies stays visible and disabled instead of becoming a dead control.
pub fn navigation(graph: &RecipeGraph, scene: &SceneRecord) -> Vec<StageNav> {
    graph
        .stages
        .iter()
        .map(|stage| {
            let status = stage_status(scene, stage.id);
            let blocking: Vec<&str> = stage
                .depends_on
                .iter()
                .copied()
                .filter(|dependency| stage_status(scene, dependency) == StageStatus::Missing)
                .collect();
            let available = blocking.is_empty() || status != StageStatus::Missing;
            let disabled_reason = if available {
                None
            } else {
                let labels: Vec<&str> = blocking
                    .iter()
                    .map(|dependency| {
                        graph
                            .stages
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
                work_area: stage.work_area.as_str().to_string(),
                status: status.as_str().to_string(),
                available,
                disabled_reason,
                depends_on: stage.depends_on.iter().map(|item| (*item).to_string()).collect(),
            }
        })
        .collect()
}

fn stage_status(scene: &SceneRecord, id: &str) -> StageStatus {
    WorkflowStage::parse(id)
        .map(|stage| scene.stages.get(stage))
        .unwrap_or(StageStatus::Missing)
}

pub fn apply_stages(state: &mut ProjectState) {
    let recipe = state
        .profile
        .as_ref()
        .map(|profile| profile.recipe)
        .unwrap_or_default();
    let active = graph(recipe);
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
pub fn clear_unrequired_mouths(graph: &RecipeGraph, scene: &mut SceneRecord) {
    if graph.requires_role("mouth_policy") {
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
        let narrative = graph(ProductionRecipe::NarrativeDialogue);
        validate_graph(narrative).unwrap();
        validate_graph(graph(ProductionRecipe::Montage)).unwrap();
        let ids: Vec<_> = narrative.stages.iter().map(|stage| stage.id).collect();
        let expected: Vec<_> = WorkflowStage::ALL.iter().map(|stage| stage.as_str()).collect();
        assert_eq!(ids, expected);
        assert!(narrative.requires_role("cast"));
        assert!(narrative.requires_role("voice"));
        assert!(narrative.requires_role("mouth_policy"));
        assert!(narrative
            .stages
            .iter()
            .any(|stage| stage.id == "render" && stage.depends_on.contains(&"production_notes")));
    }

    #[test]
    fn montage_skips_characters_and_lipsync_without_a_dead_stage() {
        let graph = graph(ProductionRecipe::Montage);
        assert!(!graph.includes("characters"));
        assert!(!graph.requires_role("cast"));
        assert!(!graph.requires_role("voice"));
        assert!(!graph.requires_role("mouth_policy"));
        assert!(graph
            .stages
            .iter()
            .all(|stage| {
                stage.id != "characters"
                    && !stage.required_roles.contains(&"mouth_policy")
                    && !stage.required_roles.contains(&"cast")
            }));

        let mut state = ProjectState::new();
        state.profile = Some(ProductionProfile {
            recipe: ProductionRecipe::Montage,
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
            recipe: ProductionRecipe::Montage,
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
            ProductionRecipe::Montage
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
        let nav = navigation(graph(ProductionRecipe::NarrativeDialogue), scene);
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
        assert_eq!(intent.disabled_reason.as_deref(), Some("Waiting on Characters."));
        assert!(nav.iter().any(|item| item.id == "script" && item.available));
    }

    #[test]
    fn montage_navigation_omits_characters_and_lipsync() {
        let mut state = ProjectState::new();
        let scene = state.scene_mut(1, "Passing");
        scene.source_excerpt = Some("Rails.".into());
        let nav = navigation(graph(ProductionRecipe::Montage), scene);
        assert!(nav.iter().all(|item| item.id != "characters"));
        assert!(nav.iter().all(|item| {
            graph(ProductionRecipe::Montage)
                .stages
                .iter()
                .find(|stage| stage.id == item.id)
                .is_some_and(|stage| !stage.required_roles.contains(&"mouth_policy"))
        }));
        assert_eq!(nav[0].id, "script");
        assert_eq!(nav[1].id, "scene_intent");
        assert_eq!(nav[1].work_area, "plan");
    }

    #[test]
    fn an_unlisted_stage_can_appear_from_recipe_data() {
        const EXTRA: &[RecipeStage] = &[
            stage("script", "Script", WorkArea::Plan, &[], &[]),
            stage(
                "color_script",
                "Color script",
                WorkArea::Plan,
                &["script"],
                &[],
            ),
        ];
        let graph = RecipeGraph {
            id: "test_color",
            label: "Color",
            stages: EXTRA,
        };
        validate_graph(&graph).unwrap();
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
            ProductionRecipe::NarrativeDialogue
        );
        assert!(unknown
            .notes
            .iter()
            .any(|note| note.contains("Unknown recipe `opera`")));

        let old = ProjectState::from_json(r#"{"version":1,"project_name":"Lite"}"#).unwrap();
        assert_eq!(old.version, 2);
        assert_eq!(old.project_name.as_deref(), Some("Lite"));

        let future = ProjectState::from_json(r#"{"version":99}"#);
        assert!(future.is_err());
    }
}
