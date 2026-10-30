//! Visual-source strategies and production-derived continuity frames.
//!
//! A source strategy is approved during preproduction. Concrete continuity
//! frames are bound later, after a predecessor take is selected.
use crate::fs_ops;
use crate::scene_plan::handoff::CutId;
use crate::workflow::state::{
    AssetRecord, AssetRole, AssetStatus, ProjectState, TakeStatus, VisualSourceSpec,
    VisualSourceStrategy,
};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderSourceResolution {
    Unplanned,
    NoStartFrame,
    Bound(String),
}

pub fn resolve_render_source(
    state: &ProjectState,
    cut_id: CutId,
) -> Result<RenderSourceResolution, String> {
    let cut = state
        .scene(cut_id.source_scene)
        .and_then(|scene| scene.cuts.iter().find(|cut| cut.id == cut_id))
        .ok_or_else(|| {
            format!(
                "workflow cut {}.{} not found",
                cut_id.source_scene, cut_id.cut
            )
        })?;
    let Some(source) = &cut.visual_source else {
        return Ok(match &cut.approved_start_frame {
            Some(path) => RenderSourceResolution::Bound(path.clone()),
            None => RenderSourceResolution::Unplanned,
        });
    };
    if source.strategy == VisualSourceStrategy::NoStartFrame {
        return Ok(RenderSourceResolution::NoStartFrame);
    }
    if let Some(asset_id) = &source.asset_id {
        let asset = state.asset(asset_id).ok_or_else(|| {
            format!(
                "visual source `{asset_id}` for Cut {} is missing",
                cut_id.cut
            )
        })?;
        if asset.status != AssetStatus::Approved {
            return Err(format!(
                "visual source `{asset_id}` for Cut {} is not approved",
                cut_id.cut
            ));
        }
        return Ok(RenderSourceResolution::Bound(asset.path.clone()));
    }
    if source.strategy == VisualSourceStrategy::PreproductionStartFrame {
        if let Some(path) = &cut.approved_start_frame {
            return Ok(RenderSourceResolution::Bound(path.clone()));
        }
    }
    let detail = match source.strategy {
        VisualSourceStrategy::PreviousSelectedTakeExitFrame => source
            .predecessor
            .map(|previous| format!("waiting for selected exit frame from Cut {}", previous.cut))
            .unwrap_or_else(|| "predecessor is not set".into()),
        VisualSourceStrategy::SharedReference => "shared reference is not selected".into(),
        VisualSourceStrategy::DeferredManual => "manual source is not attached".into(),
        VisualSourceStrategy::PreproductionStartFrame => {
            "preproduction start frame is not approved".into()
        }
        VisualSourceStrategy::NoStartFrame => unreachable!(),
    };
    Err(format!(
        "Cut {} is planned but not render ready: {detail}",
        cut_id.cut
    ))
}

