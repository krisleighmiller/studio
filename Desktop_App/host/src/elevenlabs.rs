//! ElevenLabs direct API adapter for the in-desk Voice tab.

use crate::ctx::HostCtx;
use crate::settings;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const API_BASE: &str = "https://api.elevenlabs.io";
const USER_AGENT: &str = "VideoWorkbench/0.1 (desktop; elevenlabs-voice)";

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|error| format!("Could not build HTTP client: {error}"))
}

fn api_key(app: &HostCtx) -> Result<String, String> {
    settings::elevenlabs_api_key(app)
}

fn error_body(status: reqwest::StatusCode, body: &str) -> String {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/detail/message")
                .or_else(|| value.pointer("/detail"))
                .cloned()
        })
        .map(|value| match value {
            Value::String(text) => text,
            other => other.to_string(),
        })
        .unwrap_or_else(|| body.chars().take(400).collect());
    format!("ElevenLabs request failed ({status}): {detail}")
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LibraryVoice {
    pub voice_id: String,
    pub name: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub preview_url: Option<String>,
    pub labels: std::collections::HashMap<String, String>,
    pub gender: Option<String>,
    pub age: Option<String>,
    pub accent: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryPage {
    pub voices: Vec<LibraryVoice>,
    pub has_more: bool,
    pub next_page_token: Option<String>,
}

fn label(labels: &std::collections::HashMap<String, String>, key: &str) -> Option<String> {
    labels
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn elevenlabs_list_voices(
    app: HostCtx,
    search: Option<String>,
    voice_type: Option<String>,
    page_size: Option<u32>,
    next_page_token: Option<String>,
) -> Result<LibraryPage, String> {
    let key = api_key(&app)?;
    let client = client()?;
    let mut req = client
        .get(format!("{API_BASE}/v2/voices"))
        .header("xi-api-key", &key)
        .query(&[(
            "page_size",
            page_size.unwrap_or(40).clamp(1, 100).to_string(),
        )]);
    if let Some(search) = search
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        req = req.query(&[("search", search)]);
    }
    if let Some(voice_type) = voice_type
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != "all")
    {
        req = req.query(&[("voice_type", voice_type)]);
    }
    if let Some(token) = next_page_token
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        req = req.query(&[("next_page_token", token)]);
    }

    let response = req
        .send()
        .map_err(|error| format!("Voice library request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|error| format!("Could not read voice library response: {error}"))?;
    if !status.is_success() {
        return Err(error_body(status, &body));
    }
    let parsed: Value = serde_json::from_str(&body)
        .map_err(|error| format!("Could not parse voice library JSON: {error}"))?;
    let mut voices = Vec::new();
    for item in parsed
        .get("voices")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
    {
        let voice_id = item
            .get("voice_id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if voice_id.is_empty() {
            continue;
        }
        let mut labels = std::collections::HashMap::new();
        if let Some(object) = item.get("labels").and_then(|value| value.as_object()) {
            for (key, value) in object {
                if let Some(text) = value.as_str() {
                    labels.insert(key.clone(), text.to_string());
                }
            }
        }
        voices.push(LibraryVoice {
            voice_id,
            name: item
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or("Untitled")
                .to_string(),
            category: item
                .get("category")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            description: item
                .get("description")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            preview_url: item
                .get("preview_url")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            gender: label(&labels, "gender"),
            age: label(&labels, "age"),
            accent: label(&labels, "accent"),
            labels,
        });
    }
    Ok(LibraryPage {
        voices,
        has_more: parsed
            .get("has_more")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        next_page_token: parsed
            .get("next_page_token")
            .and_then(|value| value.as_str())
            .map(str::to_string),
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioPayload {
    pub audio_base64: String,
    pub media_type: String,
}

pub fn elevenlabs_tts_preview(
    app: HostCtx,
    voice_id: String,
    text: String,
) -> Result<AudioPayload, String> {
    let key = api_key(&app)?;
    let voice_id = voice_id.trim();
    let text = text.trim();
    if voice_id.is_empty() {
        return Err("voice_id is required.".into());
    }
    if text.is_empty() {
        return Err("Sample text is required.".into());
    }
    let client = client()?;
    let response = client
        .post(format!("{API_BASE}/v1/text-to-speech/{voice_id}"))
        .header("xi-api-key", &key)
        .header("Accept", "audio/mpeg")
        .json(&json!({
            "text": text,
            "model_id": "eleven_multilingual_v2",
        }))
        .send()
        .map_err(|error| format!("TTS preview failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(error_body(status, &body));
    }
    let bytes = response
        .bytes()
        .map_err(|error| format!("Could not read TTS audio: {error}"))?;
    Ok(AudioPayload {
        audio_base64: B64.encode(bytes),
        media_type: "audio/mpeg".into(),
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignPreview {
    pub generated_voice_id: String,
    pub audio_base64: String,
    pub media_type: String,
    pub duration_secs: Option<f64>,
    pub language: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignResult {
    pub previews: Vec<DesignPreview>,
    pub text: String,
    pub voice_description: String,
}

pub fn elevenlabs_design_voice(
    app: HostCtx,
    voice_description: String,
    text: String,
) -> Result<DesignResult, String> {
    let key = api_key(&app)?;
    let description = voice_description.trim();
    let mut sample = text.trim().to_string();
    if description.is_empty() {
        return Err("Voice description is required.".into());
    }
    if sample.chars().count() < 100 {
        return Err(
            "Sample text must be at least 100 characters for ElevenLabs voice design.".into(),
        );
    }
    if sample.chars().count() > 1000 {
        sample = sample.chars().take(1000).collect();
    }
    let client = client()?;
    let response = client
        .post(format!("{API_BASE}/v1/text-to-voice/design"))
        .header("xi-api-key", &key)
        .json(&json!({
            "voice_description": description,
            "text": sample,
            "model_id": "eleven_multilingual_ttv_v2",
            "auto_generate_text": false,
        }))
        .send()
        .map_err(|error| format!("Voice design request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|error| format!("Could not read design response: {error}"))?;
    if !status.is_success() {
        return Err(error_body(status, &body));
    }
    let parsed: Value = serde_json::from_str(&body)
        .map_err(|error| format!("Could not parse design JSON: {error}"))?;
    let mut previews = Vec::new();
    for item in parsed
        .get("previews")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
    {
        let generated_voice_id = item
            .get("generated_voice_id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        let audio_base64 = item
            .get("audio_base_64")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        if generated_voice_id.is_empty() || audio_base64.is_empty() {
            continue;
        }
        previews.push(DesignPreview {
            generated_voice_id,
            audio_base64,
            media_type: item
                .get("media_type")
                .and_then(|value| value.as_str())
                .unwrap_or("audio/mpeg")
                .to_string(),
            duration_secs: item.get("duration_secs").and_then(|value| value.as_f64()),
            language: item
                .get("language")
                .and_then(|value| value.as_str())
                .map(str::to_string),
        });
    }
    if previews.is_empty() {
        return Err("ElevenLabs returned no voice design previews.".into());
    }
    Ok(DesignResult {
        previews,
        text: parsed
            .get("text")
            .and_then(|value| value.as_str())
            .unwrap_or(&sample)
            .to_string(),
        voice_description: description.to_string(),
    })
}

pub fn elevenlabs_create_designed_voice(
    app: HostCtx,
    voice_name: String,
    voice_description: String,
    generated_voice_id: String,
) -> Result<String, String> {
    let key = api_key(&app)?;
    let voice_name = voice_name.trim();
    let voice_description = voice_description.trim();
    let generated_voice_id = generated_voice_id.trim();
    if voice_name.is_empty() || voice_description.is_empty() || generated_voice_id.is_empty() {
        return Err("voice_name, voice_description, and generated_voice_id are required.".into());
    }
    let client = client()?;
    let response = client
        .post(format!("{API_BASE}/v1/text-to-voice"))
        .header("xi-api-key", &key)
        .json(&json!({
            "voice_name": voice_name,
            "voice_description": voice_description,
            "generated_voice_id": generated_voice_id,
        }))
        .send()
        .map_err(|error| format!("Create designed voice failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|error| format!("Could not read create-voice response: {error}"))?;
    if !status.is_success() {
        return Err(error_body(status, &body));
    }
    let parsed: Value = serde_json::from_str(&body)
        .map_err(|error| format!("Could not parse create-voice JSON: {error}"))?;
    parsed
        .get("voice_id")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "ElevenLabs create-voice response missing voice_id.".into())
}

pub fn elevenlabs_download_preview_url(url: String) -> Result<AudioPayload, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("preview URL is required.".into());
    }
    let client = client()?;
    let response = client
        .get(url)
        .send()
        .map_err(|error| format!("Preview download failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(error_body(status, &body));
    }
    let media_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("audio/mpeg")
        .to_string();
    let bytes = response
        .bytes()
        .map_err(|error| format!("Could not read preview audio: {error}"))?;
    Ok(AudioPayload {
        audio_base64: B64.encode(bytes),
        media_type,
    })
}
