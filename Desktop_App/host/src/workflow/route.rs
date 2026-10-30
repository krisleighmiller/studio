//! Host-owned routing: scene intent and cut sketches are not production notes.
use crate::script::requested_scene_number;

fn contains_word_phrase(text: &str, phrase: &str) -> bool {
    if phrase.is_empty() {
        return false;
    }
    // Use str::find to locate phrase occurrences and check word boundaries
    let mut pos: usize = 0;
    while let Some(start) = text[pos..].find(phrase) {
        let start = start + pos;
        let end = start + phrase.len();
        // Check character before match (if any)
        let before_ok = if start == 0 {
            true
        } else {
            text[..start].chars().next_back().map(|c| !c.is_alphanumeric()).unwrap_or(true)
        };
        // Check character after match (if any)
        let after_ok = if end == text.len() {
            true
        } else {
            text[end..].chars().next().map(|c| !c.is_alphanumeric()).unwrap_or(true)
        };
        if before_ok && after_ok {
            return true;
        }
        pos = end;
    }
    false
}

fn has_token_boundary(text: &str, token: &str) -> bool {
    // Split on non-alphanumeric boundaries (Unicode-aware)
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .any(|t| t == token)
}

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
    KEYS.iter().any(|k| contains_word_phrase(t, k))
        || (has_token_boundary(t, "draft") && has_token_boundary(t, "intent") && has_token_boundary(t, "scene"))
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

#[cfg(test)]
mod a04_regression_tests {
    use super::*;

    #[test]
    fn intent_substring_does_not_override_production_notes() {
        // A04 regression: "intentional" must not trigger SceneIntent
        assert_eq!(
            classify_planning(
                "Draft production notes for Scene 1. Car to cemetery is an intentional location change.",
                None,
            ),
            PlanningKind::ProductionNotes
        );
        // Unintentional non-trigger
        assert_eq!(
            classify_planning(
                "This is an unintentional mistake.",
                None,
            ),
            PlanningKind::ProductionNotes
        );
    }

    #[test]
    fn explicit_intent_request_kept_alongside_prod_md() {
        assert_eq!(
            classify_planning(
                "Draft the scene intent for Scene 1. prod.md already has 9 cuts.",
                Some("production_notes"),
            ),
            PlanningKind::SceneIntent
        );
    }

    #[test]
    fn punctuation_and_case_covered() {
        assert_eq!(
            classify_planning("SCENE INTENT for scene 2!", None),
            PlanningKind::SceneIntent
        );
    }

    #[test]
    fn draft_intent_statement_for_scene_scene_intent_priority() {
        // A04: "Draft an intent statement for this scene." should route to SceneIntent,
        // not ProductionNotes (regressed due to split_whitespace token mismatch with trailing punctuation).
        assert_eq!(
            classify_planning("Draft an intent statement for this scene.", None),
            PlanningKind::SceneIntent
        );
    }

    #[test]
    fn draft_intent_colon_explain_scene() {
        // A04: "Draft an intent: explain this scene" should route to SceneIntent.
        assert_eq!(
            classify_planning("Draft an intent: explain this scene", None),
            PlanningKind::SceneIntent
        );
    }

    #[test]
    fn draft_intent_statement_with_prod_md_already_exists() {
        // A04: "Draft an intent statement for this scene; prod.md already exists."
        // should route to SceneIntent even when prod.md context is present.
        assert_eq!(
            classify_planning(
                "Draft an intent statement for this scene; prod.md already exists.",
                None
            ),
            PlanningKind::SceneIntent
        );
    }

    #[test]
    fn scene_intentionally_must_not_match_scene_intent() {
        // A04: "scene intentionally" must NOT trigger SceneIntent via substring match.
        // The literal 'scene intent' phrase must respect word-boundary endpoints.
        assert_eq!(
            classify_planning("Draft production notes: scene intentionally moves location", None),
            PlanningKind::ProductionNotes
        );
    }
}
