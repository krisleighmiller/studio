//! Storyboard timing and animatic state.
//!
//! Cut sketches stay the shot cards. This module stores their order and
//! screen duration, optional scratch audio, and coverage warnings. A timing
//! or order change marks later shot packages for review and leaves takes
//! where they are.
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::output_paths::slugify;
use crate::scene_plan::handoff::{CutId, Pace};
use crate::scene_plan::timing::{normalized_words, round2};
use crate::script::BeatKind;
use crate::workflow::state::{
    AnimaticShot, CoverageWarning, CoverageWarningKind, ProjectState, SceneAnimatic, SceneRecord,
    ShotRole,
};

const MIN_DURATION: f64 = 0.4;
const MAX_DURATION: f64 = 120.0;
const SPEAKING_RATE_WPM: f64 = 165.0;

/// Ensure every cut sketch has one animatic shot. Existing ids, order,
/// durations, and scratch paths stay put.
pub fn sync(state: &mut ProjectState) -> bool {
    let mut next_shot = next_numeric_id(state, "anim_shot_");
    let mut next_animatic = next_numeric_id(state, "animatic_");
    let mut changed = false;
    for scene in &mut state.scenes {
        if scene.cuts.is_empty() {
            continue;
        }
        if scene.animatic.is_none() {
            scene.animatic = Some(SceneAnimatic {
                id: format!("animatic_{next_animatic}"),
                approved: false,
                shots: Vec::new(),
                warnings: Vec::new(),
            });
            next_animatic += 1;
            changed = true;
        }
        let seeds: Vec<(CutId, f64)> = scene
            .cuts
            .iter()
            .map(|cut| (cut.id, suggested_duration(cut)))
            .collect();
        let warnings = coverage(scene);
        let animatic = scene.animatic.as_mut().expect("just inserted");
        let before = animatic.shots.len();
        animatic
            .shots
            .retain(|shot| seeds.iter().any(|(cut_id, _)| *cut_id == shot.cut_id));
        if animatic.shots.len() != before {
            changed = true;
        }
        for (cut_id, duration) in seeds {
            if animatic.shots.iter().any(|shot| shot.cut_id == cut_id) {
                continue;
            }
            animatic.shots.push(AnimaticShot {
                id: format!("anim_shot_{next_shot}"),
                cut_id,
                duration_seconds: duration,
                scratch_audio_path: None,
            });
            next_shot += 1;
            changed = true;
        }
        if animatic.warnings != warnings {
            animatic.warnings = warnings;
            changed = true;
        }
    }
    changed
}

pub fn set_duration(scene: &mut SceneRecord, shot_id: &str, seconds: f64) -> Result<(), String> {
    if !(seconds.is_finite() && seconds >= MIN_DURATION && seconds <= MAX_DURATION) {
        return Err(format!(
            "Duration must be between {MIN_DURATION} and {MAX_DURATION} seconds."
        ));
    }
    let seconds = round2(seconds);
    let index = {
        let animatic = scene
            .animatic
            .as_mut()
            .ok_or_else(|| "This scene has no animatic.".to_string())?;
        let index = animatic
            .shots
            .iter()
            .position(|shot| shot.id == shot_id)
            .ok_or_else(|| format!("animatic shot {shot_id} not found"))?;
        if (animatic.shots[index].duration_seconds - seconds).abs() < 0.001 {
            return Ok(());
        }
        animatic.shots[index].duration_seconds = seconds;
        animatic.approved = false;
        index
    };
    mark_packages_from(scene, index);
    Ok(())
}

pub fn reorder(scene: &mut SceneRecord, shot_ids: &[String]) -> Result<(), String> {
    let index = {
        let animatic = scene
            .animatic
            .as_mut()
            .ok_or_else(|| "This scene has no animatic.".to_string())?;
        if shot_ids.len() != animatic.shots.len()
            || shot_ids.iter().collect::<HashSet<_>>().len() != animatic.shots.len()
            || animatic
                .shots
                .iter()
                .any(|shot| !shot_ids.contains(&shot.id))
        {
            return Err("Reorder must list each animatic shot once.".into());
        }
        if animatic
            .shots
            .iter()
            .map(|shot| &shot.id)
            .eq(shot_ids.iter())
        {
            return Ok(());
        }
        let previous = animatic.shots.clone();
        animatic.shots = shot_ids
            .iter()
            .map(|id| {
                previous
                    .iter()
                    .find(|shot| &shot.id == id)
                    .expect("checked")
                    .clone()
            })
            .collect();
        animatic.approved = false;
        previous
            .iter()
            .zip(animatic.shots.iter())
            .position(|(before, after)| before.id != after.id)
            .unwrap_or(0)
    };
    mark_packages_from(scene, index);
    Ok(())
}