pub fn set_strategy(
    state: &mut ProjectState,
    scene_id: u32,
    cut_number: u32,
    strategy: VisualSourceStrategy,
    predecessor: Option<CutId>,
    asset_id: Option<&str>,
) -> Result<(), String> {
    let cut_id = CutId::new(scene_id, cut_number);
    let existing = state
        .scene(scene_id)
        .and_then(|scene| scene.cuts.iter().find(|cut| cut.id == cut_id))
        .ok_or_else(|| format!("cut {scene_id}.{cut_number} not found"))?
        .visual_source
        .clone();

    let mut source = VisualSourceSpec::new(strategy);
    match strategy {
        VisualSourceStrategy::PreproductionStartFrame => {
            let cut = state
                .scene(scene_id)
                .and_then(|scene| scene.cuts.iter().find(|cut| cut.id == cut_id))
                .expect("cut checked above");
            if let Some(path) = &cut.approved_start_frame {
                source.asset_id = state
                    .assets
                    .iter()
                    .find(|asset| {
                        asset.cut_id == Some(cut_id)
                            && asset.path == *path
                            && asset.status == AssetStatus::Approved
                    })
                    .map(|asset| asset.id.clone());
            }
        }
        VisualSourceStrategy::PreviousSelectedTakeExitFrame => {
            let predecessor = predecessor.ok_or_else(|| {
                "previous-selected-take strategy needs a predecessor cut".to_string()
            })?;
            if predecessor.source_scene != scene_id {
                return Err("the predecessor must be in the same scene for this slice".into());
            }
            if predecessor.cut >= cut_number {
                return Err("the predecessor must be an earlier cut".into());
            }
            let exists = state
                .scene(scene_id)
                .is_some_and(|scene| scene.cuts.iter().any(|cut| cut.id == predecessor));
            if !exists {
                return Err(format!(
                    "predecessor cut {}.{} not found",
                    predecessor.source_scene, predecessor.cut
                ));
            }
            source.predecessor = Some(predecessor);
            source.predecessor_shot_id = state
                .scene(scene_id)
                .and_then(|scene| scene.cuts.iter().find(|cut| cut.id == predecessor))
                .map(|cut| cut.stable_id.clone())
                .filter(|id| !id.is_empty());
            if let Some(existing) = existing.filter(|existing| {
                existing.strategy == strategy && existing.predecessor == Some(predecessor)
            }) {
                source.asset_id = existing.asset_id;
                source.source_take_id = existing.source_take_id;
                source.frame_time_seconds = existing.frame_time_seconds;
                if source.predecessor_shot_id.is_none() {
                    source.predecessor_shot_id = existing.predecessor_shot_id;
                }
            }
        }
        VisualSourceStrategy::SharedReference => {
            if let Some(asset_id) = asset_id.filter(|value| !value.trim().is_empty()) {
                let asset = state
                    .asset(asset_id)
                    .filter(|asset| {
                        asset.scene_id == Some(scene_id)
                            && asset.status == AssetStatus::Approved
                            && asset.role.is_scene_look()
                    })
                    .ok_or_else(|| {
                        format!(
                            "shared reference `{asset_id}` must be an approved scene-look asset"
                        )
                    })?;
                source.asset_id = Some(asset.id.clone());
            }
        }
        VisualSourceStrategy::NoStartFrame => {}
        VisualSourceStrategy::DeferredManual => {
            if let Some(existing) = existing.filter(|existing| existing.strategy == strategy) {
                source.asset_id = existing.asset_id;
                source.source_take_id = existing.source_take_id;
                source.frame_time_seconds = existing.frame_time_seconds;
            }
        }
    }

    let cut = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .and_then(|scene| scene.cuts.iter_mut().find(|cut| cut.id == cut_id))
        .ok_or_else(|| format!("cut {scene_id}.{cut_number} not found"))?;
    cut.visual_source = Some(source);
    Ok(())
}

