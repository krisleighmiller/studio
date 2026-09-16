//! Reconcile state metadata with what is actually on disk. Runs on every
//! load; idempotent; never guesses when two files disagree.
use crate::scene_plan::commit::PlanPaths;
use crate::scene_plan::extract;
use crate::scene_plan::state::PlanState;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reconcile {
    /// State metadata changed and must be persisted.
    pub changed: bool,
    pub moved: Vec<(PathBuf, PathBuf)>,
    pub removed_duplicates: Vec<PathBuf>,
    pub conflicts: Vec<String>,
    pub notes: Vec<String>,
}

impl Reconcile {
    pub fn clean(&self) -> bool {
        self.conflicts.is_empty()
    }
    pub fn touched_files(&self) -> bool {
        !self.moved.is_empty() || !self.removed_duplicates.is_empty()
    }
    pub fn summary_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (from, to) in &self.moved {
            out.push(format!("moved `{}` -> `{}`", from.display(), to.display()));
        }
        for p in &self.removed_duplicates {
            out.push(format!(
                "removed duplicate `{}` (identical copy already in place)",
                p.display()
            ));
        }
        for c in &self.conflicts {
            out.push(format!("CONFLICT: {c}"));
        }
        out.extend(self.notes.iter().cloned());
        out
    }
}

pub enum Inference {
    Found(PathBuf),
    Ambiguous(Vec<PathBuf>),
    NotFound,
}

fn same_bytes(a: &Path, b: &Path) -> bool {
    matches!((fs::read(a), fs::read(b)), (Ok(x), Ok(y)) if x == y)
}

/// `root/SceneN/SceneN/CutM.txt` -> `root/SceneN/CutM.txt`.
fn repair_doubled_sidecars(root: &Path, source_scene: u32, r: &mut Reconcile) {
    let right = root.join(format!("Scene{source_scene}"));
    let wrong = right.join(format!("Scene{source_scene}"));
    let Ok(read) = fs::read_dir(&wrong) else {
        return;
    };
    let mut files: Vec<PathBuf> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("Cut") && n.ends_with(".txt"))
        })
        .collect();
    files.sort();
    for src in files {
        let dest = right.join(src.file_name().expect("file name"));
        if !dest.exists() {
            match fs::rename(&src, &dest) {
                Ok(()) => r.moved.push((src, dest)),
                Err(e) => r.conflicts.push(format!(
                    "could not move `{}` to `{}`: {e}",
                    src.display(),
                    dest.display()
                )),
            }
        } else if same_bytes(&src, &dest) {
            match fs::remove_file(&src) {
                Ok(()) => r.removed_duplicates.push(src),
                Err(e) => r.conflicts.push(format!(
                    "could not remove duplicate `{}`: {e}",
                    src.display()
                )),
            }
        } else {
            r.conflicts.push(format!("`{}` and `{}` both exist and differ; neither was touched — compare them and delete one", src.display(), dest.display()));
        }
    }
    if fs::read_dir(&wrong)
        .map(|mut d| d.next().is_none())
        .unwrap_or(false)
    {
        let _ = fs::remove_dir(&wrong);
    }
}

