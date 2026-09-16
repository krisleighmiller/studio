//! "Redo Cut X": replace committed cuts without disturbing coverage.
use crate::scene_plan::assets::AssetRegistry;
use crate::scene_plan::commit::{
    commit_staged, stage_all, CommitReport, LaneMix, ModelEmit, PlanPaths,
};
use crate::scene_plan::config::HostConfig;
use crate::scene_plan::coverage;
use crate::scene_plan::error::{CommitError, Issue, IssueKind};
use crate::scene_plan::extract;
use crate::scene_plan::handoff::{CutId, SceneHandoff};
use crate::scene_plan::render::{
    canonical_yaml, master_block, sidecar_document, sidecar_relative_path,
};
use crate::scene_plan::state::{CutCoverage, PlanSection, PlanState, SectionStatus};
use crate::scene_plan::timing::normalized_words;
use crate::scene_plan::validate::{validate_scene, ValidatedScene};
use regex::Regex;
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

fn numbers_in(s: &str) -> Vec<u32> {
    s.split(|c: char| !c.is_ascii_digit())
        .filter_map(|p| p.parse().ok())
        .collect()
}

fn finish(cuts: Vec<u32>, from: &str) -> Result<Option<Vec<u32>>, String> {
    let set: BTreeSet<u32> = cuts.into_iter().collect();
    if set.is_empty() || set.contains(&0) {
        return Err(format!("redo request ({from}) must name cut numbers >= 1, e.g. `redo cut 3` or `redo cuts 3, 5`"));
    }
    Ok(Some(set.into_iter().collect()))
}

/// `redo_cuts` / `redo_cut` / `redo` inputs (number, list, or "3, 5"), or
/// "redo cut 3" / "redo cuts 3 and 5" in the brief. Sorted, de-duplicated.
pub fn parse_redo(
    inputs: &serde_json::Map<String, serde_json::Value>,
    brief: &str,
) -> Result<Option<Vec<u32>>, String> {
    for key in ["redo_cuts", "redo_cut", "redo"] {
        if let Some(v) = inputs.get(key) {
            let cuts = match v {
                serde_json::Value::Array(items) => items
                    .iter()
                    .map(|i| {
                        i.as_u64()
                            .map(|n| n as u32)
                            .or_else(|| i.as_str().and_then(|s| s.trim().parse().ok()))
                            .ok_or_else(|| format!("`{key}` entries must be cut numbers"))
                    })
                    .collect::<Result<Vec<u32>, String>>()?,
                serde_json::Value::Number(n) => vec![n
                    .as_u64()
                    .ok_or_else(|| format!("`{key}` must be a positive integer"))?
                    as u32],
                serde_json::Value::String(s) => numbers_in(s),
                _ => {
                    return Err(format!(
                        "`{key}` must be a number, a list, or a string like \"3, 5\""
                    ))
                }
            };
            return finish(cuts, key);
        }
    }
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)\bre-?do\s+cuts?\s+((?:\d+\s*(?:,|and|&)?\s*)+)").unwrap()
    });
    if let Some(caps) = re.captures(brief) {
        return finish(numbers_in(&caps[1]), "brief");
    }
    Ok(None)
}