pub fn bind_selected_exit_frame(
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    cut_number: u32,
    frame_time: Option<f64>,
) -> Result<String, String> {
    let cut_id = CutId::new(scene_id, cut_number);
    let source = state
        .scene(scene_id)
        .and_then(|scene| scene.cuts.iter().find(|cut| cut.id == cut_id))
        .and_then(|cut| cut.visual_source.clone())
        .ok_or_else(|| "choose a visual-source strategy first".to_string())?;
    if source.strategy != VisualSourceStrategy::PreviousSelectedTakeExitFrame {
        return Err("this cut does not use a previous selected take".into());
    }
    let predecessor_shot_id = source.predecessor_shot_id.clone();
    let predecessor = source.predecessor;
    let previous = if let Some(shot_id) = predecessor_shot_id
        .as_deref()
        .filter(|shot_id| !shot_id.is_empty())
    {
        state
            .scene(scene_id)
            .and_then(|scene| scene.cuts.iter().find(|cut| cut.stable_id == shot_id))
            .ok_or_else(|| format!("predecessor shot `{shot_id}` not found"))?
    } else {
        let predecessor =
            predecessor.ok_or_else(|| "the predecessor cut is missing".to_string())?;
        state
            .scene(predecessor.source_scene)
            .and_then(|scene| scene.cuts.iter().find(|cut| cut.id == predecessor))
            .ok_or_else(|| {
                format!(
                    "predecessor cut {}.{} not found",
                    predecessor.source_scene, predecessor.cut
                )
            })?
    };
    let predecessor = previous.id;
    let resolved_shot_id = previous.stable_id.clone();
    let selected = previous.selected_take.as_deref().ok_or_else(|| {
        format!(
            "select a take for Cut {} before binding this continuity frame",
            predecessor.cut
        )
    })?;
    let take = previous
        .takes
        .iter()
        .find(|take| take.id == selected || take.path == selected);
    let take_asset = state.asset(selected).filter(|asset| {
        asset.role == AssetRole::VideoTake || asset.role == AssetRole::SelectedTake
    });
    let (take_id, take_path, parent_asset_id) = if let Some(take) = take {
        (
            take.id.clone(),
            take.path.clone(),
            state
                .assets
                .iter()
                .find(|asset| {
                    asset.path == take.path
                        && (asset.role == AssetRole::VideoTake
                            || asset.role == AssetRole::SelectedTake)
                })
                .map(|asset| asset.id.clone()),
        )
    } else if let Some(asset) = take_asset {
        (asset.id.clone(), asset.path.clone(), Some(asset.id.clone()))
    } else {
        return Err(format!(
            "selected take `{selected}` is not registered on Cut {}",
            predecessor.cut
        ));
    };
    let video = resolve(root, &take_path)?;
    if !video.is_file() {
        return Err(format!(
            "selected take file is missing: {}",
            video.display()
        ));
    }
    let chosen = match frame_time {
        Some(value) if value.is_finite() && value >= 0.0 => value,
        Some(_) => {
            return Err("frame time must be a non-negative number of seconds".into());
        }
        None => {
            let duration = fs_ops::probe_duration_seconds(
                video
                    .to_str()
                    .ok_or_else(|| "selected take path is not valid UTF-8".to_string())?,
            )?;
            (duration - 0.1).max(0.0)
        }
    };
    let previous_asset = state
        .scene(scene_id)
        .and_then(|scene| scene.cuts.iter().find(|cut| cut.id == cut_id))
        .and_then(|cut| cut.visual_source.as_ref())
        .and_then(|source| source.asset_id.clone());
    let rel = next_continuity_rel(root, state, scene_id, cut_number, predecessor.cut);
    let root_str = root.to_string_lossy().into_owned();
    // Confine destination BEFORE create_dir_all: resolve against the
    // project root and enforce the exact Preproduction/SceneN/continuity_frames bucket.
    let output_confined = crate::project_path::resolve(&rel, Some(&root_str), crate::project_path::PathIntent::Write)
        .map(|resolved| resolved.into_path_buf())
        .map_err(|error| format!("continuity-frame destination outside project: {error}"))?;
    crate::project_path::under_scene_bucket(root, &output_confined, "continuity_frames")
        .map_err(|error| format!("continuity-frame destination outside continuity_frames bucket: {error}"))?;
    if crate::project_path::relative_to_root(root, &output_confined)?.parent()
        != Path::new(&rel).parent()
    {
        return Err("continuity-frame destination is outside this scene continuity_frames bucket".into());
    }
    if let Some(parent) = output_confined.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create continuity-frame folder: {error}"))?;
    }
    // Re-confine AFTER create_dir_all — fs::copy follows parent chain,
    // so the final destination must still be inside the exact continuity_frames bucket.
    let output_again = crate::project_path::resolve(&rel, Some(&root_str), crate::project_path::PathIntent::Write)
        .map(|resolved| resolved.into_path_buf())
        .map_err(|error| format!("continuity-frame destination outside project: {error}"))?;
    crate::project_path::under_scene_bucket(root, &output_again, "continuity_frames")
        .map_err(|error| format!("continuity-frame destination outside continuity_frames bucket: {error}"))?;
    if crate::project_path::relative_to_root(root, &output_again)?.parent()
        != Path::new(&rel).parent()
    {
        return Err("continuity-frame destination is outside this scene continuity_frames bucket".into());
    }
    fs_ops::extract_frame_at(
        video.to_string_lossy().to_string(),
        Some(output_again.to_string_lossy().to_string()),
        Some(chosen),
    )?;
    let asset_id = state.push_asset(AssetRecord {
        id: String::new(),
        path: rel.clone(),
        role: AssetRole::ContinuityFrame,
        status: AssetStatus::Approved,
        scene_id: Some(scene_id),
        cut_id: Some(cut_id),
        character_id: None,
        source_job_id: None,
        parent_asset_id,
        notes: Some(format!(
            "selected exit frame from Scene {} Cut {} take {} at {chosen:.3}s",
            predecessor.source_scene, predecessor.cut, take_id
        )),
    });
    crate::workflow::identity::record_version(state, &asset_id, AssetStatus::Approved);
    let cut = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .and_then(|scene| scene.cuts.iter_mut().find(|cut| cut.id == cut_id))
        .ok_or_else(|| format!("cut {scene_id}.{cut_number} not found"))?;
    let source = cut
        .visual_source
        .as_mut()
        .ok_or_else(|| "visual-source strategy disappeared".to_string())?;
    source.asset_id = Some(asset_id.clone());
    source.source_take_id = Some(take_id);
    source.frame_time_seconds = Some(chosen);
    source.predecessor = Some(predecessor);
    if source
        .predecessor_shot_id
        .as_deref()
        .unwrap_or("")
        .is_empty()
        && !resolved_shot_id.is_empty()
    {
        source.predecessor_shot_id = Some(resolved_shot_id);
    }
    if let Some(old) = previous_asset.filter(|old| old != &asset_id) {
        crate::workflow::identity::mark_asset_dependents(state, scene_id, &old);
        crate::workflow::identity::mark_upstream_changed(
            state,
            scene_id,
            cut_number,
            std::slice::from_ref(&old),
        );
    }
    Ok(rel)
}

