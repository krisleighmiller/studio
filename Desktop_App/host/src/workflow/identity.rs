//! Durable ids for scenes and shots. Cut numbers stay display labels.
//!
//! Reordering renumbers labels and leaves takes and files where they are.
//! Import matches a saved shot to the production note that still has its path,
//! then restores the saved label order.
use std::collections::{HashMap, HashSet};

use crate::scene_plan::handoff::CutId;
use crate::workflow::state::{
    ArtifactVersion, AssetStatus, CutRecord, DependencyState, ProjectState, SceneRecord,
};

/// How references that store a cut number get updated after a renumber.
#[derive(Clone, Copy)]
enum Relink {
    /// Animatic shots and assets still use the labels from before this reorder.
    Live,
    /// Saved labels already match the restored order. Leave those references.
    LabelsOnly,
}

/// Add one shot with a new stable id and the next display number.
/// Existing shot ids and cut numbers stay put. The sketch is editable and
/// the source strategy and production note stay empty.
pub fn add_alternate_shot(state: &mut ProjectState, scene_id: u32) -> Result<String, String> {
    let label = {
        let scene = state
            .scenes
            .iter()
            .find(|scene| scene.id == scene_id)
            .ok_or_else(|| format!("Scene {scene_id} not found"))?;
        scene.cuts.iter().map(|cut| cut.id.cut).max().unwrap_or(0) + 1
    };
    let kept: Vec<String> = state
        .scene(scene_id)
        .map(|scene| scene.cuts.iter().map(|cut| cut.stable_id.clone()).collect())
        .unwrap_or_default();
    let scene = state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .ok_or_else(|| format!("Scene {scene_id} not found"))?;
    let mut cut = CutRecord::new(CutId::new(scene_id, label));
    cut.title = Some(format!("Alternate {label}"));
    cut.sketch = Some(crate::workflow::state::CutSketch {
        title: Some(format!("Alternate {label}")),
        visual_prompt: Some(String::new()),
        ..crate::workflow::state::CutSketch::default()
    });
    scene.cuts.push(cut);
    assign(state);
    let stable = state
        .scene(scene_id)
        .and_then(|scene| scene.cuts.iter().find(|cut| cut.id.cut == label))
        .map(|cut| cut.stable_id.clone())
        .unwrap_or_default();
    if stable.is_empty() {
        return Err("could not allocate a stable shot id".into());
    }
    let scene = state
        .scene(scene_id)
        .ok_or_else(|| format!("Scene {scene_id} not found"))?;
    if scene
        .cuts
        .iter()
        .filter(|cut| cut.id.cut != label)
        .any(|cut| !kept.iter().any(|id| id == &cut.stable_id))
    {
        return Err("adding an alternate angle renumbered an existing shot id".into());
    }
    crate::workflow::animatic::sync(state);
    Ok(stable)
}

/// Remove only a sketch-stage cut, never a source-backed or produced shot.
/// A stable id selects the target; survivors are renumbered through the same
/// relinking path as manual reorder. Nothing on disk is removed.
fn removable_planning_cut(state: &ProjectState, scene_id: u32, shot_id: &str, allow_approved: bool) -> Result<CutId, String> {
    let scene = state.scene(scene_id).ok_or_else(|| format!("scene {scene_id} not found"))?;
    let cut = scene.cuts.iter().find(|cut| !shot_id.is_empty() && cut.stable_id == shot_id)
        .ok_or_else(|| format!("shot `{shot_id}` is not in scene {scene_id}"))?;
    let cut_id = cut.id;
    let label = cut.id.cut;
    let path_fragment = format!("Scene{scene_id}/Cut{label}/");
    if cut.source_origin.is_some() || cut.has_production_note() || cut.visual_source.is_some()
        || cut.approved_start_frame.is_some() || !cut.identity_ref_paths.is_empty()
        || cut.start_frame_prompt.is_some() || cut.start_frame_negative_prompt.is_some()
        || !cut.takes.is_empty() || cut.selected_take.is_some()
        || scene.post.is_some()
        || scene.animatic.as_ref().is_some_and(|animatic| (animatic.approved && !allow_approved)
            || animatic.shots.iter().any(|shot| shot.cut_id == cut_id && shot.scratch_audio_path.is_some()))
        || state.assets.iter().any(|asset| asset.cut_id == Some(cut_id))
        || state.versions.iter().any(|version| version.path.contains(&path_fragment))
        || state.jobs.iter().any(|job| {
            job.scope.as_ref().is_some_and(|scope| scope.kind == "shot"
                && (scope.id == shot_id || scope.id == format!("Scene{scene_id}/Cut{label}")))
                || job.output_path.as_ref().is_some_and(|path| path.contains(&path_fragment))
                || job.assembly_snapshot.as_ref().is_some_and(|shots| shots.iter().any(|shot| shot.shot_id == shot_id))
        })
    {
        return Err(format!("Cut {label} has source, media, production, or approved work. Nothing was removed."));
    }
    if state.scenes.iter().flat_map(|scene| &scene.cuts).any(|other| other.id != cut_id
        && other.visual_source.as_ref().is_some_and(|source|
            source.predecessor == Some(cut_id)
                || source.predecessor_shot_id.as_deref() == Some(shot_id)))
    {
        return Err(format!("Cut {label} is used as another shot's continuity source. Nothing was removed."));
    }
    Ok(cut_id)
}

