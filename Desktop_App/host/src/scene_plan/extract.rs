//! Markdown -> cut blocks -> typed `SceneHandoff`. The only place YAML is parsed.
use crate::scene_plan::error::{Issue, IssueKind};
use crate::scene_plan::handoff::{CutId, SceneHandoff};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneBlock {
    pub source: String,
    pub heading: CutId,
    pub heading_line: usize,
    pub yaml: String,
}

struct RawSection {
    id: Option<CutId>,
    raw: String,
    heading_line: usize,
    fences: Vec<String>,
}

struct Scan {
    sections: Vec<RawSection>,
    preamble_fences: Vec<String>,
    unterminated_fence_line: Option<usize>,
}

/// `### Scene 1 — Cut 2`, also `- Cut`, `/ Cut`, `: Cut`, `Scene 1 Cut 2`.
fn heading_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^###\s+Scene\s+(\d+)\s*(?:[—–\-:/|,]+\s*)?Cut\s+(\d+)\b").unwrap()
    })
}

/// Legacy `### Scene N` without a cut number — reported, never accepted.
fn legacy_heading_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^###\s+Scene\s+\d+\b").unwrap())
}

pub fn has_legacy_headings(text: &str) -> bool {
    text.lines()
        .any(|l| legacy_heading_re().is_match(l) && !heading_re().is_match(l))
}

fn heading_id(line: &str) -> Option<CutId> {
    let caps = heading_re().captures(line)?;
    Some(CutId::new(caps[1].parse().ok()?, caps[2].parse().ok()?))
}

fn dedent(lines: &[String]) -> String {
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start_matches(' ').len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|l| l.get(indent..).unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

fn scan(text: &str) -> Scan {
    let mut out = Scan {
        sections: Vec::new(),
        preamble_fences: Vec::new(),
        unterminated_fence_line: None,
    };
    let mut fence: Option<(bool, Vec<String>)> = None;
    let mut fence_open_line = 0usize;

    for (idx, line) in text.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = line.trim();
        if fence.is_some() {
            if trimmed == "```" {
                let (is_yaml, body) = fence.take().expect("fence is open");
                if is_yaml {
                    let yaml = dedent(&body);
                    match out.sections.last_mut() {
                        Some(section) => section.fences.push(yaml),
                        None => out.preamble_fences.push(yaml),
                    }
                }
            } else if let Some((_, body)) = fence.as_mut() {
                body.push(line.to_string());
            }
            continue;
        }
        if trimmed.starts_with("```") {
            let lang = trimmed.trim_start_matches('`').trim().to_ascii_lowercase();
            fence = Some((lang == "yaml" || lang == "yml", Vec::new()));
            fence_open_line = line_no;
            continue;
        }
        if legacy_heading_re().is_match(line) {
            out.sections.push(RawSection {
                id: heading_id(line),
                raw: line.to_string(),
                heading_line: line_no,
                fences: Vec::new(),
            });
        }
    }
    if fence.is_some() {
        out.unterminated_fence_line = Some(fence_open_line);
    }
    out
}

pub fn scene_blocks_from_text(source: &str, text: &str) -> Result<Vec<SceneBlock>, Vec<Issue>> {
    let scan = scan(text);
    let mut issues = Vec::new();
    let mut blocks = Vec::new();

    if let Some(line) = scan.unterminated_fence_line {
        issues.push(Issue::new(
            IssueKind::Parse,
            None,
            format!("{source}: code fence opened at line {line} is never closed"),
        ));
    }
    if !scan.preamble_fences.is_empty() {
        issues.push(Issue::new(
            IssueKind::Parse,
            None,
            format!(
                "{source}: {} YAML fence(s) appear before the first `### Scene N — Cut M` heading; every handoff must sit under its own heading",
                scan.preamble_fences.len()
            ),
        ));
    }
    if scan.sections.is_empty() {
        issues.push(Issue::new(
            IssueKind::Parse,
            None,
            format!("{source}: contains no `### Scene N — Cut M` headings; emit one heading plus exactly one ```yaml fence per cut (do not emit sidecar files)"),
        ));
    }
    for section in scan.sections {
        let Some(id) = section.id else {
            issues.push(Issue::new(
                IssueKind::Parse,
                None,
                format!(
                    "{source}: heading `{}` (line {}) has no cut number; every generated cut is headed `### Scene N — Cut M` (N = source scene, M = cut within it)",
                    section.raw.trim(),
                    section.heading_line
                ),
            ));
            continue;
        };
        match section.fences.len() {
            1 => blocks.push(SceneBlock {
                source: source.to_string(),
                heading: id,
                heading_line: section.heading_line,
                yaml: section.fences.into_iter().next().expect("one fence"),
            }),
            0 => issues.push(Issue::new(IssueKind::Parse, Some(id), format!("{source}: `### {id}` (line {}) has no ```yaml fence", section.heading_line))),
            n => issues.push(Issue::new(
                IssueKind::Parse,
                Some(id),
                format!("{source}: `### {id}` (line {}) has {n} ```yaml fences; exactly one handoff per cut", section.heading_line),
            )),
        }
    }
    if issues.is_empty() {
        Ok(blocks)
    } else {
        Err(issues)
    }
}

