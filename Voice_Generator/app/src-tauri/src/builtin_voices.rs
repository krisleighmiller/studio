use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::PathBuf,
    time::Duration,
};

const SAMPLE_CACHE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../cache/samples");
const ATLAS_GENERATE_URL: &str = "https://api.atlascloud.ai/api/v1/model/generateAudio";
const ENGLISH_VOICES_JSON: &str = include_str!("../english_voices.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinVoice {
    pub catalog_id: String,
    pub provider: String,
    pub model_id: String,
    pub voice_id: String,
    pub voice_name: String,
    pub gender: Option<String>,
    pub language: String,
    pub language_label: String,
    pub age: Option<String>,
    pub example_url: Option<String>,
    pub voice_field: String,
}

#[derive(Debug, Serialize)]
pub struct BuiltinVoiceCatalog {
    pub language_filter: String,
    pub sample_text: String,
    pub voices: Vec<BuiltinVoice>,
}

#[derive(Debug, Deserialize)]
pub struct PreviewBuiltinVoiceRequest {
    pub catalog_id: String,
    pub sample_text: String,
}

#[derive(Debug, Serialize)]
pub struct PreviewBuiltinVoiceResult {
    pub audio_path: String,
    pub audio_bytes: Vec<u8>,
    pub cached: bool,
    pub prediction_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SaveBuiltinVoiceRequest {
    pub catalog_id: String,
    pub sample_text: String,
    pub character_directory: String,
    pub character_name: String,
    pub character_reference_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SavedBuiltinVoice {
    pub audio_path: String,
    pub audio_bytes: Vec<u8>,
    pub metadata_path: String,
}

pub fn read_audio_file(path: &str) -> Result<Vec<u8>, String> {
    let audio_path = PathBuf::from(path);
    if !audio_path.is_file() {
        return Err("The audio file is missing or is no longer available.".to_string());
    }
    std::fs::read(&audio_path).map_err(|error| format!("Could not read audio file: {error}"))
}

pub fn list_english_builtin_voices() -> Result<BuiltinVoiceCatalog, String> {
    let voices: Vec<BuiltinVoice> = serde_json::from_str(ENGLISH_VOICES_JSON)
        .map_err(|error| format!("Could not parse the English voice catalog: {error}"))?;
    Ok(BuiltinVoiceCatalog {
        language_filter: "en".to_string(),
        sample_text: "Technology is not separate from the world around us. It is one more way the world speaks.".to_string(),
        voices,
    })
}

fn find_voice(catalog_id: &str) -> Result<BuiltinVoice, String> {
    list_english_builtin_voices()?
        .voices
        .into_iter()
        .find(|voice| voice.catalog_id == catalog_id)
        .ok_or_else(|| format!("Unknown built-in voice: {catalog_id}"))
}

fn atlas_api_key() -> Result<String, String> {
    std::env::var("ATLASCLOUD_API_KEY").map_err(|_| {
        "ATLASCLOUD_API_KEY is not set. Export it before generating built-in voice samples.".to_string()
    })
}

fn sample_cache_path(voice: &BuiltinVoice, sample_text: &str) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    voice.catalog_id.hash(&mut hasher);
    sample_text.hash(&mut hasher);
    let digest = format!("{:016x}", hasher.finish());
    PathBuf::from(SAMPLE_CACHE_DIR).join(format!("{digest}.wav"))
}

fn build_generate_body(voice: &BuiltinVoice, sample_text: &str) -> Value {
    let mut body = json!({
        "model": voice.model_id,
        "text": sample_text,
    });

    match voice.model_id.as_str() {
        "xai/tts-v1" => {
            body["language"] = json!("en");
            body["voice_id"] = json!(voice.voice_id);
            body["codec"] = json!("wav");
            body["sample_rate"] = json!(24000);
        }
        "elevenlabs/v3/text-to-speech" => {
            body["voice"] = json!(voice.voice_id);
            body["stability"] = json!(0.5);
            body["apply_text_normalization"] = json!("auto");
            body["language_code"] = json!("en");
        }
        "google/gemini-2.5-pro-tts" => {
            body["voice"] = json!(voice.voice_id);
        }
        "minimax/speech-2.6-turbo" => {
            body["voice"] = json!(voice.voice_id);
            body["format"] = json!("wav");
            body["sample_rate"] = json!(24000);
            body["language_boost"] = json!("English");
        }
        _ => {
            body[&voice.voice_field] = json!(voice.voice_id);
        }
    }

    body
}

async fn atlas_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("VoiceProfileStudio/0.1")
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| format!("Could not create Atlas client: {error}"))
}

