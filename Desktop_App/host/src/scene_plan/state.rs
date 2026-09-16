use crate::scene_plan::handoff::CutId;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const STATE_VERSION: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Planning,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SectionStatus {
    #[default]
    Pending,
    InProgress,
    Done,
}

/// Which normalized source-dialogue words a committed cut carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CutCoverage {
    pub cut: u32,
    pub word_start: usize,
    pub word_end: usize,
}

/// One source scene of the script and the generated cuts that cover it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanSection {
    pub source_scene: u32,
    pub title: String,
    pub heading: String,
    #[serde(default)]
    pub status: SectionStatus,
    /// Normalized source-dialogue words covered so far. This is the only
    /// completion gate; action/sound/transition beats are not tracked.
    #[serde(default)]
    pub covered_word_count: usize,
    #[serde(default)]
    pub attempts: u32,
    #[serde(default = "one")]
    pub next_cut: u32,
    #[serde(default)]
    pub cuts: Vec<u32>,
    /// Word ranges per cut; filled on commit, reconstructed from the master
    /// for sections planned before this field existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cut_coverage: Vec<CutCoverage>,
    /// Master production file this scene's cuts are appended to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub production_path: Option<String>,
    /// `<sidecar_root>/SceneN` — where this scene's `CutM.txt` files live.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidecar_dir: Option<String>,
}

fn one() -> u32 {
    1
}

impl PlanSection {
    pub fn new(source_scene: u32, title: impl Into<String>, heading: impl Into<String>) -> Self {
        PlanSection {
            source_scene,
            title: title.into(),
            heading: heading.into(),
            status: SectionStatus::Pending,
            covered_word_count: 0,
            attempts: 0,
            next_cut: 1,
            cuts: Vec::new(),
            cut_coverage: Vec::new(),
            production_path: None,
            sidecar_dir: None,
        }
    }
    pub fn next_cut_id(&self) -> CutId {
        CutId::new(self.source_scene, self.next_cut)
    }
    pub fn cut_ids(&self) -> Vec<CutId> {
        self.cuts
            .iter()
            .map(|c| CutId::new(self.source_scene, *c))
            .collect()
    }