/// Cut ids already present in a document.
pub fn heading_ids(text: &str) -> Vec<CutId> {
    scan(text)
        .sections
        .into_iter()
        .filter_map(|s| s.id)
        .collect()
}

pub fn parse_handoff_yaml(yaml: &str) -> Result<SceneHandoff, String> {
    if yaml.trim().is_empty() {
        return Err("YAML fence is empty".to_string());
    }
    let value: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(yaml).map_err(|e| format!("YAML is not well-formed: {e}"))?;
    if !matches!(value, serde_yaml_ng::Value::Mapping(_)) {
        return Err(
            "handoff must be a YAML mapping (key: value pairs at the top level)".to_string(),
        );
    }
    serde_yaml_ng::from_str::<SceneHandoff>(yaml)
        .map_err(|e| format!("handoff does not match the schema: {e}"))
}

pub fn parse_scene_block(block: &SceneBlock) -> Result<SceneHandoff, Issue> {
    let handoff = parse_handoff_yaml(&block.yaml).map_err(|m| {
        Issue::new(
            IssueKind::Parse,
            Some(block.heading),
            format!(
                "{} (heading at line {}): {m}",
                block.source, block.heading_line
            ),
        )
    })?;
    if handoff.id() != block.heading {
        return Err(Issue::new(
            IssueKind::Sequence,
            Some(block.heading),
            format!(
                "heading says {} but the YAML says {} (source_scene: {}, cut: {}); they must match",
                block.heading,
                handoff.id(),
                handoff.source_scene,
                handoff.cut
            ),
        ));
    }
    Ok(handoff)
}

pub fn load_handoff_document(text: &str) -> Result<SceneHandoff, String> {
    let scan = scan(text);
    if let Some(line) = scan.unterminated_fence_line {
        return Err(format!("code fence opened at line {line} is never closed"));
    }
    let mut fences = scan.preamble_fences;
    for section in scan.sections {
        fences.extend(section.fences);
    }
    match fences.len() {
        1 => parse_handoff_yaml(&fences[0]),
        0 => Err("document contains no ```yaml fence".to_string()),
        n => Err(format!(
            "document contains {n} ```yaml fences; expected exactly one"
        )),
    }
}

/// For a possibly truncated document: the prefix ending after the last cut
/// whose ```yaml fence closed. `None` if no complete cut exists.
pub fn complete_scene_prefix(text: &str) -> Option<String> {
    let mut headings: Vec<(usize, bool)> = Vec::new();
    let mut fence: Option<bool> = None;
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if let Some(is_yaml) = fence {
            if trimmed == "```" {
                fence = None;
                if is_yaml {
                    if let Some(last) = headings.last_mut() {
                        last.1 = true;
                    }
                }
            }
        } else if trimmed.starts_with("```") {
            let lang = trimmed.trim_start_matches('`').trim().to_ascii_lowercase();
            fence = Some(lang == "yaml" || lang == "yml");
        } else if heading_re().is_match(line) {
            headings.push((offset, false));
        }
        offset += line.len();
    }
    let complete = headings.iter().take_while(|(_, ok)| *ok).count();
    if complete == 0 {
        return None;
    }
    let cut = headings
        .get(complete)
        .map(|(start, _)| *start)
        .unwrap_or(text.len());
    Some(format!("{}\n", text[..cut].trim_end()))
}

/// Byte span of the block headed by `id`: from its heading to the next
/// level-3 heading or EOF. Fenced content is skipped.
pub fn cut_block_span(text: &str, id: CutId) -> Option<std::ops::Range<usize>> {
    let mut offset = 0usize;
    let mut start: Option<usize> = None;
    let mut in_fence = false;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if in_fence {
            if trimmed == "```" {
                in_fence = false;
            }
        } else if trimmed.starts_with("```") {
            in_fence = true;
        } else if line.starts_with("###") && !line.starts_with("####") {
            if let Some(s) = start {
                return Some(s..offset);
            }
            if heading_id(line) == Some(id) {
                start = Some(offset);
            }
        }
        offset += line.len();
    }
    start.map(|s| s..text.len())
}

