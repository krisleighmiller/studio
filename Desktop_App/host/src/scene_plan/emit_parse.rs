//! Turn one chat reply from the scene-plan model into a structured outcome.
//! Tolerates the ways models mangle the tag protocol; never silently drops
//! content. Whatever comes out is still validated by the typed pipeline.
use crate::scene_plan::extract;
use crate::scene_plan::handoff::CutId;
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recovery {
    /// No `<workbench_file>` tag, but the reply was scene Markdown; used as `prod.md`.
    UntaggedMarkdown,
    /// The reply was cut off; only scenes whose YAML fence closed were kept.
    TruncatedSalvaged {
        kept: Vec<CutId>,
        dropped_from: Option<CutId>,
    },
}

impl Recovery {
    pub fn describe(&self) -> String {
        match self {
            Recovery::UntaggedMarkdown => {
                "the model did not wrap its output in <workbench_file>; the host used the reply body as prod.md content".to_string()
            }
            Recovery::TruncatedSalvaged { kept, dropped_from } => format!(
                "the model's reply was cut off before </workbench_file>; kept {}{}",
                kept.iter().map(|c| c.code()).collect::<Vec<_>>().join(", "),
                dropped_from.map(|c| format!(", dropped the incomplete {c} (re-planned next turn)")).unwrap_or_default()
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoFilesReason {
    Empty,
    Truncated {
        path: Option<String>,
        body_chars: usize,
    },
    TagWithoutPath {
        header: String,
    },
    NoTags {
        looks_like: &'static str,
    },
}

impl NoFilesReason {
    pub fn describe(&self) -> String {
        match self {
            NoFilesReason::Empty => "the reply was empty".to_string(),
            NoFilesReason::Truncated { path, body_chars } => format!(
                "the reply was cut off inside <workbench_file path=\"{}\"> after {body_chars} chars (no </workbench_file>) and no complete scene could be salvaged",
                path.as_deref().unwrap_or("?")
            ),
            NoFilesReason::TagWithoutPath { header } => {
                format!("a <workbench_file{header}> tag had no usable path attribute")
            }
            NoFilesReason::NoTags { looks_like } => {
                format!("the reply had no <workbench_file> tags and was not scene Markdown ({looks_like})")
            }
        }
    }

    /// Correction sent back to the model.
    pub fn nudge(&self) -> String {
        match self {
            NoFilesReason::Empty => "Workbench host: your reply was empty. Emit <workbench_file path=\"prod.md\"> containing the `### Scene N — Cut M` Markdown/YAML now.".into(),
            NoFilesReason::Truncated { .. } => "Workbench host: your reply was cut off before </workbench_file> and was discarded. Emit at most 2 scenes this turn, close the tag, and stop; the host will call you again for the remaining dialogue.".into(),
            NoFilesReason::TagWithoutPath { .. } => "Workbench host: your <workbench_file> tag had no path. Use exactly <workbench_file path=\"prod.md\"> … </workbench_file>.".into(),
            NoFilesReason::NoTags { .. } => "Workbench host: your reply had no <workbench_file> tags. Wrap the `### Scene N — Cut M` Markdown (with its ```yaml fence) in <workbench_file path=\"prod.md\"> … </workbench_file> and add <workbench_summary>. Do not ask questions.".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmitOutcome {
    Files {
        files: Vec<(String, String)>,
        summary: Option<String>,
        recovery: Option<Recovery>,
    },
    Ask(String),
    NoFiles(NoFilesReason),
}

impl EmitOutcome {
    pub fn describe(&self) -> String {
        match self {
            EmitOutcome::Files {
                files, recovery, ..
            } => format!(
                "{} file(s){}",
                files.len(),
                recovery
                    .as_ref()
                    .map(|r| format!(" (recovered: {})", r.describe()))
                    .unwrap_or_default()
            ),
            EmitOutcome::Ask(_) => "a question instead of files".to_string(),
            EmitOutcome::NoFiles(reason) => reason.describe(),
        }
    }
}

fn open_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)<workbench_file\b([^>]*)>").unwrap())
}

fn close_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)</workbench_file\s*>").unwrap())
}

fn path_attr_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?i)\b(?:path|file|filename|name)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#)
            .unwrap()
    })
}