    pub fn coverage_for(&self, cut: u32) -> Option<CutCoverage> {
        self.cut_coverage.iter().copied().find(|c| c.cut == cut)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanState {
    pub version: u32,
    pub status: PlanStatus,
    #[serde(default)]
    pub script_path: String,
    /// Root under which the host writes `SceneN/CutM.txt`. One per production.
    pub sidecar_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<String>,
    /// Source scene touched by the most recent commit (informational).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_source_scene: Option<u32>,
    pub sections: Vec<PlanSection>,
}

fn scene_dir_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^scene[_ -]?(\d+)$").unwrap())
}

/// `Scene2` / `scene_2` -> 2.
pub fn scene_number_from_stem(name: &str) -> Option<u32> {
    scene_dir_re().captures(name)?[1].parse().ok()
}

/// The host owns the per-scene folder. If `dir` already ends in `SceneN`,
/// return its parent as the root together with `Some(N)`.
pub fn sidecar_root_from(dir: &Path) -> (PathBuf, Option<u32>) {
    let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if let Some(n) = scene_number_from_stem(name) {
        let parent = dir
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        return (parent, Some(n));
    }
    (dir.to_path_buf(), None)
}

#[derive(Deserialize)]
struct V3State {
    status: PlanStatus,
    #[serde(default)]
    script_path: String,
    #[serde(default)]
    production_path: String,
    #[serde(default)]
    sidecar_dir: String,
    #[serde(default)]
    aspect_ratio: Option<String>,
    #[serde(default)]
    section_index: usize,
    #[serde(default)]
    sections: Vec<PlanSection>,
}

fn migrate_v3(value: serde_json::Value) -> Result<(PlanState, Vec<String>), String> {
    let v3: V3State = serde_json::from_value(value).map_err(|e| format!("state json (v3): {e}"))?;
    let mut notes = vec![
        "migrated scene_plan state v3 -> v4 (per-scene paths, single sidecar_root)".to_string(),
    ];
    let (root, stripped) = sidecar_root_from(Path::new(&v3.sidecar_dir));
    if let Some(n) = stripped {
        notes.push(format!(
            "v3 sidecar_dir `{}` ended in Scene{n}; sidecar_root is `{}`. v3 wrote sidecars one level too deep (`{}/Scene<N>/Scene<N>/Cut*.txt`); the host repairs that layout on load",
            v3.sidecar_dir, root.display(), root.display()
        ));
    }
    let prod_scene = Path::new(&v3.production_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(scene_number_from_stem);
    let active = v3.sections.get(v3.section_index).map(|s| s.source_scene);
    let single = v3.sections.len() == 1;
    let mut sections = v3.sections;
    for s in &mut sections {
        if s.production_path.is_none() {
            if !v3.production_path.is_empty()
                && (prod_scene == Some(s.source_scene) || (single && prod_scene.is_none()))
            {
                s.production_path = Some(v3.production_path.clone());
            } else {
                notes.push(format!(
                    "Scene {}: master production file unknown after migration; it is recorded on the next scene_plan run for that scene",
                    s.source_scene
                ));
            }
        }
        if s.sidecar_dir.is_none() {
            s.sidecar_dir = Some(
                root.join(format!("Scene{}", s.source_scene))
                    .to_string_lossy()
                    .to_string(),
            );
        }
    }
    Ok((
        PlanState {
            version: STATE_VERSION,
            status: v3.status,
            script_path: v3.script_path,
            sidecar_root: root.to_string_lossy().to_string(),
            aspect_ratio: v3.aspect_ratio,
            active_source_scene: active,
            sections,
        },
        notes,
    ))
}

impl PlanState {
    pub fn new(sidecar_root: &Path, aspect_ratio: Option<String>) -> Self {
        PlanState {
            version: STATE_VERSION,
            status: PlanStatus::Planning,
            script_path: String::new(),
            sidecar_root: sidecar_root.to_string_lossy().to_string(),
            aspect_ratio,
            active_source_scene: None,
            sections: Vec::new(),
        }
    }

    /// Parse v4, or migrate v3 with notes the host should surface.
    pub fn load(text: &str) -> Result<(Self, Vec<String>), String> {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("state json: {e}"))?;
        let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
        match version {
            4 => serde_json::from_value(value).map(|s| (s, Vec::new())).map_err(|e| format!("state json: {e}")),
            3 => migrate_v3(value),
            other => Err(format!(
                "state version {other} is not supported (expected {STATE_VERSION}, v3 is migrated automatically); v2 numbered generated cuts as flat `Scene N` — remove the old production file, sidecars and state, then re-run"
            )),
        }
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        Self::load(text).map(|(s, _)| s)
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self)
            .map(|s| s + "\n")
            .map_err(|e| format!("state json: {e}"))
    }

    pub fn is_done(&self) -> bool {
        self.status == PlanStatus::Done
    }

    pub fn section_for_scene(&self, source_scene: u32) -> Option<usize> {
        self.sections
            .iter()
            .position(|s| s.source_scene == source_scene)
    }

    pub fn ensure_section(&mut self, source_scene: u32, title: &str, heading: &str) -> usize {
        if let Some(i) = self.section_for_scene(source_scene) {
            return i;
        }
        self.sections
            .push(PlanSection::new(source_scene, title, heading));
        self.status = PlanStatus::Planning;
        self.sections.len() - 1
    }

