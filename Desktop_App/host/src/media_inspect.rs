//! Bounded deterministic acoustic activity measurement; activity is not speech.
use serde_json::{Map, Value};
use std::path::Path;

use serde_json::json;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_DURATION_SECONDS: f64 = 1800.0;
const MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
const PROCESS_BUDGET: Duration = Duration::from_secs(60);
// Exclude playlist/concat demuxers, which could read other local files.
const FORMATS: &str = "mov,matroska,webm,wav,mp3,flac,ogg,aac,aiff,avi,mpeg,mpegts";

/// Inspect the first audio stream without modifying media or calling a provider.
/// All times are source-relative seconds, not dialogue/ASR boundaries.
pub fn inspect_media(path: &Path, inputs: &Map<String, Value>) -> Result<Value, String> {
    fn parameter(
        inputs: &Map<String, Value>,
        key: &str,
        default: f64,
        min: f64,
        max: f64,
    ) -> Result<f64, String> {
        let value = match inputs.get(key) {
            None => default,
            Some(value) => value
                .as_f64()
                .ok_or_else(|| format!("{key} must be a finite number in [{min}, {max}]"))?,
        };
        if !value.is_finite() || value < min || value > max {
            return Err(format!("{key} must be a finite number in [{min}, {max}]"));
        }
        Ok(value)
    }
    let threshold = parameter(inputs, "threshold_db", -40.0, -80.0, -10.0)?;
    let min_silence = parameter(inputs, "min_silence_seconds", 0.12, 0.02, 2.0)?;
    // stderr includes attacker-controlled container tags/filenames, so it is never
    // measurement input. Clear decoded frame tags before the detector, then print
    // only detector-generated metadata to stdout (file=- is stdout, not a file).
    let filter = format!("asetpts=PTS-STARTPTS,ametadata=mode=delete,silencedetect=noise={threshold}dB:d={min_silence},ametadata=mode=print:file=-:direct=1");
    let deadline = Instant::now() + PROCESS_BUDGET;
    let source = path.to_str().ok_or("Source path must be valid UTF-8")?;
    let metadata = std::fs::metadata(path).map_err(|e| format!("Cannot inspect source: {e}"))?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err(format!(
            "Source must be a regular file of at most {MAX_FILE_BYTES} bytes"
        ));
    }
    let absolute =
        std::fs::canonicalize(path).map_err(|e| format!("Cannot resolve source: {e}"))?;
    let mut probe = Command::new("ffprobe");
    probe.args(["-v", "error", "-protocol_whitelist", "file", "-format_whitelist", FORMATS,
        "-show_entries", "format=duration,start_time:stream=index,codec_type,codec_name,sample_rate,duration,start_time,width,height,avg_frame_rate,nb_frames",
        "-of", "json"]).arg(&absolute);
    let (stdout, _) = run_bounded(&mut probe, deadline, MAX_OUTPUT_BYTES)?;
    let raw: Value =
        serde_json::from_slice(&stdout).map_err(|e| format!("Malformed ffprobe output: {e}"))?;
    // Reuse the existing pure technical parser, not probe_media's unbounded subprocess.
    let technical = crate::workflow::qc::parse_ffprobe(&raw);
    let duration = technical
        .duration_secs
        .filter(|v| v.is_finite() && *v > 0.0 && *v <= MAX_DURATION_SECONDS)
        .ok_or("Media duration must be finite, positive and at most 1800 seconds")?;
    let audio_stream = raw["streams"]
        .as_array()
        .and_then(|s| s.iter().find(|s| s["codec_type"] == "audio"));
    let mut result = json!({
        "audio_stream":technical.audio.as_ref().map(|s| json!({"index":audio_stream.and_then(|a| a.get("index")),"codec":s.codec,"sample_rate":s.sample_rate})),
        "video_stream":technical.video.as_ref().map(|s| json!({"codec":s.codec,"width":s.width,"height":s.height,"frame_rate":s.frame_rate,"nb_frames":s.nb_frames})),
        "source_path":source,"duration_seconds":duration,"has_audio":technical.audio.is_some(),
        "method":"ffmpeg_silencedetect","threshold_db":threshold,"min_silence_seconds":min_silence,
        "analyzed_range":null,"silence_intervals":[],"non_silent_intervals":[],
        "candidate_first_activity_seconds":null,"candidate_last_activity_seconds":null,
        "activity_status":"no_audio","verified_speech_boundaries":false,"warnings":[],
        "limitations":["Non-silent acoustic activity is not speech; music/noise may be active and quiet speech may be missed.","Silences shorter than min_silence_seconds are not detected; channels are analyzed together.","Only the first audio stream is measured; these are candidate activity boundaries, not verified dialogue trim points."],
        "limits":{"max_file_bytes":MAX_FILE_BYTES,"max_duration_seconds":MAX_DURATION_SECONDS,"process_budget_seconds":PROCESS_BUDGET.as_secs(),"max_output_bytes_per_pipe":MAX_OUTPUT_BYTES}
    });
    if technical.audio.is_none() {
        result["warnings"] = json!(["No audio stream; absence of audio is not measured silence."]);
        return Ok(result);
    }
    let audio = raw["streams"]
        .as_array()
        .and_then(|s| s.iter().find(|s| s["codec_type"] == "audio"));
    let format_start = raw["format"]["start_time"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    let offset = audio
        .and_then(|s| s["start_time"].as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .map(|s| (s - format_start).max(0.0))
        .unwrap_or(0.0);
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-nostdin",
            "-hide_banner",
            "-nostats",
            "-loglevel",
            "info",
            "-xerror",
            "-protocol_whitelist",
            "file",
            "-format_whitelist",
            FORMATS,
            "-threads",
            "1",
            "-i",
        ])
        .arg(&absolute)
        .args([
            "-map",
            "0:a:0",
            "-vn",
            "-sn",
            "-dn",
            "-t",
            "1800.1",
            "-filter_threads",
            "1",
            "-af",
            &filter,
            "-progress",
            "pipe:1",
            "-f",
            "null",
            "-",
        ]);
    let (progress, _) = run_bounded(&mut command, deadline, MAX_OUTPUT_BYTES)?;
    let progress = std::str::from_utf8(&progress).map_err(|_| "Malformed ffmpeg progress")?;
    let measured = progress
        .lines()
        .filter_map(|l| l.strip_prefix("out_time_us="))
        .filter_map(|v| v.parse::<f64>().ok())
        .last()
        .map(|v| v / 1_000_000.0)
        .filter(|v| v.is_finite() && *v > 0.0 && *v <= MAX_DURATION_SECONDS)
        .ok_or("Malformed ffmpeg output: no finite decoded audio duration")?;
    if !progress.lines().any(|l| l == "progress=end") {
        return Err("Incomplete ffmpeg progress".into());
    }
    let end = offset + measured;
    if raw["streams"]
        .as_array()
        .map(|streams| {
            streams
                .iter()
                .filter(|s| s["codec_type"] == "audio")
                .count()
        })
        .unwrap_or(0)
        > 1
    {
        result["warnings"].as_array_mut().unwrap().push(json!(
            "Additional audio streams were not analyzed; only 0:a:0 is measured."
        ));
    }
    if offset > 0.0 || end + 0.02 < duration {
        result["warnings"].as_array_mut().unwrap().push(json!("The first audio stream does not cover the full source timeline; uncovered time is not classified as measured silence."));
    }
    let silence = parse_silence(progress, measured)?;
    let mut non_silent = Vec::new();
    let mut cursor = 0.0;
    let intervals: Vec<Value> = silence.iter().map(|&(start, stop)| {
        if start > cursor { non_silent.push(json!({"start_seconds": offset+cursor, "end_seconds": offset+start})); }
        cursor = stop;
        json!({"start_seconds":offset+start,"end_seconds":offset+stop,"kind": if start == 0.0 && stop == measured { "full" } else if start == 0.0 { "leading" } else if stop == measured { "trailing" } else { "interior" }})
    }).collect();
    if cursor < measured {
        non_silent.push(json!({"start_seconds":offset+cursor,"end_seconds":end}));
    }
    result["analyzed_range"] = json!({"start_seconds":offset,"end_seconds":end});
    result["silence_intervals"] = json!(intervals);
    result["candidate_first_activity_seconds"] = non_silent
        .first()
        .map(|v| v["start_seconds"].clone())
        .unwrap_or(Value::Null);
    result["candidate_last_activity_seconds"] = non_silent
        .last()
        .map(|v| v["end_seconds"].clone())
        .unwrap_or(Value::Null);
    result["activity_status"] = json!(if non_silent.is_empty() {
        "fully_silent"
    } else if silence.is_empty() {
        "continuously_active"
    } else {
        "mixed"
    });
    result["non_silent_intervals"] = json!(non_silent);
    Ok(result)
}