fn header_path(header: &str) -> Option<String> {
    let caps = path_attr_re().captures(header)?;
    let raw = caps
        .get(1)
        .or_else(|| caps.get(2))
        .or_else(|| caps.get(3))?
        .as_str()
        .trim();
    (!raw.is_empty()).then(|| raw.to_string())
}

fn tagged_block(text: &str, tag: &str) -> Option<String> {
    let re = Regex::new(&format!(r"(?is)<{tag}\s*>(.*?)</{tag}\s*>")).ok()?;
    let inner = re.captures(text)?.get(1)?.as_str().trim();
    (!inner.is_empty()).then(|| inner.to_string())
}

/// Models sometimes HTML-escape the protocol tags. Only unescape when the
/// escaped form of *our* tag is present, so legitimate `<` in prose stays.
fn unescape_if_needed(reply: &str) -> String {
    if reply.contains("&lt;workbench_") || reply.contains("&lt;/workbench_") {
        reply
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&amp;", "&")
    } else {
        reply.to_string()
    }
}

/// Remove one code fence that wraps the *entire* text (e.g. ```markdown … ```).
/// A fence whose info string is yaml is left alone: that is scene content.
pub fn strip_outer_fence(text: &str) -> &str {
    let t = text.trim();
    let Some(first) = t.lines().next() else {
        return t;
    };
    let first_trimmed = first.trim();
    if !first_trimmed.starts_with("```") {
        return t;
    }
    let lang = first_trimmed
        .trim_start_matches('`')
        .trim()
        .to_ascii_lowercase();
    if lang == "yaml" || lang == "yml" {
        return t;
    }
    let Some(last) = t.lines().last() else {
        return t;
    };
    if last.trim() != "```" || t.lines().count() < 2 {
        return t;
    }
    let start = first.len();
    let end = t.len() - last.len();
    if start > end {
        return t;
    }
    t[start..end].trim_matches(|c| c == '\n' || c == '\r')
}

fn normalize_body(body: &str) -> String {
    let body = body
        .strip_prefix("\r\n")
        .or_else(|| body.strip_prefix('\n'))
        .unwrap_or(body);
    let body = body.strip_suffix('\n').unwrap_or(body);
    let body = body.strip_suffix('\r').unwrap_or(body);
    strip_outer_fence(body).to_string()
}

fn is_scene_markdown(text: &str) -> bool {
    (!extract::heading_ids(text).is_empty() || extract::has_legacy_headings(text))
        && text.contains("```yaml")
}

fn salvage(body: &str) -> Option<(String, Recovery)> {
    let prefix = extract::complete_scene_prefix(body)?;
    let kept = extract::heading_ids(&prefix);
    let dropped_from = extract::heading_ids(body)
        .into_iter()
        .find(|id| !kept.contains(id));
    Some((prefix, Recovery::TruncatedSalvaged { kept, dropped_from }))
}