pub fn approve(scene: &mut SceneRecord) -> Result<(), String> {
    let animatic = scene
        .animatic
        .as_mut()
        .ok_or_else(|| "This scene has no animatic.".to_string())?;
    if animatic.shots.is_empty() {
        return Err("Add cut sketches before approving the rough cut.".into());
    }
    if animatic.shots.iter().any(|shot| {
        !(shot.duration_seconds.is_finite()
            && shot.duration_seconds >= MIN_DURATION
            && shot.duration_seconds <= MAX_DURATION)
    }) {
        return Err("Every shot needs a duration before the rough cut can be approved.".into());
    }
    animatic.approved = true;
    Ok(())
}

pub fn attach_scratch(
    project_root: &Path,
    scene: &mut SceneRecord,
    shot_id: &str,
    source: &Path,
) -> Result<String, String> {
    if !source.is_file() {
        return Err(format!("scratch audio not found: {}", source.display()));
    }
    let ext = source
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("wav")
        .to_ascii_lowercase();
    if !matches!(ext.as_str(), "wav" | "mp3" | "m4a" | "aac" | "ogg" | "flac") {
        return Err("Scratch audio must be wav, mp3, m4a, aac, ogg, or flac.".into());
    }
    let animatic = scene
        .animatic
        .as_mut()
        .ok_or_else(|| "This scene has no animatic.".to_string())?;
    let shot = animatic
        .shots
        .iter_mut()
        .find(|shot| shot.id == shot_id)
        .ok_or_else(|| format!("animatic shot {shot_id} not found"))?;
    let rel = format!("Preproduction/Scene{}/animatic/{shot_id}.{ext}", scene.id);
    let dest = project_root.join(&rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    if source != dest.as_path() {
        fs::copy(source, &dest)
            .map_err(|error| format!("could not copy scratch audio: {error}"))?;
    }
    shot.scratch_audio_path = Some(rel.clone());
    Ok(rel)
}

pub fn clear_scratch(
    project_root: &Path,
    scene: &mut SceneRecord,
    shot_id: &str,
) -> Result<(), String> {
    let animatic = scene
        .animatic
        .as_mut()
        .ok_or_else(|| "This scene has no animatic.".to_string())?;
    let shot = animatic
        .shots
        .iter_mut()
        .find(|shot| shot.id == shot_id)
        .ok_or_else(|| format!("animatic shot {shot_id} not found"))?;
    if let Some(path) = shot.scratch_audio_path.clone() {
        let root_s = project_root.to_string_lossy().to_string();
        let file = crate::project_path::resolve(
            &path,
            Some(&root_s),
            crate::project_path::PathIntent::Write,
        )
        .map(|resolved| resolved.into_path_buf())
        .map_err(|error| error.to_string())?;
        crate::project_path::under_scene_bucket(project_root, &file, "animatic")?;
        if file.is_file() {
            crate::project_path::move_to_trash(project_root, &file)?;
        }
        shot.scratch_audio_path = None;
    }
    Ok(())
}

fn mark_packages_from(scene: &mut SceneRecord, start: usize) {
    let cut_ids: Vec<CutId> = scene
        .animatic
        .as_ref()
        .map(|animatic| {
            animatic
                .shots
                .iter()
                .skip(start)
                .map(|shot| shot.cut_id)
                .collect()
        })
        .unwrap_or_default();
    for cut in &mut scene.cuts {
        if cut_ids.contains(&cut.id) && cut.has_production_note() {
            cut.package_needs_review = true;
        }
    }
}

fn suggested_duration(cut: &crate::workflow::state::CutRecord) -> f64 {
    let mut text = String::new();
    if let Some(sketch) = &cut.sketch {
        for line in &sketch.dialogue {
            text.push_str(&line.text);
            text.push(' ');
        }
    }
    if text.trim().is_empty() {
        for line in &cut.dialogue {
            text.push_str(&line.text);
            text.push(' ');
        }
    }
    let words = normalized_words(&text).len();
    if words > 0 {
        return round2((words as f64 * 60.0 / SPEAKING_RATE_WPM).max(1.5));
    }
    let pace = cut
        .sketch
        .as_ref()
        .and_then(|sketch| sketch.pace)
        .unwrap_or(Pace::Natural);
    match pace {
        Pace::Tight => 2.0,
        Pace::Natural => 3.0,
        Pace::Held => 5.0,
    }
}

pub fn coverage(scene: &SceneRecord) -> Vec<CoverageWarning> {
    let mut warnings = Vec::new();
    let roles: Vec<ShotRole> = scene
        .cuts
        .iter()
        .filter_map(|cut| cut.sketch.as_ref().and_then(|sketch| sketch.shot_role))
        .collect();
    if roles.is_empty() && scene.dialogue.is_empty() {
        return warnings;
    }
    let has_dialogue = !scene.dialogue.is_empty()
        || scene.cuts.iter().any(|cut| {
            !cut.dialogue.is_empty()
                || cut
                    .sketch
                    .as_ref()
                    .is_some_and(|sketch| !sketch.dialogue.is_empty())
        });
    if has_dialogue && !roles.contains(&ShotRole::Reaction) {
        warnings.push(CoverageWarning {
            id: "cov_missing_reaction".into(),
            kind: CoverageWarningKind::MissingReaction,
            message: "No reaction shot is planned.".into(),
        });
    }
    if !roles.contains(&ShotRole::Insert) && !roles.contains(&ShotRole::Detail) {
        warnings.push(CoverageWarning {
            id: "cov_missing_insert".into(),
            kind: CoverageWarningKind::MissingInsert,
            message: "No insert or detail shot is planned.".into(),
        });
    }
    if !roles.contains(&ShotRole::Establishing) {
        warnings.push(CoverageWarning {
            id: "cov_missing_establishing".into(),
            kind: CoverageWarningKind::MissingEstablishing,
            message: "No establishing shot is planned.".into(),
        });
    }
    let dialogue_blob = sketch_dialogue_blob(scene);
    for line in &scene.dialogue {
        let words = normalized_words(&line.text);
        if words.is_empty() {
            continue;
        }
        let needle = words.join(" ");
        if dialogue_blob.contains(&needle) {
            continue;
        }
        warnings.push(CoverageWarning {
            id: beat_warning_id("dialogue", &line.text, &warnings),
            kind: CoverageWarningKind::UnplannedBeat,
            message: format!("Unplanned dialogue: {}: {}", line.speaker, line.text),
        });
    }
    let corpus = sketch_corpus(scene);
    let actions = if scene.action.is_empty() {
        scene
            .beats
            .iter()
            .filter(|beat| beat.kind == BeatKind::Action)
            .map(|beat| beat.text.clone())
            .collect()
    } else {
        scene.action.clone()
    };
    for action in actions {
        let words: Vec<String> = normalized_words(&action)
            .into_iter()
            .filter(|word| word.len() >= 4)
            .collect();
        if words.len() < 2 {
            continue;
        }
        let hits = words.iter().filter(|word| corpus.contains(*word)).count();
        if hits >= 2 {
            continue;
        }
        warnings.push(CoverageWarning {
            id: beat_warning_id("action", &action, &warnings),
            kind: CoverageWarningKind::UnplannedBeat,
            message: format!("Unplanned action: {}", shorten(&action, 96)),
        });
    }
    warnings
}

fn sketch_dialogue_blob(scene: &SceneRecord) -> String {
    let mut words = Vec::new();
    for cut in &scene.cuts {
        for line in &cut.dialogue {
            words.extend(normalized_words(&line.text));
        }
        if let Some(sketch) = &cut.sketch {
            for line in &sketch.dialogue {
                words.extend(normalized_words(&line.text));
            }
        }
    }
    words.join(" ")
}

fn sketch_corpus(scene: &SceneRecord) -> HashSet<String> {
    let mut text = String::new();
    for cut in &scene.cuts {
        if let Some(title) = &cut.title {
            text.push_str(title);
            text.push(' ');
        }
        if let Some(prompt) = &cut.prompt {
            text.push_str(prompt);
            text.push(' ');
        }
        if let Some(sketch) = &cut.sketch {
            for part in [
                sketch.title.as_deref(),
                sketch.subject.as_deref(),
                sketch.visual_prompt.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                text.push_str(part);
                text.push(' ');
            }
            for line in &sketch.dialogue {
                text.push_str(&line.text);
                text.push(' ');
            }
        }
    }
    normalized_words(&text)
        .into_iter()
        .filter(|word| word.len() >= 4)
        .collect()
}

fn beat_warning_id(kind: &str, text: &str, existing: &[CoverageWarning]) -> String {
    let slug = slugify(text);
    let slug = if slug.is_empty() {
        "beat".to_string()
    } else {
        slug
    };
    let base = format!("cov_{kind}_{slug}");
    if existing.iter().all(|warning| warning.id != base) {
        return base;
    }
    format!("{base}_{:x}", fnv(text))
}

fn fnv(text: &str) -> u32 {
    let mut hash = 2166136261u32;
    for byte in text.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(16777619);
    }
    hash
}

