//! Post-production records. This is not an editor.
//!
//! A cut version snapshots the selected takes. Picture lock points at one
//! version. Straightforward concat stays on the assemble job. Taste-heavy
//! editorial is recorded as an external NLE decision and leaves the files put.
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::workflow::qc::{QCFinding, QCSeverity};
use crate::workflow::state::{
    CutRecord, JobStatus, ProductionProfile, ProjectState, SceneRecord, TakeRecord, TakeStatus,
};

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

pub use crate::workflow::state::AssemblyShotSnapshot;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutVersion {
    pub id: String,
    pub label: String,
    pub stage: CutVersionStage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly_path: Option<String>,
    pub shots: Vec<VersionShot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly_snapshot: Option<Vec<AssemblyShotSnapshot>>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media_findings: Vec<QCFinding>,
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

/// Match the immutable delivered order/identity/window against the current native graph.
pub fn snapshot_matches_scene(scene: &SceneRecord, saved: &[AssemblyShotSnapshot]) -> bool {
    if scene.cuts.is_empty() || saved.len() != scene.cuts.len() { return false; }
    let mut seen = std::collections::HashSet::new();
    if scene.cuts.iter().any(|c| !seen.insert((c.id.source_scene,c.id.cut))) { return false; }
    let ordered: Vec<_> = if let Some(animatic) = &scene.animatic {
        if animatic.shots.len() != scene.cuts.len() { return false; }
        let mut visited = std::collections::HashSet::new();
        let mut cuts = Vec::new();
        for shot in &animatic.shots {
            if !visited.insert((shot.cut_id.source_scene,shot.cut_id.cut)) { return false; }
            let Some(cut) = scene.cuts.iter().find(|c| c.id == shot.cut_id) else { return false; };
            cuts.push(cut);
        }
        cuts
    } else { let mut cuts: Vec<_> = scene.cuts.iter().collect(); cuts.sort_by_key(|c| c.id.cut); cuts };
    ordered.iter().zip(saved).enumerate().all(|(i,(cut,record))| {
        let Some(take) = selected_take(cut) else { return false; };
        if record.shot_id != cut.stable_id || record.cut_label != cut.id.cut || record.take_id != take.id || record.path != take.path || !record.start_seconds.is_finite() || record.start_seconds < 0.0 || !record.duration_seconds.is_finite() || record.duration_seconds <= 0.0 { return false; }
        if let Some(animatic) = &scene.animatic {
            let shot = &animatic.shots[i]; let head=shot.trim_in_seconds.unwrap_or(0.0);let tail=shot.trim_out_seconds.unwrap_or(0.0);
            let transition = match shot.transition.unwrap_or(crate::workflow::state::SequenceTransition::Cut) { crate::workflow::state::SequenceTransition::Cut=>"cut",crate::workflow::state::SequenceTransition::Dissolve=>"dissolve",crate::workflow::state::SequenceTransition::FadeThroughBlack=>"fade_through_black" };
            head.is_finite() && tail.is_finite() && head >= 0.0 && tail >= 0.0 && shot.duration_seconds.is_finite() && shot.duration_seconds > head+tail && (record.start_seconds-head).abs()<0.000001 && (record.duration_seconds-(shot.duration_seconds-head-tail)).abs()<0.000001 && record.transition==transition
        } else { record.start_seconds==0.0 && record.transition=="cut" }
    })
}
fn version_matches_scene(scene: &SceneRecord, version: &CutVersion) -> bool {
    if version.shots.len()!=scene.cuts.len() || !version.assembly_snapshot.as_deref().is_some_and(|s|snapshot_matches_scene(scene,s)) { return false; }
    scene.cuts.iter().all(|cut| {
        let claims: Vec<_> = version.shots.iter().filter(|s|if cut.stable_id.is_empty(){s.cut_label==cut.id.cut}else{s.shot_id==cut.stable_id}).collect();
        let Some(take)=selected_take(cut) else{return false;};
        claims.len()==1 && claims[0].take_id==take.id && claims[0].path==take.path
    })
}
pub fn snapshot_runtime(saved: &[AssemblyShotSnapshot]) -> f64 {
    saved.iter().enumerate().map(|(i,s)|s.duration_seconds-if i+1<saved.len() && s.transition!="cut"{0.35}else{0.0}).sum()
}

pub fn mark_cut_version(state: &mut ProjectState, scene_id: u32) -> Result<String, String> {
    let current = state.scene(scene_id).ok_or_else(|| format!("Scene {scene_id} not found"))?;
    let inherited = state.jobs.iter().rev().find(|j| j.kind=="assemble" && j.status==JobStatus::Completed && j.raw_provider_id.as_deref()==Some(&format!("scene_{scene_id}")) && j.scope.as_ref().is_some_and(|scope|scope.kind=="scene" && scope.id==current.stable_id) && j.assembly_snapshot.as_deref().is_some_and(|snap|snapshot_matches_scene(current,snap)))
        .map(|j|(j.output_path.clone(),j.assembly_snapshot.clone()));
    let (assembly_path,assembly_snapshot)=inherited.unwrap_or_default();
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
        assembly_snapshot,
        editorial_path: editorial,
    });
    Ok(id)
}