pub fn parse_scene_plan_emit(reply: &str) -> EmitOutcome {
    let text = unescape_if_needed(reply);
    let text = text.as_str();
    if text.trim().is_empty() {
        return EmitOutcome::NoFiles(NoFilesReason::Empty);
    }

    let summary = tagged_block(text, "workbench_summary");
    let ask = tagged_block(text, "workbench_ask");

    let mut files: Vec<(String, String)> = Vec::new();
    let mut recovery: Option<Recovery> = None;
    let mut truncated: Option<NoFilesReason> = None;
    let mut missing_path: Option<String> = None;

    for caps in open_re().captures_iter(text) {
        let whole = caps.get(0).expect("match");
        let header = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let rest = &text[whole.end()..];
        let path = header_path(header);
        match close_re().find(rest) {
            Some(close) => {
                let body = normalize_body(&rest[..close.start()]);
                match path {
                    Some(p) => files.push((p, body)),
                    None => missing_path = Some(header.trim_end().to_string()),
                }
            }
            None => {
                // Cut off mid-file. Keep the scenes that finished.
                let body = normalize_body(rest);
                match salvage(&body) {
                    Some((prefix, rec)) => {
                        files.push((path.unwrap_or_else(|| "prod.md".to_string()), prefix));
                        recovery = Some(rec);
                    }
                    None => {
                        truncated = Some(NoFilesReason::Truncated {
                            path,
                            body_chars: body.chars().count(),
                        })
                    }
                }
                break;
            }
        }
    }

    if !files.is_empty() {
        return EmitOutcome::Files {
            files,
            summary,
            recovery,
        };
    }
    if let Some(reason) = truncated {
        return EmitOutcome::NoFiles(reason);
    }
    if let Some(header) = missing_path {
        return EmitOutcome::NoFiles(NoFilesReason::TagWithoutPath { header });
    }
    if let Some(question) = ask {
        return EmitOutcome::Ask(question);
    }

    // No protocol tags at all. If the reply *is* scene Markdown, use it.
    let untagged = strip_outer_fence(text);
    let candidate = match extract::complete_scene_prefix(untagged) {
        Some(prefix) if prefix.trim().len() < untagged.trim().len() => prefix,
        _ => untagged.to_string(),
    };
    if is_scene_markdown(&candidate) {
        return EmitOutcome::Files {
            files: vec![("prod.md".to_string(), candidate)],
            summary,
            recovery: Some(Recovery::UntaggedMarkdown),
        };
    }

    let looks_like =
        if !extract::heading_ids(untagged).is_empty() || extract::has_legacy_headings(untagged) {
            "scene headings without a ```yaml fence"
        } else if untagged.contains("```yaml") {
            "a ```yaml fence without a `### Scene N — Cut M` heading"
        } else if untagged.chars().count() < 400 {
            "a short prose reply"
        } else {
            "prose"
        };
    EmitOutcome::NoFiles(NoFilesReason::NoTags { looks_like })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_plan::fixtures::{cut_markdown, lipsync_scene_yaml};

    fn scene(cut: u32) -> String {
        cut_markdown(1, cut, &lipsync_scene_yaml(cut, "Hello there friend."))
    }

    fn files_of(outcome: EmitOutcome) -> (Vec<(String, String)>, Option<Recovery>) {
        match outcome {
            EmitOutcome::Files {
                files, recovery, ..
            } => (files, recovery),
            other => panic!("expected files, got {other:?}"),
        }
    }

    #[test]
    fn tagged_double_quoted_path_with_summary() {
        let reply = format!("Here you go.\n<workbench_file path=\"prod.md\">\n{}</workbench_file>\n<workbench_summary>One scene.</workbench_summary>", scene(1));
        match parse_scene_plan_emit(&reply) {
            EmitOutcome::Files {
                files,
                summary,
                recovery,
            } => {
                assert_eq!(files.len(), 1);
                assert_eq!(files[0].0, "prod.md");
                assert!(files[0].1.starts_with("### Scene 1"));
                assert_eq!(summary.as_deref(), Some("One scene."));
                assert!(recovery.is_none());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn single_quoted_unquoted_and_name_attributes_work() {
        for header in [
            "path='prod.md'",
            "path=prod.md",
            "name=\"prod.md\"",
            " PATH = \"prod.md\" ",
        ] {
            let reply = format!("<workbench_file {header}>\n{}</workbench_file>", scene(1));
            let (files, _) = files_of(parse_scene_plan_emit(&reply));
            assert_eq!(files[0].0, "prod.md", "header {header:?}");
        }
    }

    #[test]
    fn html_escaped_tags_are_unescaped() {
        let reply = format!(
            "&lt;workbench_file path=&quot;prod.md&quot;&gt;\n{}&lt;/workbench_file&gt;",
            scene(1)
        );
        let (files, _) = files_of(parse_scene_plan_emit(&reply));
        assert_eq!(files[0].0, "prod.md");
        assert!(files[0].1.contains("```yaml"));
    }

    #[test]
    fn body_wrapped_in_markdown_fence_is_unwrapped() {
        let reply = format!(
            "<workbench_file path=\"prod.md\">\n```markdown\n{}```\n</workbench_file>",
            scene(1)
        );
        let (files, _) = files_of(parse_scene_plan_emit(&reply));
        assert!(files[0].1.starts_with("### Scene 1"), "{}", files[0].1);
        assert!(files[0].1.trim_end().ends_with("```"));
        assert!(!files[0].1.starts_with("```markdown"));
    }

    #[test]
    fn untagged_scene_markdown_is_recovered_as_prod_md() {
        let reply = format!("Sure — here is the plan.\n\n{}\n{}", scene(1), scene(2));
        let (files, recovery) = files_of(parse_scene_plan_emit(&reply));
        assert_eq!(files[0].0, "prod.md");
        assert_eq!(recovery, Some(Recovery::UntaggedMarkdown));
        assert_eq!(
            extract::heading_ids(&files[0].1),
            vec![CutId::new(1, 1), CutId::new(1, 2)]
        );
    }

    #[test]
    fn untagged_markdown_inside_outer_fence_is_recovered() {
        let reply = format!("```markdown\n{}```", scene(1));
        let (files, recovery) = files_of(parse_scene_plan_emit(&reply));
        assert_eq!(recovery, Some(Recovery::UntaggedMarkdown));
        assert!(files[0].1.starts_with("### Scene 1"));
    }

    #[test]
    fn truncated_reply_salvages_complete_scenes() {
        let cut = &scene(2)[..scene(2).len() - 40]; // scene 2 fence never closes
        let reply = format!("<workbench_file path=\"prod.md\">\n{}\n{cut}", scene(1));
        let (files, recovery) = files_of(parse_scene_plan_emit(&reply));
        assert_eq!(
            recovery,
            Some(Recovery::TruncatedSalvaged {
                kept: vec![CutId::new(1, 1)],
                dropped_from: Some(CutId::new(1, 2))
            })
        );
        assert_eq!(extract::heading_ids(&files[0].1), vec![CutId::new(1, 1)]);
        assert!(!files[0].1.contains("### Scene 1 — Cut 2"));
    }

    #[test]
    fn truncated_reply_without_a_complete_scene_is_a_structured_failure() {
        let cut = &scene(1)[..scene(1).len() - 40];
        let reply = format!("<workbench_file path=\"prod.md\">\n{cut}");
        match parse_scene_plan_emit(&reply) {
            EmitOutcome::NoFiles(NoFilesReason::Truncated { path, body_chars }) => {
                assert_eq!(path.as_deref(), Some("prod.md"));
                assert!(body_chars > 100);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn ask_only_is_ask_and_files_beat_ask() {
        let ask = "<workbench_ask>Which aspect ratio?</workbench_ask>";
        assert_eq!(
            parse_scene_plan_emit(ask),
            EmitOutcome::Ask("Which aspect ratio?".into())
        );
        let both = format!(
            "{ask}\n<workbench_file path=\"prod.md\">\n{}</workbench_file>",
            scene(1)
        );
        assert!(matches!(
            parse_scene_plan_emit(&both),
            EmitOutcome::Files { .. }
        ));
    }

    #[test]
    fn prose_empty_and_pathless_tag_are_distinct_reasons() {
        assert_eq!(
            parse_scene_plan_emit("I'd be happy to help plan this scene."),
            EmitOutcome::NoFiles(NoFilesReason::NoTags {
                looks_like: "a short prose reply"
            })
        );
        assert_eq!(
            parse_scene_plan_emit("  \n"),
            EmitOutcome::NoFiles(NoFilesReason::Empty)
        );
        let pathless = format!("<workbench_file>\n{}</workbench_file>", scene(1));
        assert!(matches!(
            parse_scene_plan_emit(&pathless),
            EmitOutcome::NoFiles(NoFilesReason::TagWithoutPath { .. })
        ));
        let heading_only = "### Scene 1\n\nsome prose";
        assert_eq!(
            parse_scene_plan_emit(heading_only),
            EmitOutcome::NoFiles(NoFilesReason::NoTags {
                looks_like: "scene headings without a ```yaml fence"
            })
        );
    }

    #[test]
    fn legacy_untagged_scene_headings_still_reach_the_typed_pipeline() {
        let reply = "### Scene 1\n\n```yaml\nscene_number: 1\n```\n";
        let (files, recovery) = files_of(parse_scene_plan_emit(reply));
        assert_eq!(recovery, Some(Recovery::UntaggedMarkdown));
        assert!(files[0].1.contains("### Scene 1"));
    }

    #[test]
    fn multiple_files_are_all_returned() {
        let reply = format!(
            "<workbench_file path=\"a.md\">\n{}</workbench_file>\n<workbench_file path=\"b.md\">\n{}</workbench_file>",
            scene(1),
            scene(2)
        );
        let (files, _) = files_of(parse_scene_plan_emit(&reply));
        assert_eq!(
            files.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(),
            vec!["a.md", "b.md"]
        );
    }
}
