//! Deterministic technical QC. Probe a media artifact with ffprobe/ffmpeg and
//! emit structured findings — never ask an LLM what a codec tool can answer.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

/// Severity of a QC finding. `Error` blocks a passing deliverable; `Warning`
/// flags something suspicious without blocking; `Info` documents a fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QCSeverity {
    Info,
    Warning,
    Error,
}

/// One deterministic check result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QCFinding {
    pub id: String,
    pub severity: QCSeverity,
    pub check: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaProbe {
    pub duration_secs: Option<f64>,
    pub video: Option<VideoStream>,
    pub audio: Option<AudioStream>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VideoStream {
    pub width: u32,
    pub height: u32,
    pub frame_rate: f64,
    pub codec: String,
    pub nb_frames: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioStream {
    pub sample_rate: u32,
    pub codec: String,
    pub duration_secs: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioLevels {
    pub mean_db: f64,
    pub max_db: f64,
}

/// Run a single ffprobe and fold the JSON into a `MediaProbe`.
pub fn probe_media(path: &Path) -> Result<MediaProbe, String> {
    if !path.is_file() {
        return Err(format!("Not a file: {}", path.display()));
    }
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()
        .map_err(|error| format!("Could not run ffprobe (is it on PATH?): {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "ffprobe could not decode {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let parsed: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("ffprobe produced invalid JSON: {error}"))?;
    Ok(parse_ffprobe(&parsed))
}

/// Parse ffprobe JSON into a `MediaProbe`. Pure and unit-testable.
pub fn parse_ffprobe(value: &Value) -> MediaProbe {
    let format_duration = value
        .get("format")
        .and_then(|format| format.get("duration"))
        .and_then(Value::as_str)
        .and_then(|text| text.parse::<f64>().ok());
    let mut video = None;
    let mut audio = None;
    if let Some(streams) = value.get("streams").and_then(Value::as_array) {
        for stream in streams {
            let codec_type = stream
                .get("codec_type")
                .and_then(Value::as_str)
                .unwrap_or("");
            match codec_type {
                "video" if video.is_none() => {
                    video = Some(VideoStream {
                        width: stream.get("width").and_then(Value::as_u64).unwrap_or(0) as u32,
                        height: stream.get("height").and_then(Value::as_u64).unwrap_or(0) as u32,
                        frame_rate: parse_frame_rate(
                            stream
                                .get("avg_frame_rate")
                                .and_then(Value::as_str)
                                .unwrap_or("0/0"),
                        ),
                        codec: stream
                            .get("codec_name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        nb_frames: stream
                            .get("nb_frames")
                            .and_then(Value::as_str)
                            .and_then(|text| text.parse::<u64>().ok()),
                    });
                }
                "audio" if audio.is_none() => {
                    audio = Some(AudioStream {
                        sample_rate: stream
                            .get("sample_rate")
                            .and_then(Value::as_str)
                            .and_then(|text| text.parse::<u32>().ok())
                            .unwrap_or(0),
                        codec: stream
                            .get("codec_name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        duration_secs: stream
                            .get("duration")
                            .and_then(Value::as_str)
                            .and_then(|text| text.parse::<f64>().ok()),
                    });
                }
                _ => {}
            }
        }
    }
    MediaProbe {
        duration_secs: format_duration,
        video,
        audio,
    }
}

/// Run ffmpeg `volumedetect` and return mean/peak levels. Fails when there is no
/// audio stream to measure.
pub fn probe_audio_levels(path: &Path) -> Result<AudioLevels, String> {
    let output = Command::new("ffmpeg")
        .args(["-nostats", "-hide_banner", "-i"])
        .arg(path)
        .args(["-af", "volumedetect", "-f", "null", "-"])
        .output()
        .map_err(|error| format!("Could not run ffmpeg (is it on PATH?): {error}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_volumedetect(&stderr).ok_or_else(|| {
        format!(
            "volumedetect found no levels (audio absent?): {}",
            stderr.trim()
        )
    })
}

/// Parse `mean_volume`/`max_volume` lines out of ffmpeg stderr. Pure.
pub fn parse_volumedetect(stderr: &str) -> Option<AudioLevels> {
    Some(AudioLevels {
        mean_db: parse_db_line(stderr, "mean_volume:")?,
        max_db: parse_db_line(stderr, "max_volume:")?,
    })
}

/// Run the full deterministic media QC for one artifact. Any `Error` finding
/// means the artifact is technically invalid.
pub fn check_media_file(
    path: &Path,
    expected_runtime_secs: Option<f64>,
    runtime_tolerance_secs: f64,
) -> Vec<QCFinding> {
    check_media_file_against_spec(
        path,
        expected_runtime_secs,
        runtime_tolerance_secs,
        None,
        None,
        None,
    )
}

/// As `check_media_file`, but also compares the probed file against the
/// declared deliverable spec (resolution, aspect ratio, frame rate).
pub fn check_media_file_against_spec(
    path: &Path,
    expected_runtime_secs: Option<f64>,
    runtime_tolerance_secs: f64,
    resolution: Option<&str>,
    aspect: Option<&str>,
    frame_rate: Option<&str>,
) -> Vec<QCFinding> {
    if !path.is_file() {
        return vec![error(
            "file_exists",
            "File",
            format!("Artifact missing: {}", path.display()),
        )];
    }
    let mut findings = vec![info(
        "file_exists",
        "File",
        format!("Artifact present: {}", path.display()),
    )];
    let probe = match probe_media(path) {
        Ok(probe) => probe,
        Err(message) => {
            findings.push(error(
                "decodes",
                "Decode",
                format!("Could not decode: {message}"),
            ));
            return findings;
        }
    };
    let levels = probe_audio_levels(path).ok();
    findings.extend(findings_from_probe(
        &probe,
        levels,
        expected_runtime_secs,
        runtime_tolerance_secs,
    ));
    findings.extend(spec_findings(&probe, resolution, aspect, frame_rate));
    findings
}

/// Compare a probed file against the declared deliverable spec. Pure.
pub fn spec_findings(
    probe: &MediaProbe,
    resolution: Option<&str>,
    aspect: Option<&str>,
    frame_rate: Option<&str>,
) -> Vec<QCFinding> {
    let mut findings = Vec::new();
    let Some(video) = &probe.video else {
        return findings;
    };
    if let Some((spec_width, spec_height)) = resolution.and_then(parse_resolution) {
        if video.width != spec_width || video.height != spec_height {
            findings.push(warning(
                "resolution_mismatch",
                "Resolution",
                format!(
                    "File is {}×{} but the deliverable specifies {spec_width}×{spec_height}.",
                    video.width, video.height
                ),
            ));
        } else {
            findings.push(info(
                "resolution",
                "Resolution",
                format!("{}×{} matches the spec.", video.width, video.height),
            ));
        }
    }
    if let Some(ratio) = aspect.and_then(parse_aspect) {
        if video.height == 0 {
            return findings;
        }
        let actual = video.width as f64 / video.height as f64;
        if (actual - ratio).abs() > 0.02 {
            findings.push(warning(
                "aspect_mismatch",
                "Aspect",
                format!("File aspect {actual:.3} does not match the specified {ratio:.3}."),
            ));
        }
    }
    if let Some(fps) = frame_rate.and_then(parse_frame_rate_spec) {
        if (video.frame_rate - fps).abs() > 0.05 {
            findings.push(warning(
                "framerate_mismatch",
                "Frame rate",
                format!(
                    "File is {:.2} fps but the deliverable specifies {fps:.2} fps.",
                    video.frame_rate
                ),
            ));
        }
    }
    findings
}

fn parse_resolution(raw: &str) -> Option<(u32, u32)> {
    let (width, height) = raw.trim().split_once('x')?;
    Some((width.trim().parse().ok()?, height.trim().parse().ok()?))
}

fn parse_aspect(raw: &str) -> Option<f64> {
    let (width, height) = raw.trim().split_once(':')?;
    let width: f64 = width.trim().parse().ok()?;
    let height: f64 = height.trim().parse().ok()?;
    if height == 0.0 {
        None
    } else {
        Some(width / height)
    }
}

fn parse_frame_rate_spec(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if let Some((numerator, denominator)) = trimmed.split_once('/') {
        let numerator: f64 = numerator.trim().parse().ok()?;
        let denominator: f64 = denominator.trim().parse().ok()?;
        return if denominator == 0.0 {
            None
        } else {
            Some(numerator / denominator)
        };
    }
    trimmed.parse::<f64>().ok()
}

/// Derive findings from an already-computed probe. Pure and unit-testable.
pub fn findings_from_probe(
    probe: &MediaProbe,
    levels: Option<AudioLevels>,
    expected_runtime_secs: Option<f64>,
    runtime_tolerance_secs: f64,
) -> Vec<QCFinding> {
    let mut findings = Vec::new();

    let Some(video) = &probe.video else {
        findings.push(error(
            "no_video",
            "Video stream",
            "No video stream; the file does not decode as video.",
        ));
        return findings;
    };
    findings.push(info(
        "has_video",
        "Video stream",
        format!(
            "{} ({}×{}, {:.2} fps).",
            video.codec, video.width, video.height, video.frame_rate
        ),
    ));

    if video.width == 0 || video.height == 0 {
        findings.push(error(
            "resolution",
            "Resolution",
            "Video has zero width or height.",
        ));
    }
    if video.frame_rate <= 0.0 {
        findings.push(error(
            "frame_rate",
            "Frame rate",
            "Video frame rate is zero or unreadable.",
        ));
    }
    if let Some(frames) = video.nb_frames {
        if frames == 0 {
            findings.push(error("frames", "Frames", "Video has zero frames."));
        }
    }

    match probe.duration_secs {
        None => findings.push(error("duration", "Duration", "Media duration is missing.")),
        Some(duration) if duration <= 0.0 => {
            findings.push(error("duration", "Duration", "Media duration is zero."));
        }
        Some(duration) => findings.push(info("duration", "Duration", format!("{duration:.2}s."))),
    }

    match &probe.audio {
        None => findings.push(warning("audio", "Audio", "No audio stream.")),
        Some(audio) => {
            findings.push(info(
                "audio",
                "Audio",
                format!("{} @ {} Hz.", audio.codec, audio.sample_rate),
            ));
            if let (Some(audio_duration), Some(format_duration)) =
                (audio.duration_secs, probe.duration_secs)
            {
                let diff = (audio_duration - format_duration).abs();
                if diff > 0.3 {
                    findings.push(warning(
                        "av_sync",
                        "Audio/video",
                        format!(
                            "Audio duration {audio_duration:.2}s differs from video {format_duration:.2}s by {diff:.2}s."
                        ),
                    ));
                }
            }
        }
    }

    if let (Some(expected), Some(actual)) = (expected_runtime_secs, probe.duration_secs) {
        let diff = (actual - expected).abs();
        if diff > runtime_tolerance_secs {
            findings.push(warning(
                "runtime",
                "Runtime",
                format!(
                    "Duration {actual:.2}s differs from expected {expected:.2}s by {diff:.2}s."
                ),
            ));
        }
    }

    if let Some(levels) = levels {
        if levels.mean_db < -60.0 {
            findings.push(warning(
                "silence",
                "Audio levels",
                format!("Near-silent audio (mean {:.1} dB).", levels.mean_db),
            ));
        }
        if levels.max_db > -1.0 {
            findings.push(warning(
                "clipping",
                "Audio levels",
                format!("Possible clipping (peak {:.1} dB).", levels.max_db),
            ));
        }
    }

    findings
}

fn parse_frame_rate(raw: &str) -> f64 {
    let (numerator, denominator) = raw.split_once('/').unwrap_or((raw, "1"));
    let numerator = numerator.parse::<f64>().unwrap_or(0.0);
    let denominator = denominator.parse::<f64>().unwrap_or(1.0);
    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

fn parse_db_line(stderr: &str, key: &str) -> Option<f64> {
    let line = stderr.lines().find(|line| line.contains(key))?;
    let value = line.split(key).nth(1)?;
    let value = value.trim();
    let value = value.strip_suffix("dB").unwrap_or(value).trim();
    value.parse::<f64>().ok()
}

fn info(id: &str, check: &str, message: impl Into<String>) -> QCFinding {
    QCFinding {
        id: id.to_string(),
        severity: QCSeverity::Info,
        check: check.to_string(),
        message: message.into(),
        artifact_id: None,
    }
}

fn warning(id: &str, check: &str, message: impl Into<String>) -> QCFinding {
    QCFinding {
        id: id.to_string(),
        severity: QCSeverity::Warning,
        check: check.to_string(),
        message: message.into(),
        artifact_id: None,
    }
}

fn error(id: &str, check: &str, message: impl Into<String>) -> QCFinding {
    QCFinding {
        id: id.to_string(),
        severity: QCSeverity::Error,
        check: check.to_string(),
        message: message.into(),
        artifact_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe_json(duration: &str, with_video: bool, with_audio: bool) -> Value {
        let mut streams = Vec::new();
        if with_video {
            streams.push(serde_json::json!({
                "codec_type": "video",
                "codec_name": "h264",
                "width": 1920,
                "height": 1080,
                "avg_frame_rate": "30000/1001",
                "nb_frames": "900"
            }));
        }
        if with_audio {
            streams.push(serde_json::json!({
                "codec_type": "audio",
                "codec_name": "aac",
                "sample_rate": "48000",
                "duration": duration
            }));
        }
        serde_json::json!({
            "format": { "duration": duration },
            "streams": streams
        })
    }

    #[test]
    fn parse_ffprobe_reads_video_audio_and_duration() {
        let probe = parse_ffprobe(&probe_json("30.0", true, true));
        assert_eq!(probe.duration_secs, Some(30.0));
        let video = probe.video.unwrap();
        assert_eq!((video.width, video.height), (1920, 1080));
        assert!((video.frame_rate - 29.97).abs() < 0.01);
        assert_eq!(video.codec, "h264");
        assert_eq!(video.nb_frames, Some(900));
        let audio = probe.audio.unwrap();
        assert_eq!(audio.sample_rate, 48000);
        assert_eq!(audio.codec, "aac");
    }

    #[test]
    fn parse_ffprobe_handles_audio_only() {
        let probe = parse_ffprobe(&probe_json("12.0", false, true));
        assert!(probe.video.is_none());
        assert!(probe.audio.is_some());
    }

    #[test]
    fn frame_rate_parses_fractional_and_fixed() {
        assert!((parse_frame_rate("30000/1001") - 29.97).abs() < 0.01);
        assert_eq!(parse_frame_rate("25/1"), 25.0);
        assert_eq!(parse_frame_rate("0/0"), 0.0);
    }

    #[test]
    fn volumedetect_parses_mean_and_peak() {
        let stderr = "[Parsed_volumedetect_0 @ 0x0] n_samples: 123\n\
                      [Parsed_volumedetect_0 @ 0x0] mean_volume: -25.3 dB\n\
                      [Parsed_volumedetect_0 @ 0x0] max_volume: -0.5 dB\n";
        let levels = parse_volumedetect(stderr).unwrap();
        assert!((levels.mean_db - -25.3).abs() < 0.01);
        assert!((levels.max_db - -0.5).abs() < 0.01);
    }

    #[test]
    fn findings_flag_missing_video_as_error() {
        let probe = MediaProbe {
            duration_secs: Some(5.0),
            video: None,
            audio: None,
        };
        let findings = findings_from_probe(&probe, None, None, 0.3);
        assert!(findings.iter().any(|f| f.severity == QCSeverity::Error));
        assert!(findings.iter().any(|f| f.id == "no_video"));
    }

    #[test]
    fn findings_flag_silence_and_clipping() {
        let probe = MediaProbe {
            duration_secs: Some(10.0),
            video: Some(VideoStream {
                width: 1280,
                height: 720,
                frame_rate: 30.0,
                codec: "h264".into(),
                nb_frames: Some(300),
            }),
            audio: Some(AudioStream {
                sample_rate: 48000,
                codec: "aac".into(),
                duration_secs: Some(10.0),
            }),
        };
        let levels = AudioLevels {
            mean_db: -70.0,
            max_db: -0.4,
        };
        let findings = findings_from_probe(&probe, Some(levels), None, 0.3);
        assert!(findings.iter().any(|f| f.id == "silence"));
        assert!(findings.iter().any(|f| f.id == "clipping"));
        assert!(!findings.iter().any(|f| f.severity == QCSeverity::Error));
    }

    #[test]
    fn findings_flag_runtime_out_of_tolerance() {
        let probe = MediaProbe {
            duration_secs: Some(5.3),
            video: Some(VideoStream {
                width: 640,
                height: 360,
                frame_rate: 24.0,
                codec: "h264".into(),
                nb_frames: Some(120),
            }),
            audio: None,
        };
        let findings = findings_from_probe(&probe, None, Some(5.0), 0.1);
        assert!(findings.iter().any(|f| f.id == "runtime"));
    }

    #[test]
    fn check_media_file_probes_a_real_generated_video() {
        if Command::new("ffprobe").arg("-version").output().is_err() {
            return; // ffprobe unavailable; skip the integration check.
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.mp4");
        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=320x240:d=1:r=24",
            ])
            .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=1"])
            .args([
                "-shortest",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
            ])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success(), "ffmpeg failed to generate the sample");
        let findings = check_media_file(&path, None, 0.3);
        assert!(
            !findings.iter().any(|f| f.severity == QCSeverity::Error),
            "unexpected error findings: {findings:?}"
        );
        assert!(findings.iter().any(|f| f.id == "has_video"));
        assert!(findings.iter().any(|f| f.id == "audio"));
    }

    #[test]
    fn check_media_file_errors_on_a_missing_file() {
        let findings =
            check_media_file(Path::new("/nonexistent/definitely_missing.mp4"), None, 0.3);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].id, "file_exists");
        assert_eq!(findings[0].severity, QCSeverity::Error);
    }

    #[test]
    fn parse_spec_resolution_and_aspect() {
        assert_eq!(parse_resolution("1080x1920"), Some((1080, 1920)));
        assert_eq!(parse_resolution("bogus"), None);
        assert!((parse_aspect("9:16").unwrap() - 0.5625).abs() < 0.001);
        assert_eq!(parse_aspect("9"), None);
    }

    #[test]
    fn parse_frame_rate_spec_handles_plain_and_fraction() {
        assert_eq!(parse_frame_rate_spec("24"), Some(24.0));
        let ntsc = parse_frame_rate_spec("30000/1001").unwrap();
        assert!((ntsc - 29.97).abs() < 0.01);
        assert_eq!(parse_frame_rate_spec("nope"), None);
    }

    #[test]
    fn spec_findings_flag_resolution_aspect_and_framerate_mismatch() {
        let probe = MediaProbe {
            duration_secs: Some(1.0),
            video: Some(VideoStream {
                width: 1280,
                height: 720,
                frame_rate: 30.0,
                codec: "h264".into(),
                nb_frames: Some(30),
            }),
            audio: None,
        };
        let findings = spec_findings(&probe, Some("1920x1080"), Some("9:16"), Some("24"));
        assert!(findings.iter().any(|f| f.id == "resolution_mismatch"));
        assert!(findings.iter().any(|f| f.id == "aspect_mismatch"));
        assert!(findings.iter().any(|f| f.id == "framerate_mismatch"));
        assert!(!findings.iter().any(|f| f.severity == QCSeverity::Error));
    }

    #[test]
    fn spec_findings_pass_when_file_matches() {
        let probe = MediaProbe {
            duration_secs: Some(1.0),
            video: Some(VideoStream {
                width: 1080,
                height: 1920,
                frame_rate: 24.0,
                codec: "h264".into(),
                nb_frames: Some(24),
            }),
            audio: None,
        };
        let findings = spec_findings(&probe, Some("1080x1920"), Some("9:16"), Some("24"));
        assert!(findings.iter().any(|f| f.id == "resolution"));
        assert!(!findings.iter().any(|f| f.id.ends_with("_mismatch")));
    }
}