// Only call with the metadata-cleared machine stdout after successful progress=end.
fn parse_silence(output: &str, duration: f64) -> Result<Vec<(f64, f64)>, String> {
    let mut intervals = Vec::new();
    let mut pending = None;
    let mut pending_end = None;
    // ffmpeg metadata uses six significant digits; progress is in microseconds.
    let rounding_tolerance = 0.5 * 10_f64.powf(duration.log10().floor() - 5.0) + 0.0000005;
    for line in output.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "lavfi.silence_start" => {
                let start = value
                    .parse::<f64>()
                    .map_err(|_| "Malformed silence_start")?;
                if pending.is_some()
                    || !start.is_finite()
                    || start < 0.0
                    || start > duration
                    || intervals.last().is_some_and(|&(_, prev)| start < prev)
                {
                    return Err("Malformed silence_start ordering/range".into());
                }
                pending = Some(start);
            }
            "lavfi.silence_end" => {
                let end = value.parse::<f64>().map_err(|_| "Malformed silence_end")?;
                let start = pending.ok_or("Malformed silence_end without start")?;
                if pending_end.is_some()
                    || !end.is_finite()
                    || end <= start
                    || end > duration + 0.002_f64.max(rounding_tolerance)
                {
                    return Err("Malformed silence_end ordering/range".into());
                }
                pending_end = Some(end);
            }
            "lavfi.silence_duration" => {
                let length = value
                    .parse::<f64>()
                    .map_err(|_| "Malformed silence_duration")?;
                let start = pending
                    .take()
                    .ok_or("Malformed silence_duration without start")?;
                let end = pending_end
                    .take()
                    .ok_or("Malformed silence_duration without end")?;
                if !length.is_finite()
                    || length <= 0.0
                    || (length - (end - start)).abs() > 0.002_f64.max(rounding_tolerance * 3.0)
                {
                    return Err("Malformed silence_duration consistency".into());
                }
                intervals.push((
                    start,
                    if (end - duration).abs() <= rounding_tolerance {
                        duration
                    } else {
                        end.min(duration)
                    },
                ));
            }
            // Progress and frame headers are unrelated; never accept log prefixes
            // or substring matches as detector metadata.
            _ => {}
        }
    }
    if pending_end.is_some() {
        return Err("Malformed silence metadata: missing silence_duration".into());
    }
    if let Some(start) = pending {
        // EOF silence_end is only logged, not attached to a decoded frame. The
        // caller has verified successful decoding; close at its measured end.
        intervals.push((start, duration));
    }
    Ok(intervals)
}