/// Shots whose bound continuity frame comes from this cut. Replacing the
/// predecessor selection clears those bindings; it does not approve a new frame.
pub fn continuity_dependents(state: &ProjectState, scene_id: u32, cut_number: u32) -> Vec<String> {
    let cut_id = CutId::new(scene_id, cut_number);
    let stable = state
        .scene(scene_id)
        .and_then(|scene| scene.cuts.iter().find(|cut| cut.id == cut_id))
        .map(|cut| cut.stable_id.clone())
        .unwrap_or_default();
    let mut labels = Vec::new();
    for scene in &state.scenes {
        for cut in &scene.cuts {
            let Some(source) = cut.visual_source.as_ref() else {
                continue;
            };
            if source.strategy != VisualSourceStrategy::PreviousSelectedTakeExitFrame
                || source.asset_id.is_none()
            {
                continue;
            }
            let linked = source.predecessor == Some(cut_id)
                || (!stable.is_empty()
                    && source.predecessor_shot_id.as_deref() == Some(stable.as_str()));
            if linked {
                labels.push(format!("Scene {} Cut {}", scene.id, cut.id.cut));
            }
        }
    }
    labels
}

pub fn invalidate_dependents(
    state: &mut ProjectState,
    predecessor: CutId,
    selected_take_id: &str,
) -> usize {
    let mut changed = 0;
    for scene in &mut state.scenes {
        for cut in &mut scene.cuts {
            let Some(source) = &mut cut.visual_source else {
                continue;
            };
            if source.strategy != VisualSourceStrategy::PreviousSelectedTakeExitFrame
                || source.predecessor != Some(predecessor)
                || source.source_take_id.as_deref() == Some(selected_take_id)
            {
                continue;
            }
            source.asset_id = None;
            source.source_take_id = None;
            source.frame_time_seconds = None;
            changed += 1;
        }
    }
    changed
}

