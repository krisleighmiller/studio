//! Deterministic caption (SRT) QC. Parse and validate caption timing against
//! the video — never ask an LLM whether a subtitle file is well-formed.
//!
//! Caption files sit next to the review export: the assembler writes
//! `output_path.with_extension("srt")`, so a version's captions are derivable
//! from its `assembly_path` (and implicitly versioned with it).

use std::fs;
use std::path::Path;

use crate::workflow::qc::{QCFinding, QCSeverity};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SrtInfo {
    pub cue_count: usize,
    pub last_end_secs: f64,
}

/// Parse an SRT body into timing facts. Tolerant of cue-number and text lines;
/// strict about timecodes.
pub fn parse_srt(content: &str) -> Result<SrtInfo, String> {
    let mut cue_count = 0usize;
    let mut last_end = 0.0f64;
    for raw in content.lines() {
        let line = raw.trim();
        let Some((start, end)) = parse_timecode_line(line) else {
            continue;
        };
        if end < start {
            return Err(format!("cue ends before it starts: `{line}`"));
        }
        cue_count += 1;
        last_end = last_end.max(end);
    }
    if cue_count == 0 {
        return Err("no valid cue timecodes found".to_string());
    }
    Ok(SrtInfo {
        cue_count,
        last_end_secs: last_end,
    })
}

/// Deterministic findings from an SRT body, compared against the video duration.
pub fn findings_from_srt(content: &str, video_duration_secs: Option<f64>) -> Vec<QCFinding> {
    match parse_srt(content) {
        Ok(info) => {
            let mut findings = vec![captions_finding(
                QCSeverity::Info,
                format!(
                    "{} cues, last ends at {:.2}s.",
                    info.cue_count, info.last_end_secs
                ),
            )];
            if let Some(duration) = video_duration_secs {
                if info.last_end_secs > duration + 0.5 {
                    findings.push(captions_finding(
                        QCSeverity::Warning,
                        format!(
                            "Captions run to {:.2}s past the video duration {:.2}s.",
                            info.last_end_secs, duration
                        ),
                    ));
                }
            }
            findings
        }
        Err(message) => vec![captions_finding(
            QCSeverity::Error,
            format!("Caption file is malformed: {message}."),
        )],
    }
}

/// Check the caption file derived from a review-export path. Returns findings;
/// a missing SRT is an `info` (captions may not be required for this deliverable).
pub fn check_captions(root: &Path, video_path: &str) -> Vec<QCFinding> {
    let video_rel = Path::new(video_path);
    let srt_rel = video_rel.with_extension("srt");
    let srt_abs = if srt_rel.is_absolute() {
        srt_rel.to_path_buf()
    } else {
        root.join(&srt_rel)
    };
    if !srt_abs.is_file() {
        return vec![captions_finding(
            QCSeverity::Info,
            format!("No caption file ({})", srt_rel.display()),
        )];
    }
    let content = match fs::read_to_string(&srt_abs) {
        Ok(content) => content,
        Err(error) => {
            return vec![captions_finding(
                QCSeverity::Error,
                format!("Could not read caption file: {error}."),
            )];
        }
    };
    let video_duration = {
        let video_abs = if video_rel.is_absolute() {
            video_rel.to_path_buf()
        } else {
            root.join(video_rel)
        };
        crate::workflow::qc::probe_media(&video_abs)
            .ok()
            .and_then(|probe| probe.duration_secs)
    };
    findings_from_srt(&content, video_duration)
}

fn captions_finding(severity: QCSeverity, message: String) -> QCFinding {
    QCFinding {
        id: "captions".to_string(),
        severity,
        check: "Captions".to_string(),
        message,
        artifact_id: None,
    }
}

fn parse_timecode_line(line: &str) -> Option<(f64, f64)> {
    let (start, end) = line.split_once("-->")?;
    Some((parse_timecode(start.trim())?, parse_timecode(end.trim())?))
}

fn parse_timecode(value: &str) -> Option<f64> {
    let (hms, millis) = value.split_once(',')?;
    let mut parts = hms.split(':');
    let hours = parts.next()?.parse::<f64>().ok()?;
    let minutes = parts.next()?.parse::<f64>().ok()?;
    let seconds = parts.next()?.parse::<f64>().ok()?;
    let millis = millis.parse::<f64>().ok()?;
    Some(hours * 3600.0 + minutes * 60.0 + seconds + millis / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_srt_reads_cues_and_last_end() {
        let content = "1\n00:00:00,000 --> 00:00:02,500\nHello\n\n\
                       2\n00:00:02,500 --> 00:00:05,000\nWorld\n";
        let info = parse_srt(content).unwrap();
        assert_eq!(info.cue_count, 2);
        assert!((info.last_end_secs - 5.0).abs() < 0.001);
    }

    #[test]
    fn parse_srt_rejects_a_cue_that_ends_before_it_starts() {
        let content = "1\n00:00:05,000 --> 00:00:02,000\nbackwards\n";
        assert!(parse_srt(content).is_err());
    }

    #[test]
    fn parse_srt_rejects_empty() {
        assert!(parse_srt("no timecodes here\njust text\n").is_err());
    }

    #[test]
    fn findings_flag_captions_past_video_duration() {
        let content = "1\n00:00:00,000 --> 00:00:10,000\nlong\n";
        let findings = findings_from_srt(content, Some(8.0));
        assert!(findings.iter().any(|f| f.severity == QCSeverity::Warning));
    }

    #[test]
    fn findings_pass_for_captions_within_video() {
        let content = "1\n00:00:00,000 --> 00:00:03,000\nok\n";
        let findings = findings_from_srt(content, Some(8.0));
        assert!(findings.iter().all(|f| f.severity != QCSeverity::Error));
        assert!(findings.iter().all(|f| f.severity != QCSeverity::Warning));
    }

    #[test]
    fn check_captions_reports_info_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let findings = check_captions(dir.path(), "Final/Scene1_master.mp4");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, QCSeverity::Info);
        assert!(findings[0].message.contains("No caption file"));
    }
}