/// Reopen the scene's sketch approval only when this exact cut is otherwise
/// removable. No media, production data, jobs, or rendered review is changed.
pub fn reopen_planning_sketch(state: &mut ProjectState, scene_id: u32, shot_id: &str) -> Result<(), String> {
    removable_planning_cut(state, scene_id, shot_id, true)?;
    let scene = state.scenes.iter_mut().find(|scene| scene.id == scene_id).expect("validated scene");
    let animatic = scene.animatic.as_mut().filter(|animatic| animatic.approved)
        .ok_or("This sketch animatic is not approved; no reopen is needed.")?;
    animatic.approved = false;
    Ok(())
}

/// Remove only a sketch-stage cut. Nothing on disk is removed.
pub fn remove_planning_cut(state: &mut ProjectState, scene_id: u32, shot_id: &str) -> Result<(), String> {
    let cut_id = removable_planning_cut(state, scene_id, shot_id, false)?;
    let scene = state.scene(scene_id).expect("validated scene");
    let surviving_ids: Vec<String> = scene.cuts.iter().filter(|other| other.stable_id != shot_id)
        .map(|other| other.stable_id.clone()).collect();
    if surviving_ids.iter().any(String::is_empty) {
        return Err("A surviving cut has no stable ID; refresh the project before removing cuts.".into());
    }
    let scene = state.scenes.iter_mut().find(|scene| scene.id == scene_id).expect("checked above");
    scene.cuts.retain(|other| other.stable_id != shot_id);
    if let Some(animatic) = scene.animatic.as_mut() {
        animatic.shots.retain(|shot| shot.cut_id != cut_id);
    }
    if surviving_ids.is_empty() {
        scene.animatic = None;
    } else {
        reorder_cuts(state, scene_id, &surviving_ids)?;
        crate::workflow::animatic::sync(state);
    }
    Ok(())
}

pub fn assign(state: &mut ProjectState) -> bool {
    let mut next_scene = next_numeric(state, true);
    let mut next_shot = next_numeric(state, false);
    let mut changed = false;
    for scene in &mut state.scenes {
        if scene.stable_id.is_empty() {
            scene.stable_id = format!("scene_{next_scene}");
            next_scene += 1;
            changed = true;
        }
        for cut in &mut scene.cuts {
            if cut.stable_id.is_empty() {
                cut.stable_id = format!("shot_{next_shot}");
                next_shot += 1;
                changed = true;
            }
        }
    }
    changed
}

pub fn reorder_cuts(
    state: &mut ProjectState,
    scene_id: u32,
    ordered_stable_ids: &[String],
) -> Result<(), String> {
    permute(
        state,
        scene_id,
        ordered_stable_ids,
        Relink::Live,
        &HashSet::new(),
    )
}

/// Attach saved stable ids to the cuts that still have the same source,
/// move takes onto those cuts, and restore the saved label order.
///
/// Correspondence is one-to-one by (production path, source_origin). Legacy
/// originless records key on a unique production path or the display label pair.
/// Shared production paths without a captured source origin cannot be mapped and
/// return an explicit ambiguity error instead of guessing.
pub fn adopt(state: &mut ProjectState, previous: &ProjectState) -> Result<(), String> {
    let kept_assets: HashSet<String> = previous
        .assets
        .iter()
        .map(|asset| asset.id.clone())
        .collect();
    for old_scene in &previous.scenes {
        let Some(index) = state
            .scenes
            .iter()
            .position(|scene| scene.id == old_scene.id)
        else {
            continue;
        };
        if state.scenes[index].stable_id.is_empty() && !old_scene.stable_id.is_empty() {
            state.scenes[index].stable_id = old_scene.stable_id.clone();
        }
        // Enforce bijection: no two old cuts may claim the same destination cut.
        let mut claimed_dests: HashSet<usize> = HashSet::new();
        for old_cut in &old_scene.cuts {
            match correspond_cut(&state.scenes[index], old_cut) {
                Ok(Some(dest)) => {
                    if !claimed_dests.insert(dest) {
                        return Err(format!(
                            "duplicate saved source origin mapping: two old cuts claim destination cut {} in scene {}",
                            dest, old_scene.id
                        ));
                    }
                }
                Ok(None) => {}
                Err(message) => return Err(message),
            }
        }
        for old_cut in &old_scene.cuts {
            let dest = match correspond_cut(&state.scenes[index], old_cut) {
                Ok(Some(index)) => index,
                Ok(None) => continue,
                Err(message) => return Err(message),
            };
            let cut = &mut state.scenes[index].cuts[dest];
            if cut.stable_id.is_empty() && !old_cut.stable_id.is_empty() {
                cut.stable_id = old_cut.stable_id.clone();
            }
            if cut.dependency_state.is_none() {
                cut.dependency_state = old_cut.dependency_state;
            }
        }
        for old_cut in &old_scene.cuts {
            let dest = match correspond_cut(&state.scenes[index], old_cut) {
                Ok(Some(index)) => index,
                Ok(None) => continue,
                Err(message) => return Err(message),
            };
            claim_takes(&mut state.scenes[index], dest, old_cut);
        }
        let order = saved_order(&state.scenes[index], old_scene);
        if order.len() == state.scenes[index].cuts.len() && !order.is_empty() {
            let _ = permute(
                state,
                old_scene.id,
                &order,
                Relink::LabelsOnly,
                &kept_assets,
            );
        }
    }
    Ok(())
}