pub fn set_take_status(
    state: &mut ProjectState,
    scene_id: u32,
    cut_number: u32,
    take_id: &str,
    status: TakeStatus,
) -> Result<usize, String> {
    let cut_id = CutId::new(scene_id, cut_number);
    let cut = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .and_then(|scene| scene.cuts.iter_mut().find(|cut| cut.id == cut_id))
        .ok_or_else(|| format!("cut {scene_id}.{cut_number} not found"))?;
    if !cut.takes.iter().any(|take| take.id == take_id) {
        return Err(format!("take `{take_id}` not found on Cut {cut_number}"));
    }
    if status == TakeStatus::Selected {
        for take in &mut cut.takes {
            if take.status == TakeStatus::Selected && take.id != take_id {
                take.status = TakeStatus::Alternate;
            }
        }
    }
    let take = cut
        .takes
        .iter_mut()
        .find(|take| take.id == take_id)
        .expect("take checked above");
    take.status = status;
    let selected = if status == TakeStatus::Selected {
        cut.selected_take = Some(take_id.to_string());
        Some(take_id.to_string())
    } else if cut.selected_take.as_deref() == Some(take_id) {
        cut.selected_take = None;
        None
    } else {
        cut.selected_take.clone()
    };
    let selected = selected.unwrap_or_default();
    Ok(invalidate_dependents(state, cut_id, &selected))
}

pub fn delete_take(
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    cut_number: u32,
    take_id: &str,
) -> Result<String, String> {
    let cut_id = CutId::new(scene_id, cut_number);
    let (path, was_selected) = state
        .scene(scene_id)
        .and_then(|scene| scene.cuts.iter().find(|cut| cut.id == cut_id))
        .and_then(|cut| {
            cut.takes
                .iter()
                .find(|take| take.id == take_id)
                .map(|take| {
                    (
                        take.path.clone(),
                        cut.selected_take.as_deref() == Some(take_id),
                    )
                })
        })
        .ok_or_else(|| format!("take `{take_id}` not found on Cut {cut_number}"))?;
    let file = resolve(root, &path)?;
    crate::project_path::under_cut_dir(root, &file, scene_id, cut_number)?;
    if file.is_file() {
        crate::project_path::move_to_trash(root, &file)?;
    }
    if let Some(cut) = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .and_then(|scene| scene.cuts.iter_mut().find(|cut| cut.id == cut_id))
    {
        cut.takes.retain(|take| take.id != take_id);
        if was_selected {
            cut.selected_take = None;
        }
    }
    state.assets.retain(|asset| asset.id != take_id);
    for asset in &mut state.assets {
        if asset.parent_asset_id.as_deref() == Some(take_id) {
            asset.parent_asset_id = None;
        }
    }
    for job in &mut state.jobs {
        job.input_asset_ids.retain(|id| id != take_id);
        job.output_asset_ids.retain(|id| id != take_id);
    }
    if was_selected {
        invalidate_dependents(state, cut_id, "");
    }
    Ok(path)
}

fn next_continuity_rel(
    root: &Path,
    state: &ProjectState,
    scene_id: u32,
    cut_number: u32,
    predecessor_cut: u32,
) -> String {
    for version in 1..=999 {
        let rel = format!(
            "Preproduction/Scene{scene_id}/continuity_frames/cut_{cut_number}_from_cut_{predecessor_cut}_v{version}.png"
        );
        if !root.join(&rel).exists() && !state.assets.iter().any(|asset| asset.path == rel) {
            return rel;
        }
    }
    format!(
        "Preproduction/Scene{scene_id}/continuity_frames/cut_{cut_number}_from_cut_{predecessor_cut}_v1000.png"
    )
}

