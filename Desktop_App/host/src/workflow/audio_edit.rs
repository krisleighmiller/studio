//! Non-destructive, local-only WAV derivative construction.
//!
//! This native seam deliberately does not select audio, issue consent, submit
//! voice/video work, or infer that acoustic activity proves complete speech.
use super::{identity, import, state::{AssetRecord, AssetRole, AssetStatus, ProjectState, STATE_REL}};
use crate::{media_inspect::run_bounded, project_path::{self, PathIntent}, session};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs::{self, OpenOptions}, io::Write, path::{Path, PathBuf}, process::Command, time::{Duration, Instant}};

/// Finalize only a newly encoded, finite PCM WAV from our local pipe muxer.
/// Pipe muxers cannot seek back to replace RIFF/data length sentinels.
pub(crate) fn finalize_pcm_wav(wav: &mut [u8]) -> Result<(), String> {
    if wav.len() < 44 || !wav.starts_with(b"RIFF") || &wav[8..12] != b"WAVE" {
        return Err("FFmpeg produced no playable WAV".into());
    }
    let riff_len = u32::try_from(wav.len() - 8).map_err(|_| "WAV too large")?;
    wav[4..8].copy_from_slice(&riff_len.to_le_bytes());
    let mut offset = 12usize;
    while offset + 8 <= wav.len() {
        if &wav[offset..offset + 4] == b"data" {
            let length = u32::try_from(wav.len() - offset - 8).map_err(|_| "WAV data too large")?;
            wav[offset + 4..offset + 8].copy_from_slice(&length.to_le_bytes());
            return Ok(());
        }
        let length = u32::from_le_bytes(wav[offset + 4..offset + 8].try_into().unwrap()) as usize;
        offset = offset.checked_add(8 + length + (length % 2)).ok_or("Invalid WAV chunks")?;
    }
    Err("FFmpeg WAV has no PCM data chunk".into())
}

fn resolve(root: &Path, raw: &str, intent: PathIntent) -> Result<PathBuf, String> {
    project_path::resolve(raw, Some(&root.to_string_lossy()), intent)
        .map(|p| p.into_path_buf()).map_err(|e| e.to_string())
}

/// Create a draft PCM WAV at a fresh sibling version path, preserving pitch.
/// Requires the exact open root and a local project WAV (no external grants).
/// Limits: speed 0.5–2, input <=64MiB/300s, output <=128MiB, 60s process budget.
/// Returns the persisted native asset with parent lineage; UI/selection is separate.
pub fn version_speed(root: &Path, source: &str, speed: f64) -> Result<AssetRecord, String> {
    version_local(root, source, Edit::Speed(speed))
}

/// Independent pitch multiplier 0.5–2.0 (one octave down/up), approximately
/// retaining duration. Uses ordinary FFmpeg filters, not optional rubberband.
/// Same local-only input/process/output bounds and draft lineage as speed edits.
/// This resampling technique also changes timbre; audition before selecting.
pub fn version_pitch(root: &Path, source: &str, pitch: f64) -> Result<AssetRecord, String> {
    version_local(root, source, Edit::Pitch(pitch))
}

/// Keep the half-open source window [start, end) in seconds. The persisted
/// recipe also records the actual sample indices used by FFmpeg.
pub fn version_trim(root: &Path, source: &str, start: f64, end: f64) -> Result<AssetRecord, String> {
    version_local(root, source, Edit::Trim { start, end })
}

#[derive(Clone, Copy)]
enum Edit { Speed(f64), Pitch(f64), Trim { start: f64, end: f64 } }

