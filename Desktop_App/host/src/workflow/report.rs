//! Human-readable and typed status for an imported V2 project.
use crate::scene_plan::handoff::SpeechMode;
use crate::scene_plan::strategy::SceneStrategy;
use crate::workflow::state::{ProjectState, SceneRecord, StageStatus};
use crate::workflow::validate::{validate_scene_mix, MixVerdict, SketchMix};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MixCounts {
    pub lipsync: usize,
    pub voice_over: usize,
    pub silent: usize,
}

impl MixCounts {
    pub fn of(scene: &SceneRecord) -> Self {
        let mut m = MixCounts::default();
        for cut in &scene.cuts {
            match cut.speech_mode {
                Some(SpeechMode::SourceVideoLipsync) => m.lipsync += 1,
                Some(SpeechMode::VoiceOver) => m.voice_over += 1,
                _ => m.silent += 1,
            }
        }
        m
    }

    pub fn total(self) -> usize {
        self.lipsync + self.voice_over + self.silent
    }

    pub fn display(self) -> String {
        format!(
            "{} lip-sync / {} voice-over / {} silent",
            self.lipsync, self.voice_over, self.silent
        )
    }

    /// Observational label. Not a rejection — strategy validation comes later.
    pub fn performance_heavy_for_balanced(self) -> bool {
        let dialogue_cuts = self.lipsync + self.voice_over;
        if dialogue_cuts < 4 || self.total() < 3 {
            return false;
        }
        let pct = self.lipsync * 100 / dialogue_cuts;
        pct >= 80 || (pct > 60 && self.voice_over == 0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneReport {
    pub id: u32,
    pub title: String,
    pub cuts_found: usize,
    pub mix: MixCounts,
    pub approved_start_frames: usize,
    pub planned_visual_sources: usize,
    pub render_ready_sources: usize,
    pub unresolved_visual_sources: usize,
    pub production_notes: bool,
    pub scene_intent: bool,
    pub scene_look: bool,
    pub cut_sketches: bool,
    pub can_backfill_sketches: bool,
    pub effectively_performance_heavy: bool,
    pub strategy: SceneStrategy,
    pub sketch_mix: SketchMix,
    pub mix_verdict: MixVerdict,
    pub warnings: Vec<String>,
    pub summary: String,
    pub navigation: Vec<crate::workflow::recipe::StageNav>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectReport {
    pub project_name: Option<String>,
    pub script_count: usize,
    pub character_count: usize,
    pub identity_ready: usize,
    pub voice_ready: usize,
    pub scenes: Vec<SceneReport>,
    pub warnings: Vec<String>,
    pub summary: String,
    pub recipe: crate::workflow::recipe::RecipeReport,
    pub recipes: Vec<crate::workflow::recipe::RecipeChoice>,
}

fn scene_report(scene: &SceneRecord, graph: &crate::workflow::recipe::RecipeGraph) -> SceneReport {
    let mix = MixCounts::of(scene);
    let approved_start_frames = scene
        .cuts
        .iter()
        .filter(|c| c.approved_start_frame.is_some())
        .count();
    let planned_visual_sources = scene
        .cuts
        .iter()
        .filter(|cut| cut.visual_source_planned())
        .count();
    let render_ready_sources = scene
        .cuts
        .iter()
        .filter(|cut| cut.visual_source_ready())
        .count();
    let unresolved_visual_sources = planned_visual_sources.saturating_sub(render_ready_sources);
    let production_notes = scene.cuts.iter().any(|c| c.has_production_note());
    let scene_intent = scene.intent.as_ref().is_some_and(|i| !i.is_empty());
    let scene_look = scene.look_board.as_ref().is_some_and(|b| !b.is_empty());
    let cut_sketches = scene
        .cuts
        .iter()
        .any(|c| c.sketch.as_ref().is_some_and(|s| !s.is_empty()));
    let can_backfill_sketches = production_notes && !cut_sketches;
    let effectively_performance_heavy = mix.performance_heavy_for_balanced();
    let mix_verdict = validate_scene_mix(scene);
    let sketch_mix = mix_verdict.mix;
    let strategy = mix_verdict.strategy;
    let mut warnings = Vec::new();
    if production_notes && planned_visual_sources == 0 {
        warnings.push("0 approved visual-source strategies; imported production notes currently choose sources outside workflow state".into());
    }
    if unresolved_visual_sources > 0 {
        warnings.push(format!(
            "{unresolved_visual_sources} planned visual source(s) await a concrete production asset"
        ));
    }
    if scene.stages.scene_look == StageStatus::Missing {
        warnings.push("scene look board missing".into());
    } else if scene
        .look_board
        .as_ref()
        .is_some_and(|b| b.approved_count() == 0)
    {
        warnings.push("scene look has no approved stills yet".into());
    }
    if can_backfill_sketches {
        warnings.push("cut sketches can be backfilled from production notes".into());
    }
    if effectively_performance_heavy {
        warnings
            .push("mix is effectively performance-heavy for a cinematic-balanced request".into());
    }
    if scene.stages.script == StageStatus::Missing {
        warnings.push("script scene text missing".into());
    }

    let mut lines = vec![
        format!("{}:", scene.title),
        format!("- {} cuts found", scene.cuts.len()),
        format!("- {}", mix.display()),
        format!("- {approved_start_frames} approved cut-specific start frames"),
        format!(
            "- {planned_visual_sources} source strategies planned; {render_ready_sources} render ready"
        ),
    ];
    if production_notes {
        lines.push("- production notes exist".into());
    } else {
        lines.push("- production notes missing".into());
    }
    if !scene_look {
        lines.push("- scene look board missing".into());
    } else {
        let approved = scene
            .look_board
            .as_ref()
            .map(|b| b.approved_count())
            .unwrap_or(0);
        lines.push(format!("- scene look: {approved} approved still(s)"));
    }
    if !scene_intent {
        lines.push("- scene intent missing".into());
    }
    if can_backfill_sketches {
        lines.push("- cut sketches can be backfilled from production notes".into());
    } else if cut_sketches {
        lines.push("- cut sketches present".into());
    } else {
        lines.push("- cut sketches missing".into());
    }
    if effectively_performance_heavy {
        lines.push("- flagged as effectively performance-heavy for cinematic-balanced".into());
    }
    if sketch_mix.total() > 0 {
        lines.push(format!("- sketch mix: {}", sketch_mix.display()));
        match mix_verdict.status {
            crate::workflow::validate::VerdictStatus::Ok => {
                lines.push(format!("- {} mix accepted", strategy.label()));
            }
            crate::workflow::validate::VerdictStatus::Warn => {
                lines.push(format!("- {} mix warning", strategy.label()));
            }
            crate::workflow::validate::VerdictStatus::Reject => {
                lines.push(format!("- {} mix rejected", strategy.label()));
            }
        }
        warnings.extend(mix_verdict.messages.iter().cloned());
    }

    SceneReport {
        id: scene.id,
        title: scene.title.clone(),
        cuts_found: scene.cuts.len(),
        mix,
        approved_start_frames,
        planned_visual_sources,
        render_ready_sources,
        unresolved_visual_sources,
        production_notes,
        scene_intent,
        scene_look,
        cut_sketches,
        can_backfill_sketches,
        effectively_performance_heavy,
        strategy,
        sketch_mix,
        mix_verdict,
        warnings,
        summary: lines.join("\n"),
        navigation: crate::workflow::recipe::navigation(graph, scene),
    }
}

pub fn project_report(state: &ProjectState) -> ProjectReport {
    let active = crate::workflow::recipe::graph(
        state
            .profile
            .as_ref()
            .map(|profile| profile.recipe)
            .unwrap_or_default(),
    );
    let scenes: Vec<SceneReport> = state
        .scenes
        .iter()
        .map(|scene| scene_report(scene, active))
        .collect();
    let identity_ready = state.characters.iter().filter(|c| c.identity_ready).count();
    let voice_ready = state.characters.iter().filter(|c| c.voice_ready).count();
    let mut warnings = state.notes.clone();
    for scene in &scenes {
        warnings.extend(
            scene
                .warnings
                .iter()
                .map(|w| format!("Scene {}: {w}", scene.id)),
        );
    }
    let mut summary = String::new();
    if let Some(name) = &state.project_name {
        summary.push_str(&format!("{name}\n"));
    }
    summary.push_str(&format!(
        "{} script(s), {} character(s) ({} identity-ready, {} voice-ready), {} scene(s)\n",
        state.scripts.len(),
        state.characters.len(),
        identity_ready,
        voice_ready,
        scenes.len()
    ));
    for scene in &scenes {
        summary.push('\n');
        summary.push_str(&scene.summary);
        summary.push('\n');
    }
    ProjectReport {
        project_name: state.project_name.clone(),
        script_count: state.scripts.len(),
        character_count: state.characters.len(),
        identity_ready,
        voice_ready,
        scenes,
        warnings,
        summary: summary.trim().to_string(),
        recipe: crate::workflow::recipe::graph(
            state
                .profile
                .as_ref()
                .map(|profile| profile.recipe)
                .unwrap_or_default(),
        )
        .report(),
        recipes: crate::workflow::recipe::choices(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_plan::handoff::CutId;
    use crate::workflow::state::CutRecord;

    #[test]
    fn flags_six_one_two_as_performance_heavy() {
        let mut scene = SceneRecord::new(1, "Scene 1 — The last train");
        for n in 1..=6 {
            let mut c = CutRecord::new(CutId::new(1, n));
            c.speech_mode = Some(SpeechMode::SourceVideoLipsync);
            scene.cuts.push(c);
        }
        let mut vo = CutRecord::new(CutId::new(1, 8));
        vo.speech_mode = Some(SpeechMode::VoiceOver);
        scene.cuts.push(vo);
        scene.cuts.push(CutRecord::new(CutId::new(1, 7)));
        scene.cuts.push(CutRecord::new(CutId::new(1, 9)));
        let mix = MixCounts::of(&scene);
        assert_eq!((mix.lipsync, mix.voice_over, mix.silent), (6, 1, 2));
        assert!(mix.performance_heavy_for_balanced());
        assert_eq!(mix.display(), "6 lip-sync / 1 voice-over / 2 silent");
    }
}
