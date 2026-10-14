//! Character voice tab: local recording + profile save helpers.

use crate::ctx::HostCtx;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

use crate::elevenlabs;

struct RecordingProcess {
    child: Child,
    output_path: String,
    raw_path: String,
    spectrum: Arc<Mutex<Vec<f32>>>,
    reader: Option<JoinHandle<std::io::Result<()>>>,
}

pub struct RecorderState(Mutex<Option<RecordingProcess>>);

impl RecorderState {
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSource {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedVoiceProfile {
    pub audio_path: String,
    pub metadata_path: String,
}

fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn safe_character_name(name: &str) -> String {
    let normalized = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else if character.is_whitespace() {
                '_'
            } else {
                '-'
            }
        })
        .collect::<String>();
    if normalized.is_empty() {
        "Character".to_string()
    } else {
        normalized
    }
}

fn now_stamp() -> String {
    format!("{:?}", std::time::SystemTime::now())
}

fn append_character_audio_profile(reference_path: &Path, profile_yaml: &str) -> Result<(), String> {
    let mut reference = std::fs::read_to_string(reference_path)
        .map_err(|error| format!("Could not read character reference sheet: {error}"))?;
    if reference.contains(profile_yaml.trim()) {
        return Ok(());
    }
    let addition = format!("\n\n```yaml\n{profile_yaml}```\n");
    if let Some(section_start) = reference.find("# Audio Profile") {
        let section_end = reference[section_start..]
            .find("\n# ")
            .map(|offset| section_start + offset)
            .unwrap_or(reference.len());
        reference.insert_str(section_end, &addition);
    } else {
        reference.push_str("\n\n# Audio Profile");
        reference.push_str(&addition);
    }
    std::fs::write(reference_path, reference)
        .map_err(|error| format!("Could not update character reference sheet: {error}"))
}

fn write_wav_from_pcm(raw_path: &Path, wav_path: &Path) -> Result<(), String> {
    let pcm = std::fs::read(raw_path)
        .map_err(|error| format!("Could not read captured audio: {error}"))?;
    if pcm.is_empty() {
        return Err("The microphone returned no audio data.".into());
    }
    let data_length = pcm.len() as u32;
    let riff_length = 36u32.saturating_add(data_length);
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&riff_length.to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&44100u32.to_le_bytes());
    wav.extend_from_slice(&(44100u32 * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_length.to_le_bytes());
    wav.extend_from_slice(&pcm);
    std::fs::write(wav_path, wav)
        .map_err(|error| format!("Could not finalize the WAV file: {error}"))
}

fn voice_dir(character_directory: &Path) -> Result<PathBuf, String> {
    if !character_directory.is_dir() {
        return Err("Choose an existing character directory before saving.".into());
    }
    let voice_directory = character_directory.join("voice");
    std::fs::create_dir_all(&voice_directory)
        .map_err(|error| format!("Could not create the voice directory: {error}"))?;
    Ok(voice_directory)
}