/// Record a review export on a new cut version. An existing version for this
/// path is left alone, and selected takes stay selected.
pub fn attach_review_export(
    state: &mut ProjectState,
    scene_id: u32,
    assembly_path: &str,
) -> Result<String, String> {
    // Prefer matching prepared assembly job snapshot over mutable selection.
    let job_snapshot = state.jobs.iter().find(|j| j.kind == "assemble" && j.status == JobStatus::Completed && j.raw_provider_id.as_deref() == Some(&format!("scene_{scene_id}")) && j.output_path.as_deref() == Some(assembly_path)).and_then(|j| j.assembly_snapshot.clone());
    if let Some(existing) = state.scene(scene_id).and_then(|scene| {
        scene.post.as_ref().and_then(|post| {
            post.versions
                .iter()
                .find(|version| version.assembly_path.as_deref() == Some(assembly_path))
                .map(|version| version.id.clone())
        })
    }) {
        if let Some(ref snap) = job_snapshot {
            mark_snapshot_takes_used(state, scene_id, snap)?;
        }
        return Ok(existing);
    }
    let scene = scene_mut(state, scene_id)?;
    let mut shots = Vec::new();
    if let Some(ref snap) = job_snapshot {
        for entry in snap {
            shots.push(VersionShot {
                shot_id: entry.shot_id.clone(),
                cut_label: entry.cut_label,
                take_id: entry.take_id.clone(),
                path: entry.path.clone(),
            });
        }
    } else {
        let mut ordered = scene.cuts.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|cut| cut.id.cut);
        for cut in ordered {
            if let Some(take) = selected_take(cut) {
                shots.push(VersionShot {
                    shot_id: cut.stable_id.clone(),
                    cut_label: cut.id.cut,
                    take_id: take.id.clone(),
                    path: take.path.clone(),
                });
            }
        }
    }
    let editorial = scene
        .post
        .as_ref()
        .map(|post| post.editorial_path)
        .unwrap_or_default();
    let number = scene
        .post
        .as_ref()
        .map(|post| {
            post.versions
                .iter()
                .filter(|version| version.stage == CutVersionStage::Assembly)
                .count()
        })
        .unwrap_or(0)
        + 1;
    let id = next_version_id(state);
    let scene = scene_mut(state, scene_id)?;
    let post = scene.post.get_or_insert_with(ScenePost::default);
    let assembly_snapshot = job_snapshot.clone();
    post.versions.push(CutVersion {
        id: id.clone(),
        label: format!("Assembly {number}"),
        stage: CutVersionStage::Assembly,
        assembly_path: Some(assembly_path.to_string()),
        shots,
        assembly_snapshot,
        editorial_path: editorial,
    });
    if let Some(ref snap) = job_snapshot { mark_snapshot_takes_used(state, scene_id, snap)?; }
    Ok(id)
}

