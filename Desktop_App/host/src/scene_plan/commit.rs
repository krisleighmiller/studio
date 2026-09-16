//! Plan (pure) then commit (staged, atomic renames, state last).
use crate::scene_plan::assets::AssetRegistry;
use crate::scene_plan::config::HostConfig;
use crate::scene_plan::coverage;
use crate::scene_plan::error::{CommitError, Issue, IssueKind};
use crate::scene_plan::extract;
use crate::scene_plan::handoff::{CutId, SpeechMode};
use crate::scene_plan::render::{master_block, sidecar_document, sidecar_relative_path};
use crate::scene_plan::state::{CutCoverage, PlanSection, PlanState, PlanStatus, SectionStatus};
use crate::scene_plan::validate::{validate_scene, ValidatedScene};
use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Default)]
pub struct ModelEmit {
    pub files: Vec<(String, String)>,
}

/// Where one turn writes. `sidecar_root` is the *root*: the host appends
/// `SceneN/CutM.txt` itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPaths {
    pub prod_md: PathBuf,
    pub sidecar_root: PathBuf,
    pub state: PathBuf,
}

impl PlanPaths {
    pub fn sidecar_dir_for(&self, source_scene: u32) -> PathBuf {
        self.sidecar_root.join(format!("Scene{source_scene}"))
    }

    /// Host-side guard: one sidecar root per production, one master per scene.
    pub fn check(&self, state: &PlanState, section_index: usize) -> Result<(), String> {
        if !state.sidecar_root.is_empty()
            && Path::new(&state.sidecar_root) != self.sidecar_root.as_path()
        {
            return Err(format!(
                "state records sidecar_root `{}` but this request resolves to `{}`; keep one sidecar root per production",
                state.sidecar_root,
                self.sidecar_root.display()
            ));
        }
        let section = state
            .sections
            .get(section_index)
            .ok_or_else(|| format!("section_index {section_index} is out of range"))?;
        if let Some(recorded) = &section.production_path {
            if Path::new(recorded) != self.prod_md.as_path() {
                return Err(format!(
                    "Scene {} is recorded in `{recorded}` but this request names `{}`; pass the same production_path so the scene's cuts stay in one master (or delete the scene from the state to start over)",
                    section.source_scene,
                    self.prod_md.display()
                ));
            }
        }
        Ok(())
    }
}

/// How a chunk's cuts split between speech modes — shown to the user so a
/// monoculture (nine lip-sync cuts in a row) is visible at a glance.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LaneMix {
    pub lipsync: usize,
    pub voice_over: usize,
    pub silent: usize,
}

impl LaneMix {
    pub fn of(scenes: &[ValidatedScene]) -> Self {
        let mut m = LaneMix::default();
        for v in scenes {
            match v.handoff.speech.mode {
                Some(SpeechMode::SourceVideoLipsync) => m.lipsync += 1,
                Some(SpeechMode::VoiceOver) => m.voice_over += 1,
                _ => m.silent += 1,
            }
        }
        m
    }
}

impl fmt::Display for LaneMix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} lip-sync / {} voice-over / {} silent",
            self.lipsync, self.voice_over, self.silent
        )
    }
}

#[derive(Debug, Clone)]
pub struct PlannedChunk {
    pub source_scene: u32,
    pub scenes: Vec<ValidatedScene>,
    pub fragment: String,
    /// (relative sidecar path like `Scene1/Cut2.txt`, contents)
    pub sidecars: Vec<(String, String)>,
    pub new_state: PlanState,
    pub section_done: bool,
    pub plan_done: bool,
    pub covered_word_count: usize,
    pub total_word_count: usize,
    pub remaining_source_speech: String,
    pub warnings: Vec<String>,
    pub mix: LaneMix,
}

#[derive(Debug, Clone)]
pub struct CommitReport {
    pub source_scene: u32,
    /// Cuts committed in this turn.
    pub cuts: Vec<CutId>,
    /// All cuts of the section after this commit.
    pub section_cuts: Vec<u32>,
    pub sidecars: Vec<PathBuf>,
    pub section_done: bool,
    pub plan_done: bool,
    pub covered_word_count: usize,
    pub total_word_count: usize,
    pub remaining_source_speech: String,
    pub warnings: Vec<String>,
    pub mix: LaneMix,
    /// True when this commit replaced existing cuts instead of appending.
    pub redo: bool,
}