fn committed_handoffs(
    prod_text: &str,
    source_scene: u32,
) -> Result<HashMap<u32, SceneHandoff>, String> {
    let blocks = extract::scene_blocks_from_text("master", prod_text).map_err(|issues| {
        issues
            .iter()
            .map(|i| i.message.clone())
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    let mut map = HashMap::new();
    for b in blocks {
        if b.heading.source_scene == source_scene {
            map.insert(
                b.heading.cut,
                extract::parse_scene_block(&b).map_err(|i| i.message)?,
            );
        }
    }
    Ok(map)
}

/// Fill `cut_coverage` for sections planned before the field existed by
/// replaying the committed cuts against the source dialogue. Returns whether
/// the section changed.
pub fn ensure_cut_coverage(
    section: &mut PlanSection,
    prod_text: &str,
    source_spoken: &str,
) -> Result<bool, String> {
    if section
        .cuts
        .iter()
        .all(|c| section.coverage_for(*c).is_some())
    {
        return Ok(false);
    }
    let handoffs = committed_handoffs(prod_text, section.source_scene)?;
    let mut pos = 0usize;
    let mut coverage = Vec::new();
    for cut in &section.cuts {
        let h = handoffs.get(cut).ok_or_else(|| {
            format!(
                "Scene {} — Cut {cut} is in the state but not in the master file",
                section.source_scene
            )
        })?;
        let words: Vec<String> = h
            .speech
            .cues
            .iter()
            .flat_map(|c| normalized_words(&c.text))
            .collect();
        let cov = coverage::consume_prefix(source_spoken, pos, &words).map_err(|i| {
            format!(
                "cannot reconstruct dialogue coverage for Scene {} — Cut {cut}: {}",
                section.source_scene, i.message
            )
        })?;
        coverage.push(CutCoverage {
            cut: *cut,
            word_start: pos,
            word_end: cov.covered_word_count,
        });
        pos = cov.covered_word_count;
    }
    section.cut_coverage = coverage;
    Ok(true)
}

#[derive(Debug, Clone, PartialEq)]
pub struct CutContext {
    pub id: CutId,
    /// Exact dialogue this cut must keep ("" for a silent cut).
    pub dialogue: String,
    pub previous_yaml: String,
    pub prev_closing_state: Option<String>,
    pub next_opening_state: Option<String>,
}

pub fn cut_contexts(
    section: &PlanSection,
    prod_text: &str,
    source_spoken: &str,
    cuts: &[u32],
) -> Result<Vec<CutContext>, String> {
    let handoffs = committed_handoffs(prod_text, section.source_scene)?;
    cuts.iter()
        .map(|cut| {
            if !section.cuts.contains(cut) {
                return Err(format!(
                    "Scene {} has no Cut {cut}; committed cuts are {:?}",
                    section.source_scene, section.cuts
                ));
            }
            let h = handoffs.get(cut).ok_or_else(|| {
                format!(
                    "Scene {} — Cut {cut} is missing from the master file",
                    section.source_scene
                )
            })?;
            let cov = section.coverage_for(*cut).ok_or_else(|| {
                format!(
                    "Scene {} — Cut {cut} has no dialogue coverage record",
                    section.source_scene
                )
            })?;
            Ok(CutContext {
                id: CutId::new(section.source_scene, *cut),
                dialogue: coverage::text_between(source_spoken, cov.word_start, cov.word_end),
                previous_yaml: canonical_yaml(h)?,
                prev_closing_state: handoffs
                    .get(&(cut - 1))
                    .and_then(|p| p.continuity.closing_state.clone()),
                next_opening_state: handoffs
                    .get(&(cut + 1))
                    .and_then(|n| n.continuity.opening_state.clone()),
            })
        })
        .collect()
}

/// Replace `cuts` of the section with the model's re-emitted blocks.
pub fn commit_redo(
    paths: &PlanPaths,
    config: &HostConfig,
    state: &mut PlanState,
    section_index: usize,
    source_spoken: &str,
    registry: Option<&AssetRegistry>,
    cuts: &[u32],
    emit: &ModelEmit,
) -> Result<CommitReport, CommitError> {
    paths
        .check(state, section_index)
        .map_err(CommitError::Host)?;
    let prod_text = fs::read_to_string(&paths.prod_md)
        .map_err(|e| CommitError::Host(format!("read master {}: {e}", paths.prod_md.display())))?;
    let mut new_state = state.clone();
    ensure_cut_coverage(
        &mut new_state.sections[section_index],
        &prod_text,
        source_spoken,
    )
    .map_err(CommitError::Host)?;
    let section = new_state.sections[section_index].clone();
    let source_scene = section.source_scene;

    let mut issues: Vec<Issue> = Vec::new();
    let mut blocks = Vec::new();
    for (path, contents) in &emit.files {
        match extract::scene_blocks_from_text(path, contents) {
            Ok(found) => blocks.extend(found),
            Err(found) => issues.extend(found),
        }
    }
    let ids: Vec<CutId> = blocks.iter().map(|b| b.heading).collect();
    let expected: Vec<CutId> = cuts.iter().map(|c| CutId::new(source_scene, *c)).collect();
    if ids != expected {
        let list = |v: &[CutId]| v.iter().map(|c| c.code()).collect::<Vec<_>>().join(", ");
        issues.push(Issue::new(
            IssueKind::Sequence,
            None,
            format!(
                "a redo turn must re-emit exactly [{}] in that order and nothing else; got [{}]",
                list(&expected),
                list(&ids)
            ),
        ));
    }
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
    if issues.is_empty() {
        for v in &validated {
            let Some(cov) = section.coverage_for(v.id().cut) else {
                issues.push(Issue::new(
                    IssueKind::Sequence,
                    Some(v.id()),
                    "this cut has no dialogue coverage record and cannot be redone",
                ));
                continue;
            };
            let original = coverage::text_between(source_spoken, cov.word_start, cov.word_end);
            let original_label = if original.is_empty() {
                "no dialogue".to_string()
            } else {
                format!("\"{original}\"")
            };
            match coverage::consume_prefix(source_spoken, cov.word_start, &v.spoken_words) {
                Ok(c) if c.covered_word_count == cov.word_end => {}
                Ok(c) => issues.push(Issue::new(
                    IssueKind::Coverage,
                    Some(v.id()),
                    format!("{} must carry exactly its original dialogue ({original_label}) but its cues cover {} of {} words; the line may play as voice-over over a different image, but it cannot move to another cut", v.id(), c.consumed_words, cov.word_end - cov.word_start),
                )),
                Err(i) => issues.push(Issue::new(IssueKind::Coverage, Some(v.id()), format!("{} must carry exactly its original dialogue ({original_label}): {}", v.id(), i.message))),
            }
        }
    }
    if !issues.is_empty() {
        return Err(CommitError::Rejected(issues));
    }

    let mut prod = prod_text;
    for v in &validated {
        prod =
            extract::replace_cut_block(&prod, v.id(), &master_block(&v.handoff, &v.canonical_yaml))
                .map_err(CommitError::Host)?;
    }
    new_state.sections[section_index].attempts += 1;
    new_state.active_source_scene = Some(source_scene);
    let state_json = new_state.to_json().map_err(CommitError::Host)?;
    let sidecars: Vec<(String, String)> = validated
        .iter()
        .map(|v| {
            (
                sidecar_relative_path(v.id()),
                sidecar_document(&v.handoff, &v.canonical_yaml),
            )
        })
        .collect();
    let mut entries: Vec<(PathBuf, Vec<u8>)> = sidecars
        .iter()
        .map(|(rel, c)| (paths.sidecar_root.join(rel), c.clone().into_bytes()))
        .collect();
    entries.push((paths.prod_md.clone(), prod.into_bytes()));
    entries.push((paths.state.clone(), state_json.into_bytes()));
    let staged = stage_all(entries)?;
    commit_staged(staged)?;

    *state = new_state;
    let section = &state.sections[section_index];
    let total = coverage::total_words(source_spoken);
    Ok(CommitReport {
        source_scene,
        cuts: validated.iter().map(|v| v.id()).collect(),
        section_cuts: section.cuts.clone(),
        sidecars: sidecars
            .iter()
            .map(|(rel, _)| paths.sidecar_root.join(rel))
            .collect(),
        section_done: section.status == SectionStatus::Done,
        plan_done: state.is_done(),
        covered_word_count: section.covered_word_count,
        total_word_count: total,
        remaining_source_speech: coverage::remaining_text(
            source_spoken,
            section.covered_word_count,
        ),
        warnings: validated
            .iter()
            .flat_map(|v| v.warnings.iter().cloned())
            .collect(),
        mix: LaneMix::of(&validated),
        redo: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_plan::commit::commit_model_chunk;
    use crate::scene_plan::fixtures::{
        broll_scene_yaml, emit, lipsync_scene_yaml, plan_state, scene_markdown, voiceover_cut_yaml,
    };
    use serde_json::json;

    const SOURCE: &str = "This is a twelve word sentence that needs a reliable duration estimate. Then Sam turns and leaves the room.";
    const S1: &str = "This is a twelve word sentence that needs a reliable duration estimate.";
    const S2: &str = "Then Sam turns and leaves the room.";

    #[test]
    fn parse_redo_from_inputs_and_brief() {
        let mut inputs = serde_json::Map::new();
        assert_eq!(
            parse_redo(&inputs, "Please redo cut 3 — I'd rather see the train here").unwrap(),
            Some(vec![3])
        );
        assert_eq!(
            parse_redo(&inputs, "Redo cuts 5, 3 and 5").unwrap(),
            Some(vec![3, 5])
        );
        assert_eq!(
            parse_redo(&inputs, "Create the notes for Scene 3").unwrap(),
            None
        );
        inputs.insert("redo_cuts".into(), json!([4, "2"]));
        assert_eq!(parse_redo(&inputs, "").unwrap(), Some(vec![2, 4]));
        inputs.insert("redo_cuts".into(), json!("0"));
        assert!(parse_redo(&inputs, "").unwrap_err().contains(">= 1"));
        inputs.insert("redo_cuts".into(), json!({"x": 1}));
        assert!(parse_redo(&inputs, "").is_err());
    }

    fn committed_project() -> (tempfile::TempDir, PlanPaths, PlanState) {
        let dir = tempfile::tempdir().unwrap();
        let paths = PlanPaths {
            prod_md: dir.path().join("prod.md"),
            sidecar_root: dir.path().join("sc"),
            state: dir.path().join("state.json"),
        };
        let mut state = plan_state(1);
        state.sidecar_root = paths.sidecar_root.to_string_lossy().to_string();
        let e = emit(&[(
            "prod.md",
            format!(
                "{}{}{}",
                scene_markdown(1, &lipsync_scene_yaml(1, S1)),
                scene_markdown(2, &broll_scene_yaml(2)),
                scene_markdown(3, &lipsync_scene_yaml(3, S2))
            ),
        )]);
        commit_model_chunk(
            &paths,
            &HostConfig::default(),
            &mut state,
            0,
            SOURCE,
            None,
            &e,
        )
        .unwrap();
        (dir, paths, state)
    }

    #[test]
    fn coverage_is_reconstructed_from_the_master_when_missing() {
        let (_dir, paths, mut state) = committed_project();
        let prod = fs::read_to_string(&paths.prod_md).unwrap();
        let recorded = state.sections[0].cut_coverage.clone();
        state.sections[0].cut_coverage.clear(); // pretend it was planned before the field existed
        assert!(ensure_cut_coverage(&mut state.sections[0], &prod, SOURCE).unwrap());
        assert_eq!(state.sections[0].cut_coverage, recorded);
        assert!(!ensure_cut_coverage(&mut state.sections[0], &prod, SOURCE).unwrap());
        let ctx = cut_contexts(&state.sections[0], &prod, SOURCE, &[2, 3]).unwrap();
        assert_eq!(ctx[0].dialogue, "");
        assert_eq!(ctx[1].dialogue, S2);
        assert_eq!(ctx[1].prev_closing_state, None); // b-roll fixture has no closing_state
        assert!(ctx[0]
            .prev_closing_state
            .as_deref()
            .unwrap()
            .contains("Sam settles"));
        assert!(cut_contexts(&state.sections[0], &prod, SOURCE, &[7])
            .unwrap_err()
            .contains("has no Cut 7"));
    }

    #[test]
    fn redo_replaces_one_block_and_sidecar_and_keeps_coverage() {
        let (_dir, paths, mut state) = committed_project();
        let before = state.clone();
        // Cut 3 becomes voice-over over an insert: same words, different lane.
        let redo = emit(&[(
            "prod.md",
            scene_markdown(3, &voiceover_cut_yaml(1, 3, &[("a", "Sam", S2, false)])),
        )]);
        let report = commit_redo(
            &paths,
            &HostConfig::default(),
            &mut state,
            0,
            SOURCE,
            None,
            &[3],
            &redo,
        )
        .unwrap();
        assert!(report.redo);
        assert_eq!(report.cuts, vec![CutId::new(1, 3)]);
        assert_eq!(report.mix.voice_over, 1);
        assert_eq!(report.section_cuts, vec![1, 2, 3]);
        assert!(report.section_done);
        let prod = fs::read_to_string(&paths.prod_md).unwrap();
        assert_eq!(
            extract::heading_ids(&prod),
            vec![CutId::new(1, 1), CutId::new(1, 2), CutId::new(1, 3)]
        );
        assert!(prod.contains("### Scene 1 — Cut 3 — Rain on the window"));
        assert!(!prod.contains("Cut 3 — Sam addresses the camera"));
        let sidecar = fs::read_to_string(paths.sidecar_root.join("Scene1/Cut3.txt")).unwrap();
        assert!(sidecar.contains("mode: voice_over"));
        assert_eq!(
            state.sections[0].cut_coverage,
            before.sections[0].cut_coverage
        );
        assert_eq!(
            state.sections[0].covered_word_count,
            before.sections[0].covered_word_count
        );
        assert_eq!(state.sections[0].attempts, before.sections[0].attempts + 1);
    }

    #[test]
    fn redo_rejects_wrong_ids_and_wrong_dialogue_without_writing() {
        let (_dir, paths, mut state) = committed_project();
        let prod_before = fs::read_to_string(&paths.prod_md).unwrap();
        let wrong_id = emit(&[("prod.md", scene_markdown(2, &broll_scene_yaml(2)))]);
        let err = commit_redo(
            &paths,
            &HostConfig::default(),
            &mut state,
            0,
            SOURCE,
            None,
            &[3],
            &wrong_id,
        )
        .unwrap_err();
        assert!(
            err.issues()
                .iter()
                .any(|i| i.message.contains("must re-emit exactly [S01C03]")),
            "{err}"
        );
        let wrong_words = emit(&[(
            "prod.md",
            scene_markdown(3, &lipsync_scene_yaml(3, "Then Sam turns")),
        )]);
        let err = commit_redo(
            &paths,
            &HostConfig::default(),
            &mut state,
            0,
            SOURCE,
            None,
            &[3],
            &wrong_words,
        )
        .unwrap_err();
        assert!(err.issues().iter().any(|i| i.kind == IssueKind::Coverage && i.message.contains("must carry exactly its original dialogue (\"Then Sam turns and leaves the room.\")")), "{err}");
        let silent_gets_words = emit(&[("prod.md", scene_markdown(2, &lipsync_scene_yaml(2, S2)))]);
        let err = commit_redo(
            &paths,
            &HostConfig::default(),
            &mut state,
            0,
            SOURCE,
            None,
            &[2],
            &silent_gets_words,
        )
        .unwrap_err();
        assert!(
            err.issues()
                .iter()
                .any(|i| i.message.contains("(no dialogue)")),
            "{err}"
        );
        assert_eq!(fs::read_to_string(&paths.prod_md).unwrap(), prod_before);
    }
}
