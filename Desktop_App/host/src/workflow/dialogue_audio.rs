//! Cut-owned standalone speech. Audio delivery is terminal, never video admission.

/// Record terminal failure without deleting a delivered/partial file or admitting replacement work.
pub fn fail(root: &Path, action: &DirectorAction, error: &str) -> Result<(), String> {
    let _lock = import::lock_project(root)?;
    let mut state = render_jobs::read_persisted(root)?;
    let raw = action
        .inputs
        .get("output_path")
        .and_then(Value::as_str)
        .ok_or("Missing speech destination")?;
    let job = state
        .jobs
        .iter_mut()
        .find(|j| j.kind == "dialogue_audio" && j.output_path.as_deref() == Some(raw))
        .ok_or("Missing speech reservation")?;
    if job.status == JobStatus::Pending {
        job.fail(None, error.to_string());
    }
    import::persist_state_locked(root, &state).map(|_| ())
}

use super::{
    identity, import, render_jobs,
    state::{AssetRecord, AssetRole, AssetStatus, JobRecord, JobScope, JobStatus, ProjectState},
};
use crate::{
    ctx::HostCtx,
    director_actions::{ActionKind, DirectorAction},
    project_path::{self, PathIntent},
    session,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Revalidate current dialogue, voice identity and exact reserved destination on every admission/collection.
pub fn validate(root: &Path, action: &DirectorAction) -> Result<PathBuf, String> {
    let state = render_jobs::read_persisted(root)?;
    validate_state(root, action, &state)
}

/// Validate the exact snapshot to be mutated; never reacquire the project lock.
fn validate_state(root: &Path, action: &DirectorAction, state: &ProjectState) -> Result<PathBuf, String> {
    let i = &action.inputs;
    let scene = i
        .get("scene_id")
        .and_then(Value::as_u64)
        .ok_or("Missing speech scene")?;
    let number = i
        .get("cut")
        .and_then(Value::as_u64)
        .ok_or("Missing speech cut")?;
    let cut = state
        .scene(u32::try_from(scene).map_err(|_| "Invalid speech scene")?)
        .and_then(|s| s.cuts.iter().find(|c| u64::from(c.id.cut) == number))
        .ok_or("Speech cut no longer exists")?;
    if cut.stable_id.is_empty()
        || i.get("shot_id").and_then(Value::as_str) != Some(cut.stable_id.as_str())
        || cut.script_dialogue_review.is_some()
        || i.get("dialogue") != Some(&json!(cut.dialogue))
    {
        return Err(
            "Current cut dialogue or identity changed; prepare fresh audio approval".into(),
        );
    }
    let first = cut.dialogue.first().ok_or("Cut has no dialogue")?;
    if first.speaker.trim().is_empty()
        || cut
            .dialogue
            .iter()
            .any(|l| l.text.trim().is_empty() || l.speaker != first.speaker)
    {
        return Err("One speech performance requires complete dialogue from one speaker; split multiple speakers into separate cuts".into());
    }
    let voice = state
        .asset(
            i.get("voice_asset_id")
                .and_then(Value::as_str)
                .ok_or("Choose a voice")?,
        )
        .ok_or("Voice no longer registered")?;
    let key = match voice.role {
        AssetRole::VoiceSample => "voice_clone",
        AssetRole::VoiceProfile => "voice_profile",
        _ => return Err("Choose a saved voice sample or profile".into()),
    };
    if i.get(key).and_then(Value::as_str) != Some(voice.path.as_str()) {
        return Err("Chosen voice changed".into());
    }
    project_path::resolve(&voice.path, root.to_str(), PathIntent::Read)
        .map_err(|e| e.to_string())?;
    let raw = i
        .get("output_path")
        .and_then(Value::as_str)
        .ok_or("Missing speech destination")?;
    let prefix = format!("Scenes/Scene{scene}/Cut{number}/dialogue_");
    if !raw.starts_with(&prefix) || raw[prefix.len()..].contains('/') || !raw.ends_with(".wav") {
        return Err("Speech destination must belong to the exact cut".into());
    }
    let jobs: Vec<_> = state
        .jobs
        .iter()
        .filter(|j| j.kind == "dialogue_audio" && j.output_path.as_deref() == Some(raw))
        .collect();
    if jobs.len() != 1
        || jobs[0].status != JobStatus::Pending
        || !jobs[0]
            .scope
            .as_ref()
            .is_some_and(|s| s.kind == "shot" && s.id == cut.stable_id)
        || jobs[0].input_asset_ids != vec![voice.id.clone()]
    {
        return Err("Speech reservation is no longer active or uniquely owned".into());
    }
    let destination = project_path::resolve(raw, root.to_str(), PathIntent::Write)
        .map_err(|e| e.to_string())?
        .into_path_buf();
    if destination != root.join(raw) {
        return Err("Speech destination redirected outside its exact cut bucket".into());
    }
    Ok(destination)
}
pub fn prepare(app: &HostCtx, args: &Value) -> Result<Value, String> {
    let root = session::command_root(args["projectRoot"].as_str())?;
    let epoch = crate::pending::generation();
    let _lock = import::lock_project(&root)?;
    let mut state = render_jobs::read_persisted(&root)?;
    if state.jobs.iter().any(|j| {
        j.kind.starts_with("atlas_generate_")
            && matches!(j.status, JobStatus::Pending | JobStatus::Running)
    }) {
        return Err("Collect or cancel unsettled media before preparing speech".into());
    }
    let scene = args["sceneId"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or("Invalid sceneId")?;
    let number = args["cut"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or("Invalid cut")?;
    let cut = state
        .scene(scene)
        .and_then(|s| s.cuts.iter().find(|c| c.id.cut == number))
        .ok_or("Speech cut not found")?;
    if cut.stable_id.is_empty()
        || cut.script_dialogue_review.is_some()
        || cut.dialogue.is_empty()
        || cut.dialogue.iter().any(|line| {
            line.text.trim().is_empty()
                || line.speaker.trim().is_empty()
                || line.speaker != cut.dialogue[0].speaker
        })
    {
        return Err(
            "Reconcile complete single-speaker cut dialogue before generating audio".into(),
        );
    }
    let voice = state
        .asset(
            args["voiceAssetId"]
                .as_str()
                .ok_or("Choose a saved voice")?,
        )
        .ok_or("Voice not found")?;
    let key = match voice.role {
        AssetRole::VoiceSample => "voice_clone",
        AssetRole::VoiceProfile => "voice_profile",
        _ => return Err("Choose a voice sample or profile".into()),
    };
    let output = (1u32..)
        .map(|n| format!("Scenes/Scene{scene}/Cut{number}/dialogue_{n}.wav"))
        .find(|p| {
            !root.join(p).exists() && !state.jobs.iter().any(|j| j.output_path.as_ref() == Some(p))
        })
        .ok_or("No free audio version")?;
    let mut inputs = serde_json::Map::new();
    for (k, v) in [
        ("scene_id", json!(scene)),
        ("cut", json!(number)),
        ("shot_id", json!(cut.stable_id)),
        ("dialogue", json!(cut.dialogue)),
        ("voice_asset_id", json!(voice.id)),
        (key, json!(voice.path)),
        ("output_path", json!(output)),
    ] {
        inputs.insert(k.into(), v);
    }
    let stable = cut.stable_id.clone();
    let voice_id = voice.id.clone();
    let action = DirectorAction {
        action: ActionKind::RunSpecialist,
        specialist: Some("dialogue_audio".into()),
        inputs,
        message: None,
        op: None,
        path: None,
    };
    let context = crate::director::current_workflow_context(app, &root)?;
    let raw = crate::director::prepare_workflow_approval(app, &context, &action)?;
    for job in &mut state.jobs {
        if job.kind == "dialogue_audio" && job.status == JobStatus::Pending {
            job.status = JobStatus::Cancelled;
        }
    }
    let mut job = JobRecord::open(state.next_job_id(), "dialogue_audio");
    job.output_path = Some(output);
    job.scope = Some(JobScope {
        kind: "shot".into(),
        id: stable,
    });
    job.input_asset_ids = vec![voice_id];
    state.jobs.push(job);
    import::persist_state_locked(&root, &state)?;
    validate(&root, &action)?;
    let raw=format!("Dialogue audio only · Scene {scene} Cut {number}\nChosen voice: `{}`\nComplete current dialogue: {}\nOne speech request, zero video requests. Provider charges and disclosure apply. Saved as Draft; audition and explicitly select it before separately preparing video.\n\n{raw}", action.inputs["voice_asset_id"], action.inputs["dialogue"]);
    let card = crate::pending::publish_guarded(&raw, epoch, None, || {
        !app.director_cancelled() && session::root().as_deref() == Some(root.as_path())
    })?;
    let state = render_jobs::read_persisted(&root)?;
    Ok(
        json!({"state":state,"report":super::report::project_report(&state),"note":"Speech-only approval prepared. No media submitted; no video will follow automatically.","pendingAction":card}),
    )
}
const STANDARD_SAMPLE_RATES: &[u32] = &[
    8000, 11025, 16000, 22050, 24000, 32000, 44100, 48000, 88200, 96000, 192000,
];

/// Chromium's <audio> element reliably decodes only common sample rates. Some
/// TTS providers return WAVs at unusual rates (e.g. Seed Audio delivers 40 kHz)
/// that play in VLC but silently fail in the in-app player. Resample those to
/// 44.1 kHz before registration; standard-rate files are left untouched.
fn normalize_sample_rate(output: &Path) -> Result<(), String> {
    use std::io::Read;
    let mut header = [0u8; 28];
    let mut file = std::fs::File::open(output).map_err(|e| e.to_string())?;
    file.read_exact(&mut header).map_err(|e| e.to_string())?;
    if &header[..4] != b"RIFF" || &header[8..12] != b"WAVE" || &header[12..16] != b"fmt " {
        return Ok(()); // not a recognized WAV; the caller validates the container separately
    }
    let sample_rate = u32::from_le_bytes([header[24], header[25], header[26], header[27]]);
    if STANDARD_SAMPLE_RATES.contains(&sample_rate) {
        return Ok(());
    }
    let input_arg = crate::ffmpeg_path::ffmpeg_local_arg(output)?;
    let stem = output
        .file_stem()
        .ok_or("Invalid speech path")?
        .to_string_lossy();
    let temp = output.with_file_name(format!("{stem}.resample.wav"));
    let out_arg = crate::ffmpeg_path::ffmpeg_local_arg(&temp)?;
    let status = std::process::Command::new("ffmpeg")
        .arg("-y")
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(&input_arg)
        .arg("-ar")
        .arg("44100")
        .arg("-c:a")
        .arg("pcm_s16le")
        .arg(&out_arg)
        .output()
        .map_err(|e| format!("Could not normalize sample rate with ffmpeg (is it on PATH?): {e}"))?;
    if !status.status.success() {
        return Err(format!(
            "ffmpeg failed normalizing speech sample rate ({} Hz) for {}: {}",
            sample_rate,
            output.display(),
            String::from_utf8_lossy(&status.stderr)
        ));
    }
    std::fs::rename(&temp, output)
        .map_err(|e| format!("Could not replace normalized speech: {e}"))?;
    Ok(())
}

pub fn settle(root: &Path, action: &DirectorAction, output: &Path) -> Result<(), String> {
    settle_guarded(root, action, output, || Ok(()))
}

/// Production callers retain their originating admission through final registration.
pub(crate) fn settle_guarded(
    root: &Path,
    action: &DirectorAction,
    output: &Path,
    admission: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if validate(root, action)? != output {
        return Err("Speech delivery destination changed".into());
    }
    let findings = super::qc::check_audio_decode(output);
    if findings
        .iter()
        .any(|f| f.severity == super::qc::QCSeverity::Error)
    {
        return Err(
            "Delivered speech could not be decoded; saved file retained for inspection".into(),
        );
    }
    let mut header = [0u8; 12];
    use std::io::Read;
    std::fs::File::open(output)
        .and_then(|mut f| f.read_exact(&mut header))
        .map_err(|e| e.to_string())?;
    if &header[..4] != b"RIFF" || &header[8..] != b"WAVE" {
        return Err(
            "Provider returned non-WAV speech; saved file retained, not registered as WAV".into(),
        );
    }
    normalize_sample_rate(output)?;
    let _lock = import::lock_project(root)?;
    let mut state = render_jobs::read_persisted(root)?;
    let relative = project_path::relative_to_root(root, output)?
        .to_string_lossy()
        .replace('\\', "/");
    let index = state
        .jobs
        .iter()
        .position(|j| j.kind == "dialogue_audio" && j.output_path.as_deref() == Some(&relative))
        .ok_or("Missing speech reservation")?;
    let job_id = state.jobs[index].id.clone();
    let scene = action.inputs["scene_id"].as_u64().ok_or("Missing scene")? as u32;
    let cut = action.inputs["cut"].as_u64().ok_or("Missing cut")? as u32;
    if state.assets.iter().any(|a| a.path == relative) {
        return Err(
            "Speech destination already registered; refusing duplicate registration".into(),
        );
    }
    if validate_state(root, action, &state)? != output {
        return Err("Speech delivery destination changed".into());
    }
    admission()?;
    let id=state.push_asset(AssetRecord{id:String::new(),path:relative,role:AssetRole::SourceAudio,status:AssetStatus::Draft,scene_id:Some(scene),cut_id:Some(crate::scene_plan::handoff::CutId::new(scene,cut)),character_id:None,source_job_id:Some(job_id),parent_asset_id:None,notes:Some(json!({"operation":"dialogue_audio","dialogue":action.inputs["dialogue"],"voice_asset_id":action.inputs["voice_asset_id"],"speech_completeness_verified":false}).to_string())});
    identity::record_version(&mut state, &id, AssetStatus::Draft);
    state.jobs[index].finish(None, None);
    import::persist_state_locked(root, &state).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_pcm_wav(path: &Path, sample_rate: u32, frames: usize) {
        // Minimal 16-bit stereo PCM WAV (silence).
        let data_len = (frames * 4) as u32;
        let mut bytes = Vec::with_capacity(44 + frames * 4);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"fmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&2u16.to_le_bytes()); // stereo
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&(sample_rate * 4).to_le_bytes()); // byte rate
        bytes.extend_from_slice(&4u16.to_le_bytes()); // block align
        bytes.extend_from_slice(&16u16.to_le_bytes()); // bits
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        bytes.extend(std::iter::repeat(0u8).take(frames * 4));
        std::fs::write(path, &bytes).unwrap();
    }

    fn sample_rate_of(path: &Path) -> u32 {
        let bytes = std::fs::read(path).unwrap();
        u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]])
    }

    #[test]
    fn nonstandard_rate_is_resampled_to_44100() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("speech.wav");
        write_pcm_wav(&wav, 40000, 1600);
        normalize_sample_rate(&wav).unwrap();
        assert_eq!(sample_rate_of(&wav), 44100);
    }

    #[test]
    fn standard_rate_is_left_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("speech.wav");
        write_pcm_wav(&wav, 44100, 1600);
        normalize_sample_rate(&wav).unwrap();
        assert_eq!(sample_rate_of(&wav), 44100);
    }
}