fn run_bounded(
    command: &mut Command,
    deadline: Instant,
    limit: usize,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let name = command.get_program().to_string_lossy().into_owned();
    if Instant::now() >= deadline {
        return Err(format!("{name} exceeded media inspection time limit"));
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not run {name}: {e}"))?;
    let exceeded = Arc::new(AtomicBool::new(false));
    fn reader(
        pipe: impl Read + Send + 'static,
        limit: usize,
        exceeded: Arc<AtomicBool>,
    ) -> std::thread::JoinHandle<std::io::Result<Vec<u8>>> {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = pipe.take(limit as u64 + 1).read_to_end(&mut bytes);
            if bytes.len() > limit {
                exceeded.store(true, Ordering::SeqCst);
            }
            result.map(|_| bytes)
        })
    }
    let out = reader(child.stdout.take().unwrap(), limit, exceeded.clone());
    let err = reader(child.stderr.take().unwrap(), limit, exceeded.clone());
    let status = loop {
        if exceeded.load(Ordering::SeqCst) {
            break Err(format!("{name} output exceeded {limit} bytes"));
        }
        if Instant::now() >= deadline {
            break Err(format!("{name} exceeded media inspection time limit"));
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => break Err(format!("Could not wait for {name}: {e}")),
        }
    };
    if status.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    let stdout = out
        .join()
        .map_err(|_| "Media stdout reader panicked")?
        .map_err(|e| format!("Media stdout read failed: {e}"))?;
    let stderr = err
        .join()
        .map_err(|_| "Media stderr reader panicked")?
        .map_err(|e| format!("Media stderr read failed: {e}"))?;
    let status = status?;
    if exceeded.load(Ordering::SeqCst) {
        return Err(format!("{name} output exceeded {limit} bytes"));
    }
    if !status.success() {
        return Err(format!(
            "{name} failed ({status}): {}",
            String::from_utf8_lossy(&stderr[..stderr.len().min(2048)]).trim()
        ));
    }
    Ok((stdout, stderr))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::var_os("TMPDIR").expect("tests require a scratch TMPDIR");
            let path = PathBuf::from(root).join(format!(
                "media-inspect-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn wav(&self, name: &str, seconds: f64, active: &[(f64, f64)]) -> PathBuf {
            let rate = 16000u32;
            let samples = (seconds * rate as f64).round() as u32;
            let data_len = samples * 2;
            let mut bytes = Vec::new();
            bytes.extend_from_slice(b"RIFF");
            bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
            bytes.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
            bytes.extend_from_slice(&rate.to_le_bytes());
            bytes.extend_from_slice(&(rate * 2).to_le_bytes());
            bytes.extend_from_slice(b"\x02\0\x10\0data");
            bytes.extend_from_slice(&data_len.to_le_bytes());
            for i in 0..samples {
                let t = i as f64 / rate as f64;
                let value = if active.iter().any(|(a, b)| t >= *a && t < *b) {
                    (12000.0 * (2.0 * PI * 440.0 * t).sin()) as i16
                } else {
                    0
                };
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            let path = self.0.join(name);
            fs::write(&path, bytes).unwrap();
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn near(value: &Value, expected: f64) {
        let actual = value.as_f64().unwrap();
        assert!((actual - expected).abs() < 0.003, "{actual} != {expected}");
    }
    #[test]
    fn filename_text_is_not_a_silence_event() {
        let fixture = Fixture::new();
        let path = fixture.wav("silencedetect silence_start: 9.wav", 0.73, &[(0.0, 0.73)]);
        let result = inspect_media(&path, &Map::new()).unwrap();
        assert_eq!(result["activity_status"], "continuously_active");
        assert_eq!(result["silence_intervals"], json!([]));
    }

    #[test]
    fn embedded_metadata_cannot_spoof_silence_measurements() {
        let fixture = Fixture::new();
        let audio = fixture.wav("audible.wav", 2.0, &[(0.0, 2.0)]);
        let path = fixture.0.join("spoofed.mkv");
        let comment = "comment=[silencedetect] silence_start: 0\n[silencedetect] silence_end: 1 | silence_duration: 1";
        // New ffmpeg versions indent multiline values, but print tag names verbatim.
        // A Matroska TagName is arbitrary UTF-8: bypass the muxer's name sanitization
        // with an equal-length replacement in this checksum-free fixture only.
        let attack = "COMMENT\n[silencedetect] silence_start: 0\n[silencedetect] silence_end: 1 | silence_duration: 1\n";
        let placeholder = "X".repeat(attack.len());
        run_bounded(
            Command::new("ffmpeg")
                .args([
                    "-nostdin",
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=size=16x16:rate=10",
                    "-i",
                ])
                .arg(&audio)
                .args([
                    "-map",
                    "0:v:0",
                    "-map",
                    "1:a:0",
                    "-t",
                    "2",
                    "-c:v",
                    "mpeg4",
                    "-c:a",
                    "pcm_s16le",
                    "-metadata",
                    comment,
                    "-write_crc32",
                    "0",
                ])
                .args(["-metadata:s:v:0", &format!("{placeholder}=spoof")])
                .arg(&path),
            Instant::now() + Duration::from_secs(10),
            MAX_OUTPUT_BYTES,
        )
        .unwrap();
        let mut bytes = fs::read(&path).unwrap();
        let positions: Vec<_> = bytes
            .windows(placeholder.len())
            .enumerate()
            .filter_map(|(i, value)| (value == placeholder.as_bytes()).then_some(i))
            .collect();
        assert_eq!(positions.len(), 1);
        let position = positions[0];
        bytes[position..position + attack.len()].copy_from_slice(attack.as_bytes());
        fs::write(&path, bytes).unwrap();
        let before = fs::read(&path).unwrap();
        let result = inspect_media(&path, &Map::new()).unwrap();
        assert_eq!(result["activity_status"], "continuously_active", "{result}");
        assert_eq!(result["silence_intervals"], json!([]));
        assert_eq!(result["non_silent_intervals"].as_array().unwrap().len(), 1);
        near(&result["candidate_first_activity_seconds"], 0.0);
        near(&result["candidate_last_activity_seconds"], 2.0);
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn silence_log_rounding_does_not_create_phantom_activity() {
        let intervals = parse_silence(
            "lavfi.silence_start=0\nlavfi.silence_end=1.33331\nlavfi.silence_duration=1.33331",
            1.333313,
        )
        .unwrap();
        assert_eq!(intervals, vec![(0.0, 1.333313)]);
        // Do not hide a genuine short tail that exceeds ffmpeg metadata precision.
        let intervals = parse_silence(
            "lavfi.silence_start=0\nlavfi.silence_end=1.33\nlavfi.silence_duration=1.33",
            1.333313,
        )
        .unwrap();
        assert_eq!(intervals, vec![(0.0, 1.33)]);
    }

    #[test]
    fn machine_metadata_closes_eof_silence_at_decoded_end() {
        // At EOF silencedetect logs the end to stderr but emits no final frame metadata.
        let intervals = parse_silence(
            "frame:3 pts:3072 pts_time:0.192\nlavfi.silence_start=0.17\nout_time_us=731250\nprogress=end\n",
            0.73125,
        ).unwrap();
        assert_eq!(intervals, vec![(0.17, 0.73125)]);
        assert_eq!(parse_silence(
            "[silencedetect] silence_start: 0\n[silencedetect] silence_end: 1 | silence_duration: 1\n",
            2.0,
        ).unwrap(), vec![]);
    }

    #[test]
    fn continuously_active_is_deterministic_and_invents_no_silence() {
        let fixture = Fixture::new();
        let path = fixture.wav("continuous ; not-a-command.wav", 1.19, &[(0.0, 1.19)]);
        let before = fs::read(&path).unwrap();
        let result = inspect_media(&path, &Map::new()).unwrap();
        assert_eq!(result, inspect_media(&path, &Map::new()).unwrap());
        assert_eq!(result["source_path"], path.to_str().unwrap());
        assert_eq!(result["activity_status"], "continuously_active");
        assert_eq!(result["silence_intervals"], json!([]));
        let intervals = result["non_silent_intervals"].as_array().unwrap();
        assert_eq!(intervals.len(), 1);
        near(&intervals[0]["start_seconds"], 0.0);
        near(&intervals[0]["end_seconds"], 1.19);
        near(&result["candidate_first_activity_seconds"], 0.0);
        near(&result["candidate_last_activity_seconds"], 1.19);
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn rejects_corrupt_oversize_overduration_and_nonfile_sources() {
        let fixture = Fixture::new();
        let corrupt = fixture.0.join("corrupt.mp4");
        fs::write(&corrupt, b"this is not media").unwrap();
        let error = inspect_media(&corrupt, &Map::new()).unwrap_err();
        assert!(error.contains("ffprobe failed"), "{error}");
        let oversized = fixture.0.join("oversized.wav");
        fs::File::create(&oversized)
            .unwrap()
            .set_len(MAX_FILE_BYTES + 1)
            .unwrap();
        assert!(inspect_media(&oversized, &Map::new())
            .unwrap_err()
            .contains("regular file"));
        assert!(inspect_media(&fixture.0, &Map::new())
            .unwrap_err()
            .contains("regular file"));
        assert!(inspect_media(&fixture.0.join("missing.wav"), &Map::new())
            .unwrap_err()
            .contains("Cannot inspect source"));
        // A sparse WAV is genuinely parseable but has a timeline exceeding the analysis cap.
        let long = fixture.wav("too-long.wav", 0.01, &[]);
        let mut header = fs::read(&long).unwrap();
        let data_len = ((MAX_DURATION_SECONDS + 1.0) * 16000.0 * 2.0) as u32;
        header[4..8].copy_from_slice(&(36 + data_len).to_le_bytes());
        header[40..44].copy_from_slice(&data_len.to_le_bytes());
        fs::write(&long, &header[..44]).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&long)
            .unwrap()
            .set_len(44 + u64::from(data_len))
            .unwrap();
        assert!(inspect_media(&long, &Map::new())
            .unwrap_err()
            .contains("at most 1800"));
    }

    #[test]
    fn subprocess_timeout_output_caps_and_nonzero_status_are_errors() {
        let start = Instant::now();
        let error = run_bounded(
            Command::new("ffmpeg").args([
                "-nostdin",
                "-v",
                "error",
                "-re",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440",
                "-t",
                "30",
                "-f",
                "null",
                "-",
            ]),
            start + Duration::from_millis(75),
            MAX_OUTPUT_BYTES,
        )
        .unwrap_err();
        assert!(error.contains("time limit"), "{error}");
        assert!(start.elapsed() < Duration::from_secs(3));
        let error = run_bounded(
            Command::new("ffprobe").arg("-version"),
            Instant::now() + Duration::from_secs(5),
            16,
        )
        .unwrap_err();
        assert!(error.contains("output exceeded 16 bytes"), "{error}");
        let error = run_bounded(
            Command::new("ffmpeg").arg("-media-inspect-invalid-option"),
            Instant::now() + Duration::from_secs(5),
            16,
        )
        .unwrap_err();
        assert!(error.contains("output exceeded 16 bytes"), "{error}");
        let error = run_bounded(
            Command::new("ffmpeg").arg("-media-inspect-invalid-option"),
            Instant::now() + Duration::from_secs(5),
            MAX_OUTPUT_BYTES,
        )
        .unwrap_err();
        assert!(error.contains("ffmpeg failed"), "{error}");
        assert!(error.len() < 2200);
    }

    #[test]
    fn expired_budget_does_not_spawn_another_process() {
        let error = run_bounded(
            &mut Command::new("media-inspect-nonexistent-binary"),
            Instant::now() - Duration::from_millis(1),
            16,
        )
        .unwrap_err();
        assert!(error.contains("time limit"), "{error}");
    }

    #[test]
    fn measures_only_first_embedded_audio_with_source_timeline_offset() {
        let fixture = Fixture::new();
        let first = fixture.wav("first.wav", 1.17, &[(0.21, 0.86)]);
        let second = fixture.wav("second.wav", 2.4, &[(0.0, 2.4)]);
        let path = fixture.0.join("embedded.mkv");
        run_bounded(
            Command::new("ffmpeg")
                .args([
                    "-nostdin",
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=size=16x16:rate=10",
                    "-itsoffset",
                    "0.43",
                    "-i",
                ])
                .arg(&first)
                .arg("-i")
                .arg(&second)
                .args([
                    "-map",
                    "0:v:0",
                    "-map",
                    "1:a:0",
                    "-map",
                    "2:a:0",
                    "-t",
                    "2.4",
                    "-c:v",
                    "mpeg4",
                    "-c:a",
                    "pcm_s16le",
                ])
                .arg(&path),
            Instant::now() + Duration::from_secs(10),
            MAX_OUTPUT_BYTES,
        )
        .unwrap();
        let before = fs::read(&path).unwrap();
        let result = inspect_media(&path, &Map::new()).unwrap();
        near(&result["duration_seconds"], 2.4);
        near(&result["analyzed_range"]["start_seconds"], 0.43);
        near(&result["analyzed_range"]["end_seconds"], 1.60);
        near(&result["candidate_first_activity_seconds"], 0.64);
        near(&result["candidate_last_activity_seconds"], 1.29);
        assert_eq!(result["audio_stream"]["sample_rate"], 16000);
        assert_eq!(result["audio_stream"]["index"], 1);
        assert_eq!(result["video_stream"]["width"], 16);
        assert!(!result["warnings"].as_array().unwrap().is_empty());
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn rejects_inconsistent_or_malformed_silence_events() {
        for log in [
            "lavfi.silence_start=0.2\nlavfi.silence_end=0.8\nlavfi.silence_duration=9",
            "lavfi.silence_start=NaN",
            "lavfi.silence_start=0.3\nlavfi.silence_start=0.4",
            "lavfi.silence_end=0.8\nlavfi.silence_duration=0.8",
            "lavfi.silence_start=0.3\nlavfi.silence_end=0.2\nlavfi.silence_duration=-0.1",
            "lavfi.silence_start=0.3\nlavfi.silence_end=0.8",
            "lavfi.silence_start=garbage",
            "lavfi.silence_start=1.1",
            "lavfi.silence_start=0\nlavfi.silence_end=0.8\nlavfi.silence_duration=0.8\nlavfi.silence_start=0.5",
        ] {
            assert!(parse_silence(log, 1.0).is_err(), "accepted malformed log: {log}");
        }
    }

    #[test]
    fn fully_silent_does_not_invent_subsample_tail_activity() {
        let fixture = Fixture::new();
        let path = fixture.wav("silent.wav", 1.3333125, &[]);
        let result = inspect_media(&path, &Map::new()).unwrap();
        assert_eq!(result["activity_status"], "fully_silent", "{result}");
        assert_eq!(result["non_silent_intervals"], json!([]));
        assert!(result["candidate_first_activity_seconds"].is_null());
        assert!(result["candidate_last_activity_seconds"].is_null());
        assert_eq!(result["silence_intervals"][0]["kind"], "full");
        near(&result["silence_intervals"][0]["end_seconds"], 1.3333125);
    }

    #[test]
    fn distinguishes_no_audio_from_silence_without_inventing_intervals() {
        let fixture = Fixture::new();
        let path = fixture.0.join("noaudio.mp4");
        run_bounded(
            Command::new("ffmpeg")
                .args([
                    "-nostdin",
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=size=16x16:rate=10",
                    "-t",
                    "0.8",
                    "-c:v",
                    "mpeg4",
                ])
                .arg(&path),
            Instant::now() + Duration::from_secs(10),
            MAX_OUTPUT_BYTES,
        )
        .unwrap();
        let result = inspect_media(&path, &Map::new()).unwrap();
        assert_eq!(result["has_audio"], false);
        assert_eq!(result["activity_status"], "no_audio");
        near(&result["duration_seconds"], 0.8);
        assert_eq!(result["silence_intervals"], json!([]));
        assert_eq!(result["non_silent_intervals"], json!([]));
        assert!(result["candidate_first_activity_seconds"].is_null());
        assert!(result["analyzed_range"].is_null());
        assert!(!result["warnings"].as_array().unwrap().is_empty());
    }

    #[test]
    fn rejects_invalid_parameters_before_opening_source() {
        for (key, value) in [
            ("threshold_db", json!(-80.1)),
            ("threshold_db", json!(-9.9)),
            ("threshold_db", json!("-40")),
            ("threshold_db", Value::Null),
            ("min_silence_seconds", json!(0.019)),
            ("min_silence_seconds", json!(2.001)),
            ("min_silence_seconds", json!(true)),
        ] {
            let error = inspect_media(
                Path::new("/missing/media-inspect-fixture"),
                &Map::from_iter([(key.into(), value)]),
            )
            .unwrap_err();
            assert!(error.contains(key), "{key}: {error}");
        }
    }

    #[test]
    fn uses_requested_detection_parameters_in_real_measurement() {
        let fixture = Fixture::new();
        let path = fixture.wav("quiet.wav", 1.41, &[(0.23, 0.68)]);
        let mut bytes = fs::read(&path).unwrap();
        for sample in bytes[44..].chunks_exact_mut(2) {
            let value = i16::from_le_bytes([sample[0], sample[1]]) / 10;
            sample.copy_from_slice(&value.to_le_bytes());
        }
        fs::write(&path, bytes).unwrap();
        let mut inputs = Map::new();
        inputs.insert("threshold_db".into(), json!(-10.0));
        inputs.insert("min_silence_seconds".into(), json!(0.5));
        let result = inspect_media(&path, &inputs).unwrap();
        assert_eq!(result["threshold_db"], -10.0);
        assert_eq!(result["min_silence_seconds"], 0.5);
        assert_eq!(result["activity_status"], "fully_silent");
        let result = inspect_media(
            &path,
            &Map::from_iter([("min_silence_seconds".into(), json!(0.5))]),
        )
        .unwrap();
        let silence = result["silence_intervals"].as_array().unwrap();
        assert_eq!(silence.len(), 1);
        near(&silence[0]["start_seconds"], 0.68);
        assert_eq!(silence[0]["kind"], "trailing");
    }

    // Reversed endpoints, estimated dialogue times, or container-wide activity fail this fixture.
    #[test]
    fn measures_asymmetric_source_relative_silence_and_activity() {
        let fixture = Fixture::new();
        let path = fixture.wav("asymmetric.wav", 2.31, &[(0.37, 0.98), (1.23, 1.70)]);
        let result = inspect_media(&path, &Map::new()).unwrap();
        assert_eq!(result["source_path"], path.to_str().unwrap());
        assert_eq!(result["has_audio"], true);
        assert_eq!(result["activity_status"], "mixed");
        near(&result["duration_seconds"], 2.31);
        let silence = result["silence_intervals"].as_array().unwrap();
        assert_eq!(silence.len(), 3);
        for (item, (start, end, kind)) in silence.iter().zip([
            (0.0, 0.37, "leading"),
            (0.98, 1.23, "interior"),
            (1.70, 2.31, "trailing"),
        ]) {
            near(&item["start_seconds"], start);
            near(&item["end_seconds"], end);
            assert_eq!(item["kind"], kind);
        }
        let activity = result["non_silent_intervals"].as_array().unwrap();
        assert_eq!(activity.len(), 2);
        near(&activity[0]["start_seconds"], 0.37);
        near(&activity[0]["end_seconds"], 0.98);
        near(&activity[1]["start_seconds"], 1.23);
        near(&activity[1]["end_seconds"], 1.70);
        near(&result["candidate_first_activity_seconds"], 0.37);
        near(&result["candidate_last_activity_seconds"], 1.70);
        assert_eq!(result["verified_speech_boundaries"], false);
        assert!(result["limitations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap().contains("not speech")));
    }
}