fn version_local(root: &Path, source: &str, edit: Edit) -> Result<AssetRecord, String> {
    let operation = match edit { Edit::Speed(_) => "speed", Edit::Pitch(_) => "pitch", Edit::Trim { .. } => "trim" };
    match edit {
        Edit::Speed(factor) | Edit::Pitch(factor) if !factor.is_finite() || !(0.5..=2.0).contains(&factor) =>
            return Err(format!("Audio {operation} must be a finite number between 0.5 and 2.0")),
        Edit::Trim { start, end } if !start.is_finite() || !end.is_finite() || start < 0.0 || end <= start || end > 300.0 =>
            return Err("Trim requires a finite start >= 0 and end > start, at most 300 seconds".into()),
        _ => {}
    }
    let root = session::command_root(Some(&root.to_string_lossy()))?;
    let input = resolve(&root, source, PathIntent::Read)?;
    if !input.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("wav")) {
        return Err(format!("Local {operation} editing requires a WAV"));
    }
    let meta = fs::metadata(&input).map_err(|e| format!("Cannot read WAV: {e}"))?;
    if !meta.is_file() || meta.len() > 64 * 1024 * 1024 {
        return Err("WAV must be a regular file of at most 64MiB".into());
    }
    let state_path = resolve(&root, STATE_REL, PathIntent::Read)?;
    // No load_or_import repairs on rejected inputs or metadata-only observation.
    let mut state = ProjectState::from_json(&fs::read_to_string(state_path).map_err(|e| e.to_string())?)?;
    let relative = project_path::relative_to_root(&root, &input)?.to_string_lossy().replace('\\', "/");
    let existing = state.assets.iter().find(|a| a.path == relative).cloned();
    if existing.as_ref().is_some_and(|a| !a.role.is_audio()) {
        return Err("Source is registered as a non-audio asset".into());
    }
    let before = fs::read(&input).map_err(|e| e.to_string())?;
    let source_hash = Sha256::digest(&before).iter().map(|b| format!("{b:02x}")).collect::<String>();
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut probe = Command::new("ffprobe");
    probe.args(["-v", "error", "-protocol_whitelist", "file", "-format_whitelist", "wav", "-f", "wav", "-show_entries", "format=duration:stream=codec_type,sample_rate,channels", "-of", "json"]).arg(&input);
    let (raw, _) = run_bounded(&mut probe, deadline, 64 * 1024)?;
    let probe: Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    let duration = probe["format"]["duration"].as_str().and_then(|s| s.parse::<f64>().ok())
        .filter(|d| d.is_finite() && *d > 0.0 && *d <= 300.0)
        .ok_or("WAV duration must be positive and at most 300 seconds")?;
    if !probe["streams"].as_array().is_some_and(|streams| streams.len() == 1 && streams[0]["codec_type"] == "audio") {
        return Err("WAV must contain one audio stream".into());
    }
    let mut window = None;
    let filter = if let Edit::Pitch(factor) = edit {
        let rate = probe["streams"][0]["sample_rate"].as_str()
            .and_then(|s| s.parse::<u32>().ok()).filter(|r| (8000..=192000).contains(r))
            .ok_or("Pitch editing requires a sample rate between 8000 and 192000 Hz")?;
        format!("asetrate={rate}*{factor},aresample={rate},atempo={}", 1.0 / factor)
    } else if let Edit::Speed(factor) = edit { format!("atempo={factor}") }
    else if let Edit::Trim { start, end } = edit {
        let rate = probe["streams"][0]["sample_rate"].as_str()
            .and_then(|s| s.parse::<u32>().ok()).filter(|r| (8000..=192000).contains(r))
            .ok_or("Trim requires a sample rate between 8000 and 192000 Hz")?;
        // Explicit nearest-sample rounding avoids timestamp-based imprecision.
        let first = (start * f64::from(rate)).round() as u64;
        let last = (end * f64::from(rate)).round() as u64;
        if first >= last || end > duration + 0.000001 || last > (duration * f64::from(rate)).round() as u64 {
            return Err("Trim window must contain samples inside the source WAV".into());
        }
        window = Some((first, last, rate));
        format!("atrim=start_sample={first}:end_sample={last},asetpts=PTS-STARTPTS")
    } else { unreachable!() };
    let mut ffmpeg = Command::new("ffmpeg");
    ffmpeg.args(["-nostdin", "-v", "error", "-xerror", "-err_detect", "explode", "-protocol_whitelist", "file", "-format_whitelist", "wav", "-f", "wav", "-i"])
        .arg(&input).args(["-map", "0:a:0", "-vn", "-map_metadata", "-1", "-af", &filter, "-c:a", "pcm_s16le", "-f", "wav", "pipe:1"]);
    let (mut wav, _) = run_bounded(&mut ffmpeg, deadline, 128 * 1024 * 1024)?;
    finalize_pcm_wav(&mut wav)?;
    if let Some((first, last, rate)) = window {
        let expected = (last - first) as f64 / f64::from(rate);
        // The encoded output is our own bounded in-memory WAV. Check its exact
        // PCM payload length instead of accepting a silently shortened edit.
        let mut offset = 12usize;
        let bytes = loop {
            if offset + 8 > wav.len() { return Err("Trim output has no data chunk".into()); }
            let size = u32::from_le_bytes(wav[offset+4..offset+8].try_into().unwrap()) as usize;
            if &wav[offset..offset+4] == b"data" { break size; }
            offset = offset.checked_add(8 + size + (size % 2)).ok_or("Invalid WAV chunks")?;
        };
        let channels = probe["streams"][0]["channels"].as_u64().filter(|c| *c > 0 && *c <= 32).ok_or("Invalid WAV channels")?;
        if bytes as u64 != (last - first) * channels * 2 {
            return Err(format!("Trim output did not contain the requested {expected:.6}s source window"));
        }

    }
    if fs::read(&input).map_err(|e| e.to_string())? != before {
        return Err("Source WAV changed during editing; reload before retrying".into());
    }
    let parent = input.parent().ok_or("WAV has no parent directory")?;
    let stem = input.file_stem().and_then(|s| s.to_str()).ok_or("WAV name must be UTF-8")?;
    let mut created = None;
    for number in 2..=999 {
        let candidate = parent.join(format!("{stem}_{operation}_v{number}.wav"));
        // symlink_metadata also reserves dangling links; never follow/overwrite them.
        if fs::symlink_metadata(&candidate).is_ok() { continue; }
        let output = resolve(&root, &candidate.to_string_lossy(), PathIntent::Write)?;
        let output_rel = project_path::relative_to_root(&root, &output)?.to_string_lossy().replace('\\', "/");
        if state.assets.iter().any(|a| a.path == output_rel) || state.versions.iter().any(|v| v.path == output_rel)
            || state.jobs.iter().any(|j| j.output_path.as_deref() == Some(output_rel.as_str())) { continue; }
        match OpenOptions::new().write(true).create_new(true).open(&output) {
            Ok(mut file) => {
                file.write_all(&wav).and_then(|_|file.sync_all()).map_err(|e| format!("Cannot save derivative: {e}"))?;
                created = Some(output_rel); break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Cannot reserve audio version: {e}")),
        }
    }
    let path = created.ok_or("No free local WAV version path")?;
    let original = existing.unwrap_or_else(|| {
        let id = state.push_asset(AssetRecord { id: String::new(), path: relative.clone(), role: AssetRole::SourceAudio,
            status: AssetStatus::Draft, scene_id: None, cut_id: None, character_id: None, source_job_id: None, parent_asset_id: None, notes: None });
        state.asset(&id).unwrap().clone()
    });
    identity::record_version(&mut state, &original.id, original.status);
    let mut edited = original.clone();
    edited.id = state.next_asset_id(); edited.path = path.clone(); edited.status = AssetStatus::Draft;
    edited.parent_asset_id = Some(original.id); edited.source_job_id = None;
    let mut notes = json!({"operation":format!("local_wav_{operation}"),"filter":filter,"source_path":relative,"source_sha256":source_hash,"output_sha256":Sha256::digest(&wav).iter().map(|b| format!("{b:02x}")).collect::<String>(),"source_duration_seconds":duration,"speech_completeness_verified":false});
    match edit {
        Edit::Speed(factor) | Edit::Pitch(factor) => notes[operation] = json!(factor),
        Edit::Trim { start, end } => {
            let (first, last, rate) = window.expect("validated trim window");
            notes["source_window"] = json!({"start_seconds":start,"end_seconds":end,"start_sample":first,"end_sample_exclusive":last,"sample_rate":rate});
        }
    }
    edited.notes = Some(notes.to_string());
    state.push_asset(edited.clone()); identity::record_version(&mut state, &edited.id, AssetStatus::Draft);
    // Optimistic persistence preserves jobs/permissions and refuses stale writers.
    // If saving fails, keep only our new WAV for rediscovery; never remove history.
    import::persist_state(&root, &state).map_err(|e| format!("New WAV retained at {path}, but registration failed: {e}"))?;
    Ok(edited)
}
