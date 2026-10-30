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
    SequenceTransition, ShotRole,
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
                trim_in_seconds: None,
                trim_out_seconds: None,
                transition: None,
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
        let shot = &animatic.shots[index];
        if shot.trim_in_seconds.unwrap_or(0.0) + shot.trim_out_seconds.unwrap_or(0.0) >= seconds {
            return Err("Trim points leave the shot with no runtime. Set a longer duration or smaller trim.".into());
        }
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

/// Set non-destructive trim points (seconds off the head/tail) on an animatic
/// shot. Either may be `None` to leave that side untouched; pass `Some(0.0)` to
/// clear. Trim points must leave a positive runtime. Marks later packages stale —
/// no frames or takes are deleted.
pub fn set_trim(
    scene: &mut SceneRecord,
    shot_id: &str,
    trim_in: Option<f64>,
    trim_out: Option<f64>,
) -> Result<(), String> {
    for value in [trim_in, trim_out].into_iter().flatten() {
        if !value.is_finite() || value < 0.0 {
            return Err("Trim must be a non-negative number of seconds.".into());
        }
    }
    let head = trim_in.unwrap_or(0.0);
    let tail = trim_out.unwrap_or(0.0);
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
        let shot = &mut animatic.shots[index];
        if head + tail >= shot.duration_seconds {
            return Err("Trim points leave the shot with no runtime. Set a smaller trim.".into());
        }
        if shot.trim_in_seconds == trim_in && shot.trim_out_seconds == trim_out {
            return Ok(());
        }
        shot.trim_in_seconds = trim_in;
        shot.trim_out_seconds = trim_out;
        animatic.approved = false;
        index
    };
    mark_packages_from(scene, index);
    Ok(())
}

