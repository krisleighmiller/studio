mod builtin_voices;

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

struct RecordingProcess {
    child: Child,
    output_path: String,
    raw_path: String,
    spectrum: Arc<Mutex<Vec<f32>>>,
    reader: Option<JoinHandle<std::io::Result<()>>>,
}

struct RecorderState(Mutex<Option<RecordingProcess>>);

#[derive(Debug, Deserialize)]
struct SaveRecordingRequest {
    temp_path: String,
    character_directory: String,
    character_name: String,
    sample_text: String,
    character_reference_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct SavedRecording {
    audio_path: String,
    metadata_path: String,
}

#[derive(Debug, Serialize)]
struct RecordingSource {
    id: u32,
    name: String,
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

fn append_character_audio_profile(
    reference_path: &PathBuf,
    profile_yaml: &str,
) -> Result<(), String> {
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

fn write_wav_from_pcm(raw_path: &PathBuf, wav_path: &PathBuf) -> Result<(), String> {
    let pcm = std::fs::read(raw_path)
        .map_err(|error| format!("Could not read captured audio: {error}"))?;
    if pcm.is_empty() {
        return Err("The microphone returned no audio data.".to_string());
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

#[tauri::command]
fn list_recording_sources() -> Result<Vec<RecordingSource>, String> {
    let output = Command::new("wpctl")
        .args(["status"])
        .output()
        .map_err(|error| format!("Could not list PipeWire audio sources: {error}"))?;
    if !output.status.success() {
        return Err("PipeWire could not list audio sources.".to_string());
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
        if name.to_lowercase().contains("[v4l2]") {
            continue;
        }
        if !name.is_empty() {
            sources.push(RecordingSource {
                id,
                name: name.to_string(),
            });
        }
    }
    Ok(sources)
}

#[tauri::command]
fn start_recording(
    state: tauri::State<'_, RecorderState>,
    output_path: String,
    source_id: Option<u32>,
) -> Result<String, String> {
    let mut active_process = state
        .0
        .lock()
        .map_err(|_| "Recorder state is unavailable.".to_string())?;
    if active_process.is_some() {
        return Err("A recording is already in progress.".to_string());
    }

    let path = PathBuf::from(&output_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create recording directory: {error}"))?;
    }

    let raw_path = PathBuf::from(format!("{output_path}.raw"));
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
                .map(|sample| i16::from_le_bytes([sample[0], sample[1]]) as f32 / 32768.0);
            let samples = samples.collect::<Vec<_>>();
            if !samples.is_empty() {
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
        }
        Ok(())
    });

    *active_process = Some(RecordingProcess {
        child,
        output_path: output_path.clone(),
        raw_path: raw_path.to_string_lossy().into_owned(),
        spectrum,
        reader: Some(reader),
    });
    Ok(output_path)
}

#[tauri::command]
fn recording_level(state: tauri::State<'_, RecorderState>) -> Result<Vec<f32>, String> {
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
            return Err("The recorder stopped unexpectedly. Check microphone permissions and PipeWire/ALSA.".to_string());
        }
    }
    Ok(active_process
        .as_ref()
        .and_then(|recording| recording.spectrum.lock().ok().map(|spectrum| spectrum.clone()))
        .unwrap_or_else(|| vec![0.0; 48]))
}

#[tauri::command]
fn stop_recording(state: tauri::State<'_, RecorderState>) -> Result<String, String> {
    let mut active_process = state
        .0
        .lock()
        .map_err(|_| "Recorder state is unavailable.".to_string())?;
    let Some(mut recording) = active_process.take() else {
        return Err("No recording is currently in progress.".to_string());
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
        if signal_result.is_err() || !signal_result.map(|status| status.success()).unwrap_or(false) {
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
    write_wav_from_pcm(&PathBuf::from(&recording.raw_path), &PathBuf::from(&recording.output_path))?;
    let _ = std::fs::remove_file(&recording.raw_path);

    Ok(recording.output_path)
}

#[tauri::command]
fn pick_character_directory() -> Result<Option<String>, String> {
    Ok(rfd::FileDialog::new().set_title("Choose character directory").pick_folder().map(
        |path| path.to_string_lossy().into_owned(),
    ))
}

#[tauri::command]
fn save_recording(request: SaveRecordingRequest) -> Result<SavedRecording, String> {
    let character_directory = PathBuf::from(&request.character_directory);
    let temp_path = PathBuf::from(&request.temp_path);
    if !temp_path.is_file() {
        return Err("The recording file is missing or is no longer available.".to_string());
    }
    if !character_directory.is_dir() {
        return Err("Choose an existing character directory before saving.".to_string());
    }

    let voice_directory = character_directory.join("voice");
    std::fs::create_dir_all(&voice_directory)
        .map_err(|error| format!("Could not create the voice directory: {error}"))?;

    let file_stem = safe_character_name(&request.character_name);
    let mut index = 1;
    let audio_path = loop {
        let candidate = voice_directory.join(format!("{file_stem}_voice_sample_{index:02}.wav"));
        if !candidate.exists() {
            break candidate;
        }
        index += 1;
    };
    std::fs::copy(&temp_path, &audio_path)
        .map_err(|error| format!("Could not save the approved recording: {error}"))?;
    let _ = std::fs::remove_file(&temp_path);

    let profile_path = voice_directory.join(format!("{file_stem}_voice_profile.yaml"));
    let relative_audio_path = format!(
        "voice/{}",
        audio_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("voice_sample.wav")
    );
    let reference_path = request.character_reference_path.as_deref().map(PathBuf::from);
    if let Some(reference_path) = reference_path.as_ref() {
        if !reference_path.is_file() {
            return Err("The supplied character reference sheet does not exist.".to_string());
        }
    }
    let now = format!("{:?}", std::time::SystemTime::now());
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
        yaml_string(&now),
        yaml_string(&now),
    );
    let metadata_path = if let Some(reference_path) = reference_path.as_ref() {
        append_character_audio_profile(
            reference_path,
            &profile,
        )?;
        format!("{}# Audio Profile", reference_path.to_string_lossy())
    } else {
        std::fs::write(&profile_path, &profile)
            .map_err(|error| format!("Could not write voice profile metadata: {error}"))?;
        profile_path.to_string_lossy().into_owned()
    };

    Ok(SavedRecording {
        audio_path: audio_path.to_string_lossy().into_owned(),
        metadata_path,
    })
}

#[tauri::command]
fn list_builtin_voices() -> Result<builtin_voices::BuiltinVoiceCatalog, String> {
    builtin_voices::list_english_builtin_voices()
}

#[tauri::command]
fn read_audio_file(path: String) -> Result<Vec<u8>, String> {
    builtin_voices::read_audio_file(&path)
}

#[tauri::command]
async fn preview_builtin_voice(
    request: builtin_voices::PreviewBuiltinVoiceRequest,
) -> Result<builtin_voices::PreviewBuiltinVoiceResult, String> {
    builtin_voices::preview_builtin_voice(request).await
}

#[tauri::command]
async fn save_builtin_voice(
    request: builtin_voices::SaveBuiltinVoiceRequest,
) -> Result<builtin_voices::SavedBuiltinVoice, String> {
    builtin_voices::save_builtin_voice(request).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Ubuntu's WebKitGTK/Mesa stack is aborting in libgallium while
    // initializing the WebKit GPU compositor on this machine.
    std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(RecorderState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            list_builtin_voices,
            read_audio_file,
            preview_builtin_voice,
            save_builtin_voice,
            list_recording_sources,
            start_recording,
            stop_recording,
            recording_level,
            pick_character_directory,
            save_recording
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