fn shorten(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn next_numeric_id(state: &ProjectState, prefix: &str) -> u64 {
    let mut next = 1u64;
    for scene in &state.scenes {
        let Some(animatic) = &scene.animatic else {
            continue;
        };
        if let Some(value) = animatic
            .id
            .strip_prefix(prefix)
            .and_then(|rest| rest.parse::<u64>().ok())
        {
            next = next.max(value + 1);
        }
        for shot in &animatic.shots {
            if let Some(value) = shot
                .id
                .strip_prefix(prefix)
                .and_then(|rest| rest.parse::<u64>().ok())
            {
                next = next.max(value + 1);
            }
        }
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::DialogueLine;
    use crate::workflow::state::{
        CutRecord, CutSketch, ProjectState, ShotRole, SketchDialogue, TakeRecord, TakeStatus,
    };

    fn sketch(role: ShotRole, dialogue: &str) -> CutSketch {
        CutSketch {
            title: Some(role.label().into()),
            shot_role: Some(role),
            dialogue: if dialogue.is_empty() {
                Vec::new()
            } else {
                vec![SketchDialogue {
                    speaker: "Tom".into(),
                    text: dialogue.into(),
                    heard_as: None,
                }]
            },
            ..CutSketch::default()
        }
    }

    fn packaged(scene_id: u32, cut: u32, role: ShotRole, dialogue: &str) -> CutRecord {
        let mut record = CutRecord::new(CutId::new(scene_id, cut));
        record.title = Some(format!("Cut {cut}"));
        record.sketch = Some(sketch(role, dialogue));
        record.dialogue = if dialogue.is_empty() {
            Vec::new()
        } else {
            vec![DialogueLine {
                speaker: "Tom".into(),
                text: dialogue.into(),
            }]
        };
        record.prompt = Some(format!("package {cut}"));
        record.production_note_path = Some(format!("Scene_Prompts/Scene{scene_id}/Cut{cut}.txt"));
        record.takes.push(TakeRecord {
            id: format!("take_{cut}"),
            path: format!("Renders/Scene{scene_id}/Cut{cut}.mp4"),
            status: TakeStatus::Selected,
            notes: None,
        });
        record.selected_take = Some(format!("take_{cut}"));
        record
    }

    #[test]
    fn timing_change_marks_later_packages_and_keeps_renders() {
        let mut state = ProjectState::new();
        let scene = state.scene_mut(1, "The last train");
        scene.cuts.push(packaged(
            1,
            1,
            ShotRole::Performance,
            "We shall be there before supper.",
        ));
        scene.cuts.push(packaged(
            1,
            2,
            ShotRole::Performance,
            "Will there be cocoa?",
        ));
        scene.cuts.push(packaged(1, 3, ShotRole::Atmosphere, ""));
        scene.dialogue.push(DialogueLine {
            speaker: "Tom".into(),
            text: "We shall be there before supper.".into(),
        });
        scene.dialogue.push(DialogueLine {
            speaker: "Pip".into(),
            text: "Will there be cocoa?".into(),
        });
        scene
            .action
            .push("Rain streams down the carriage windows.".into());
        assert!(sync(&mut state));
        let scene = state.scene_mut(1, "The last train");
        let ids: Vec<String> = scene
            .animatic
            .as_ref()
            .unwrap()
            .shots
            .iter()
            .map(|shot| shot.id.clone())
            .collect();
        assert_eq!(ids.len(), 3);
        assert!(ids.iter().all(|id| id.starts_with("anim_shot_")));
        assert!(ids.iter().all(|id| id != "1" && id != "2" && id != "3"));
        scene.animatic.as_mut().unwrap().approved = true;
        let first = ids[0].clone();
        set_duration(scene, &first, 6.5).unwrap();
        let scene = state.scene(1).unwrap();
        assert!(!scene.animatic.as_ref().unwrap().approved);
        assert!(scene.cuts[0].package_needs_review);
        assert!(scene.cuts[1].package_needs_review);
        assert!(scene.cuts[2].package_needs_review);
        assert_eq!(scene.cuts[0].takes.len(), 1);
        assert_eq!(scene.cuts[1].takes[0].path, "Renders/Scene1/Cut2.mp4");
        assert_eq!(scene.cuts[2].selected_take.as_deref(), Some("take_3"));
        assert_eq!(
            scene.animatic.as_ref().unwrap().shots[0].duration_seconds,
            6.5
        );
        assert_eq!(scene.animatic.as_ref().unwrap().shots[0].id, first);

        let scene = state.scene_mut(1, "The last train");
        for cut in &mut scene.cuts {
            cut.package_needs_review = false;
        }
        let last = ids[2].clone();
        set_duration(scene, &last, 4.0).unwrap();
        let scene = state.scene(1).unwrap();
        assert!(!scene.cuts[0].package_needs_review);
        assert!(!scene.cuts[1].package_needs_review);
        assert!(scene.cuts[2].package_needs_review);
        assert_eq!(scene.cuts[0].takes.len(), 1);
    }

    #[test]
    fn reorder_keeps_shot_ids_and_marks_the_moved_tail() {
        let mut state = ProjectState::new();
        let scene = state.scene_mut(1, "The last train");
        scene.cuts.push(packaged(1, 1, ShotRole::Establishing, ""));
        scene.cuts.push(packaged(1, 2, ShotRole::Insert, ""));
        scene
            .cuts
            .push(packaged(1, 3, ShotRole::Reaction, "And jam?"));
        sync(&mut state);
        let scene = state.scene_mut(1, "The last train");
        let shots = scene.animatic.as_ref().unwrap().shots.clone();
        let order = vec![
            shots[2].id.clone(),
            shots[0].id.clone(),
            shots[1].id.clone(),
        ];
        reorder(scene, &order).unwrap();
        let scene = state.scene(1).unwrap();
        let after = &scene.animatic.as_ref().unwrap().shots;
        assert_eq!(after[0].id, shots[2].id);
        assert_eq!(after[0].cut_id, CutId::new(1, 3));
        assert_eq!(after[1].id, shots[0].id);
        assert_eq!(after[2].id, shots[1].id);
        assert!(scene.cuts.iter().all(|cut| cut.package_needs_review));
        assert!(scene.cuts.iter().all(|cut| cut.takes.len() == 1));
    }

    #[test]
    fn coverage_warns_for_missing_roles_and_unplanned_dialogue() {
        let mut state = ProjectState::new();
        let scene = state.scene_mut(1, "The last train");
        scene.cuts.push(packaged(
            1,
            1,
            ShotRole::Performance,
            "We shall be there before supper.",
        ));
        scene.cuts[0].sketch.as_mut().unwrap().visual_prompt =
            Some("Rain on the carriage windows.".into());
        scene.dialogue.push(DialogueLine {
            speaker: "Tom".into(),
            text: "We shall be there before supper.".into(),
        });
        scene.dialogue.push(DialogueLine {
            speaker: "Pip".into(),
            text: "Will there be cocoa?".into(),
        });
        scene
            .action
            .push("A hidden cipher waits under the floorboards.".into());
        scene
            .action
            .push("Rain streams down the carriage windows.".into());
        let warnings = coverage(scene);
        let ids: Vec<&str> = warnings.iter().map(|warning| warning.id.as_str()).collect();
        assert!(ids.contains(&"cov_missing_reaction"));
        assert!(ids.contains(&"cov_missing_insert"));
        assert!(ids.contains(&"cov_missing_establishing"));
        assert!(warnings.iter().any(|warning| {
            warning.kind == CoverageWarningKind::UnplannedBeat && warning.message.contains("cocoa")
        }));
        assert!(warnings.iter().any(|warning| {
            warning.kind == CoverageWarningKind::UnplannedBeat && warning.message.contains("cipher")
        }));
        assert!(warnings
            .iter()
            .all(|warning| !warning.message.contains("carriage windows")));
        assert!(warnings
            .iter()
            .all(|warning| !warning.id.chars().all(|ch| ch.is_ascii_digit())));
    }
}