async fn generate_and_download(
    voice: &BuiltinVoice,
    sample_text: &str,
) -> Result<(PathBuf, String), String> {
    let api_key = atlas_api_key()?;
    let client = atlas_client().await?;
    let body = build_generate_body(voice, sample_text);

    let start = client
        .post(ATLAS_GENERATE_URL)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("Atlas generateAudio request failed: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Atlas generateAudio returned an error: {error}"))?
        .json::<Value>()
        .await
        .map_err(|error| format!("Atlas generateAudio response was not JSON: {error}"))?;

    let prediction_id = start
        .pointer("/data/id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Atlas did not return a prediction id: {start}"))?
        .to_string();

    let poll_url = format!("https://api.atlascloud.ai/api/v1/model/prediction/{prediction_id}");
    let mut output_url = None;
    for _ in 0..90 {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let status_payload = client
            .get(&poll_url)
            .header("Authorization", format!("Bearer {api_key}"))
            .send()
            .await
            .map_err(|error| format!("Atlas prediction poll failed: {error}"))?
            .error_for_status()
            .map_err(|error| format!("Atlas prediction poll returned an error: {error}"))?
            .json::<Value>()
            .await
            .map_err(|error| format!("Atlas prediction poll was not JSON: {error}"))?;

        let status = status_payload
            .pointer("/data/status")
            .and_then(Value::as_str)
            .unwrap_or("");
        if status == "completed" || status == "succeeded" {
            output_url = status_payload
                .pointer("/data/outputs/0")
                .and_then(Value::as_str)
                .map(str::to_string);
            break;
        }
        if status == "failed" || status == "timeout" {
            let error = status_payload
                .pointer("/data/error")
                .and_then(Value::as_str)
                .unwrap_or("Generation failed");
            return Err(format!("Atlas generation failed for {}: {error}", voice.voice_name));
        }
    }

    let output_url =
        output_url.ok_or_else(|| "Atlas generation timed out before audio was ready.".to_string())?;

    let bytes = client
        .get(&output_url)
        .send()
        .await
        .map_err(|error| format!("Could not download generated audio: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Audio download failed: {error}"))?
        .bytes()
        .await
        .map_err(|error| format!("Could not read generated audio bytes: {error}"))?;

    let cache_dir = PathBuf::from(SAMPLE_CACHE_DIR);
    std::fs::create_dir_all(&cache_dir)
        .map_err(|error| format!("Could not create sample cache directory: {error}"))?;

    let extension = PathBuf::from(&output_url)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("bin")
        .to_lowercase();
    let temp_input = cache_dir.join(format!(
        "tmp_{}_{}.{}",
        prediction_id.replace('/', "_"),
        std::process::id(),
        extension
    ));
    std::fs::write(&temp_input, &bytes)
        .map_err(|error| format!("Could not store downloaded audio: {error}"))?;

    let wav_path = sample_cache_path(voice, sample_text);
    convert_to_wav(&temp_input, &wav_path)?;
    let _ = std::fs::remove_file(&temp_input);

    Ok((wav_path, prediction_id))
}

fn convert_to_wav(input: &PathBuf, output: &PathBuf) -> Result<(), String> {
    if input
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
    {
        std::fs::copy(input, output)
            .map_err(|error| format!("Could not cache WAV sample: {error}"))?;
        return Ok(());
    }

    let status = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            input.to_str().unwrap_or_default(),
            "-acodec",
            "pcm_s16le",
            "-ar",
            "44100",
            "-ac",
            "1",
            output.to_str().unwrap_or_default(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|error| format!("Could not start ffmpeg to convert audio: {error}"))?;
    if !status.success() {
        return Err("ffmpeg could not convert the generated audio to WAV.".to_string());
    }
    Ok(())
}