fn sequence_issues(ids: &[CutId], section: &PlanSection) -> Vec<Issue> {
    let mut out = Vec::new();
    if ids.is_empty() {
        return out;
    }
    let mut seen = HashSet::new();
    for id in ids {
        if id.source_scene != section.source_scene {
            out.push(Issue::new(
                IssueKind::Sequence,
                Some(*id),
                format!("this turn plans source Scene {}, but this block is headed {id}; every cut in the chunk must belong to Scene {}", section.source_scene, section.source_scene),
            ));
        }
        if !seen.insert(*id) {
            out.push(Issue::new(
                IssueKind::Sequence,
                Some(*id),
                format!("{id} appears more than once in this chunk"),
            ));
        }
    }
    let expected = section.next_cut_id();
    if ids[0] != expected {
        out.push(Issue::new(
            IssueKind::Sequence,
            Some(ids[0]),
            format!(
                "the chunk must start at {expected} (next unassigned cut) but starts at {}",
                ids[0]
            ),
        ));
    }
    for pair in ids.windows(2) {
        if pair[1].source_scene != pair[0].source_scene || pair[1].cut != pair[0].cut + 1 {
            out.push(Issue::new(
                IssueKind::Sequence,
                Some(pair[1]),
                format!(
                    "cuts must be consecutive and ascending; {} follows {}",
                    pair[1], pair[0]
                ),
            ));
        }
    }
    out
}

/// Parse, validate and plan the whole chunk without touching the file system.
pub fn plan_chunk(
    config: &HostConfig,
    state: &PlanState,
    section_index: usize,
    source_spoken: &str,
    registry: Option<&AssetRegistry>,
    emit: &ModelEmit,
) -> Result<PlannedChunk, Vec<Issue>> {
    let mut issues: Vec<Issue> = Vec::new();
    if let Err(e) = config.validate() {
        return Err(vec![Issue::new(
            IssueKind::Validation,
            None,
            format!("host configuration invalid: {e}"),
        )]);
    }
    let Some(section) = state.sections.get(section_index) else {
        return Err(vec![Issue::new(
            IssueKind::Sequence,
            None,
            format!(
                "section_index {section_index} is out of range ({} sections in state)",
                state.sections.len()
            ),
        )]);
    };
    if emit.files.is_empty() {
        return Err(vec![Issue::new(IssueKind::Parse, None, "the model emitted no files; expected Markdown with one `### Scene N — Cut M` block per cut")]);
    }

    let mut blocks = Vec::new();
    for (path, contents) in &emit.files {
        match extract::scene_blocks_from_text(path, contents) {
            Ok(found) => blocks.extend(found),
            Err(found) => issues.extend(found),
        }
    }
    if blocks.is_empty() && issues.is_empty() {
        issues.push(Issue::new(
            IssueKind::Parse,
            None,
            "no cut blocks found in the emitted files",
        ));
    }

    let ids: Vec<CutId> = blocks.iter().map(|b| b.heading).collect();
    issues.extend(sequence_issues(&ids, section));

    let mut validated: Vec<ValidatedScene> = Vec::new();
    for block in &blocks {
        match extract::parse_scene_block(block) {
            Err(issue) => issues.push(issue),
            Ok(handoff) => {
                match validate_scene(handoff, config, state.aspect_ratio.as_deref(), registry) {
                    Ok(v) => validated.push(v),
                    Err(found) => issues.extend(found),
                }
            }
        }
    }

    let mut coverage_result = None;
    if issues.is_empty() {
        let words: Vec<String> = validated
            .iter()
            .flat_map(|v| v.spoken_words.iter().cloned())
            .collect();
        match coverage::consume_prefix(source_spoken, section.covered_word_count, &words) {
            Ok(c) => coverage_result = Some(c),
            Err(issue) => issues.push(issue),
        }
    }
    if !issues.is_empty() {
        return Err(issues);
    }
    let cov = coverage_result.expect("coverage computed when there are no issues");

    let fragment = validated
        .iter()
        .map(|v| master_block(&v.handoff, &v.canonical_yaml))
        .collect::<Vec<_>>()
        .join("\n");
    let sidecars = validated
        .iter()
        .map(|v| {
            (
                sidecar_relative_path(v.id()),
                sidecar_document(&v.handoff, &v.canonical_yaml),
            )
        })
        .collect();

    let total_word_count = coverage::total_words(source_spoken);
    let section_done = cov.covered_word_count >= total_word_count;
    let last = validated.last().map(|v| v.id()).expect("at least one cut");
    let mut new_state = state.clone();
    {
        let s = &mut new_state.sections[section_index];
        let mut pos = section.covered_word_count;
        for v in &validated {
            let n = v.spoken_words.len();
            s.cut_coverage.retain(|c| c.cut != v.id().cut);
            s.cut_coverage.push(CutCoverage {
                cut: v.id().cut,
                word_start: pos,
                word_end: pos + n,
            });
            pos += n;
        }
        s.covered_word_count = cov.covered_word_count;
        s.attempts += 1;
        s.cuts.extend(validated.iter().map(|v| v.id().cut));
        s.next_cut = last.cut + 1;
        s.status = if section_done {
            SectionStatus::Done
        } else {
            SectionStatus::InProgress
        };
    }
    new_state.active_source_scene = Some(section.source_scene);
    let plan_done = new_state
        .sections
        .iter()
        .all(|s| s.status == SectionStatus::Done);
    new_state.status = if plan_done {
        PlanStatus::Done
    } else {
        PlanStatus::Planning
    };

    let warnings = validated
        .iter()
        .flat_map(|v| v.warnings.iter().cloned())
        .collect();
    let mix = LaneMix::of(&validated);
    Ok(PlannedChunk {
        source_scene: section.source_scene,
        scenes: validated,
        fragment,
        sidecars,
        new_state,
        section_done,
        plan_done,
        covered_word_count: cov.covered_word_count,
        total_word_count,
        remaining_source_speech: cov.remaining_text,
        warnings,
        mix,
    })
}

