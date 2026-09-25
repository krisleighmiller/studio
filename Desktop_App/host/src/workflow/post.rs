//! Post-production records. This is not an editor.
//!
//! A cut version snapshots the selected takes. Picture lock points at one
//! version. Straightforward concat stays on the assemble job. Taste-heavy
//! editorial is recorded as an external NLE decision and leaves the files put.
use serde::{Deserialize, Serialize};

use crate::workflow::state::{CutRecord, ProductionProfile, ProjectState, SceneRecord, TakeRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CutVersionStage {
    #[default]
    Selects,
    Assembly,
    FineCut,
    PictureLock,
}

impl CutVersionStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Selects => "selects",
            Self::Assembly => "assembly",
            Self::FineCut => "fine_cut",
            Self::PictureLock => "picture_lock",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Selects => "Selects",
            Self::Assembly => "Assembly",
            Self::FineCut => "Fine cut",
            Self::PictureLock => "Picture lock",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value
            .trim()
            .to_ascii_lowercase()
            .replace(['-', ' '], "_")
            .as_str()
        {
            "selects" | "select" => Some(Self::Selects),
            "assembly" | "assemble" => Some(Self::Assembly),
            "fine_cut" | "fine" => Some(Self::FineCut),
            "picture_lock" | "locked" | "lock" => Some(Self::PictureLock),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PostStatus {
    #[default]
    NotStarted,
    InProgress,
    NeedsReview,
    Approved,
}

impl PostStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::InProgress => "in_progress",
            Self::NeedsReview => "needs_review",
            Self::Approved => "approved",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::NotStarted => "Not started",
            Self::InProgress => "In progress",
            Self::NeedsReview => "Needs review",
            Self::Approved => "Approved",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value
            .trim()
            .to_ascii_lowercase()
            .replace(['-', ' '], "_")
            .as_str()
        {
            "not_started" | "missing" => Some(Self::NotStarted),
            "in_progress" | "draft" => Some(Self::InProgress),
            "needs_review" | "review" => Some(Self::NeedsReview),
            "approved" => Some(Self::Approved),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EditorialPath {
    #[default]
    HostFfmpeg,
    ExternalNle,
}

impl EditorialPath {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HostFfmpeg => "host_ffmpeg",
            Self::ExternalNle => "external_nle",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value
            .trim()
            .to_ascii_lowercase()
            .replace(['-', ' '], "_")
            .as_str()
        {
            "host_ffmpeg" | "ffmpeg" | "host" => Some(Self::HostFfmpeg),
            "external_nle" | "nle" | "external" => Some(Self::ExternalNle),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostField {
    AudioMix,
    Vfx,
    Compositing,
    Captions,
    Mastering,
    Qc,
}

impl PostField {
    pub fn parse(value: &str) -> Option<Self> {
        match value
            .trim()
            .to_ascii_lowercase()
            .replace(['-', ' '], "_")
            .as_str()
        {
            "audio_mix" | "mix" | "audio" => Some(Self::AudioMix),
            "vfx" | "effects" => Some(Self::Vfx),
            "compositing" | "composite" => Some(Self::Compositing),
            "captions" | "subtitles" => Some(Self::Captions),
            "mastering" | "master" => Some(Self::Mastering),
            "qc" | "quality" => Some(Self::Qc),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VersionShot {
    pub shot_id: String,
    pub cut_label: u32,
    pub take_id: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutVersion {
    pub id: String,
    pub label: String,
    pub stage: CutVersionStage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly_path: Option<String>,
    pub shots: Vec<VersionShot>,
    #[serde(default)]
    pub editorial_path: EditorialPath,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MissingShot {
    pub shot_id: String,
    pub cut_label: u32,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeliverableCheck {
    pub deliverable_id: String,
    pub version_id: Option<String>,
    pub passed: bool,
    pub resolution_ok: bool,
    pub aspect_ok: bool,
    pub missing_shots: Vec<MissingShot>,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ScenePost {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<CutVersion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture_locked_id: Option<String>,
    #[serde(default)]
    pub editorial_path: EditorialPath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_nle_note: Option<String>,
    #[serde(default)]
    pub audio_mix: PostStatus,
    #[serde(default)]
    pub vfx: PostStatus,
    #[serde(default)]
    pub compositing: PostStatus,
    #[serde(default)]
    pub captions: PostStatus,
    #[serde(default)]
    pub mastering: PostStatus,
    #[serde(default)]
    pub qc: PostStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_check: Option<DeliverableCheck>,
}

pub fn mark_cut_version(state: &mut ProjectState, scene_id: u32) -> Result<String, String> {
    let assembly_path = state
        .jobs
        .iter()
        .rev()
        .find(|job| {
            job.kind == "assemble"
                && job.raw_provider_id.as_deref() == Some(&format!("scene_{scene_id}"))
        })
        .and_then(|job| job.output_path.clone());
    let scene = scene_mut(state, scene_id)?;
    if scene.cuts.is_empty() {
        return Err(format!("Scene {scene_id} has no cuts to version"));
    }
    let mut shots = Vec::new();
    let mut missing = Vec::new();
    let mut ordered = scene.cuts.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|cut| cut.id.cut);
    for cut in ordered {
        match selected_take(cut) {
            Some(take) => shots.push(VersionShot {
                shot_id: cut.stable_id.clone(),
                cut_label: cut.id.cut,
                take_id: take.id.clone(),
                path: take.path.clone(),
            }),
            None => missing.push(cut.id.cut),
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "Select one take for every cut before marking a cut version. Missing: Cut {}.",
            missing
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", Cut ")
        ));
    }
    let editorial = scene
        .post
        .as_ref()
        .map(|post| post.editorial_path)
        .unwrap_or_default();
    let number = scene
        .post
        .as_ref()
        .map(|post| post.versions.len())
        .unwrap_or(0)
        + 1;
    let id = next_version_id(state);
    let scene = scene_mut(state, scene_id)?;
    let post = scene.post.get_or_insert_with(ScenePost::default);
    post.versions.push(CutVersion {
        id: id.clone(),
        label: format!("Selects {number}"),
        stage: CutVersionStage::Selects,
        assembly_path,
        shots,
        editorial_path: editorial,
    });
    Ok(id)
}

pub fn set_version_stage(
    state: &mut ProjectState,
    scene_id: u32,
    version_id: &str,
    stage: CutVersionStage,
) -> Result<(), String> {
    version_mut(state, scene_id, version_id)?.stage = stage;
    let post = scene_mut(state, scene_id)?
        .post
        .get_or_insert_with(ScenePost::default);
    if stage == CutVersionStage::PictureLock {
        post.picture_locked_id = Some(version_id.to_string());
    } else if post.picture_locked_id.as_deref() == Some(version_id) {
        post.picture_locked_id = None;
    }
    Ok(())
}

pub fn lock_picture(
    state: &mut ProjectState,
    scene_id: u32,
    version_id: &str,
) -> Result<(), String> {
    let shots = version_mut(state, scene_id, version_id)?.shots.len();
    if shots == 0 {
        return Err("This cut version has no selected takes to lock.".into());
    }
    set_version_stage(state, scene_id, version_id, CutVersionStage::PictureLock)
}

pub fn set_status(
    state: &mut ProjectState,
    scene_id: u32,
    field: PostField,
    status: PostStatus,
) -> Result<(), String> {
    let post = scene_mut(state, scene_id)?
        .post
        .get_or_insert_with(ScenePost::default);
    match field {
        PostField::AudioMix => post.audio_mix = status,
        PostField::Vfx => post.vfx = status,
        PostField::Compositing => post.compositing = status,
        PostField::Captions => post.captions = status,
        PostField::Mastering => post.mastering = status,
        PostField::Qc => post.qc = status,
    }
    Ok(())
}

pub fn set_editorial(
    state: &mut ProjectState,
    scene_id: u32,
    path: EditorialPath,
    note: Option<String>,
) -> Result<(), String> {
    let post = scene_mut(state, scene_id)?
        .post
        .get_or_insert_with(ScenePost::default);
    post.editorial_path = path;
    post.external_nle_note = match path {
        EditorialPath::ExternalNle => note.filter(|note| !note.trim().is_empty()),
        EditorialPath::HostFfmpeg => None,
    };
    Ok(())
}

pub fn check_deliverable(
    state: &mut ProjectState,
    scene_id: u32,
    deliverable_id: &str,
) -> Result<DeliverableCheck, String> {
    let profile = state
        .profile
        .clone()
        .ok_or_else(|| "This project has no production profile.".to_string())?;
    let deliverable = profile
        .deliverables
        .iter()
        .find(|item| item.id == deliverable_id)
        .cloned()
        .ok_or_else(|| format!("deliverable `{deliverable_id}` not found"))?;
    let (missing, version_id) = {
        let scene = state
            .scene(scene_id)
            .ok_or_else(|| format!("Scene {scene_id} not found"))?;
        let version = scene.post.as_ref().and_then(|post| {
            post.picture_locked_id
                .as_ref()
                .and_then(|id| post.versions.iter().find(|version| &version.id == id))
                .or_else(|| post.versions.last())
        });
        let version_id = version.map(|version| version.id.clone());
        let mut missing = Vec::new();
        for cut in &scene.cuts {
            let covered = version.is_some_and(|version| {
                version.shots.iter().any(|shot| {
                    (!cut.stable_id.is_empty() && shot.shot_id == cut.stable_id)
                        || (cut.stable_id.is_empty() && shot.cut_label == cut.id.cut)
                })
            });
            if version.is_some() && !covered {
                missing.push(MissingShot {
                    shot_id: cut.stable_id.clone(),
                    cut_label: cut.id.cut,
                    reason: "not in the cut version".into(),
                });
            } else if version.is_none() && selected_take(cut).is_none() {
                missing.push(MissingShot {
                    shot_id: cut.stable_id.clone(),
                    cut_label: cut.id.cut,
                    reason: "no selected take".into(),
                });
            }
        }
        (missing, version_id)
    };
    let (resolution_ok, resolution_message) = resolution_gate(&profile, &deliverable.resolution);
    let (aspect_ok, aspect_message) = aspect_gate(
        state.settings.aspect_ratio.as_deref(),
        profile.aspect_ratios.first().map(String::as_str),
        deliverable.aspect_ratio.as_deref(),
    );
    let mut messages = Vec::new();
    messages.push(resolution_message);
    messages.push(aspect_message);
    if missing.is_empty() {
        messages.push("No missing shots.".into());
    } else {
        messages.push(format!(
            "Missing shots: {}.",
            missing
                .iter()
                .map(|shot| format!("Cut {} ({})", shot.cut_label, shot.reason))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let check = DeliverableCheck {
        deliverable_id: deliverable.id,
        version_id,
        passed: resolution_ok && aspect_ok && missing.is_empty(),
        resolution_ok,
        aspect_ok,
        missing_shots: missing,
        messages,
    };
    let scene = scene_mut(state, scene_id)?;
    scene.post.get_or_insert_with(ScenePost::default).last_check = Some(check.clone());
    Ok(check)
}

fn resolution_gate(profile: &ProductionProfile, deliverable: &Option<String>) -> (bool, String) {
    let Some(expected) = deliverable
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return (false, "Deliverable has no resolution.".into());
    };
    match profile
        .resolution
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(actual) if normalize(actual) != normalize(expected) => (
            false,
            format!("Resolution {actual} does not match the deliverable {expected}."),
        ),
        _ => (true, format!("Resolution matches {expected}.")),
    }
}

fn aspect_gate(
    settings: Option<&str>,
    profile: Option<&str>,
    deliverable: Option<&str>,
) -> (bool, String) {
    let Some(expected) = deliverable.map(str::trim).filter(|value| !value.is_empty()) else {
        return (false, "Deliverable has no aspect ratio.".into());
    };
    let actual = settings
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| profile.map(str::trim).filter(|value| !value.is_empty()));
    match actual {
        Some(actual) if normalize(actual) != normalize(expected) => (
            false,
            format!("Aspect {actual} does not match the deliverable {expected}."),
        ),
        Some(_) => (true, format!("Aspect matches {expected}.")),
        None => (
            false,
            format!("Project has no aspect ratio to check against {expected}."),
        ),
    }
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn selected_take(cut: &CutRecord) -> Option<&TakeRecord> {
    let selected = cut.selected_take.as_deref()?;
    cut.takes
        .iter()
        .find(|take| take.id == selected || take.path == selected)
}

fn scene_mut(state: &mut ProjectState, scene_id: u32) -> Result<&mut SceneRecord, String> {
    state
        .scenes
        .iter_mut()
        .find(|scene| scene.id == scene_id)
        .ok_or_else(|| format!("Scene {scene_id} not found"))
}

fn version_mut<'a>(
    state: &'a mut ProjectState,
    scene_id: u32,
    version_id: &str,
) -> Result<&'a mut CutVersion, String> {
    scene_mut(state, scene_id)?
        .post
        .as_mut()
        .and_then(|post| {
            post.versions
                .iter_mut()
                .find(|version| version.id == version_id)
        })
        .ok_or_else(|| format!("cut version `{version_id}` not found"))
}

fn next_version_id(state: &ProjectState) -> String {
    let next = state
        .scenes
        .iter()
        .filter_map(|scene| scene.post.as_ref())
        .flat_map(|post| post.versions.iter())
        .filter_map(|version| version.id.strip_prefix("cutver_")?.parse::<u32>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    format!("cutver_{next}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_plan::handoff::CutId;
    use crate::workflow::state::{
        CutRecord, DeliverableRecord, ProductionProfile, TakeRecord, TakeStatus,
    };
    use std::fs;

    fn take(id: &str, path: &str) -> TakeRecord {
        TakeRecord {
            id: id.into(),
            path: path.into(),
            status: TakeStatus::Selected,
            notes: None,
        }
    }

    #[test]
    fn selected_takes_mark_a_cut_version_and_the_deliverable_checks() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.mp4"), b"take-a").unwrap();
        fs::write(dir.path().join("b.mp4"), b"take-b").unwrap();
        let mut state = ProjectState::new();
        state.settings.aspect_ratio = Some("9:16".into());
        state.profile = Some(ProductionProfile {
            resolution: Some("1080x1920".into()),
            aspect_ratios: vec!["9:16".into()],
            deliverables: vec![DeliverableRecord {
                id: "del_scene".into(),
                name: "Scene master".into(),
                audience: None,
                platforms: Vec::new(),
                runtime: None,
                aspect_ratio: Some("9:16".into()),
                resolution: Some("1080x1920".into()),
                frame_rate: None,
                language: None,
            }],
            ..ProductionProfile::default()
        });
        let scene = state.scene_mut(1, "Scene");
        let mut first = CutRecord::new(CutId::new(1, 1));
        first.stable_id = "shot_a".into();
        first.takes.push(take("take_a", "a.mp4"));
        first.selected_take = Some("take_a".into());
        let mut second = CutRecord::new(CutId::new(1, 2));
        second.stable_id = "shot_b".into();
        second.takes.push(take("take_b", "b.mp4"));
        second.selected_take = Some("take_b".into());
        scene.cuts = vec![first, second];

        let version_id = mark_cut_version(&mut state, 1).unwrap();
        assert_eq!(version_id, "cutver_1");
        lock_picture(&mut state, 1, &version_id).unwrap();

        let post = state.scene(1).unwrap().post.as_ref().unwrap();
        let version = &post.versions[0];
        assert_eq!(post.picture_locked_id.as_deref(), Some("cutver_1"));
        assert_eq!(version.stage, CutVersionStage::PictureLock);
        assert_eq!(version.shots[0].shot_id, "shot_a");
        assert_eq!(version.shots[0].path, "a.mp4");
        assert_eq!(version.shots[1].path, "b.mp4");
        assert_eq!(fs::read(dir.path().join("a.mp4")).unwrap(), b"take-a");
        assert_eq!(fs::read(dir.path().join("b.mp4")).unwrap(), b"take-b");

        let check = check_deliverable(&mut state, 1, "del_scene").unwrap();
        assert!(check.passed, "{:?}", check.messages);
        assert!(check.resolution_ok);
        assert!(check.aspect_ok);
        assert!(check.missing_shots.is_empty());

        state.profile.as_mut().unwrap().deliverables[0].aspect_ratio = Some("16:9".into());
        state.profile.as_mut().unwrap().deliverables[0].resolution = Some("1920x1080".into());
        let mismatch = check_deliverable(&mut state, 1, "del_scene").unwrap();
        assert!(!mismatch.resolution_ok);
        assert!(!mismatch.aspect_ok);
        assert!(!mismatch.passed);

        let scene = state.scene_mut(1, "Scene");
        let mut third = CutRecord::new(CutId::new(1, 3));
        third.stable_id = "shot_c".into();
        scene.cuts.push(third);
        let missing = check_deliverable(&mut state, 1, "del_scene").unwrap();
        assert_eq!(missing.missing_shots.len(), 1);
        assert_eq!(missing.missing_shots[0].cut_label, 3);
        assert_eq!(
            state.scene(1).unwrap().post.as_ref().unwrap().versions[0]
                .shots
                .len(),
            2
        );

        set_editorial(
            &mut state,
            1,
            EditorialPath::ExternalNle,
            Some("Frame-specific grade in an external NLE.".into()),
        )
        .unwrap();
        let post = state.scene(1).unwrap().post.as_ref().unwrap();
        assert_eq!(post.editorial_path, EditorialPath::ExternalNle);
        assert!(post
            .external_nle_note
            .as_ref()
            .unwrap()
            .contains("external NLE"));
        assert_eq!(post.versions.len(), 1);
        assert_eq!(fs::read(dir.path().join("b.mp4")).unwrap(), b"take-b");
    }
}