fn markdown_files(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file()
                        && p.extension()
                            .and_then(|e| e.to_str())
                            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// `SceneN.md` in a candidate dir wins; otherwise any `*.md` whose headings
/// include `### Scene N — Cut M`. Several matches = ambiguous.
pub fn infer_production_path(candidate_dirs: &[PathBuf], source_scene: u32) -> Inference {
    for dir in candidate_dirs {
        let obvious = dir.join(format!("Scene{source_scene}.md"));
        if obvious.is_file() {
            return Inference::Found(obvious);
        }
    }
    let mut matches = Vec::new();
    for dir in candidate_dirs {
        for md in markdown_files(dir) {
            if fs::metadata(&md)
                .map(|m| m.len() > 8_000_000)
                .unwrap_or(true)
            {
                continue;
            }
            let Ok(text) = fs::read_to_string(&md) else {
                continue;
            };
            if extract::heading_ids(&text)
                .iter()
                .any(|id| id.source_scene == source_scene)
                && !matches.contains(&md)
            {
                matches.push(md);
            }
        }
    }
    match matches.len() {
        0 => Inference::NotFound,
        1 => Inference::Found(matches.remove(0)),
        _ => Inference::Ambiguous(matches),
    }
}

pub fn reconcile(paths: &PlanPaths, state: &mut PlanState) -> Reconcile {
    let mut r = Reconcile::default();
    let root = if state.sidecar_root.is_empty() {
        paths.sidecar_root.clone()
    } else {
        PathBuf::from(&state.sidecar_root)
    };
    let mut candidate_dirs: Vec<PathBuf> = Vec::new();
    for d in [
        Some(root.clone()),
        paths.state.parent().map(Path::to_path_buf),
        paths.prod_md.parent().map(Path::to_path_buf),
    ]
    .into_iter()
    .flatten()
    {
        if !candidate_dirs.contains(&d) {
            candidate_dirs.push(d);
        }
    }
    for section in &mut state.sections {
        repair_doubled_sidecars(&root, section.source_scene, &mut r);
        let want_dir = root
            .join(format!("Scene{}", section.source_scene))
            .to_string_lossy()
            .to_string();
        if section.sidecar_dir.as_deref() != Some(want_dir.as_str()) {
            section.sidecar_dir = Some(want_dir);
            r.changed = true;
        }
        if section.production_path.is_none() {
            match infer_production_path(&candidate_dirs, section.source_scene) {
                Inference::Found(p) => {
                    r.notes.push(format!("Scene {}: master production file recovered as `{}`", section.source_scene, p.display()));
                    section.production_path = Some(p.to_string_lossy().to_string());
                    r.changed = true;
                }
                Inference::Ambiguous(list) => r.conflicts.push(format!(
                    "Scene {}: several files contain its cuts ({}); pass production_path explicitly",
                    section.source_scene,
                    list.iter().map(|p| format!("`{}`", p.display())).collect::<Vec<_>>().join(", ")
                )),
                Inference::NotFound => {
                    if !section.cuts.is_empty() {
                        r.notes.push(format!("Scene {}: no master file with its cuts was found under {}", section.source_scene, candidate_dirs.iter().map(|d| format!("`{}`", d.display())).collect::<Vec<_>>().join(", ")));
                    }
                }
            }
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_plan::fixtures::{cut_markdown, lipsync_cut_yaml, SAM_REF};
    use crate::scene_plan::state::PlanState;
    use std::fs;

    fn cut_md(scene: u32, cut: u32) -> String {
        cut_markdown(
            scene,
            cut,
            &lipsync_cut_yaml(scene, cut, &[("a", "Sam", "Hello there.", true)], SAM_REF),
        )
    }

    /// The real mess: v3 doubled Scene 1/2 sidecars, a clean v4 Scene 3, a
    /// missing Scene 2 production_path, an oddly named Scene 3 master, and a
    /// genuine conflict on Scene 2 Cut 2.
    fn mixed_project(root: &Path) -> PlanPaths {
        let notes = root.join("Prod_Notes");
        for cut in 1..=3 {
            fs::create_dir_all(notes.join("Scene1/Scene1")).unwrap();
            fs::write(
                notes.join(format!("Scene1/Scene1/Cut{cut}.txt")),
                format!("old s1 cut {cut}"),
            )
            .unwrap();
        }
        fs::create_dir_all(notes.join("Scene2/Scene2")).unwrap();
        fs::write(notes.join("Scene2/Scene2/Cut1.txt"), "s2 cut 1").unwrap();
        fs::write(notes.join("Scene2/Scene2/Cut2.txt"), "s2 cut 2 OLD").unwrap();
        fs::write(notes.join("Scene2/Cut2.txt"), "s2 cut 2 NEW").unwrap(); // conflict
        fs::write(notes.join("Scene2/Scene2/Cut3.txt"), "s2 cut 3").unwrap();
        fs::write(notes.join("Scene2/Cut3.txt"), "s2 cut 3").unwrap(); // identical duplicate
        fs::create_dir_all(notes.join("Scene3")).unwrap();
        fs::write(notes.join("Scene3/Cut1.txt"), "s3 cut 1").unwrap();
        fs::write(notes.join("Scene1.md"), cut_md(1, 1)).unwrap();
        fs::write(notes.join("Scene2.md"), cut_md(2, 1)).unwrap();
        fs::write(
            notes.join("Lantern_at_Willowby_Halt_Scene3.md"),
            cut_md(3, 1),
        )
        .unwrap();
        PlanPaths {
            prod_md: notes.join("Scene3.md"),
            sidecar_root: notes.clone(),
            state: notes.join("scene_plan_state.json"),
        }
    }

    fn mixed_state(notes: &Path) -> PlanState {
        let mut s = PlanState::new(notes, Some("9:16".into()));
        let i1 = s.ensure_section(1, "Scene 1", "Scene 1");
        s.sections[i1].cuts = vec![1, 2, 3];
        s.sections[i1].production_path =
            Some(notes.join("Scene1.md").to_string_lossy().to_string());
        let i2 = s.ensure_section(2, "Scene 2", "Scene 2");
        s.sections[i2].cuts = vec![1, 2, 3];
        let i3 = s.ensure_section(3, "Scene 3", "Scene 3");
        s.sections[i3].cuts = vec![1];
        s
    }

    #[test]
    fn repairs_doubled_sidecars_infers_masters_and_reports_conflicts() {
        let dir = tempfile::tempdir().unwrap();
        let paths = mixed_project(dir.path());
        let notes = paths.sidecar_root.clone();
        let mut state = mixed_state(&notes);
        let r = reconcile(&paths, &mut state);

        // Scene 1: all three moved, wrong dir removed.
        for cut in 1..=3 {
            assert_eq!(
                fs::read_to_string(notes.join(format!("Scene1/Cut{cut}.txt"))).unwrap(),
                format!("old s1 cut {cut}")
            );
        }
        assert!(!notes.join("Scene1/Scene1").exists());
        // Scene 2: Cut1 moved, Cut3 duplicate removed, Cut2 conflict left alone.
        assert!(notes.join("Scene2/Cut1.txt").is_file());
        assert!(!notes.join("Scene2/Scene2/Cut3.txt").exists());
        assert!(
            notes.join("Scene2/Scene2/Cut2.txt").is_file(),
            "conflict source must stay"
        );
        assert_eq!(
            fs::read_to_string(notes.join("Scene2/Cut2.txt")).unwrap(),
            "s2 cut 2 NEW"
        );
        assert!(!r.clean());
        assert!(
            r.conflicts.iter().any(
                |c| c.contains("Scene2/Scene2/Cut2.txt") && c.contains("both exist and differ")
            ),
            "{:?}",
            r.conflicts
        );
        assert_eq!(r.moved.len(), 4);
        assert_eq!(r.removed_duplicates.len(), 1);
        // Scene 3 untouched.
        assert_eq!(
            fs::read_to_string(notes.join("Scene3/Cut1.txt")).unwrap(),
            "s3 cut 1"
        );

        // Metadata: Scene 2 -> Scene2.md, Scene 3 -> scanned from headings.
        assert!(r.changed);
        assert_eq!(
            state.sections[1].production_path.as_deref(),
            Some(notes.join("Scene2.md").to_string_lossy().as_ref())
        );
        assert_eq!(
            state.sections[2].production_path.as_deref(),
            Some(
                notes
                    .join("Lantern_at_Willowby_Halt_Scene3.md")
                    .to_string_lossy()
                    .as_ref()
            )
        );
        assert_eq!(
            state.sections[2].sidecar_dir.as_deref(),
            Some(notes.join("Scene3").to_string_lossy().as_ref())
        );
        assert!(r.summary_lines().iter().any(|l| l.starts_with("CONFLICT:")));

        // Idempotent: second pass changes nothing new (conflict persists).
        let mut again = state.clone();
        let r2 = reconcile(&paths, &mut again);
        assert_eq!(again, state);
        assert!(!r2.changed && r2.moved.is_empty() && r2.removed_duplicates.is_empty());
        assert_eq!(r2.conflicts.len(), 1);
    }

    #[test]
    fn ambiguous_masters_are_a_conflict_not_a_guess() {
        let dir = tempfile::tempdir().unwrap();
        let notes = dir.path().join("Prod_Notes");
        fs::create_dir_all(&notes).unwrap();
        fs::write(notes.join("A_Scene2.md"), cut_md(2, 1)).unwrap();
        fs::write(notes.join("B_Scene2.md"), cut_md(2, 1)).unwrap();
        let paths = PlanPaths {
            prod_md: notes.join("x.md"),
            sidecar_root: notes.clone(),
            state: notes.join("state.json"),
        };
        let mut state = PlanState::new(&notes, None);
        let i = state.ensure_section(2, "t", "h");
        state.sections[i].cuts = vec![1];
        let r = reconcile(&paths, &mut state);
        assert!(state.sections[i].production_path.is_none());
        assert!(r.conflicts[0].contains("several files contain its cuts"));
        assert!(matches!(
            infer_production_path(&[notes.clone()], 5),
            Inference::NotFound
        ));
    }
}
