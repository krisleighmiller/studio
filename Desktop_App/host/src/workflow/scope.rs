//! Resolve Director scope ids against the current project. The desk shows the
//! same lines this module returns.
use crate::workflow::state::ProjectState;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirectorScope {
    pub work_area: Option<String>,
    pub scene_stable_id: Option<String>,
    pub shot_stable_id: Option<String>,
    pub asset_id: Option<String>,
    pub look_slot_id: Option<String>,
    pub requirement_id: Option<String>,
}

impl DirectorScope {
    pub fn is_empty(&self) -> bool {
        self.work_area.is_none()
            && self.scene_stable_id.is_none()
            && self.shot_stable_id.is_none()
            && self.asset_id.is_none()
            && self.look_slot_id.is_none()
            && self.requirement_id.is_none()
    }
}

pub fn describe(state: Option<&ProjectState>, scope: &DirectorScope) -> String {
    let mut lines = vec!["Workflow scope:".to_string()];
    if let Some(area) = scope.work_area.as_deref().filter(|value| !value.is_empty()) {
        lines.push(format!("- work area: {area}"));
    }
    if let Some(id) = nonempty(&scope.scene_stable_id) {
        let label = state
            .and_then(|state| state.scenes.iter().find(|scene| scene.stable_id == id))
            .map(scene_label)
            .unwrap_or_else(|| "unresolved".into());
        lines.push(format!("- scene: {id} — {label}"));
    }
    if let Some(id) = nonempty(&scope.shot_stable_id) {
        let label = state
            .and_then(|state| {
                state.scenes.iter().find_map(|scene| {
                    scene
                        .cuts
                        .iter()
                        .find(|cut| cut.stable_id == id)
                        .map(shot_label)
                })
            })
            .unwrap_or_else(|| "unresolved".into());
        lines.push(format!("- shot: {id} — {label}"));
    }
    if let Some(id) = nonempty(&scope.asset_id) {
        let label = state
            .and_then(|state| state.asset(id))
            .map(|asset| asset.role.label().to_string())
            .unwrap_or_else(|| "unresolved".into());
        lines.push(format!("- asset: {id} — {label}"));
    }
    if let Some(id) = nonempty(&scope.look_slot_id) {
        let label = state
            .and_then(|state| {
                state.scenes.iter().find_map(|scene| {
                    scene
                        .look_board
                        .as_ref()
                        .and_then(|board| board.slots.iter().find(|slot| slot.id == id))
                        .map(|slot| {
                            let title = slot.title.trim();
                            if title.is_empty() {
                                slot.role.display_label().to_string()
                            } else {
                                title.to_string()
                            }
                        })
                })
            })
            .unwrap_or_else(|| "unresolved".into());
        lines.push(format!("- look slot: {id} — {label}"));
    }
    if let Some(id) = nonempty(&scope.requirement_id) {
        let label = state
            .and_then(|state| state.requirements.iter().find(|item| item.id == id))
            .map(|item| format!("{} ({})", item.name, item.status.label()))
            .unwrap_or_else(|| "unresolved".into());
        lines.push(format!("- requirement: {id} — {label}"));
    }
    lines.join("\n")
}

fn nonempty(value: &Option<String>) -> Option<&str> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn scene_label(scene: &crate::workflow::state::SceneRecord) -> String {
    let title = scene.title.trim();
    if title.is_empty() {
        format!("Scene {}", scene.id)
    } else {
        format!("Scene {} {title}", scene.id)
    }
}

fn shot_label(cut: &crate::workflow::state::CutRecord) -> String {
    let title = cut.title.as_deref().unwrap_or("").trim();
    if title.is_empty() {
        format!("Cut {}", cut.id.cut)
    } else {
        format!("Cut {} {title}", cut.id.cut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_plan::handoff::CutId;
    use crate::workflow::identity::reorder_cuts;
    use crate::workflow::state::{CutRecord, ProjectState};

    #[test]
    fn reordering_cuts_keeps_the_shot_attached_to_director() {
        let mut state = ProjectState::new();
        let scene = state.scene_mut(1, "The last train");
        scene.stable_id = "scene_1".into();
        let mut first = CutRecord::new(CutId::new(1, 1));
        first.stable_id = "shot_a".into();
        first.title = Some("Arrival".into());
        let mut second = CutRecord::new(CutId::new(1, 2));
        second.stable_id = "shot_b".into();
        second.title = Some("Departure".into());
        scene.cuts = vec![first, second];
        let scope = DirectorScope {
            work_area: Some("produce".into()),
            scene_stable_id: Some("scene_1".into()),
            shot_stable_id: Some("shot_b".into()),
            ..DirectorScope::default()
        };
        let before = describe(Some(&state), &scope);
        assert!(before.contains("- work area: produce"));
        assert!(before.contains("- scene: scene_1 — Scene 1 The last train"));
        assert!(before.contains("- shot: shot_b — Cut 2 Departure"));
        reorder_cuts(&mut state, 1, &["shot_b".into(), "shot_a".into()]).unwrap();
        let after = describe(Some(&state), &scope);
        assert!(after.contains("- shot: shot_b — Cut 1 Departure"));
        assert!(!after.contains("shot_a — Cut 1"));
        assert!(after.contains("scene_1"));
    }

    #[test]
    fn unknown_ids_stay_unresolved() {
        let state = ProjectState::new();
        let text = describe(
            Some(&state),
            &DirectorScope {
                shot_stable_id: Some("shot_missing".into()),
                ..DirectorScope::default()
            },
        );
        assert_eq!(text, "Workflow scope:\n- shot: shot_missing — unresolved");
    }
}