/// Set the transition out of an animatic shot (`cut`, `dissolve`, or
/// `fade_through_black`); `None`/empty clears it back to a plain cut. Returns
/// the effective transition.
pub fn set_transition(
    scene: &mut SceneRecord,
    shot_id: &str,
    transition: Option<&str>,
) -> Result<SequenceTransition, String> {
    let parsed = match transition.map(str::trim) {
        None | Some("") => None,
        Some(value) => Some(
            SequenceTransition::parse(value)
                .ok_or_else(|| format!("unknown transition '{value}'"))?,
        ),
    };
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
        if animatic.shots[index].transition == parsed {
            return Ok(parsed.unwrap_or(SequenceTransition::Cut));
        }
        animatic.shots[index].transition = parsed;
        animatic.approved = false;
        index
    };
    mark_packages_from(scene, index);
    Ok(parsed.unwrap_or(SequenceTransition::Cut))
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
    if animatic.shots.iter().any(|shot| {
        [shot.trim_in_seconds, shot.trim_out_seconds]
            .into_iter()
            .flatten()
            .any(|value| !value.is_finite() || value < 0.0)
            || shot.trim_in_seconds.unwrap_or(0.0) + shot.trim_out_seconds.unwrap_or(0.0)
                >= shot.duration_seconds
    }) {
        return Err("Trim points leave a shot with invalid runtime. Set a longer duration or smaller trim.".into());
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
    // Validate shot_id: nonempty ASCII alphanumeric / _ / - only. Any
    // slash, backslash, dot, or other path-bearing input is rejected.
    if shot_id.is_empty() || !shot_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err("invalid shot id: only ASCII alphanumeric, '_', and '-' are allowed".into());
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
    // Build the known relative path from validated components.
    let rel = format!("Preproduction/Scene{}/animatic/{shot_id}.{ext}", scene.id);
    let root_str = project_root.to_string_lossy().into_owned();
    // Confine destination BEFORE create_dir_all: resolve against the
    // project root and enforce the exact Preproduction/SceneN/animatic bucket.
    let dest = crate::project_path::resolve(&rel, Some(&root_str), crate::project_path::PathIntent::Write)
        .map(|resolved| resolved.into_path_buf())
        .map_err(|error| format!("scratch destination outside project: {error}"))?;
    crate::project_path::under_scene_bucket(project_root, &dest, "animatic")
        .map_err(|error| format!("scratch destination outside animatic bucket: {error}"))?;
    if crate::project_path::relative_to_root(project_root, &dest)?.parent()
        != Path::new(&rel).parent()
    {
        return Err("scratch destination is outside this scene animatic bucket".into());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    // Re-confine AFTER create_dir_all — fs::copy follows the parent chain,
    // so the final destination must still be inside the exact animatic bucket
    // of this scene. This blocks directory or existing-file symlinks that
    // could redirect the write outside root.
    let dest_again = crate::project_path::resolve(&rel, Some(&root_str), crate::project_path::PathIntent::Write)
        .map(|resolved| resolved.into_path_buf())
        .map_err(|error| format!("scratch destination outside project: {error}"))?;
    crate::project_path::under_scene_bucket(project_root, &dest_again, "animatic")
        .map_err(|error| format!("scratch destination outside animatic bucket: {error}"))?;
    if crate::project_path::relative_to_root(project_root, &dest_again)?.parent()
        != Path::new(&rel).parent()
    {
        return Err("scratch destination is outside this scene animatic bucket".into());
    }
    if source != dest_again.as_path() {
        fs::copy(source, &dest_again)
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
    fn clear_scratch_trashes_only_the_animatic_bucket() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let rel = "Preproduction/Scene1/animatic/shot.wav";
        let file = root.join(rel);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"wav").unwrap();
        let outside = root.join("Scripts/secret.md");
        fs::create_dir_all(outside.parent().unwrap()).unwrap();
        fs::write(&outside, b"keep").unwrap();

        let mut state = ProjectState::new();
        let scene = state.scene_mut(1, "Scene");
        scene.animatic = Some(SceneAnimatic {
            id: "animatic_1".into(),
            approved: false,
            shots: vec![AnimaticShot {
                id: "shot".into(),
                cut_id: CutId::new(1, 1),
                duration_seconds: 2.0,
                scratch_audio_path: Some(rel.into()),
                trim_in_seconds: None,
                trim_out_seconds: None,
                transition: None,
            }],
            warnings: Vec::new(),
        });
        clear_scratch(root, scene, "shot").unwrap();
        assert!(!file.exists());
        assert!(root
            .join(".workbench-trash")
            .read_dir()
            .unwrap()
            .next()
            .is_some());
        assert!(scene.animatic.as_ref().unwrap().shots[0]
            .scratch_audio_path
            .is_none());

        scene.animatic.as_mut().unwrap().shots[0].scratch_audio_path =
            Some("Scripts/secret.md".into());
        let err = clear_scratch(root, scene, "shot").unwrap_err();
        assert!(err.contains("animatic"), "{err}");
        assert_eq!(fs::read_to_string(&outside).unwrap(), "keep");
        assert!(scene.animatic.as_ref().unwrap().shots[0]
            .scratch_audio_path
            .is_some());
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

    #[test]
    fn trims_and_transitions_are_non_destructive() {
        let mut state = ProjectState::new();
        let scene = state.scene_mut(1, "Scene");
        scene.animatic = Some(SceneAnimatic {
            id: "animatic_1".into(),
            approved: true,
            shots: vec![AnimaticShot {
                id: "shot".into(),
                cut_id: CutId::new(1, 1),
                duration_seconds: 10.0,
                scratch_audio_path: None,
                trim_in_seconds: None,
                trim_out_seconds: None,
                transition: None,
            }],
            warnings: Vec::new(),
        });
        scene.cut_mut(1).prompt = Some("a close up".into());

        // Trimming marks the animatic unapproved and the cut package stale,
        // and leaves takes untouched (non-destructive).
        set_trim(scene, "shot", Some(1.0), Some(2.0)).unwrap();
        let shot = &scene.animatic.as_ref().unwrap().shots[0];
        assert_eq!(shot.trim_in_seconds, Some(1.0));
        assert_eq!(shot.trim_out_seconds, Some(2.0));
        assert!(!scene.animatic.as_ref().unwrap().approved);
        assert!(scene.cuts[0].package_needs_review);
        assert!(scene.cuts[0].takes.is_empty());

        // Transitions parse, store, clear back to a plain cut, and reject junk.
        assert_eq!(
            set_transition(scene, "shot", Some("dissolve")).unwrap(),
            SequenceTransition::Dissolve
        );
        assert_eq!(
            scene.animatic.as_ref().unwrap().shots[0].transition,
            Some(SequenceTransition::Dissolve)
        );
        assert!(set_transition(scene, "shot", Some("whoosh")).is_err());
        assert_eq!(
            set_transition(scene, "shot", None).unwrap(),
            SequenceTransition::Cut
        );
        assert_eq!(scene.animatic.as_ref().unwrap().shots[0].transition, None);

        // A trim that consumes the whole shot is rejected.
        assert!(set_trim(scene, "shot", Some(5.0), Some(5.0)).is_err());
        assert!(set_trim(scene, "shot", Some(-1.0), None).is_err());
    }
}