fn mark_snapshot_takes_used(state: &mut ProjectState, scene_id: u32, snap: &[crate::workflow::state::AssemblyShotSnapshot]) -> Result<(), String> {
    let scene = scene_mut(state, scene_id)?;
    for entry in snap {
        for cut in &mut scene.cuts {
            if if cut.stable_id.is_empty() {cut.id.cut == entry.cut_label} else {cut.stable_id == entry.shot_id} {
                if let Some(take_ref) = cut.takes.iter_mut().find(|t| t.id == entry.take_id && t.path == entry.path) {
                    if take_ref.status == TakeStatus::Selected {
                        take_ref.status = TakeStatus::UsedInAssembly;
                    }
                }
            }
        }
    }
    Ok(())
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
    project_root: &Path,
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
    let (missing, version_id, assembly_path, snapshot_ok, expected_runtime) = {
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
        let assembly_path = version.and_then(|version| version.assembly_path.clone());
        let mut missing = Vec::new();
        for cut in &scene.cuts {
            if version.is_none() {
                if selected_take(cut).is_some() {
                    missing.push(MissingShot {
                        shot_id: cut.stable_id.clone(),
                        cut_label: cut.id.cut,
                        reason: "no version selected".into(),
                    });
                } else {
                    missing.push(MissingShot {
                        shot_id: cut.stable_id.clone(),
                        cut_label: cut.id.cut,
                        reason: "no selected take".into(),
                    });
                }
                continue;
            }
            let version_shot = version
                .unwrap()
                .shots
                .iter()
                .find(|shot| {
                    if !cut.stable_id.is_empty() {
                        shot.shot_id == cut.stable_id
                    } else {
                        shot.cut_label == cut.id.cut
                    }
                });
            if version_shot.is_none() {
                missing.push(MissingShot {
                    shot_id: cut.stable_id.clone(),
                    cut_label: cut.id.cut,
                    reason: "not in the cut version".into(),
                });
            } else if let Some(shot) = version_shot {
                let selected = selected_take(cut);
                let matches = selected.is_some_and(|take| {
                    take.id == shot.take_id && take.path == shot.path
                });
                if !matches {
                    missing.push(MissingShot {
                        shot_id: cut.stable_id.clone(),
                        cut_label: cut.id.cut,
                        reason: "selection changed".into(),
                    });
                }
            }
        }
        let snapshot_ok = version.is_some_and(|v| version_matches_scene(scene,v));
        let expected_runtime = version.and_then(|v|v.assembly_snapshot.as_deref()).map(snapshot_runtime);
        (missing, version_id, assembly_path, snapshot_ok, expected_runtime)
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
    let mut media_findings = if version_id.is_none() || assembly_path.is_none() {
        vec![crate::workflow::qc::QCFinding {
            id: "absent_assembly".into(),
            severity: crate::workflow::qc::QCSeverity::Error,
            check: "Assembly".into(),
            message: if version_id.is_none() {
                "No cut version selected; assembly file absent.".into()
            } else {
                "Cut version has no assembly file.".into()
            },
            artifact_id: version_id.clone(),
        }]
    } else {
        let relative = assembly_path.as_ref().unwrap();
        let absolute = {
            use crate::project_path::{resolve, PathIntent};
            match resolve(relative, Some(project_root.to_str().unwrap_or("")), PathIntent::Read) {
                Ok(project_path) => project_path.into_path_buf(),
                Err(_) => {
                    // If confinement fails (absolute outside / traversal / symlink escape),
                    // treat as a failed media finding so it does not silently pass.
                    return Err(format!(
                        "Assembly path `{}` escapes the project root",
                        relative
                    ));
                }
            }
        };
        let mut findings = crate::workflow::qc::check_media_file_against_spec(
            &absolute,
            expected_runtime,
            0.15,
            deliverable.resolution.as_deref(),
            deliverable.aspect_ratio.as_deref(),
            deliverable.frame_rate.as_deref(),
        );
        findings.extend(crate::workflow::qc::check_full_video_decode(&absolute));
        findings.extend(crate::workflow::captions::check_captions(
            project_root,
            relative,
        ));
        findings
            .into_iter()
            .map(|mut finding| {
                finding.artifact_id = version_id.clone();
                finding
            })
            .collect::<Vec<_>>()
    };
    if !snapshot_ok {
        media_findings.push(QCFinding { id:"assembly_snapshot".into(), severity:QCSeverity::Error, check:"Assembly snapshot".into(), message:"Assembly does not verify the current ordered shot/take/timing snapshot. Reassemble and check again.".into(), artifact_id:version_id.clone() });
    }
    // Dependency gap repair: actual selected source, required audio, master audio stream.
    {
        use crate::project_path::{resolve, PathIntent};
        let root_str = project_root.to_str().unwrap_or("");
        if let Some(scene) = state.scene(scene_id) {
            for cut in &scene.cuts {
                let Some(take) = selected_take(cut) else { continue };
                // Resolve selected source under root; fail closed on escape.
                let source_rel = &take.path;
                let source_abs = match resolve(source_rel, Some(root_str), PathIntent::Read) {
                    Ok(p) => p.into_path_buf(),
                    Err(_) => {
                        media_findings.push(QCFinding { id:"selected_source_escape".into(), severity:QCSeverity::Error, check:"Source".into(), message:format!("Selected source `{}` escapes project root.",source_rel), artifact_id:version_id.clone() });
                        continue;
                    }
                };
                // Window from assembly snapshot if available, else just decode check.
                let (start_secs, duration_secs) = if let Some(snap) = scene.post.as_ref().and_then(|p| p.versions.iter().find(|v| v.id == version_id.as_deref().unwrap_or(""))).and_then(|v| v.assembly_snapshot.as_ref()) {
                    let record = snap.iter().find(|r| {
                        if !cut.stable_id.is_empty() { r.shot_id == cut.stable_id } else { r.cut_label == cut.id.cut }
                    });
                    record.map(|r| (r.start_seconds, r.duration_seconds)).unwrap_or((0.0, 0.0))
                } else { (0.0, 0.0) };
                let mut src_findings = if start_secs > 0.0 || duration_secs > 0.0 {
                    crate::workflow::qc::check_video_source_window(&source_abs, start_secs, duration_secs)
                } else {
                    crate::workflow::qc::check_video_source_window(&source_abs, 0.0, 0.0)
                };
                src_findings.iter_mut().for_each(|f| f.artifact_id = version_id.clone());
                // Identify cut/path in message for actionable findings
                for f in &mut src_findings {
                    if f.severity == QCSeverity::Error && (f.id == "source_exists" || f.id == "source_window" || f.id == "no_video" || f.id == "full_decode") {
                        f.message = format!("Cut {} take `{}`: {}", cut.id.cut, take.id, f.message);
                    }
                }
                media_findings.extend(src_findings);
            }
            // Required audio roles / asset bytes / output stream.
            if let Some(audio_status) = crate::workflow::audio::scene_audio_status(state, scene_id) {
                let required_roles = &audio_status.required;
                if !required_roles.is_empty() {
                    // Resolve required audio assets (exclude voice_profile from required audio roles)
                    for role in required_roles {
                        if role == &crate::workflow::state::AssetRole::VoiceProfile { continue; }
                        let assets = crate::workflow::audio::scene_audio_assets(state, scene_id).iter().filter(|a| a.role == *role).cloned().collect::<Vec<_>>();
                        if assets.is_empty() {
                            media_findings.push(QCFinding { id:"audio_role_missing".into(), severity:QCSeverity::Error, check:"Audio".into(), message:format!("Required audio role `{}` missing for scene.",role.label()), artifact_id:version_id.clone() });
                        } else {
                            for asset in &assets {
                                if asset.status != crate::workflow::state::AssetStatus::Approved {
                                    media_findings.push(QCFinding { id:"audio_unapproved".into(), severity:QCSeverity::Error, check:"Audio".into(), message:format!("Required audio role `{}` asset `{}` not approved.",role.label(),asset.id), artifact_id:version_id.clone() });
                                }
                                let asset_abs = match resolve(&asset.path, Some(root_str), PathIntent::Read) {
                                    Ok(p) => p.into_path_buf(),
                                    Err(_) => {
                                        media_findings.push(QCFinding { id:"audio_asset_escape".into(), severity:QCSeverity::Error, check:"Audio".into(), message:format!("Audio asset `{}` path escapes root.",asset.id), artifact_id:version_id.clone() });
                                        continue;
                                    }
                                };
                                let mut audio_findings = crate::workflow::qc::check_audio_decode(&asset_abs);
                                audio_findings.iter_mut().for_each(|f| f.artifact_id = version_id.clone());
                                for f in &mut audio_findings {
                                    if f.severity == QCSeverity::Error && (f.id == "audio_decode" || f.id == "audio_stream" || f.id == "file_exists") {
                                        f.message = format!("Audio role `{}` asset `{}`: {}", role.label(), asset.id, f.message);
                                    }
                                }
                                media_findings.extend(audio_findings);
                            }
                        }
                    }
                }
                // Master output must contain audio stream when any required role exists.
                if !required_roles.is_empty() {
                    if let Some(master_abs) = assembly_path.as_ref().and_then(|rel| resolve(rel, Some(root_str), PathIntent::Read).ok()).map(|p| p.into_path_buf()) {
                        match crate::workflow::qc::probe_media(&master_abs) {
                            Ok(probe) => {
                                if probe.audio.is_none() {
                                    media_findings.push(QCFinding { id:"master_audio_missing".into(), severity:QCSeverity::Error, check:"Master audio".into(), message:"Master output lacks required audio stream.".into(), artifact_id:version_id.clone() });
                                }
                            }
                            Err(_) => {
                                // If master itself unprobeable, already handled by assembly media findings.
                            }
                        }
                    }
                }
            }
        }
    }
    for finding in &mut media_findings { if finding.id=="runtime" && expected_runtime.is_some() {finding.severity=QCSeverity::Error;} }
    let media_ok = !media_findings
        .iter()
        .any(|finding| finding.severity == QCSeverity::Error);
    if !media_findings.is_empty() {
        let errors = media_findings
            .iter()
            .filter(|finding| finding.severity == QCSeverity::Error)
            .count();
        let warnings = media_findings
            .iter()
            .filter(|finding| finding.severity == QCSeverity::Warning)
            .count();
        let summary = if errors == 0 && warnings == 0 {
            "media probes clean".to_string()
        } else {
            let mut parts = Vec::new();
            if errors > 0 {
                parts.push(format!("{errors} technical error(s)"));
            }
            if warnings > 0 {
                parts.push(format!("{warnings} warning(s)"));
            }
            parts.join(", ")
        };
        messages.push(format!("Media: {summary}."));
    }
    let check = DeliverableCheck {
        deliverable_id: deliverable.id,
        version_id,
        passed: resolution_ok && aspect_ok && missing.is_empty() && media_ok,
        resolution_ok,
        aspect_ok,
        missing_shots: missing,
        messages,
        media_findings,
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
        let a_path = dir.path().join("a.mp4");
        let b_path = dir.path().join("b.mp4");
        // Generate real brief videos so media QC passes, preserving original intent.
        for p in [&a_path, &b_path] {
            let status = std::process::Command::new("ffmpeg")
                .args([
                    "-y", "-f", "lavfi", "-i", "color=c=black:s=320x240:d=0.5:r=24",
                    "-f", "lavfi", "-i", "sine=frequency=440:duration=0.5",
                    "-shortest", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac",
                ])
                .arg(p)
                .status()
                .unwrap();
            assert!(status.success(), "ffmpeg failed for {}", p.display());
        }
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
        // Give the locked version the real assembly file so media QC passes.
        let version = version_mut(&mut state, 1, &version_id).unwrap();
        let master=dir.path().join("master.mp4");
        let generated=std::process::Command::new("ffmpeg").args(["-v","error","-y","-f","lavfi","-i","color=black:s=320x240:r=24:d=1","-c:v","libx264","-threads","1"]).arg(&master).output().unwrap();
        assert!(generated.status.success());
        version.assembly_path = Some("master.mp4".into());
        version.assembly_snapshot=Some(version.shots.iter().map(|s|AssemblyShotSnapshot{shot_id:s.shot_id.clone(),cut_label:s.cut_label,take_id:s.take_id.clone(),path:s.path.clone(),start_seconds:0.0,duration_seconds:0.5,transition:"cut".into()}).collect());
        assert!(fs::metadata(&a_path).is_ok());
        assert!(fs::metadata(&b_path).is_ok());
        let a_bytes_before = fs::read(&a_path).unwrap();
        let b_bytes_before = fs::read(&b_path).unwrap();

        let check = check_deliverable(&mut state, 1, "del_scene", dir.path()).unwrap();
        assert!(check.passed, "{:?}", check.messages);
        assert!(check.resolution_ok);
        assert!(check.aspect_ok);
        assert!(check.missing_shots.is_empty());

        state.profile.as_mut().unwrap().deliverables[0].aspect_ratio = Some("16:9".into());
        state.profile.as_mut().unwrap().deliverables[0].resolution = Some("1920x1080".into());
        let mismatch = check_deliverable(&mut state, 1, "del_scene", dir.path()).unwrap();
        assert!(!mismatch.resolution_ok);
        assert!(!mismatch.aspect_ok);
        assert!(!mismatch.passed);

        let scene = state.scene_mut(1, "Scene");
        let mut third = CutRecord::new(CutId::new(1, 3));
        third.stable_id = "shot_c".into();
        scene.cuts.push(third);
        let missing = check_deliverable(&mut state, 1, "del_scene", dir.path()).unwrap();
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
        assert_eq!(fs::read(&a_path).unwrap(), a_bytes_before, "a.mp4 bytes changed");
        assert_eq!(fs::read(&b_path).unwrap(), b_bytes_before, "b.mp4 bytes changed");
    }
}