pub async fn preview_builtin_voice(
    request: PreviewBuiltinVoiceRequest,
) -> Result<PreviewBuiltinVoiceResult, String> {
    let sample_text = request.sample_text.trim();
    if sample_text.is_empty() {
        return Err("Sample text is required before generating a preview.".to_string());
    }
    let voice = find_voice(&request.catalog_id)?;
    let cache_path = sample_cache_path(&voice, sample_text);
    if cache_path.is_file() {
        let audio_path = cache_path.to_string_lossy().into_owned();
        let audio_bytes = read_audio_file(&audio_path)?;
        return Ok(PreviewBuiltinVoiceResult {
            audio_path,
            audio_bytes,
            cached: true,
            prediction_id: None,
        });
    }

    let (wav_path, prediction_id) = generate_and_download(&voice, sample_text).await?;
    let audio_path = wav_path.to_string_lossy().into_owned();
    let audio_bytes = read_audio_file(&audio_path)?;
    Ok(PreviewBuiltinVoiceResult {
        audio_path,
        audio_bytes,
        cached: false,
        prediction_id: Some(prediction_id),
    })
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

pub async fn save_builtin_voice(
    request: SaveBuiltinVoiceRequest,
) -> Result<SavedBuiltinVoice, String> {
    let sample_text = request.sample_text.trim();
    if sample_text.is_empty() {
        return Err("Sample text is required before saving a built-in voice.".to_string());
    }

    let character_directory = PathBuf::from(&request.character_directory);
    if !character_directory.is_dir() {
        return Err("Choose an existing character directory before saving.".to_string());
    }

    let voice = find_voice(&request.catalog_id)?;
    let preview = preview_builtin_voice(PreviewBuiltinVoiceRequest {
        catalog_id: request.catalog_id.clone(),
        sample_text: sample_text.to_string(),
    })
    .await?;
    let temp_path = PathBuf::from(&preview.audio_path);
    if !temp_path.is_file() {
        return Err("The generated voice sample is missing.".to_string());
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
        .map_err(|error| format!("Could not save the approved voice sample: {error}"))?;

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
    let gender = voice
        .gender
        .as_deref()
        .map(|value| yaml_string(value))
        .unwrap_or_else(|| "null".to_string());
    let settings = json!({
        "voice_field": voice.voice_field,
        "language": voice.language,
        "language_label": voice.language_label,
        "catalog_id": voice.catalog_id,
        "prediction_id": preview.prediction_id,
    });
    let profile = format!(
        "profile_version: 1\n\
character: {}\n\
source_mode: built_in_voice\n\
provider: atlascloud\n\
model_id: {}\n\
voice_id: {}\n\
voice_name: {}\n\
language: {}\n\
gender: {}\n\
voice_description: {}\n\
settings: {}\n\
prompt: \"\"\n\
approved_audio:\n\
  - path: {}\n\
    purpose: canonical_sample\n\
    sample_text: {}\n\
created_at: {}\n\
updated_at: {}\n",
        yaml_string(&request.character_name),
        yaml_string(&voice.model_id),
        yaml_string(&voice.voice_id),
        yaml_string(&voice.voice_name),
        yaml_string(&voice.language),
        gender,
        yaml_string(&format!(
            "{} {} voice via {}",
            voice.provider, voice.voice_name, voice.model_id
        )),
        settings,
        yaml_string(&relative_audio_path),
        yaml_string(sample_text),
        yaml_string(&now),
        yaml_string(&now),
    );

    let metadata_path = if let Some(reference_path) = reference_path.as_ref() {
        append_character_audio_profile(reference_path, &profile)?;
        format!("{}# Audio Profile", reference_path.to_string_lossy())
    } else {
        std::fs::write(&profile_path, &profile)
            .map_err(|error| format!("Could not write voice profile metadata: {error}"))?;
        profile_path.to_string_lossy().into_owned()
    };

    let saved_audio_path = audio_path.to_string_lossy().into_owned();
    let audio_bytes = read_audio_file(&saved_audio_path)?;
    Ok(SavedBuiltinVoice {
        audio_path: saved_audio_path,
        audio_bytes,
        metadata_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn preview_and_save_english_xai_eve() {
        if std::env::var("ATLASCLOUD_API_KEY").is_err() {
            return;
        }
        let sample_text =
            "Technology is not separate from the world around us. It is one more way the world speaks.";
        let preview = preview_builtin_voice(PreviewBuiltinVoiceRequest {
            catalog_id: "xai/tts-v1::eve".to_string(),
            sample_text: sample_text.to_string(),
        })
        .await
        .expect("preview should succeed");
        assert!(PathBuf::from(&preview.audio_path).is_file());
        assert!(!preview.audio_bytes.is_empty());

        let temp_character = std::env::temp_dir().join(format!(
            "voice-studio-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&temp_character).unwrap();
        let saved = save_builtin_voice(SaveBuiltinVoiceRequest {
            catalog_id: "xai/tts-v1::eve".to_string(),
            sample_text: sample_text.to_string(),
            character_directory: temp_character.to_string_lossy().into_owned(),
            character_name: "Test Character".to_string(),
            character_reference_path: None,
        })
        .await
        .expect("save should succeed");
        assert!(PathBuf::from(&saved.audio_path).is_file());
        assert!(!saved.audio_bytes.is_empty());
        assert!(PathBuf::from(&saved.metadata_path).is_file());
        let profile = std::fs::read_to_string(&saved.metadata_path).unwrap();
        assert!(profile.contains("source_mode: built_in_voice"));
        assert!(profile.contains("voice_id: \"eve\""));
        assert!(profile.contains("model_id: \"xai/tts-v1\""));
        let _ = std::fs::remove_dir_all(temp_character);
    }
}
