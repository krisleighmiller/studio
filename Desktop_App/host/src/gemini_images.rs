//! Google Gemini / Imagen image generation on the existing `gemini` protocol kind.

use crate::ctx::HostCtx;
use crate::media_util::{emit_progress, http_client, param_str, write_b64};
use crate::settings::ApiEntry;
use base64::Engine;
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

fn base(api: &ApiEntry) -> String {
    let b = api.base_url.trim().trim_end_matches('/');
    if b.is_empty() {
        "https://generativelanguage.googleapis.com/v1beta".into()
    } else {
        b.to_string()
    }
}

pub fn looks_like_image_model(model: &str) -> bool {
    let id = model.to_ascii_lowercase();
    id.contains("image") || id.starts_with("imagen-") || id.contains("nano-banana")
}

fn is_imagen(model: &str) -> bool {
    model.to_ascii_lowercase().starts_with("imagen-")
}

fn read_inline_image(raw: &str, project_root: Option<&str>) -> Result<(String, String), String> {
    let raw = raw.trim();
    if let Some(rest) = raw.strip_prefix("data:") {
        let (meta, b64) = rest
            .split_once(",")
            .ok_or_else(|| "Invalid data URI".to_string())?;
        let mime = meta.split(';').next().unwrap_or("image/png");
        return Ok((mime.to_string(), b64.to_string()));
    }
    if raw.starts_with("http://") || raw.starts_with("https://") {
        // Gemini generateContent prefers inline data; fetch remote.
        let response = http_client(60)?
            .get(raw)
            .send()
            .map_err(|error| format!("Could not fetch image URL: {error}"))?;
        let mime = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/png")
            .split(';')
            .next()
            .unwrap_or("image/png")
            .to_string();
        let bytes = response
            .bytes()
            .map_err(|error| format!("Could not read image bytes: {error}"))?;
        return Ok((
            mime,
            base64::engine::general_purpose::STANDARD.encode(bytes),
        ));
    }
    let path = crate::media_util::resolve_path(raw, project_root)?;
    let (mime, bytes) =
        crate::media_util::load_media_as(&path, crate::media_util::MediaRole::Image)?;
    Ok((
        mime,
        base64::engine::general_purpose::STANDARD.encode(bytes),
    ))
}

pub fn generate_image(
    api: &ApiEntry,
    model: &str,
    params: &Value,
    project_root: Option<&str>,
    save_path: Option<&str>,
    app: Option<&HostCtx>,
) -> Result<String, String> {
    let key = api.api_key.trim();
    if key.is_empty() {
        return Err("Add a Gemini API key in Settings first.".into());
    }
    let prompt = param_str(params, &["prompt", "text"])
        .ok_or_else(|| "Gemini image generation requires params.prompt".to_string())?;
    let model_id = model.strip_prefix("models/").unwrap_or(model);
    emit_progress(app, format!("Submitting Gemini/Imagen image ({model_id})…"));

    let prediction_id = format!(
        "gemini:{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );

    if is_imagen(model_id) {
        let url = format!("{}/models/{model_id}:predict", base(api));
        let body = json!({
            "instances": [{ "prompt": prompt }],
            "parameters": {
                "sampleCount": 1,
                "aspectRatio": param_str(params, &["aspect_ratio", "aspectRatio"]).unwrap_or_else(|| "1:1".into()),
            }
        });
        let response = http_client(180)?
            .post(url)
            .header("Content-Type", "application/json")
            .header("x-goog-api-key", key)
            .json(&body)
            .send()
            .map_err(|error| {
                crate::http_util::format_reqwest_error("Imagen request failed", error)
            })?;
        let status = response.status();
        let text = response
            .text()
            .map_err(|error| format!("Could not read Imagen response: {error}"))?;
        if !status.is_success() {
            return Err(format!("Imagen failed ({status}): {text}"));
        }
        let parsed: Value = serde_json::from_str(&text)
            .map_err(|error| format!("Could not parse Imagen JSON: {error}"))?;
        let b64 = parsed
            .pointer("/predictions/0/bytesBase64Encoded")
            .or_else(|| parsed.pointer("/predictions/0/image/bytesBase64Encoded"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("Imagen response missing image bytes: {parsed}"))?;
        let mut result = json!({
            "prediction_id": prediction_id,
            "provider": "gemini",
            "status": "completed",
        });
        if let Some(dest) = save_path {
            let saved = write_b64(dest, project_root, b64)?;
            result["saved"] = serde_json::from_str(&saved).unwrap_or(json!({ "raw": saved }));
        }
        return Ok(result.to_string());
    }

    // Native Gemini image models (Nano Banana / flash-image / pro-image).
    let mut parts = vec![json!({ "text": prompt })];
    if let Some(image) = param_str(params, &["image", "image_url", "input_image"]) {
        let (mime, b64) = read_inline_image(&image, project_root)?;
        parts.push(json!({
            "inline_data": { "mime_type": mime, "data": b64 }
        }));
    }
    let body = json!({
        "contents": [{ "role": "user", "parts": parts }],
        "generationConfig": {
            "responseModalities": ["TEXT", "IMAGE"]
        }
    });
    let url = format!("{}/models/{model_id}:generateContent", base(api));
    let response = http_client(180)?
        .post(url)
        .header("Content-Type", "application/json")
        .header("x-goog-api-key", key)
        .json(&body)
        .send()
        .map_err(|error| {
            crate::http_util::format_reqwest_error("Gemini image request failed", error)
        })?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|error| format!("Could not read Gemini response: {error}"))?;
    if !status.is_success() {
        return Err(format!("Gemini image failed ({status}): {text}"));
    }
    let parsed: Value = serde_json::from_str(&text)
        .map_err(|error| format!("Could not parse Gemini JSON: {error}"))?;
    let b64 = parsed
        .pointer("/candidates/0/content/parts")
        .and_then(|v| v.as_array())
        .and_then(|parts| {
            parts.iter().find_map(|part| {
                part.pointer("/inlineData/data")
                    .or_else(|| part.pointer("/inline_data/data"))
                    .and_then(|v| v.as_str())
            })
        })
        .ok_or_else(|| {
            format!(
                "Gemini response had no image part: {}",
                parsed.to_string().chars().take(500).collect::<String>()
            )
        })?;

    let mut result = json!({
        "prediction_id": prediction_id,
        "provider": "gemini",
        "status": "completed",
    });
    if let Some(dest) = save_path {
        let saved = write_b64(dest, project_root, b64)?;
        result["saved"] = serde_json::from_str(&saved).unwrap_or(json!({ "raw": saved }));
        emit_progress(app, format!("Saved Gemini image → {dest}"));
    }
    Ok(result.to_string())
}