pub fn record_version(state: &mut ProjectState, asset_id: &str, status: AssetStatus) {
    if let Some(existing) = state
        .versions
        .iter_mut()
        .find(|version| version.asset_id == asset_id)
    {
        existing.status = status;
        return;
    }
    let Some(path) = state.asset(asset_id).map(|asset| asset.path.clone()) else {
        return;
    };
    let next = state
        .versions
        .iter()
        .filter_map(|version| version.id.strip_prefix("ver_")?.parse::<u32>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    state.versions.push(ArtifactVersion {
        id: format!("ver_{next}"),
        asset_id: asset_id.to_string(),
        version: next,
        path,
        status,
    });
}

/// Mark shots that still point at an approved artifact which was just replaced.
/// Takes and files are left in place.
pub fn mark_upstream_changed(
    state: &mut ProjectState,
    scene_id: u32,
    cut_number: u32,
    superseded_asset_ids: &[String],
) {
    let cut_id = CutId::new(scene_id, cut_number);
    let stable = state
        .scene(scene_id)
        .and_then(|scene| scene.cuts.iter().find(|cut| cut.id.cut == cut_number))
        .map(|cut| cut.stable_id.clone())
        .unwrap_or_default();
    for cut in state.scenes.iter_mut().flat_map(|scene| &mut scene.cuts) {
        if cut.id == cut_id {
            if !cut.takes.is_empty() {
                cut.dependency_state = Some(DependencyState::NeedsReview);
            }
            continue;
        }
        let linked = cut.visual_source.as_ref().is_some_and(|source| {
            source.predecessor == Some(cut_id)
                || (!stable.is_empty()
                    && source.predecessor_shot_id.as_deref() == Some(stable.as_str()))
                || source
                    .asset_id
                    .as_ref()
                    .is_some_and(|id| superseded_asset_ids.iter().any(|old| old == id))
        });
        if linked {
            cut.dependency_state = Some(DependencyState::NeedsReview);
        }
    }
}

pub fn mark_asset_dependents(state: &mut ProjectState, scene_id: u32, asset_id: &str) {
    let Some(scene) = state.scenes.iter_mut().find(|scene| scene.id == scene_id) else {
        return;
    };
    for cut in &mut scene.cuts {
        let linked = cut
            .visual_source
            .as_ref()
            .and_then(|source| source.asset_id.as_deref())
            == Some(asset_id);
        if linked {
            cut.dependency_state = Some(DependencyState::NeedsReview);
        }
    }
}

fn permute(
    state: &mut ProjectState,
    scene_id: u32,
    ordered_stable_ids: &[String],
    relink: Relink,
    keep_asset_ids: &HashSet<String>,
) -> Result<(), String> {
    let scene_index = state
        .scenes
        .iter()
        .position(|scene| scene.id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} not found"))?;
    if ordered_stable_ids.len() != state.scenes[scene_index].cuts.len()
        || ordered_stable_ids.iter().collect::<HashSet<_>>().len() != ordered_stable_ids.len()
    {
        return Err("Reorder must list every shot once.".into());
    }
    let stable_for_old = {
        let scene = &mut state.scenes[scene_index];
        if matches!(relink, Relink::Live) {
            backfill_predecessor_shot_ids(scene);
        }
        let stable_for_old: HashMap<CutId, String> = scene
            .cuts
            .iter()
            .map(|cut| (cut.id, cut.stable_id.clone()))
            .collect();
        let mut remaining = std::mem::take(&mut scene.cuts);
        let mut ordered = Vec::with_capacity(ordered_stable_ids.len());
        for id in ordered_stable_ids {
            let index = remaining
                .iter()
                .position(|cut| &cut.stable_id == id)
                .ok_or_else(|| format!("shot `{id}` is not in scene {scene_id}"))?;
            ordered.push(remaining.remove(index));
        }
        for (index, cut) in ordered.iter_mut().enumerate() {
            cut.id.cut = index as u32 + 1;
        }
        scene.cuts = ordered;
        retarget_predecessors(scene);
        if matches!(relink, Relink::Live) {
            retarget_animatic(scene, &stable_for_old);
        }
        stable_for_old
    };
    if matches!(relink, Relink::Live) {
        retarget_assets(state, scene_id, &stable_for_old, keep_asset_ids);
    }
    let labels: HashMap<String, CutId> = state.scenes[scene_index].cuts.iter()
        .filter(|cut| !cut.stable_id.is_empty())
        .map(|cut| (cut.stable_id.clone(), cut.id)).collect();
    let final_cut = state.scenes[scene_index].cuts.iter().map(|cut| cut.id.cut).max();
    for scene in &mut state.scenes {
        if scene.id == scene_id { continue; }
        for cut in &mut scene.cuts {
            if let Some(source) = cut.visual_source.as_mut() {
                if source.predecessor.is_some_and(|id| id.source_scene == scene_id) {
                    if let Some(id) = source.predecessor_shot_id.as_ref().and_then(|stable| labels.get(stable)) {
                        source.predecessor = Some(*id);
                        if Some(scene.id) == scene_id.checked_add(1) && cut.id.cut == 1 && final_cut != Some(id.cut) {
                            source.asset_id = None;
                            source.source_take_id = None;
                            source.frame_time_seconds = None;
                            cut.dependency_state = Some(DependencyState::NeedsReview);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn backfill_predecessor_shot_ids(scene: &mut SceneRecord) {
    let labels: Vec<(CutId, String)> = scene
        .cuts
        .iter()
        .filter(|cut| !cut.stable_id.is_empty())
        .map(|cut| (cut.id, cut.stable_id.clone()))
        .collect();
    for cut in &mut scene.cuts {
        let Some(source) = cut.visual_source.as_mut() else {
            continue;
        };
        if source
            .predecessor_shot_id
            .as_ref()
            .is_some_and(|id| !id.is_empty())
        {
            continue;
        }
        let Some(predecessor) = source.predecessor else {
            continue;
        };
        if let Some((_, stable)) = labels.iter().find(|(id, _)| *id == predecessor) {
            source.predecessor_shot_id = Some(stable.clone());
        }
    }
}

fn retarget_predecessors(scene: &mut SceneRecord) {
    let labels: Vec<(String, CutId)> = scene
        .cuts
        .iter()
        .filter(|cut| !cut.stable_id.is_empty())
        .map(|cut| (cut.stable_id.clone(), cut.id))
        .collect();
    for cut in &mut scene.cuts {
        let Some(source) = cut.visual_source.as_mut() else {
            continue;
        };
        let Some(shot_id) = source.predecessor_shot_id.clone() else {
            continue;
        };
        if let Some((_, id)) = labels.iter().find(|(stable, _)| stable == &shot_id) {
            source.predecessor = Some(*id);
        }
    }
}

fn retarget_animatic(scene: &mut SceneRecord, stable_for_old: &HashMap<CutId, String>) {
    let labels: HashMap<String, CutId> = scene
        .cuts
        .iter()
        .map(|cut| (cut.stable_id.clone(), cut.id))
        .collect();
    let Some(animatic) = scene.animatic.as_mut() else {
        return;
    };
    for shot in &mut animatic.shots {
        let Some(stable) = stable_for_old.get(&shot.cut_id) else {
            continue;
        };
        if let Some(id) = labels.get(stable) {
            shot.cut_id = *id;
        }
    }
}

fn retarget_assets(
    state: &mut ProjectState,
    scene_id: u32,
    stable_for_old: &HashMap<CutId, String>,
    keep_asset_ids: &HashSet<String>,
) {
    let labels: HashMap<String, CutId> = state
        .scene(scene_id)
        .map(|scene| {
            scene
                .cuts
                .iter()
                .map(|cut| (cut.stable_id.clone(), cut.id))
                .collect()
        })
        .unwrap_or_default();
    for asset in &mut state.assets {
        if keep_asset_ids.contains(&asset.id) {
            continue;
        }
        let Some(old) = asset.cut_id else {
            continue;
        };
        if old.source_scene != scene_id {
            continue;
        }
        let Some(stable) = stable_for_old.get(&old) else {
            continue;
        };
        if let Some(id) = labels.get(stable) {
            asset.cut_id = Some(*id);
        }
    }
}

/// Common one-to-one source correspondence between a saved cut and the cuts
/// currently held in a scene.
///
/// Keys on the saved source production path plus the captured source origin
/// (`source_scene`/`cut`) whenever one exists. This makes reorder-then-reimport
/// of a shared production master stable: two cuts pointing at the same
/// `production_note_path` stay distinct by their source origin.
///
/// Legacy originless records key on a unique production path only, or on the
/// display label pair when no production path survived. Unique per-cut source
/// paths therefore keep working unchanged.
///
/// Returns `Ok(Some(index))` for an unambiguous match, `Ok(None)` when the
/// saved source is gone, and `Err(...)` when a shared production path lacks a
/// captured source origin and cannot be mapped unambiguously (never guesses).
pub fn correspond_cut(scene: &SceneRecord, old: &CutRecord) -> Result<Option<usize>, String> {
    let Some(path) = old.production_note_path.clone() else {
        // Legacy fallback: no production path survived. Match the display label
        // pair, falling back to the stable id for renumbered saved cuts.
        return Ok(scene
            .cuts
            .iter()
            .position(|cut| cut.id.cut == old.id.cut)
            .or_else(|| scene.cuts.iter().position(|cut| cut.stable_id == old.stable_id)));
    };
    let origin = old.source_origin.as_ref();
    let path_filter = |cut: &CutRecord| cut.production_note_path.as_ref() == Some(&path);
    let origin_filter = |cut: &CutRecord| cut.source_origin.as_ref() == origin;

    // Prefer exact (source path, source origin) correspondence.
    if let Some(origin) = origin {
        let matches: Vec<usize> = scene
            .cuts
            .iter()
            .enumerate()
            .filter(|(_, cut)| path_filter(*cut) && origin_filter(*cut))
            .map(|(index, _)| index)
            .collect();
        match matches.len() {
            0 => return Ok(None),
            1 => return Ok(Some(matches[0])),
            _ => {
                return Err(format!(
                    "ambiguous source origin for saved shot `{}`: {}/{} cuts share source `{path}` \
                     and origin `{}`",
                    old.stable_id,
                    matches.len(),
                    scene.cuts.len(),
                    CutId {
                        source_scene: origin.source_scene,
                        cut: origin.cut,
                    }
                ));
            }
        }
    }

    // Fall back for originless saved records or missed origins: match by source
    // path alone. Shared paths without a saved origin are ambiguous.
    let matches: Vec<usize> = scene
        .cuts
        .iter()
        .enumerate()
        .filter(|(_, cut)| path_filter(*cut))
        .map(|(index, _)| index)
        .collect();
    if matches.is_empty() {
        return Ok(None);
    }
    if matches.len() > 1 && origin.is_none() {
        return Err(format!(
            "ambiguous production path `{path}` for saved shot `{}`: {}/{} current cuts share \
             the path but the saved cut has no source origin; refusing to guess a source",
            old.stable_id,
            matches.len(),
            scene.cuts.len()
        ));
    }
    Ok(matches.first().copied())
}

fn claim_takes(scene: &mut SceneRecord, dest: usize, old: &CutRecord) {
    let paths: HashSet<String> = old.takes.iter().map(|take| take.path.clone()).collect();
    if paths.is_empty() {
        if scene.cuts[dest].selected_take.is_none() {
            scene.cuts[dest].selected_take = old.selected_take.clone();
        }
        return;
    }
    let mut incoming = Vec::new();
    for (index, cut) in scene.cuts.iter_mut().enumerate() {
        if index == dest {
            continue;
        }
        let mut keep = Vec::new();
        for take in cut.takes.drain(..) {
            if paths.contains(&take.path) {
                incoming.push(take);
            } else {
                keep.push(take);
            }
        }
        cut.takes = keep;
        if cut.selected_take.as_ref().is_some_and(|selected| {
            incoming
                .iter()
                .any(|take| &take.id == selected || &take.path == selected)
        }) {
            cut.selected_take = None;
        }
    }
    let dest_cut = &mut scene.cuts[dest];
    for take in incoming {
        if !dest_cut
            .takes
            .iter()
            .any(|current| current.path == take.path)
        {
            dest_cut.takes.push(take);
        }
    }
    if dest_cut.selected_take.is_none() {
        dest_cut.selected_take = old.selected_take.clone().and_then(|selected| {
            let saved_path = old
                .takes
                .iter()
                .find(|take| take.id == selected || take.path == selected)
                .map(|take| take.path.clone());
            dest_cut
                .takes
                .iter()
                .find(|take| {
                    take.id == selected
                        || take.path == selected
                        || saved_path.as_ref() == Some(&take.path)
                })
                .map(|take| take.id.clone())
        });
    }
}

fn saved_order(scene: &SceneRecord, old: &SceneRecord) -> Vec<String> {
    let mut order: Vec<String> = old
        .cuts
        .iter()
        .filter(|cut| !cut.stable_id.is_empty())
        .filter(|cut| {
            scene
                .cuts
                .iter()
                .any(|current| current.stable_id == cut.stable_id)
        })
        .map(|cut| cut.stable_id.clone())
        .collect();
    for cut in &scene.cuts {
        if !cut.stable_id.is_empty() && !order.contains(&cut.stable_id) {
            order.push(cut.stable_id.clone());
        }
    }
    order
}

fn next_numeric(state: &ProjectState, scenes: bool) -> u32 {
    let mut max = 0u32;
    for scene in &state.scenes {
        if scenes {
            if let Some(value) = scene
                .stable_id
                .strip_prefix("scene_")
                .and_then(|text| text.parse().ok())
            {
                max = max.max(value);
            }
        } else {
            for cut in &scene.cuts {
                if let Some(value) = cut
                    .stable_id
                    .strip_prefix("shot_")
                    .and_then(|text| text.parse().ok())
                {
                    max = max.max(value);
                }
            }
        }
    }
    max + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use crate::workflow::state::{
        AssetRecord, AssetRole, AssetStatus, CutRecord, JobRecord, JobStatus, TakeRecord,
        TakeStatus, VisualSourceSpec, VisualSourceStrategy,
    };

    fn cut(scene: u32, number: u32, stable: &str, note: &str) -> CutRecord {
        let mut cut = CutRecord::new(CutId::new(scene, number));
        cut.stable_id = stable.into();
        cut.production_note_path = Some(note.into());
        cut
    }

    #[test]
    fn removal_refuses_produced_source_or_dependent_cuts_without_mutation() {
        let mut base = ProjectState::new();
        let scene = base.scene_mut(1, "Scene");
        let mut first = CutRecord::new(CutId::new(1, 1));
        first.stable_id = "shot_a".into();
        let mut second = CutRecord::new(CutId::new(1, 2));
        second.stable_id = "shot_b".into();
        scene.cuts = vec![first, second];
        for kind in ["source", "note", "asset", "dependent", "approved"] {
            let mut state = base.clone();
            match kind {
                "source" => state.scenes[0].cuts[0].source_origin = Some(CutId::new(1, 1)),
                "note" => state.scenes[0].cuts[0].production_note_path = Some("Scene_Prompts/Scene1/Cut1.txt".into()),
                "asset" => state.assets.push(AssetRecord {
                    id: "asset_1".into(), path: "Scenes/Scene1/Cut1/frame.png".into(),
                    role: AssetRole::CutStartFrame, status: AssetStatus::Present,
                    scene_id: Some(1), cut_id: Some(CutId::new(1, 1)),
                    character_id: None, source_job_id: None, parent_asset_id: None, notes: None,
                }),
                "dependent" => state.scenes[0].cuts[1].visual_source = Some(VisualSourceSpec {
                    strategy: VisualSourceStrategy::PreproductionStartFrame,
                    predecessor: Some(CutId::new(1, 1)),
                    predecessor_shot_id: Some("shot_a".into()),
                    asset_id: None,
                    frame_time_seconds: None,
                    source_take_id: None,
                }),
                "approved" => {
                    crate::workflow::animatic::sync(&mut state);
                    state.scenes[0].animatic.as_mut().unwrap().approved = true;
                }
                _ => unreachable!(),
            }
            let original = state.clone();
            assert!(remove_planning_cut(&mut state, 1, "shot_a").is_err(), "{kind}");
            assert_eq!(state, original, "{kind}");
        }
    }

    #[test]
    fn cross_scene_predecessor_follows_reorder_and_stops_binding_nonfinal() {
        use crate::workflow::state::{VisualSourceSpec, VisualSourceStrategy};
        let mut state = ProjectState::new();
        state.scene_mut(1, "First").cut_mut(1).stable_id = "first".into();
        state.scene_mut(1, "First").cut_mut(2).stable_id = "last".into();
        state.scene_mut(2, "Second").cut_mut(1).stable_id = "next".into();
        let mut source = VisualSourceSpec::new(VisualSourceStrategy::PreviousSelectedTakeExitFrame);
        source.predecessor = Some(CutId::new(1, 2));
        source.predecessor_shot_id = Some("last".into());
        source.asset_id = Some("frame".into());
        state.scene_mut(2, "Second").cut_mut(1).visual_source = Some(source);
        reorder_cuts(&mut state, 1, &["last".into(), "first".into()]).unwrap();
        let next = &state.scene(2).unwrap().cuts[0];
        assert_eq!(next.visual_source.as_ref().unwrap().predecessor, Some(CutId::new(1, 1)));
        assert!(next.visual_source.as_ref().unwrap().asset_id.is_none());
        assert_eq!(next.dependency_state, Some(DependencyState::NeedsReview));
    }

    #[test]
    fn cross_scene_planned_source_prevents_removing_predecessor() {
        use crate::workflow::state::{VisualSourceSpec, VisualSourceStrategy};
        let mut state = ProjectState::new();
        state.scene_mut(1, "First").cut_mut(1).stable_id = "first".into();
        state.scene_mut(2, "Second").cut_mut(1).stable_id = "next".into();
        let mut source = VisualSourceSpec::new(VisualSourceStrategy::PreviousSelectedTakeExitFrame);
        source.predecessor = Some(CutId::new(1, 1));
        source.predecessor_shot_id = Some("first".into());
        state.scene_mut(2, "Second").cut_mut(1).visual_source = Some(source);
        assert!(remove_planning_cut(&mut state, 1, "first").unwrap_err().contains("continuity source"));
    }

    #[test]
    fn reorder_keeps_shot_ids_and_takes() {
        let dir = tempfile::tempdir().unwrap();
        let take_path = "Renders/Scene1/shot_b.mp4";
        fs::write(dir.path().join("shot_b.mp4"), b"take-bytes").unwrap();
        let mut state = ProjectState::new();
        let scene = state.scene_mut(1, "Scene");
        scene.stable_id = "scene_1".into();
        let mut first = cut(1, 1, "shot_a", "Scene_Prompts/Scene1/Cut1.txt");
        first.takes.push(TakeRecord {
            id: "take_a".into(),
            path: "Renders/Scene1/shot_a.mp4".into(),
            status: TakeStatus::Selected,
            notes: None,
        });
        let mut second = cut(1, 2, "shot_b", "Scene_Prompts/Scene1/Cut2.txt");
        second.takes.push(TakeRecord {
            id: "take_b".into(),
            path: take_path.into(),
            status: TakeStatus::Selected,
            notes: None,
        });
        second.selected_take = Some("take_b".into());
        scene.cuts = vec![first, second];

        reorder_cuts(&mut state, 1, &["shot_b".into(), "shot_a".into()]).unwrap();

        let scene = state.scene(1).unwrap();
        assert_eq!(scene.cuts[0].stable_id, "shot_b");
        assert_eq!(scene.cuts[0].id.cut, 1);
        assert_eq!(scene.cuts[1].stable_id, "shot_a");
        assert_eq!(scene.cuts[1].id.cut, 2);
        assert_eq!(scene.cuts[0].takes[0].path, take_path);
        assert_eq!(scene.cuts[0].takes[0].id, "take_b");
        assert!(dir.path().join("shot_b.mp4").is_file());
        assert_eq!(
            fs::read(dir.path().join("shot_b.mp4")).unwrap(),
            b"take-bytes"
        );
    }

    #[test]
    fn reimport_keeps_the_stable_id_on_the_production_note() {
        let mut fresh = ProjectState::new();
        let scene = fresh.scene_mut(1, "Scene");
        scene.cuts = vec![
            cut(1, 1, "", "Scene_Prompts/Scene1/Cut1.txt"),
            cut(1, 2, "", "Scene_Prompts/Scene1/Cut2.txt"),
        ];
        scene.cuts[0].takes.push(TakeRecord {
            id: "take_from_number".into(),
            path: "Renders/b.mp4".into(),
            status: TakeStatus::Selected,
            notes: None,
        });

        let mut previous = ProjectState::new();
        let scene = previous.scene_mut(1, "Scene");
        scene.stable_id = "scene_4".into();
        let mut first = cut(1, 1, "shot_b", "Scene_Prompts/Scene1/Cut2.txt");
        first.takes.push(TakeRecord {
            id: "take_b".into(),
            path: "Renders/b.mp4".into(),
            status: TakeStatus::Selected,
            notes: None,
        });
        first.selected_take = Some("take_b".into());
        let second = cut(1, 2, "shot_a", "Scene_Prompts/Scene1/Cut1.txt");
        scene.cuts = vec![first, second];

        adopt(&mut fresh, &previous).unwrap();

        let scene = fresh.scene(1).unwrap();
        assert_eq!(scene.stable_id, "scene_4");
        assert_eq!(scene.cuts[0].stable_id, "shot_b");
        assert_eq!(
            scene.cuts[0].production_note_path.as_deref(),
            Some("Scene_Prompts/Scene1/Cut2.txt")
        );
        assert_eq!(scene.cuts[0].id.cut, 1);
        assert_eq!(scene.cuts[0].takes[0].path, "Renders/b.mp4");
        assert_eq!(
            scene.cuts[0].selected_take.as_deref(),
            Some("take_from_number")
        );
        assert_eq!(scene.cuts[1].stable_id, "shot_a");
        assert_eq!(
            scene.cuts[1].production_note_path.as_deref(),
            Some("Scene_Prompts/Scene1/Cut1.txt")
        );
        assert!(scene.cuts[1]
            .takes
            .iter()
            .all(|take| take.path != "Renders/b.mp4"));
    }

    #[test]
    fn look_and_render_jobs_are_the_same_record() {
        let look = JobRecord::open("job_1", "scene_look");
        let render = JobRecord::open("job_2", "render");
        assert_eq!(look.attempts.len(), 1);
        assert_eq!(render.attempts.len(), 1);
        assert_eq!(look.attempts[0].status, JobStatus::Pending);
        assert_eq!(render.attempts[0].status, JobStatus::Pending);
        assert_eq!(look.status, render.status);
        assert_ne!(look.kind, render.kind);
    }

    #[test]
    fn upstream_change_marks_the_successor_and_keeps_the_take() {
        let dir = tempfile::tempdir().unwrap();
        let take_rel = dir.path().join("successor.mp4");
        fs::write(&take_rel, b"successor-take").unwrap();
        let mut state = ProjectState::new();
        let scene = state.scene_mut(1, "Scene");
        let first = cut(1, 1, "shot_a", "Scene_Prompts/Scene1/Cut1.txt");
        let mut second = cut(1, 2, "shot_b", "Scene_Prompts/Scene1/Cut2.txt");
        second.takes.push(TakeRecord {
            id: "take_b".into(),
            path: take_rel.to_string_lossy().to_string(),
            status: TakeStatus::Selected,
            notes: None,
        });
        let mut source = VisualSourceSpec::new(VisualSourceStrategy::PreviousSelectedTakeExitFrame);
        source.predecessor = Some(CutId::new(1, 1));
        source.predecessor_shot_id = Some("shot_a".into());
        second.visual_source = Some(source);
        scene.cuts = vec![first, second];
        let old_id = state.push_asset(AssetRecord {
            id: String::new(),
            path: "Preproduction/Scene1/start_frames/cut_1_v1.png".into(),
            role: AssetRole::CutStartFrame,
            status: AssetStatus::Approved,
            scene_id: Some(1),
            cut_id: Some(CutId::new(1, 1)),
            character_id: None,
            source_job_id: None,
            parent_asset_id: None,
            notes: None,
        });
        let _new_id = state.push_asset(AssetRecord {
            id: String::new(),
            path: "Preproduction/Scene1/start_frames/cut_1_v2.png".into(),
            role: AssetRole::CutStartFrame,
            status: AssetStatus::Draft,
            scene_id: Some(1),
            cut_id: Some(CutId::new(1, 1)),
            character_id: None,
            source_job_id: None,
            parent_asset_id: None,
            notes: None,
        });

        mark_upstream_changed(&mut state, 1, 1, &[old_id]);

        let successor = &state.scene(1).unwrap().cuts[1];
        assert_eq!(
            successor.dependency_state,
            Some(DependencyState::NeedsReview)
        );
        assert_eq!(
            successor.takes[0].path,
            take_rel.to_string_lossy().to_string()
        );
        assert_eq!(fs::read(&take_rel).unwrap(), b"successor-take");
    }

    #[test]
    fn assign_fills_empty_ids_once() {
        let mut state = ProjectState::new();
        state
            .scene_mut(1, "Scene")
            .cuts
            .push(CutRecord::new(CutId::new(1, 1)));
        assert!(assign(&mut state));
        assert_eq!(state.scene(1).unwrap().stable_id, "scene_1");
        assert_eq!(state.scene(1).unwrap().cuts[0].stable_id, "shot_1");
        assert!(!assign(&mut state));
    }

    #[test]
    fn alternate_angle_allocates_a_new_shot_without_renumbering() {
        let mut state = ProjectState::new();
        let scene = state.scene_mut(1, "Scene 1");
        let mut existing = CutRecord::new(CutId::new(1, 8));
        existing.stable_id = "shot_8".into();
        existing.title = Some("Arrival".into());
        scene.cuts.push(existing);
        let stable = add_alternate_shot(&mut state, 1).unwrap();
        let scene = state.scene(1).unwrap();
        assert_eq!(scene.cuts[0].stable_id, "shot_8");
        assert_eq!(scene.cuts[0].id.cut, 8);
        let added = scene
            .cuts
            .iter()
            .find(|cut| cut.stable_id == stable)
            .unwrap();
        assert_ne!(added.stable_id, "shot_8");
        assert_eq!(added.id.cut, 9);
        assert!(added
            .sketch
            .as_ref()
            .is_some_and(|sketch| !sketch.is_empty()));
        assert!(added.visual_source.is_none());
        assert!(added.production_note_path.is_none());
        assert!(scene
            .animatic
            .as_ref()
            .unwrap()
            .shots
            .iter()
            .any(|shot| shot.cut_id == added.id));
    }
}