pub(crate) struct StagedFile {
    final_path: PathBuf,
    staging_path: PathBuf,
    previous: Option<Vec<u8>>,
}

fn staging_path_for(final_path: &Path) -> PathBuf {
    let name = final_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    final_path.with_file_name(format!("{name}.staging-{}-{nanos}", std::process::id()))
}

fn stage_one(final_path: &Path, bytes: &[u8]) -> Result<StagedFile, String> {
    if let Some(parent) = final_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
    }
    let previous = if final_path.is_file() {
        Some(fs::read(final_path).map_err(|e| format!("read {}: {e}", final_path.display()))?)
    } else {
        None
    };
    let staging_path = staging_path_for(final_path);
    let mut file = fs::File::create(&staging_path)
        .map_err(|e| format!("create {}: {e}", staging_path.display()))?;
    file.write_all(bytes)
        .map_err(|e| format!("write {}: {e}", staging_path.display()))?;
    file.sync_all()
        .map_err(|e| format!("sync {}: {e}", staging_path.display()))?;
    Ok(StagedFile {
        final_path: final_path.to_path_buf(),
        staging_path,
        previous,
    })
}

pub(crate) fn stage_all(entries: Vec<(PathBuf, Vec<u8>)>) -> Result<Vec<StagedFile>, CommitError> {
    let mut staged = Vec::with_capacity(entries.len());
    for (final_path, bytes) in entries {
        match stage_one(&final_path, &bytes) {
            Ok(s) => staged.push(s),
            Err(e) => {
                for s in &staged {
                    let _ = fs::remove_file(&s.staging_path);
                }
                return Err(CommitError::Host(e));
            }
        }
    }
    Ok(staged)
}

pub(crate) fn commit_staged(staged: Vec<StagedFile>) -> Result<(), CommitError> {
    let mut done: Vec<&StagedFile> = Vec::new();
    for s in &staged {
        if let Err(e) = fs::rename(&s.staging_path, &s.final_path) {
            let failure = format!(
                "rename {} -> {}: {e}",
                s.staging_path.display(),
                s.final_path.display()
            );
            let mut rollback_errors = Vec::new();
            for d in done.iter().rev() {
                let restored = match &d.previous {
                    Some(bytes) => fs::write(&d.final_path, bytes),
                    None => fs::remove_file(&d.final_path),
                };
                if let Err(e) = restored {
                    rollback_errors.push(format!("restore {}: {e}", d.final_path.display()));
                }
            }
            for s in &staged {
                let _ = fs::remove_file(&s.staging_path);
            }
            return if rollback_errors.is_empty() {
                Err(CommitError::RolledBack(failure))
            } else {
                Err(CommitError::RollbackFailed {
                    failure,
                    rollback_failure: rollback_errors.join("; "),
                })
            };
        }
        done.push(s);
    }
    Ok(())
}