fn resolve(root: &Path, value: &str) -> Result<PathBuf, String> {
    crate::project_path::resolve(
        value,
        Some(root.to_string_lossy().as_ref()),
        crate::project_path::PathIntent::Write,
    )
    .map(|path| path.into_path_buf())
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::state::{CutRecord, TakeRecord};

    #[test]
    fn previous_take_strategy_is_planned_but_unbound() {
        let mut state = ProjectState::new();
        let scene = state.scene_mut(1, "Scene 1");
        scene.cuts.push(CutRecord::new(CutId::new(1, 1)));
        scene.cuts.push(CutRecord::new(CutId::new(1, 2)));
        set_strategy(
            &mut state,
            1,
            2,
            VisualSourceStrategy::PreviousSelectedTakeExitFrame,
            Some(CutId::new(1, 1)),
            None,
        )
        .unwrap();
        let cut = &state.scenes[0].cuts[1];
        assert!(cut.visual_source_planned());
        assert!(!cut.visual_source_ready());
        assert_eq!(
            cut.visual_source.as_ref().unwrap().predecessor,
            Some(CutId::new(1, 1))
        );
    }

    #[test]
    fn no_start_frame_is_immediately_ready() {
        let mut state = ProjectState::new();
        state
            .scene_mut(1, "Scene 1")
            .cuts
            .push(CutRecord::new(CutId::new(1, 1)));
        set_strategy(
            &mut state,
            1,
            1,
            VisualSourceStrategy::NoStartFrame,
            None,
            None,
        )
        .unwrap();
        assert!(state.scenes[0].cuts[0].visual_source_ready());
    }

    #[test]
    fn deleting_take_removes_file_registry_and_selection() {
        let dir = tempfile::tempdir().unwrap();
        let rel = "Scenes/Scene1/Cut1/S01C01.mp4";
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"video").unwrap();
        let mut state = ProjectState::new();
        let cut = state.scene_mut(1, "Scene 1").cut_mut(1);
        cut.takes.push(TakeRecord {
            id: "asset_1".into(),
            path: rel.into(),
            status: TakeStatus::Selected,
            notes: None,
        });
        cut.selected_take = Some("asset_1".into());
        state.assets.push(AssetRecord {
            id: "asset_1".into(),
            path: rel.into(),
            role: AssetRole::VideoTake,
            status: AssetStatus::Present,
            scene_id: Some(1),
            cut_id: Some(CutId::new(1, 1)),
            character_id: None,
            source_job_id: None,
            parent_asset_id: None,
            notes: None,
        });

        assert_eq!(
            delete_take(dir.path(), &mut state, 1, 1, "asset_1").unwrap(),
            rel
        );
        assert!(!path.exists());
        assert!(state.assets.is_empty());
        assert!(state.scenes[0].cuts[0].takes.is_empty());
        assert!(state.scenes[0].cuts[0].selected_take.is_none());
    }

    #[test]
    fn changing_the_predecessor_selection_unbinds_without_approving_a_replacement() {
        let mut state = ProjectState::new();
        let first = state.scene_mut(1, "Scene 1").cut_mut(1);
        first.stable_id = "shot_1".into();
        first.takes.push(TakeRecord {
            id: "take_a".into(),
            path: "a.mp4".into(),
            status: TakeStatus::Selected,
            notes: None,
        });
        first.takes.push(TakeRecord {
            id: "take_b".into(),
            path: "b.mp4".into(),
            status: TakeStatus::Alternate,
            notes: None,
        });
        first.selected_take = Some("take_a".into());
        let second = state.scene_mut(1, "Scene 1").cut_mut(2);
        second.stable_id = "shot_2".into();
        let mut source = VisualSourceSpec::new(VisualSourceStrategy::PreviousSelectedTakeExitFrame);
        source.predecessor = Some(CutId::new(1, 1));
        source.predecessor_shot_id = Some("shot_1".into());
        source.asset_id = Some("frame_1".into());
        source.source_take_id = Some("take_a".into());
        source.frame_time_seconds = Some(1.25);
        second.visual_source = Some(source);
        state.assets.push(AssetRecord {
            id: "frame_1".into(),
            path: "Preproduction/Scene1/continuity_frames/old.png".into(),
            role: AssetRole::ContinuityFrame,
            status: AssetStatus::Approved,
            scene_id: Some(1),
            cut_id: Some(CutId::new(1, 2)),
            character_id: None,
            source_job_id: None,
            parent_asset_id: Some("take_a".into()),
            notes: None,
        });

        let invalidated =
            set_take_status(&mut state, 1, 1, "take_b", TakeStatus::Selected).unwrap();
        assert_eq!(invalidated, 1);
        let bound = state.scenes[0].cuts[1].visual_source.as_ref().unwrap();
        assert!(bound.asset_id.is_none());
        assert!(bound.source_take_id.is_none());
        assert!(bound.frame_time_seconds.is_none());
        assert!(!state.scenes[0].cuts[1].visual_source_ready());
        assert_eq!(state.assets.len(), 1);
        assert_eq!(continuity_dependents(&state, 1, 1), Vec::<String>::new());
    }

    #[test]
    fn binding_persists_the_chosen_frame_time() {
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("Scenes/Scene1/Cut1/take.mp4");
        fs::create_dir_all(video.parent().unwrap()).unwrap();
        let status = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=16x16:r=25",
                "-t",
                "2",
                video.to_str().unwrap(),
            ])
            .output();
        let Ok(status) = status else {
            panic!("ffmpeg is required to bind a reviewed continuity frame");
        };
        assert!(
            status.status.success(),
            "ffmpeg could not write a test clip: {}",
            String::from_utf8_lossy(&status.stderr)
        );
        let mut state = ProjectState::new();
        let first = state.scene_mut(1, "Scene 1").cut_mut(1);
        first.stable_id = "shot_1".into();
        first.takes.push(TakeRecord {
            id: "take_a".into(),
            path: "Scenes/Scene1/Cut1/take.mp4".into(),
            status: TakeStatus::Selected,
            notes: None,
        });
        first.selected_take = Some("take_a".into());
        state.assets.push(AssetRecord {
            id: "take_a".into(),
            path: "Scenes/Scene1/Cut1/take.mp4".into(),
            role: AssetRole::VideoTake,
            status: AssetStatus::Present,
            scene_id: Some(1),
            cut_id: Some(CutId::new(1, 1)),
            character_id: None,
            source_job_id: None,
            parent_asset_id: None,
            notes: None,
        });
        let second = state.scene_mut(1, "Scene 1").cut_mut(2);
        second.stable_id = "shot_2".into();
        let mut source = VisualSourceSpec::new(VisualSourceStrategy::PreviousSelectedTakeExitFrame);
        source.predecessor = Some(CutId::new(1, 1));
        source.predecessor_shot_id = Some("shot_1".into());
        second.visual_source = Some(source);

        let path = bind_selected_exit_frame(dir.path(), &mut state, 1, 2, Some(0.4)).unwrap();
        assert!(dir.path().join(&path).is_file());
        let bound = state.scenes[0].cuts[1].visual_source.as_ref().unwrap();
        assert_eq!(bound.frame_time_seconds, Some(0.4));
        assert_eq!(bound.source_take_id.as_deref(), Some("take_a"));
        let asset = state.asset(bound.asset_id.as_deref().unwrap()).unwrap();
        assert_eq!(asset.parent_asset_id.as_deref(), Some("take_a"));
        assert_eq!(asset.role, AssetRole::ContinuityFrame);
        assert!(state
            .versions
            .iter()
            .any(|version| version.asset_id == asset.id && version.path == path));
        assert!(state.scenes[0].cuts[1].visual_source_ready());
    }

}
