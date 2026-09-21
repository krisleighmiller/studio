//! Host-owned routing: scene intent and cut sketches are not production notes.
use crate::script::requested_scene_number;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanningKind {
    SceneIntent,
    SceneLook,
    CutSketches,
    ProductionNotes,
}

pub fn classify_planning(text: &str, workflow_stage: Option<&str>) -> PlanningKind {
    let t = text.to_ascii_lowercase();
    // Explicit V2 stages win even if the model stuffed prod.md into the brief.
    if asks_for_scene_intent(&t) {
        return PlanningKind::SceneIntent;
    }
    if asks_for_scene_look(&t) {
        return PlanningKind::SceneLook;
    }
    if asks_for_cut_sketches(&t) {
        return PlanningKind::CutSketches;
    }
    if asks_for_production_notes(&t) {
        return PlanningKind::ProductionNotes;
    }
    match workflow_stage.map(|s| s.trim()) {
        Some("scene_intent") => PlanningKind::SceneIntent,
        Some("scene_look") => PlanningKind::SceneLook,
        Some("cut_sketches") => PlanningKind::CutSketches,
        Some("production_notes") => PlanningKind::ProductionNotes,
        _ => PlanningKind::ProductionNotes,
    }
}

pub fn resolve_scene_id(text: &str, input_scene: Option<u32>, workflow_scene: Option<u32>) -> u32 {
    input_scene
        .or_else(|| requested_scene_number(text))
        .or(workflow_scene)
        .unwrap_or(1)
}

fn asks_for_production_notes(t: &str) -> bool {
    const KEYS: &[&str] = &[
        "production notes",
        "production note",
        "prod.md",
        "scene_prompts",
        "scene prompts",
        "direct notes",
        "direct production",
        "skip to notes",
        "build prod.md",
        "redo cut",
        "re-plan the scene",
        "replan the scene",
    ];
    KEYS.iter().any(|k| t.contains(k))
        || (t.contains("scene plan") && !t.contains("scene intent") && !t.contains("cut sketch"))
}

fn asks_for_scene_intent(t: &str) -> bool {
    const KEYS: &[&str] = &[
        "scene intent",
        "scene-intent",
        "intent card",
        "intent for",
        "intent for the scene",
        "narrative purpose",
        "emotional purpose",
        "what the scene must accomplish",
        "draft the intent",
        "draft scene intent",
        "draft the scene intent",
    ];
    KEYS.iter().any(|k| t.contains(k))
        || (t.contains("draft") && t.contains("intent") && t.contains("scene"))
}

fn asks_for_scene_look(t: &str) -> bool {
    const KEYS: &[&str] = &[
        "scene look",
        "scene-look",
        "look board",
        "reference board",
        "wardrobe reference",
        "mood reference",
        "interior reference",
        "exterior reference",
        "prop reference",
        "draft the look",
        "draft scene look",
        "draft the scene look",
    ];
    KEYS.iter().any(|k| t.contains(k))
}

fn asks_for_cut_sketches(t: &str) -> bool {
    const KEYS: &[&str] = &[
        "cut sketch",
        "cut sketches",
        "shot role",
        "mouth policy",
        "scene strategy",
        "cinematic balanced",
        "cinematic-balanced",
        "performance-heavy",
        "atmosphere-heavy",
    ];
    KEYS.iter().any(|k| t.contains(k))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_intent_is_not_production_notes() {
        let prompt = "Please draft the scene intent for ### Scene 1 — The last train";
        assert_eq!(classify_planning(prompt, None), PlanningKind::SceneIntent);
        assert_eq!(resolve_scene_id(prompt, None, None), 1);
        assert_eq!(
            classify_planning(prompt, Some("production_notes")),
            PlanningKind::SceneIntent
        );
        assert_eq!(
            classify_planning(
                "Please draft the scene intent for Scene 1. prod.md already has 9 cuts.",
                Some("production_notes"),
            ),
            PlanningKind::SceneIntent
        );
    }

    #[test]
    fn existing_notes_shortcut_stays_scene_plan() {
        assert_eq!(
            classify_planning("Build prod.md and Scene_Prompts for scene 1", None),
            PlanningKind::ProductionNotes
        );
        assert_eq!(
            classify_planning("redo cut 2, make Pip colder", None),
            PlanningKind::ProductionNotes
        );
    }

    #[test]
    fn cinematic_balanced_routes_to_sketches() {
        assert_eq!(
            classify_planning("Scene Strategy cinematic balanced", None),
            PlanningKind::CutSketches
        );
    }

    #[test]
    fn selected_stage_fills_in_when_the_prompt_is_generic() {
        assert_eq!(
            classify_planning("please draft this", Some("scene_intent")),
            PlanningKind::SceneIntent
        );
        assert_eq!(
            classify_planning("please draft this", Some("cut_sketches")),
            PlanningKind::CutSketches
        );
        assert_eq!(
            classify_planning("please draft this", Some("scene_look")),
            PlanningKind::SceneLook
        );
    }

    #[test]
    fn scene_look_is_not_production_notes() {
        assert_eq!(
            classify_planning("Please draft the scene look for Scene 1", None),
            PlanningKind::SceneLook
        );
    }
}