/// Write one file atomically (stage + rename). Used for state persistence
/// outside a chunk commit.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let staged = stage_one(path, bytes)?;
    fs::rename(&staged.staging_path, &staged.final_path).map_err(|e| {
        let _ = fs::remove_file(&staged.staging_path);
        format!(
            "rename {} -> {}: {e}",
            staged.staging_path.display(),
            staged.final_path.display()
        )
    })
}

pub fn commit_model_chunk(
    paths: &PlanPaths,
    config: &HostConfig,
    state: &mut PlanState,
    section_index: usize,
    source_spoken: &str,
    registry: Option<&AssetRegistry>,
    emit: &ModelEmit,
) -> Result<CommitReport, CommitError> {
    paths
        .check(state, section_index)
        .map_err(CommitError::Host)?;
    let mut planned = plan_chunk(config, state, section_index, source_spoken, registry, emit)
        .map_err(CommitError::Rejected)?;

    // Record where this scene lives; the state's paths are per section.
    planned.new_state.sidecar_root = paths.sidecar_root.to_string_lossy().to_string();
    {
        let s = &mut planned.new_state.sections[section_index];
        s.production_path = Some(paths.prod_md.to_string_lossy().to_string());
        s.sidecar_dir = Some(
            paths
                .sidecar_dir_for(s.source_scene)
                .to_string_lossy()
                .to_string(),
        );
    }

    let existing = if paths.prod_md.is_file() {
        fs::read_to_string(&paths.prod_md)
            .map_err(|e| CommitError::Host(format!("read {}: {e}", paths.prod_md.display())))?
    } else {
        String::new()
    };
    let existing_ids = extract::heading_ids(&existing);
    for scene in &planned.scenes {
        if existing_ids.contains(&scene.id()) {
            return Err(CommitError::Host(format!(
                "{} already contains {}; production file and state disagree, refusing to append",
                paths.prod_md.display(),
                scene.id()
            )));
        }
    }

    let mut prod = existing.trim_end().to_string();
    if !prod.is_empty() {
        prod.push_str("\n\n");
    }
    prod.push_str(planned.fragment.trim_end());
    prod.push('\n');

    let state_json = planned.new_state.to_json().map_err(CommitError::Host)?;
    let mut entries: Vec<(PathBuf, Vec<u8>)> = planned
        .sidecars
        .iter()
        .map(|(rel, contents)| (paths.sidecar_root.join(rel), contents.clone().into_bytes()))
        .collect();
    entries.push((paths.prod_md.clone(), prod.into_bytes()));
    entries.push((paths.state.clone(), state_json.into_bytes())); // state last: commit marker

    let staged = stage_all(entries)?;
    commit_staged(staged)?;

    let section_cuts = planned.new_state.sections[section_index].cuts.clone();
    *state = planned.new_state.clone();
    Ok(CommitReport {
        source_scene: planned.source_scene,
        cuts: planned.scenes.iter().map(|v| v.id()).collect(),
        section_cuts,
        sidecars: planned
            .sidecars
            .iter()
            .map(|(rel, _)| paths.sidecar_root.join(rel))
            .collect(),
        section_done: planned.section_done,
        plan_done: planned.plan_done,
        covered_word_count: planned.covered_word_count,
        total_word_count: planned.total_word_count,
        remaining_source_speech: planned.remaining_source_speech,
        warnings: planned.warnings,
        mix: planned.mix,
        redo: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_plan::error::retry_feedback;
    use crate::scene_plan::fixtures::{
        cut_markdown, emit, lipsync_cut_yaml, lipsync_scene_yaml, plan_state, scene_markdown,
        SAM_REF,
    };

    const SOURCE: &str = "This is a twelve word sentence that needs a reliable duration estimate. Then Sam turns and leaves the room.";
    const S1: &str = "This is a twelve word sentence that needs a reliable duration estimate.";
    const S2: &str = "Then Sam turns and leaves the room.";

    fn has(issues: &[Issue], kind: IssueKind, needle: &str) -> bool {
        issues
            .iter()
            .any(|i| i.kind == kind && i.message.contains(needle))
    }

    #[test]
    fn sequence_rules_use_cuts_within_the_source_scene() {
        let section = PlanSection::new(1, "t", "h");
        let c = |n| CutId::new(1, n);
        assert!(sequence_issues(&[c(1), c(2), c(3)], &section).is_empty());
        assert!(has(
            &sequence_issues(&[c(1), c(1)], &section),
            IssueKind::Sequence,
            "appears more than once"
        ));
        assert!(has(
            &sequence_issues(&[c(1), c(3)], &section),
            IssueKind::Sequence,
            "Scene 1 — Cut 3 follows Scene 1 — Cut 1"
        ));
        assert!(has(
            &sequence_issues(&[c(2)], &section),
            IssueKind::Sequence,
            "must start at Scene 1 — Cut 1"
        ));
        assert!(has(
            &sequence_issues(&[CutId::new(2, 1)], &section),
            IssueKind::Sequence,
            "this turn plans source Scene 1"
        ));
    }

    #[test]
    fn plan_chunk_records_cut_coverage_and_mix() {
        let state = plan_state(1);
        let e = emit(&[(
            "prod.md",
            format!(
                "{}{}",
                scene_markdown(1, &lipsync_scene_yaml(1, S1)),
                scene_markdown(2, &crate::scene_plan::fixtures::broll_scene_yaml(2))
            ),
        )]);
        let planned = plan_chunk(&HostConfig::default(), &state, 0, SOURCE, None, &e).unwrap();
        let s = &planned.new_state.sections[0];
        assert_eq!(
            s.cut_coverage,
            vec![
                CutCoverage {
                    cut: 1,
                    word_start: 0,
                    word_end: 12
                },
                CutCoverage {
                    cut: 2,
                    word_start: 12,
                    word_end: 12
                }
            ]
        );
        assert_eq!(
            planned.mix,
            LaneMix {
                lipsync: 1,
                voice_over: 0,
                silent: 1
            }
        );
        assert_eq!(
            planned.mix.to_string(),
            "1 lip-sync / 0 voice-over / 1 silent"
        );
    }

    #[test]
    fn plan_chunk_builds_fragment_sidecars_and_state() {
        let state = plan_state(1);
        let e = emit(&[("prod.md", scene_markdown(1, &lipsync_scene_yaml(1, S1)))]);
        let planned = plan_chunk(&HostConfig::default(), &state, 0, SOURCE, None, &e).unwrap();
        assert_eq!(planned.source_scene, 1);
        assert!(planned.fragment.starts_with("### Scene 1 — Cut 1"));
        assert_eq!(planned.sidecars[0].0, "Scene1/Cut1.txt");
        assert!(!planned.section_done);
        assert_eq!(
            (planned.covered_word_count, planned.total_word_count),
            (12, 19)
        );
        assert_eq!(planned.remaining_source_speech, S2);
        assert_eq!(planned.new_state.active_source_scene, Some(1));
        let s = &planned.new_state.sections[0];
        assert_eq!(
            (s.next_cut, s.cuts.clone(), s.status),
            (2, vec![1], SectionStatus::InProgress)
        );
    }

    #[test]
    fn section_done_only_when_dialogue_fully_covered_and_plan_done_when_all_sections_are() {
        let mut state = plan_state(2);
        state.sections[0].covered_word_count = 12;
        state.sections[0].attempts = 99;
        state.sections[0].next_cut = 2;
        let partial = emit(&[(
            "prod.md",
            scene_markdown(2, &lipsync_scene_yaml(2, "Then Sam turns")),
        )]);
        let planned =
            plan_chunk(&HostConfig::default(), &state, 0, SOURCE, None, &partial).unwrap();
        assert!(!planned.section_done);
        assert_eq!(planned.new_state.sections[0].attempts, 100);

        let full = emit(&[("prod.md", scene_markdown(2, &lipsync_scene_yaml(2, S2)))]);
        let planned = plan_chunk(&HostConfig::default(), &state, 0, SOURCE, None, &full).unwrap();
        assert!(planned.section_done && !planned.plan_done);

        let s2 = emit(&[(
            "prod.md",
            cut_markdown(
                2,
                1,
                &lipsync_cut_yaml(2, 1, &[("a", "Sam", "Here we are.", true)], SAM_REF),
            ),
        )]);
        let planned2 = plan_chunk(
            &HostConfig::default(),
            &planned.new_state,
            1,
            "Here we are.",
            None,
            &s2,
        )
        .unwrap();
        assert!(planned2.plan_done);
        assert_eq!(planned2.new_state.active_source_scene, Some(2));
        assert_eq!(planned2.sidecars[0].0, "Scene2/Cut1.txt");
    }

    #[test]
    fn plan_paths_check_guards_root_and_master() {
        let paths = PlanPaths {
            prod_md: PathBuf::from("/p/Prod_Notes/Scene2.md"),
            sidecar_root: PathBuf::from("/p/Prod_Notes"),
            state: PathBuf::from("/p/Prod_Notes/state.json"),
        };
        assert_eq!(
            paths.sidecar_dir_for(2),
            PathBuf::from("/p/Prod_Notes/Scene2")
        );
        let mut state = PlanState::new(Path::new("/p/Prod_Notes"), None);
        let i = state.ensure_section(2, "t", "h");
        paths.check(&state, i).unwrap();
        state.sections[i].production_path = Some("/p/Prod_Notes/Scene2.md".into());
        paths.check(&state, i).unwrap();
        state.sections[i].production_path = Some("/p/Prod_Notes/Scene1.md".into());
        assert!(paths
            .check(&state, i)
            .unwrap_err()
            .contains("Scene 2 is recorded in `/p/Prod_Notes/Scene1.md`"));
        state.sections[i].production_path = None;
        state.sidecar_root = "/elsewhere".into();
        assert!(paths
            .check(&state, i)
            .unwrap_err()
            .contains("keep one sidecar root per production"));
        assert!(paths
            .check(&PlanState::new(Path::new("/p/Prod_Notes"), None), 0)
            .unwrap_err()
            .contains("out of range"));
    }

    #[test]
    fn all_problems_are_reported_together() {
        let state = plan_state(1);
        let e = emit(&[(
            "prod.md",
            format!(
                "{}{}",
                scene_markdown(2, &lipsync_scene_yaml(2, S1)),
                scene_markdown(
                    4,
                    &lipsync_scene_yaml(4, S2).replace("stage: base_video", "stage: nope")
                )
            ),
        )]);
        let issues = plan_chunk(&HostConfig::default(), &state, 0, SOURCE, None, &e).unwrap_err();
        let fb = retry_feedback(&issues);
        assert!(
            has(
                &issues,
                IssueKind::Sequence,
                "must start at Scene 1 — Cut 1"
            ),
            "{fb}"
        );
        assert!(
            has(
                &issues,
                IssueKind::Sequence,
                "Scene 1 — Cut 4 follows Scene 1 — Cut 2"
            ),
            "{fb}"
        );
        assert!(
            has(&issues, IssueKind::Parse, "unknown variant `nope`"),
            "{fb}"
        );
    }

    #[test]
    fn cue_text_must_continue_the_source_speech() {
        let state = plan_state(1);
        let e = emit(&[(
            "prod.md",
            scene_markdown(
                1,
                &lipsync_scene_yaml(1, "This is a thirteen word sentence."),
            ),
        )]);
        let issues = plan_chunk(&HostConfig::default(), &state, 0, SOURCE, None, &e).unwrap_err();
        assert!(has(
            &issues,
            IssueKind::Coverage,
            "expected `twelve` but the cue says `thirteen`"
        ));
    }

    #[test]
    fn bad_section_index_and_empty_emit_are_rejected() {
        let state = plan_state(1);
        assert!(has(
            &plan_chunk(
                &HostConfig::default(),
                &state,
                5,
                SOURCE,
                None,
                &ModelEmit::default()
            )
            .unwrap_err(),
            IssueKind::Sequence,
            "out of range"
        ));
        assert!(has(
            &plan_chunk(
                &HostConfig::default(),
                &state,
                0,
                SOURCE,
                None,
                &ModelEmit::default()
            )
            .unwrap_err(),
            IssueKind::Parse,
            "emitted no files"
        ));
    }
}