/// Replace one cut's block in a master document, keeping everything else.
pub fn replace_cut_block(text: &str, id: CutId, new_block: &str) -> Result<String, String> {
    let span = cut_block_span(text, id).ok_or_else(|| format!("{id} is not in the master file"))?;
    let mut out = String::with_capacity(text.len() + new_block.len());
    out.push_str(&text[..span.start]);
    out.push_str(new_block.trim_end());
    out.push('\n');
    let rest = text[span.end..].trim_start_matches(['\r', '\n']);
    if !rest.is_empty() {
        out.push('\n');
        out.push_str(rest);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_plan::fixtures::{
        cut_markdown, lipsync_scene_yaml, scene_markdown, slim_broll_yaml,
    };

    const TEXT: &str = "This is a twelve word sentence that needs a reliable duration estimate.";

    #[test]
    fn extracts_blocks_in_order_with_dual_ids() {
        let md = format!(
            "# Production\n\nintro\n\n{}{}",
            scene_markdown(1, &lipsync_scene_yaml(1, TEXT)),
            scene_markdown(2, &lipsync_scene_yaml(2, TEXT))
        );
        let blocks = scene_blocks_from_text("prod.md", &md).unwrap();
        assert_eq!(
            blocks.iter().map(|b| b.heading).collect::<Vec<_>>(),
            vec![CutId::new(1, 1), CutId::new(1, 2)]
        );
        assert!(blocks[0].yaml.starts_with("source_scene: 1\ncut: 1"));
        assert_eq!(heading_ids(&md), vec![CutId::new(1, 1), CutId::new(1, 2)]);
    }

    #[test]
    fn heading_separator_variants_are_accepted() {
        for h in [
            "### Scene 3 — Cut 2",
            "### Scene 3 - Cut 2",
            "### Scene 3 / Cut 2",
            "### Scene 3 Cut 2 — Title",
            "### scene 3: cut 2",
        ] {
            assert_eq!(heading_id(h), Some(CutId::new(3, 2)), "{h}");
        }
    }

    #[test]
    fn legacy_scene_heading_without_cut_is_a_specific_parse_error() {
        let md = "### Scene 1\n\n```yaml\nsource_scene: 1\n```\n";
        assert!(has_legacy_headings(md));
        let issues = scene_blocks_from_text("f", md).unwrap_err();
        assert!(
            issues[0].message.contains("has no cut number"),
            "{}",
            issues[0].message
        );
    }

    #[test]
    fn heading_and_yaml_identity_must_match() {
        let blocks =
            scene_blocks_from_text("f", &scene_markdown(2, &lipsync_scene_yaml(1, TEXT))).unwrap();
        let issue = parse_scene_block(&blocks[0]).unwrap_err();
        assert_eq!(issue.kind, IssueKind::Sequence);
        assert!(issue
            .message
            .contains("heading says Scene 1 — Cut 2 but the YAML says Scene 1 — Cut 1"));
        let blocks =
            scene_blocks_from_text("f", &cut_markdown(2, 1, &lipsync_scene_yaml(1, TEXT))).unwrap();
        assert!(parse_scene_block(&blocks[0])
            .unwrap_err()
            .message
            .contains("heading says Scene 2 — Cut 1"));
    }

    #[test]
    fn fence_problems_are_reported() {
        assert!(
            scene_blocks_from_text("f", "### Scene 1 — Cut 1\n\nno yaml\n").unwrap_err()[0]
                .message
                .contains("has no ```yaml fence")
        );
        let two = "### Scene 1 — Cut 1\n```yaml\na: 1\n```\n```yaml\nb: 2\n```\n";
        assert!(scene_blocks_from_text("f", two).unwrap_err()[0]
            .message
            .contains("has 2 ```yaml fences"));
        let issues = scene_blocks_from_text(
            "f",
            "```yaml\na: 1\n```\n### Scene 1 — Cut 1\n```yaml\nb: 2\n",
        )
        .unwrap_err();
        let joined = issues
            .iter()
            .map(|i| i.message.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            joined.contains("never closed") && joined.contains("before the first"),
            "{joined}"
        );
        assert!(
            scene_blocks_from_text("scene_prompts/Scene1.txt", "Character_Reference: none\n")
                .unwrap_err()[0]
                .message
                .contains("no `### Scene N — Cut M` headings")
        );
    }

    #[test]
    fn schema_errors_are_specific() {
        assert!(parse_handoff_yaml("source_scene: [1\nprovider: x")
            .unwrap_err()
            .contains("not well-formed"));
        let dup = format!("{}\nprovider: other\n", lipsync_scene_yaml(1, TEXT));
        assert!(parse_handoff_yaml(&dup)
            .unwrap_err()
            .to_lowercase()
            .contains("duplicate"));
        let missing = lipsync_scene_yaml(1, TEXT).replace("cut: 1\n", "");
        assert!(parse_handoff_yaml(&missing)
            .unwrap_err()
            .contains("missing field `cut`"));
        let legacy =
            lipsync_scene_yaml(1, TEXT).replace("source_scene: 1\ncut: 1\n", "scene_number: 1\n");
        let err = parse_handoff_yaml(&legacy).unwrap_err();
        assert!(err.contains("unknown field `scene_number`"), "{err}");
        assert!(parse_handoff_yaml(
            &lipsync_scene_yaml(1, TEXT).replace("stage: base_video", "stage: nope")
        )
        .unwrap_err()
        .contains("unknown variant `nope`"));
        assert!(parse_handoff_yaml(&lipsync_scene_yaml(1, TEXT).replace(
            "estimated_screen_duration_seconds: 6.0",
            "estimated_screen_duration_seconds: \"six\""
        ))
        .unwrap_err()
        .contains("invalid type"));
        assert!(parse_handoff_yaml(&lipsync_scene_yaml(1, TEXT).replace(
            "  aspect_ratio: \"9:16\"\n",
            "  aspect_ratio: \"9:16\"\n  target_duration_seconds: 8.0\n"
        ))
        .unwrap_err()
        .contains("unknown field `target_duration_seconds`"));
        let slim = parse_handoff_yaml(&slim_broll_yaml(2)).unwrap();
        assert_eq!(slim.cut, 2);
        assert_eq!(slim.provider, "atlascloud");
        assert!(slim.generation_requirements.aspect_ratio.is_empty());
        assert!(slim.speech.cues.is_empty());
    }

    #[test]
    fn replace_cut_block_swaps_only_that_block() {
        let s1 = scene_markdown(1, &lipsync_scene_yaml(1, TEXT));
        let s2 = scene_markdown(2, &lipsync_scene_yaml(2, TEXT));
        let s3 = scene_markdown(3, &lipsync_scene_yaml(3, TEXT));
        let master = format!("# Production\n\n{s1}\n{s2}\n{s3}");
        let new2 = scene_markdown(2, &lipsync_scene_yaml(2, "Replaced line here."));
        let out = replace_cut_block(&master, CutId::new(1, 2), &new2).unwrap();
        assert!(out.starts_with("# Production\n\n### Scene 1 — Cut 1"));
        assert!(out.contains("Replaced line here."));
        assert_eq!(
            heading_ids(&out),
            vec![CutId::new(1, 1), CutId::new(1, 2), CutId::new(1, 3)]
        );
        assert_eq!(out.matches("### Scene 1 — Cut 2").count(), 1);
        assert!(out.contains("### Scene 1 — Cut 3"));
        let last =
            replace_cut_block(&master, CutId::new(1, 3), &new2.replace("Cut 2", "Cut 3")).unwrap();
        assert!(last.trim_end().ends_with("```"));
        assert!(replace_cut_block(&master, CutId::new(1, 9), &new2)
            .unwrap_err()
            .contains("not in the master file"));
    }

    #[test]
    fn load_document_and_complete_prefix() {
        let one = format!(
            "Voice: Sam\n\nPrompt:\n\n```yaml\n{}\n```\n",
            lipsync_scene_yaml(3, TEXT)
        );
        assert_eq!(load_handoff_document(&one).unwrap().id(), CutId::new(1, 3));
        assert!(load_handoff_document("no fence")
            .unwrap_err()
            .contains("no ```yaml fence"));

        let s1 = scene_markdown(1, &lipsync_scene_yaml(1, TEXT));
        let s2 = scene_markdown(2, &lipsync_scene_yaml(2, TEXT));
        let full = format!("{s1}\n{s2}");
        assert_eq!(
            complete_scene_prefix(&full).unwrap().trim_end(),
            full.trim_end()
        );
        let prefix = complete_scene_prefix(&full[..full.len() - 30]).unwrap();
        assert_eq!(heading_ids(&prefix), vec![CutId::new(1, 1)]);
        assert!(complete_scene_prefix(&s1[..s1.len() - 30]).is_none());
    }
}