fn next_audio_path(voice_directory: &Path, stem: &str, extension: &str) -> PathBuf {
    let mut index = 1;
    loop {
        let candidate = voice_directory.join(format!("{stem}_voice_sample_{index:02}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
        index += 1;
    }
}

fn write_profile_files(
    character_directory: &Path,
    character_name: &str,
    character_reference_path: Option<&str>,
    profile: &str,
    audio_path: &Path,
) -> Result<SavedVoiceProfile, String> {
    let stem = safe_character_name(character_name);
    let profile_path = character_directory
        .join("voice")
        .join(format!("{stem}_voice_profile.yaml"));
    // The sheet is optional, and its path must be scoped to the open project (or a
    // one-time grant). A guessed, stale, or out-of-project path must not block the
    // voice profile and must never append to a file the host has not authorized.
    let reference_path = character_reference_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|raw| {
            crate::session::authorize_user_path(raw, crate::project_path::PathIntent::Write)
                .ok()
                .filter(|path| path.is_file())
        });
    let metadata_path = if let Some(reference_path) = reference_path {
        append_character_audio_profile(&reference_path, profile)?;
        // Also keep a machine-readable sidecar for pipeline tools.
        let _ = std::fs::write(&profile_path, profile);
        format!("{}# Audio Profile", reference_path.to_string_lossy())
    } else {
        std::fs::write(&profile_path, profile)
            .map_err(|error| format!("Could not write voice profile metadata: {error}"))?;
        profile_path.to_string_lossy().into_owned()
    };
    Ok(SavedVoiceProfile {
        audio_path: audio_path.to_string_lossy().into_owned(),
        metadata_path,
    })
}

pub fn voice_list_recording_sources() -> Result<Vec<RecordingSource>, String> {
    let output = Command::new("wpctl")
        .args(["status"])
        .output()
        .map_err(|error| format!("Could not list PipeWire audio sources: {error}"))?;
    if !output.status.success() {
        return Err("PipeWire could not list audio sources.".into());
    }
    let status = String::from_utf8_lossy(&output.stdout);
    let source_line_pattern =
        Regex::new(r"(?:\*\s*)?(\d+)\.\s+(.+)$").map_err(|error| error.to_string())?;
    let mut in_sources = false;
    let mut sources = Vec::new();
    for line in status.lines() {
        if line.contains("Sources:") {
            in_sources = true;
            continue;
        }
        if in_sources && line.contains("Sinks:") {
            break;
        }
        if !in_sources {
            continue;
        }
        let Some(captures) = source_line_pattern.captures(line) else {
            continue;
        };
        let Ok(id) = captures[1].parse::<u32>() else {
            continue;
        };
        let name = captures[2].trim();
        if name.to_lowercase().contains("[v4l2]") || name.is_empty() {
            continue;
        }
        sources.push(RecordingSource {
            id,
            name: name.to_string(),
        });
    }
    Ok(sources)
}

pub fn voice_start_recording(
    state: &RecorderState,
    output_path: String,
    source_id: Option<u32>,
) -> Result<String, String> {
    let mut active_process = state
        .0
        .lock()
        .map_err(|_| "Recorder state is unavailable.".to_string())?;
    if active_process.is_some() {
        return Err("A recording is already in progress.".into());
    }

    let path =
        crate::session::authorize_user_path(&output_path, crate::project_path::PathIntent::Write)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create recording directory: {error}"))?;
    }

    let raw_path = PathBuf::from(format!("{}.raw", path.display()));
    let _ = std::fs::remove_file(&raw_path);
    let mut pw_record = Command::new("pw-record");
    pw_record.args(["--format", "s16", "--rate", "44100", "--channels", "1"]);
    if let Some(source_id) = source_id {
        pw_record.args(["--target", &source_id.to_string()]);
    }
    let mut child = pw_record
        .arg("-")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .or_else(|_| {
            Command::new("arecord")
                .args(["-f", "S16_LE", "-r", "44100", "-c", "1", "-"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        })
        .map_err(|error| format!("Could not start a Linux audio recorder: {error}"))?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "The recorder did not provide an audio stream.".to_string())?;
    let spectrum = Arc::new(Mutex::new(vec![0.0; 48]));
    let reader_spectrum = Arc::clone(&spectrum);
    let reader_path = raw_path.clone();
    let reader = std::thread::spawn(move || {
        let mut raw_file = std::fs::File::create(reader_path)?;
        let mut buffer = [0u8; 8192];
        loop {
            let bytes_read = stdout.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            raw_file.write_all(&buffer[..bytes_read])?;
            let samples = buffer[..bytes_read]
                .chunks_exact(2)
                .map(|sample| i16::from_le_bytes([sample[0], sample[1]]) as f32 / 32768.0)
                .collect::<Vec<_>>();
            if samples.is_empty() {
                continue;
            }
            let mut next_spectrum = Vec::with_capacity(48);
            for bin in 0..48 {
                let frequency = 80.0 + bin as f32 * (8000.0 - 80.0) / 47.0;
                let coefficient = 2.0 * (2.0 * std::f32::consts::PI * frequency / 44100.0).cos();
                let mut previous = 0.0;
                let mut previous_previous = 0.0;
                for sample in &samples {
                    let current = *sample + coefficient * previous - previous_previous;
                    previous_previous = previous;
                    previous = current;
                }
                let magnitude = (previous * previous + previous_previous * previous_previous
                    - coefficient * previous * previous_previous)
                    .max(0.0)
                    .sqrt()
                    / samples.len() as f32;
                next_spectrum.push((magnitude * 4.0).min(1.0));
            }
            if let Ok(mut current_spectrum) = reader_spectrum.lock() {
                *current_spectrum = next_spectrum;
            }
        }
        Ok(())
    });

    *active_process = Some(RecordingProcess {
        child,
        output_path: path.to_string_lossy().into_owned(),
        raw_path: raw_path.to_string_lossy().into_owned(),
        spectrum,
        reader: Some(reader),
    });
    Ok(path.to_string_lossy().into_owned())
}

pub fn voice_recording_level(state: &RecorderState) -> Result<Vec<f32>, String> {
    let mut active_process = state
        .0
        .lock()
        .map_err(|_| "Recorder state is unavailable.".to_string())?;
    if let Some(recording) = active_process.as_mut() {
        if recording
            .child
            .try_wait()
            .map_err(|error| format!("Could not inspect the recorder: {error}"))?
            .is_some()
        {
            return Err(
                "The recorder stopped unexpectedly. Check microphone permissions and PipeWire/ALSA."
                    .into(),
            );
        }
    }
    Ok(active_process
        .as_ref()
        .and_then(|recording| {
            recording
                .spectrum
                .lock()
                .ok()
                .map(|spectrum| spectrum.clone())
        })
        .unwrap_or_else(|| vec![0.0; 48]))
}

pub fn voice_stop_recording(state: &RecorderState) -> Result<String, String> {
    let mut active_process = state
        .0
        .lock()
        .map_err(|_| "Recorder state is unavailable.".to_string())?;
    let Some(mut recording) = active_process.take() else {
        return Err("No recording is currently in progress.".into());
    };

    if recording
        .child
        .try_wait()
        .map_err(|error| format!("Could not inspect the recording process: {error}"))?
        .is_none()
    {
        let signal_result = Command::new("kill")
            .args(["-INT", &recording.child.id().to_string()])
            .status();
        if signal_result.is_err()
            || !signal_result
                .map(|status| status.success())
                .unwrap_or(false)
        {
            recording
                .child
                .kill()
                .map_err(|error| format!("Could not stop the recording: {error}"))?;
        }
    }
    recording
        .child
        .wait()
        .map_err(|error| format!("Could not finalize the recording: {error}"))?;
    if let Some(reader) = recording.reader {
        reader
            .join()
            .map_err(|_| "The audio capture thread stopped unexpectedly.".to_string())?
            .map_err(|error| format!("Could not store captured audio: {error}"))?;
    }
    write_wav_from_pcm(
        &PathBuf::from(&recording.raw_path),
        &PathBuf::from(&recording.output_path),
    )?;
    let _ = std::fs::remove_file(&recording.raw_path);
    Ok(recording.output_path)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveRecordingRequest {
    pub temp_path: String,
    pub character_directory: String,
    pub character_name: String,
    pub sample_text: String,
    pub character_reference_path: Option<String>,
}

const AUDIO_EXTENSIONS: &[&str] = &["wav", "mp3", "m4a", "ogg", "flac", "aac"];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachExistingVoiceRequest {
    pub audio_path: String,
    pub character_directory: String,
    pub character_name: String,
    pub sample_text: String,
    pub character_reference_path: Option<String>,
}

fn inside_directory(file: &Path, directory: &Path) -> bool {
    let file = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    let directory = directory
        .canonicalize()
        .unwrap_or_else(|_| directory.to_path_buf());
    file.starts_with(directory)
}

/// Register an audio file that is already on disk as this character's voice.
/// A file already inside the character voice folder stays where it is.
pub fn voice_attach_existing(
    request: AttachExistingVoiceRequest,
) -> Result<SavedVoiceProfile, String> {
    let character_directory = crate::session::authorize_user_path(
        request.character_directory.trim(),
        crate::project_path::PathIntent::Write,
    )?;
    let source = crate::session::authorize_user_path(
        request.audio_path.trim(),
        crate::project_path::PathIntent::Read,
    )?;
    if !source.is_file() {
        return Err("That audio file is missing or is no longer available.".into());
    }
    let extension = source
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !AUDIO_EXTENSIONS.contains(&extension.as_str()) {
        return Err("Choose a WAV, MP3, M4A, OGG, FLAC, or AAC file.".into());
    }
    let voice_directory = voice_dir(&character_directory)?;
    let audio_path = if inside_directory(&source, &voice_directory) {
        source
    } else {
        let stem = safe_character_name(&request.character_name);
        let destination = next_audio_path(&voice_directory, &stem, &extension);
        std::fs::copy(&source, &destination).map_err(|error| {
            format!("Could not copy the audio file into the voice folder: {error}")
        })?;
        destination
    };
    let relative_audio_path = audio_path
        .strip_prefix(&character_directory)
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| {
            format!(
                "voice/{}",
                audio_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("voice_sample.wav")
            )
        });
    let stamp = now_stamp();
    let profile = format!(
        "profile_version: 1\n\
character: {}\n\
source_mode: existing_file\n\
provider: null\n\
model_id: null\n\
voice_id: null\n\
voice_name: null\n\
language: null\n\
gender: null\n\
voice_description: \"Existing audio file\"\n\
settings: {{}}\n\
prompt: \"\"\n\
approved_audio:\n\
  - path: {}\n\
    purpose: canonical_sample\n\
    sample_text: {}\n\
created_at: {}\n\
updated_at: {}\n",
        yaml_string(&request.character_name),
        yaml_string(&relative_audio_path),
        yaml_string(&request.sample_text),
        yaml_string(&stamp),
        yaml_string(&stamp),
    );
    write_profile_files(
        &character_directory,
        &request.character_name,
        request.character_reference_path.as_deref(),
        &profile,
        &audio_path,
    )
}

pub fn voice_save_recording(request: SaveRecordingRequest) -> Result<SavedVoiceProfile, String> {
    let character_directory = crate::session::authorize_user_path(
        &request.character_directory,
        crate::project_path::PathIntent::Write,
    )?;
    let temp_path = crate::session::authorize_user_path(
        &request.temp_path,
        crate::project_path::PathIntent::Read,
    )?;
    if !temp_path.is_file() {
        return Err("The recording file is missing or is no longer available.".into());
    }
    let voice_directory = voice_dir(&character_directory)?;
    let stem = safe_character_name(&request.character_name);
    let audio_path = next_audio_path(&voice_directory, &stem, "wav");
    std::fs::copy(&temp_path, &audio_path)
        .map_err(|error| format!("Could not save the approved recording: {error}"))?;
    let _ = std::fs::remove_file(&temp_path);
    let relative_audio_path = format!(
        "voice/{}",
        audio_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("voice_sample.wav")
    );
    let stamp = now_stamp();
    let profile = format!(
        "profile_version: 1\n\
character: {}\n\
source_mode: recorded_voice\n\
provider: null\n\
model_id: null\n\
voice_id: null\n\
voice_name: null\n\
language: null\n\
gender: null\n\
voice_description: \"Recorded reference clip\"\n\
settings: {{}}\n\
prompt: \"\"\n\
approved_audio:\n\
  - path: {}\n\
    purpose: canonical_sample\n\
    sample_text: {}\n\
created_at: {}\n\
updated_at: {}\n",
        yaml_string(&request.character_name),
        yaml_string(&relative_audio_path),
        yaml_string(&request.sample_text),
        yaml_string(&stamp),
        yaml_string(&stamp),
    );
    write_profile_files(
        &character_directory,
        &request.character_name,
        request.character_reference_path.as_deref(),
        &profile,
        &audio_path,
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveLibraryVoiceRequest {
    pub character_directory: String,
    pub character_name: String,
    pub character_reference_path: Option<String>,
    pub sample_text: String,
    pub voice_id: String,
    pub voice_name: String,
    pub voice_description: Option<String>,
    pub gender: Option<String>,
    pub age: Option<String>,
    pub accent: Option<String>,
    pub audio_base64: String,
    pub media_type: Option<String>,
}

pub fn voice_save_library_voice(
    request: SaveLibraryVoiceRequest,
) -> Result<SavedVoiceProfile, String> {
    let character_directory = crate::session::authorize_user_path(
        &request.character_directory,
        crate::project_path::PathIntent::Write,
    )?;
    let voice_directory = voice_dir(&character_directory)?;
    let stem = safe_character_name(&request.character_name);
    let bytes = B64
        .decode(request.audio_base64.trim())
        .map_err(|error| format!("Could not decode preview audio: {error}"))?;
    let extension = if request
        .media_type
        .as_deref()
        .unwrap_or("audio/mpeg")
        .contains("wav")
    {
        "wav"
    } else {
        "mp3"
    };
    let audio_path = next_audio_path(&voice_directory, &stem, extension);
    std::fs::write(&audio_path, bytes)
        .map_err(|error| format!("Could not save library voice audio: {error}"))?;
    let relative_audio_path = format!(
        "voice/{}",
        audio_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("voice_sample.mp3")
    );
    let stamp = now_stamp();
    let description = request
        .voice_description
        .as_deref()
        .unwrap_or("ElevenLabs library voice")
        .to_string();
    let profile = format!(
        "profile_version: 1\n\
character: {}\n\
source_mode: library_voice\n\
provider: elevenlabs\n\
model_id: eleven_multilingual_v2\n\
voice_id: {}\n\
voice_name: {}\n\
language: null\n\
gender: {}\n\
age: {}\n\
accent: {}\n\
voice_description: {}\n\
settings: {{}}\n\
prompt: \"\"\n\
approved_audio:\n\
  - path: {}\n\
    purpose: canonical_sample\n\
    sample_text: {}\n\
created_at: {}\n\
updated_at: {}\n",
        yaml_string(&request.character_name),
        yaml_string(request.voice_id.trim()),
        yaml_string(request.voice_name.trim()),
        request
            .gender
            .as_deref()
            .map(yaml_string)
            .unwrap_or_else(|| "null".into()),
        request
            .age
            .as_deref()
            .map(yaml_string)
            .unwrap_or_else(|| "null".into()),
        request
            .accent
            .as_deref()
            .map(yaml_string)
            .unwrap_or_else(|| "null".into()),
        yaml_string(&description),
        yaml_string(&relative_audio_path),
        yaml_string(&request.sample_text),
        yaml_string(&stamp),
        yaml_string(&stamp),
    );
    write_profile_files(
        &character_directory,
        &request.character_name,
        request.character_reference_path.as_deref(),
        &profile,
        &audio_path,
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDesignedVoiceRequest {
    pub character_directory: String,
    pub character_name: String,
    pub character_reference_path: Option<String>,
    pub sample_text: String,
    pub voice_description: String,
    pub generated_voice_id: String,
    pub audio_base64: String,
    pub media_type: Option<String>,
    pub design_fields: Option<serde_json::Value>,
    pub project_root: Option<String>,
    pub confirmation: Option<String>,
}

pub fn voice_save_designed_voice(
    app: HostCtx,
    request: SaveDesignedVoiceRequest,
) -> Result<SavedVoiceProfile, String> {
    let voice_id = elevenlabs::elevenlabs_create_designed_voice(
        app,
        request.character_name.clone(),
        request.voice_description.clone(),
        request.generated_voice_id.clone(),
        request
            .project_root
            .clone()
            .ok_or("Project root required for voice creation")?,
        request
            .confirmation
            .clone()
            .ok_or("One-use confirmation required for voice creation")?,
    )?;
    let character_directory = crate::session::authorize_user_path(
        &request.character_directory,
        crate::project_path::PathIntent::Write,
    )?;
    let voice_directory = voice_dir(&character_directory)?;
    let stem = safe_character_name(&request.character_name);
    let bytes = B64
        .decode(request.audio_base64.trim())
        .map_err(|error| format!("Could not decode design preview audio: {error}"))?;
    let extension = if request
        .media_type
        .as_deref()
        .unwrap_or("audio/mpeg")
        .contains("wav")
    {
        "wav"
    } else {
        "mp3"
    };
    let audio_path = next_audio_path(&voice_directory, &stem, extension);
    std::fs::write(&audio_path, bytes)
        .map_err(|error| format!("Could not save designed voice audio: {error}"))?;
    let relative_audio_path = format!(
        "voice/{}",
        audio_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("voice_sample.mp3")
    );
    let stamp = now_stamp();
    let fields = request
        .design_fields
        .map(|value| value.to_string())
        .unwrap_or_else(|| "{}".into());
    let profile = format!(
        "profile_version: 1\n\
character: {}\n\
source_mode: designed_voice\n\
provider: elevenlabs\n\
model_id: eleven_multilingual_ttv_v2\n\
voice_id: {}\n\
voice_name: {}\n\
language: null\n\
gender: null\n\
voice_description: {}\n\
design_fields: {}\n\
generated_voice_id: {}\n\
settings: {{}}\n\
prompt: {}\n\
approved_audio:\n\
  - path: {}\n\
    purpose: canonical_sample\n\
    sample_text: {}\n\
created_at: {}\n\
updated_at: {}\n",
        yaml_string(&request.character_name),
        yaml_string(&voice_id),
        yaml_string(&request.character_name),
        yaml_string(request.voice_description.trim()),
        yaml_string(&fields),
        yaml_string(request.generated_voice_id.trim()),
        yaml_string(request.voice_description.trim()),
        yaml_string(&relative_audio_path),
        yaml_string(&request.sample_text),
        yaml_string(&stamp),
        yaml_string(&stamp),
    );
    let saved = write_profile_files(
        &character_directory,
        &request.character_name,
        request.character_reference_path.as_deref(),
        &profile,
        &audio_path,
    )?;
    if let Some(root) = request
        .project_root
        .as_deref()
        .filter(|root| !root.trim().is_empty())
    {
        record_voice_clone_job(root, &voice_id)?;
    }
    Ok(saved)
}

/// Record a voice clone as a completed job with unknown actual cost.
/// This post-dispatch record does not enforce budget policy; the call site
/// must preflight before issuing a billable provider request.
fn record_voice_clone_job(project_root: &str, voice_id: &str) -> Result<(), String> {
    let mut state = crate::workflow::import::load_state(Path::new(project_root))?;
    let job_id = state.next_job_id();
    let mut job = crate::workflow::state::JobRecord::open(job_id, "voice_clone");
    job.provider = Some("elevenlabs".into());
    job.raw_provider_id = Some(voice_id.to_string());
    if let Some((currency, minor)) = crate::workflow::cost::known_price("elevenlabs", "voice_clone")
    {
        job.currency = Some(currency);
        job.estimated_cost_minor = Some(minor);
    }
    job.finish(None, None);
    state.jobs.push(job);
    crate::workflow::import::save_project(Path::new(project_root), state)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_designed_voice_clone_is_recorded_as_a_completed_ledger_job() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        crate::workflow::import::save_project(root, crate::workflow::state::ProjectState::new())
            .unwrap();

        record_voice_clone_job(root.to_string_lossy().as_ref(), "voice_abc").unwrap();

        let state = crate::workflow::import::load_state(root).unwrap();
        assert_eq!(state.jobs.len(), 1);
        let job = &state.jobs[0];
        assert_eq!(job.kind, "voice_clone");
        assert_eq!(job.provider.as_deref(), Some("elevenlabs"));
        assert_eq!(job.raw_provider_id.as_deref(), Some("voice_abc"));
        assert_eq!(job.status, crate::workflow::state::JobStatus::Completed);
        assert_eq!(job.estimated_cost_minor, Some(0));
        assert_eq!(job.cost_minor, None);
        assert_eq!(job.currency.as_deref(), Some("USD"));
    }

    #[test]
    fn attach_keeps_a_file_already_in_the_voice_folder() {
        let dir = tempfile::tempdir().unwrap();
        let character = dir.path().join("Characters/Tom Vale");
        let voice = character.join("voice");
        std::fs::create_dir_all(&voice).unwrap();
        let sample = voice.join("Tom_voice_sample_01.wav");
        std::fs::write(&sample, b"RIFF").unwrap();

        let saved = voice_attach_existing(AttachExistingVoiceRequest {
            audio_path: sample.to_string_lossy().into_owned(),
            character_directory: character.to_string_lossy().into_owned(),
            character_name: "Tom Vale".into(),
            sample_text: "Moved sample.".into(),
            character_reference_path: None,
        })
        .unwrap();

        assert_eq!(saved.audio_path, sample.to_string_lossy());
        let with_missing_sheet = voice_attach_existing(AttachExistingVoiceRequest {
            audio_path: sample.to_string_lossy().into_owned(),
            character_directory: character.to_string_lossy().into_owned(),
            character_name: "Tom Vale".into(),
            sample_text: "Moved sample.".into(),
            character_reference_path: Some(
                character.join("Tom_Vale.md").to_string_lossy().into_owned(),
            ),
        });
        assert!(with_missing_sheet.is_ok(), "{with_missing_sheet:?}");
        let names: Vec<String> = std::fs::read_dir(&voice)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(names.iter().any(|name| name == "Tom_voice_sample_01.wav"));
        assert!(names
            .iter()
            .any(|name| name.ends_with("voice_profile.yaml")));

        let tom = crate::scene_plan::assets::AssetRegistry::discover(dir.path())
            .find_character("Tom")
            .unwrap()
            .clone();
        let voice_asset = tom.voice.expect("voice profile should be discovered");
        assert!(voice_asset.voice_id.is_empty());
        assert_eq!(voice_asset.sample_path.as_deref(), Some(sample.as_path()));
    }

    #[test]
    fn attach_copies_a_file_from_outside_the_voice_folder() {
        let dir = tempfile::tempdir().unwrap();
        let character = dir.path().join("Characters/Tom Vale");
        std::fs::create_dir_all(&character).unwrap();
        let outside = dir.path().join("old/tom.mp3");
        std::fs::create_dir_all(outside.parent().unwrap()).unwrap();
        std::fs::write(&outside, b"ID3").unwrap();

        let saved = voice_attach_existing(AttachExistingVoiceRequest {
            audio_path: outside.to_string_lossy().into_owned(),
            character_directory: character.to_string_lossy().into_owned(),
            character_name: "Tom Vale".into(),
            sample_text: String::new(),
            character_reference_path: None,
        })
        .unwrap();

        let audio = PathBuf::from(&saved.audio_path);
        assert!(audio.is_file());
        assert!(audio.starts_with(character.join("voice")));
        assert_ne!(audio, outside);
        assert!(outside.is_file());
    }
}