    pub fn committed_cuts(&self) -> Vec<CutId> {
        self.sections.iter().flat_map(|s| s.cut_ids()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_root_strips_a_trailing_scene_folder_only() {
        let (root, n) = sidecar_root_from(Path::new("/p/Prod_Notes/Scene2"));
        assert_eq!((root.as_path(), n), (Path::new("/p/Prod_Notes"), Some(2)));
        let (root, n) = sidecar_root_from(Path::new("/p/Prod_Notes"));
        assert_eq!((root.as_path(), n), (Path::new("/p/Prod_Notes"), None));
        let (root, n) = sidecar_root_from(Path::new("Scene_7"));
        assert_eq!((root.as_path(), n), (Path::new("."), Some(7)));
        assert_eq!(scene_number_from_stem("Scene1"), Some(1));
        assert_eq!(scene_number_from_stem("Scenery"), None);
    }

    #[test]
    fn v4_round_trip_and_section_helpers() {
        let mut s = PlanState::new(Path::new("/p/Prod_Notes"), Some("9:16".into()));
        let i = s.ensure_section(1, "A", "Scene 1 — A");
        s.sections[i].next_cut = 3;
        s.sections[i].cuts = vec![1, 2];
        s.sections[i].production_path = Some("/p/Prod_Notes/Scene1.md".into());
        s.active_source_scene = Some(1);
        let json = s.to_json().unwrap();
        assert!(json.contains("\"sidecar_root\": \"/p/Prod_Notes\""));
        assert!(json.contains("\"active_source_scene\": 1"));
        assert!(!json.contains("section_index"));
        let (back, notes) = PlanState::load(&json).unwrap();
        assert_eq!(back, s);
        assert!(notes.is_empty());
        assert_eq!(
            s.sections[0].cut_ids(),
            vec![CutId::new(1, 1), CutId::new(1, 2)]
        );
        assert_eq!(s.ensure_section(2, "B", "Scene 2 — B"), 1);
    }

    #[test]
    fn v3_state_migrates_with_explicit_notes() {
        let v3 = r#"{
          "version": 3, "status": "done", "script_path": "",
          "production_path": "/p/Prod_Notes/Scene1.md",
          "sidecar_dir": "/p/Prod_Notes/Scene1",
          "aspect_ratio": "9:16", "section_index": 1,
          "sections": [
            {"source_scene": 1, "title": "Scene 1 — The last train", "heading": "Scene 1 — The last train", "status": "done", "covered_word_count": 33, "attempts": 3, "next_cut": 10, "cuts": [1,2,3,4,5,6,7,8,9]},
            {"source_scene": 2, "title": "Scene 2 — An empty platform", "heading": "Scene 2 — An empty platform", "status": "done", "covered_word_count": 55, "attempts": 3, "next_cut": 8, "cuts": [1,2,3,4,5,6,7]}
          ]}"#;
        let (s, notes) = PlanState::load(v3).unwrap();
        assert_eq!(s.version, 4);
        assert_eq!(s.sidecar_root, "/p/Prod_Notes");
        assert_eq!(s.active_source_scene, Some(2));
        assert_eq!(
            s.sections[0].production_path.as_deref(),
            Some("/p/Prod_Notes/Scene1.md")
        );
        assert_eq!(
            s.sections[0].sidecar_dir.as_deref(),
            Some("/p/Prod_Notes/Scene1")
        );
        assert_eq!(s.sections[1].production_path, None);
        assert_eq!(
            s.sections[1].sidecar_dir.as_deref(),
            Some("/p/Prod_Notes/Scene2")
        );
        assert!(
            notes
                .iter()
                .any(|n| n.contains("repairs that layout on load")),
            "{notes:?}"
        );
        assert!(
            notes
                .iter()
                .any(|n| n.contains("Scene 2: master production file unknown")),
            "{notes:?}"
        );
        // Re-saving produces clean v4.
        let (again, notes2) = PlanState::load(&s.to_json().unwrap()).unwrap();
        assert_eq!(again, s);
        assert!(notes2.is_empty());
    }

    #[test]
    fn v3_single_flat_prod_md_is_assigned_to_its_only_section() {
        let v3 = r#"{"version":3,"status":"planning","production_path":"/p/prod.md","sidecar_dir":"/p/Scene_Prompts","sections":[{"source_scene":1,"title":"t","heading":"h"}]}"#;
        let (s, _) = PlanState::load(v3).unwrap();
        assert_eq!(s.sections[0].production_path.as_deref(), Some("/p/prod.md"));
        assert_eq!(s.sidecar_root, "/p/Scene_Prompts");
    }

    #[test]
    fn legacy_v2_state_is_refused_with_guidance() {
        let v2 = r#"{"version":2,"status":"done","script_path":"","production_path":"p","sidecar_dir":"s","next_scene_number":4,"sections":[]}"#;
        assert!(PlanState::from_json(v2)
            .unwrap_err()
            .contains("state version 2 is not supported"));
    }
}
