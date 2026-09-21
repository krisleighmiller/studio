//! Host-owned strategy checks for cut sketches.
//!
//! The LLM may propose shot roles. This module decides whether the mix
//! matches the requested strategy. Ratios start simple and can tighten later.
use crate::scene_plan::handoff::SpeechMode;
use crate::scene_plan::strategy::SceneStrategy;
use crate::workflow::state::{CutRecord, CutSketch, MouthPolicy, SceneRecord, ShotRole};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VerdictStatus {
    #[default]
    Ok,
    Warn,
    Reject,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SketchMix {
    /// Dialogue cuts that will play as on-camera performance / lip-sync.
    pub performance: usize,
    /// Dialogue cuts carried as VO / off-camera / closed-mouth coverage.
    pub other_dialogue: usize,
    pub silent: usize,
}

impl SketchMix {
    pub fn of(scene: &SceneRecord) -> Self {
        let mut mix = SketchMix::default();
        for cut in &scene.cuts {
            if !cut.sketch.as_ref().is_some_and(|s| !s.is_empty()) {
                continue;
            }
            if is_performance_dialogue(cut) {
                mix.performance += 1;
            } else if has_dialogue(cut) {
                mix.other_dialogue += 1;
            } else {
                mix.silent += 1;
            }
        }
        mix
    }

    pub fn dialogue_cuts(self) -> usize {
        self.performance + self.other_dialogue
    }

    pub fn total(self) -> usize {
        self.performance + self.other_dialogue + self.silent
    }

    pub fn display(self) -> String {
        format!(
            "{} performance / {} other dialogue / {} silent",
            self.performance, self.other_dialogue, self.silent
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MixVerdict {
    pub status: VerdictStatus,
    pub strategy: SceneStrategy,
    pub mix: SketchMix,
    pub messages: Vec<String>,
}

impl MixVerdict {
    pub fn ok(strategy: SceneStrategy, mix: SketchMix) -> Self {
        MixVerdict {
            status: VerdictStatus::Ok,
            strategy,
            mix,
            messages: Vec::new(),
        }
    }

    pub fn is_reject(&self) -> bool {
        self.status == VerdictStatus::Reject
    }
}

pub fn scene_strategy(scene: &SceneRecord) -> SceneStrategy {
    scene.strategy.unwrap_or(SceneStrategy::CinematicBalanced)
}

/// Derive host-owned speech mode from the sketch. The LLM does not set this.
pub fn speech_from_sketch(sketch: &CutSketch, has_dialogue: bool) -> SpeechMode {
    match sketch.mouth_policy {
        Some(MouthPolicy::SpeakingVisible) => SpeechMode::SourceVideoLipsync,
        Some(MouthPolicy::OffCamera) => {
            if has_dialogue {
                SpeechMode::VoiceOver
            } else {
                SpeechMode::Silent
            }
        }
        Some(MouthPolicy::ClosedOrNotVisible) => {
            if has_dialogue {
                SpeechMode::VoiceOver
            } else {
                SpeechMode::Silent
            }
        }
        Some(MouthPolicy::NoPeople) => SpeechMode::Silent,
        None => match sketch.shot_role {
            Some(ShotRole::Performance) if has_dialogue => SpeechMode::SourceVideoLipsync,
            Some(_) if has_dialogue => SpeechMode::VoiceOver,
            _ => SpeechMode::Silent,
        },
    }
}

pub fn apply_derived_speech(cut: &mut CutRecord) {
    let has_dialogue = has_dialogue(cut);
    if let Some(sketch) = cut.sketch.as_ref().filter(|s| !s.is_empty()) {
        cut.speech_mode = Some(speech_from_sketch(sketch, has_dialogue));
        if cut.subject.is_none() {
            cut.subject = sketch.subject.clone();
        }
        if cut.title.is_none() {
            cut.title = sketch.title.clone();
        }
    }
}

pub fn default_mouth(role: ShotRole, has_dialogue: bool) -> MouthPolicy {
    MouthPolicy::default_for(role, has_dialogue)
}

pub fn validate_scene_mix(scene: &SceneRecord) -> MixVerdict {
    let strategy = scene_strategy(scene);
    let mix = SketchMix::of(scene);
    if mix.total() == 0 {
        return MixVerdict {
            status: VerdictStatus::Ok,
            strategy,
            mix,
            messages: Vec::new(),
        };
    }
    let mut messages = Vec::new();
    let mut status = VerdictStatus::Ok;
    let dialogue_lines = scene
        .dialogue
        .len()
        .max(scene.cuts.iter().map(|c| c.dialogue.len()).sum::<usize>());

    match strategy {
        SceneStrategy::CinematicBalanced => {
            if dialogue_lines >= 4 && mix.total() >= 3 {
                if mix.dialogue_cuts() > 0 && mix.other_dialogue == 0 {
                    status = VerdictStatus::Reject;
                    messages.push(
                        "cinematic-balanced requires at least one non-performance dialogue cut"
                            .into(),
                    );
                }
                if mix.dialogue_cuts() > 0 && mix.performance == mix.dialogue_cuts() {
                    status = VerdictStatus::Reject;
                    messages.push(
                        "every dialogue cut is performance/speaking-visible; balanced mix rejected"
                            .into(),
                    );
                }
                if mix.dialogue_cuts() > 0 {
                    let pct = mix.performance * 100 / mix.dialogue_cuts();
                    if pct > 60 {
                        if status != VerdictStatus::Reject {
                            status = VerdictStatus::Warn;
                        }
                        messages.push(format!(
                            "performance/lip-sync cuts are {pct}% of dialogue cuts (warn above 60%)"
                        ));
                    }
                }
            }
        }
        SceneStrategy::AtmosphereHeavy => {
            if mix.performance > 2 {
                status = VerdictStatus::Warn;
                messages.push(
                    "atmosphere-heavy should reserve lip-sync for one or two pivotal lines".into(),
                );
            }
            if mix.dialogue_cuts() > 0 {
                let pct = mix.performance * 100 / mix.dialogue_cuts();
                if pct > 40 {
                    status = VerdictStatus::Warn;
                    messages.push(format!(
                        "atmosphere-heavy mix is {pct}% performance (prefer most lines off-camera)"
                    ));
                }
            }
        }
        SceneStrategy::PerformanceHeavy => {
            if mix.dialogue_cuts() > 0 && mix.performance == 0 {
                status = VerdictStatus::Warn;
                messages.push(
                    "performance-heavy requested but no speaking-visible dialogue cuts".into(),
                );
            }
        }
    }

    if scene.strategy_override && status == VerdictStatus::Reject {
        status = VerdictStatus::Warn;
        messages.push("mix override is on; rejection downgraded to a warning".into());
    }

    MixVerdict {
        status,
        strategy,
        mix,
        messages,
    }
}

pub fn has_dialogue(cut: &CutRecord) -> bool {
    if !cut.dialogue.is_empty() {
        return true;
    }
    cut.sketch.as_ref().is_some_and(|s| !s.dialogue.is_empty())
}

pub fn is_performance_dialogue(cut: &CutRecord) -> bool {
    if !has_dialogue(cut) {
        return false;
    }
    let Some(sketch) = cut.sketch.as_ref() else {
        return matches!(cut.speech_mode, Some(SpeechMode::SourceVideoLipsync));
    };
    sketch.shot_role == Some(ShotRole::Performance)
        || sketch.mouth_policy == Some(MouthPolicy::SpeakingVisible)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_plan::handoff::CutId;
    use crate::script::DialogueLine;
    use crate::workflow::state::CutSketch;

    fn dialogue_cut(n: u32, role: ShotRole, mouth: MouthPolicy) -> CutRecord {
        let mut cut = CutRecord::new(CutId::new(1, n));
        cut.dialogue = vec![DialogueLine {
            speaker: "Tom".into(),
            text: format!("Line {n}"),
        }];
        cut.sketch = Some(CutSketch {
            title: Some(format!("Cut {n}")),
            shot_role: Some(role),
            mouth_policy: Some(mouth),
            dialogue: vec![crate::workflow::state::SketchDialogue {
                speaker: "Tom".into(),
                text: format!("Line {n}"),
                heard_as: None,
            }],
            ..CutSketch::default()
        });
        apply_derived_speech(&mut cut);
        cut
    }

    fn scene_with(cuts: Vec<CutRecord>) -> SceneRecord {
        let mut scene = SceneRecord::new(1, "Scene 1");
        scene.dialogue = cuts.iter().flat_map(|c| c.dialogue.clone()).collect();
        scene.cuts = cuts;
        scene.strategy = Some(SceneStrategy::CinematicBalanced);
        scene
    }

    #[test]
    fn balanced_rejects_all_performance_dialogue() {
        let scene = scene_with(
            (1..=6)
                .map(|n| dialogue_cut(n, ShotRole::Performance, MouthPolicy::SpeakingVisible))
                .collect(),
        );
        let v = validate_scene_mix(&scene);
        assert!(v.is_reject(), "{:?}", v.messages);
        assert_eq!(v.mix.performance, 6);
        assert_eq!(v.mix.other_dialogue, 0);
    }

    #[test]
    fn balanced_accepts_mixed_roles() {
        let mut cuts: Vec<CutRecord> = (1..=3)
            .map(|n| dialogue_cut(n, ShotRole::Performance, MouthPolicy::SpeakingVisible))
            .collect();
        cuts.push(dialogue_cut(
            4,
            ShotRole::Reaction,
            MouthPolicy::ClosedOrNotVisible,
        ));
        cuts.push(dialogue_cut(
            5,
            ShotRole::Atmosphere,
            MouthPolicy::OffCamera,
        ));
        let mut silent = CutRecord::new(CutId::new(1, 6));
        silent.sketch = Some(CutSketch {
            title: Some("Rain".into()),
            shot_role: Some(ShotRole::Insert),
            mouth_policy: Some(MouthPolicy::NoPeople),
            ..CutSketch::default()
        });
        cuts.push(silent);
        let scene = scene_with(cuts);
        let v = validate_scene_mix(&scene);
        assert_eq!(v.status, VerdictStatus::Ok, "{:?}", v.messages);
        assert_eq!(v.mix.performance, 3);
        assert_eq!(v.mix.other_dialogue, 2);
        assert_eq!(scene.cuts[3].speech_mode, Some(SpeechMode::VoiceOver));
    }

    #[test]
    fn override_downgrades_reject() {
        let mut scene = scene_with(
            (1..=4)
                .map(|n| dialogue_cut(n, ShotRole::Performance, MouthPolicy::SpeakingVisible))
                .collect(),
        );
        scene.strategy_override = true;
        let v = validate_scene_mix(&scene);
        assert_eq!(v.status, VerdictStatus::Warn);
        assert!(v.messages.iter().any(|m| m.contains("override")));
    }

    #[test]
    fn speech_ignores_missing_sketch_mouth_for_reaction() {
        let mut cut = CutRecord::new(CutId::new(1, 1));
        cut.dialogue = vec![DialogueLine {
            speaker: "Pip".into(),
            text: "Jam?".into(),
        }];
        cut.sketch = Some(CutSketch {
            shot_role: Some(ShotRole::Reaction),
            mouth_policy: Some(MouthPolicy::ClosedOrNotVisible),
            ..CutSketch::default()
        });
        apply_derived_speech(&mut cut);
        assert_eq!(cut.speech_mode, Some(SpeechMode::VoiceOver));
        assert!(!is_performance_dialogue(&cut));
    }
}
