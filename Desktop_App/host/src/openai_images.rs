//! OpenAI Images API adapter (`/v1/images/generations` and `/v1/images/edits`).
//!
//! Works with OpenAI and OpenAI-compatible image endpoints when the Settings
//! entry kind is `openai-chat` (same API key / base URL family).

use crate::atlas;
use crate::ctx::HostCtx;
use crate::settings::ApiEntry;
use base64::Engine;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

fn emit_progress(app: Option<&HostCtx>, message: impl Into<String>) {
    if let Some(app) = app {
        app.emit("director-progress", message.into());
    }
}

fn normalize_base(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return "https://api.openai.com/v1".into();
    }
    if trimmed.ends_with("/v1") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1")
    }
}

fn param_str(params: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = params.get(*key).and_then(|v| v.as_str()) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn map_size(params: &Value) -> String {
    if let Some(size) = param_str(params, &["size"]) {
        return size;
    }
    match param_str(params, &["aspect_ratio", "aspectRatio"])
        .unwrap_or_else(|| "1:1".into())
        .as_str()
    {
        "16:9" | "3:2" => "1536x1024".into(),
        "9:16" | "2:3" => "1024x1536".into(),
        _ => "1024x1024".into(),
    }
}

fn resolve_local_image(raw: &str, project_root: Option<&str>) -> Result<PathBuf, String> {
    let path = if PathBuf::from(raw).is_absolute() {
        PathBuf::from(raw)
    } else if let Some(root) = project_root {
        PathBuf::from(root).join(raw)
    } else {
        PathBuf::from(raw)
    };
    if !path.is_file() {
        return Err(format!("Image path not found: {}", path.display()));
    }
    Ok(path)
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(180))
        .user_agent("VideoWorkbench/0.1 (desktop; openai-images)")
        .build()
        .map_err(|error| format!("Could not build HTTP client: {error}"))
}

fn write_b64_image(b64: &str, dest: &str, project_root: Option<&str>) -> Result<String, String> {
    let path = {
        let raw = dest.trim();
        let path = if PathBuf::from(raw).is_absolute() {
            PathBuf::from(raw)
        } else if let Some(root) = project_root {
            PathBuf::from(root).join(raw)
        } else {
            PathBuf::from(raw)
        };
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("Could not create parent dirs: {error}"))?;
            }
        }
        path
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|error| format!("Could not decode image base64: {error}"))?;
    if bytes.is_empty() {
        return Err("OpenAI image response was empty".into());
    }
    fs::write(&path, &bytes).map_err(|error| format!("Could not write image: {error}"))?;
    Ok(json!({
        "saved": true,
        "path": path.to_string_lossy(),
        "bytes": bytes.len(),
        "provider": "openai",
    })
    .to_string())
}

/// Synchronous generate (and optional save). Returns a prediction-shaped JSON string.
pub fn generate_image(
    api: &ApiEntry,
    model: &str,
    params: &Value,
    project_root: Option<&str>,
    save_path: Option<&str>,
    app: Option<&HostCtx>,
) -> Result<String, String> {
    let prompt = param_str(params, &["prompt", "text"])
        .ok_or_else(|| "OpenAI image generation requires params.prompt".to_string())?;
    let source = param_str(
        params,
        &["image", "image_url", "imageUrl", "input_image", "images"],
    );
    let base = normalize_base(&api.base_url);
    let key = api.api_key.trim();
    if key.is_empty() {
        return Err("Add an OpenAI API key in Settings first.".into());
    }

    emit_progress(app, format!("Submitting OpenAI image ({model})…"));

    let parsed: Value = if let Some(source) = source {
        // Focused edit path — multipart /images/edits
        let path = resolve_local_image(&source, project_root)?;
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("image.png")
            .to_string();
        let bytes = fs::read(&path).map_err(|error| format!("Could not read image: {error}"))?;
        let part = reqwest::blocking::multipart::Part::bytes(bytes)
            .file_name(file_name)
            .mime_str("application/octet-stream")
            .map_err(|error| format!("Could not build multipart image: {error}"))?;
        let form = reqwest::blocking::multipart::Form::new()
            .text("model", model.to_string())
            .text("prompt", prompt)
            .text("size", map_size(params))
            .part("image", part);
        let response = client()?
            .post(format!("{base}/images/edits"))
            .header("Authorization", format!("Bearer {key}"))
            .multipart(form)
            .send()
            .map_err(|error| format!("OpenAI image edit failed: {error}"))?;
        let status = response.status();
        let text = response
            .text()
            .map_err(|error| format!("Could not read OpenAI response: {error}"))?;
        if !status.is_success() {
            return Err(format!("OpenAI image edit failed ({status}): {text}"));
        }
        serde_json::from_str(&text)
            .map_err(|error| format!("Could not parse OpenAI JSON: {error}\n{text}"))?
    } else {
        let body = json!({
            "model": model,
            "prompt": prompt,
            "size": map_size(params),
            "n": 1,
        });
        let response = client()?
            .post(format!("{base}/images/generations"))
            .header("Authorization", format!("Bearer {key}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .map_err(|error| format!("OpenAI image generate failed: {error}"))?;
        let status = response.status();
        let text = response
            .text()
            .map_err(|error| format!("Could not read OpenAI response: {error}"))?;
        if !status.is_success() {
            return Err(format!("OpenAI image generate failed ({status}): {text}"));
        }
        serde_json::from_str(&text)
            .map_err(|error| format!("Could not parse OpenAI JSON: {error}\n{text}"))?
    };

    let prediction_id = format!(
        "openai:images:{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );

    // Prefer URL when present (some proxies); otherwise b64_json (gpt-image-*).
    if let Some(url) = parsed.pointer("/data/0/url").and_then(|v| v.as_str()) {
        let mut result = json!({
            "prediction_id": prediction_id,
            "provider": "openai",
            "status": "completed",
            "output_url": url,
            "raw": parsed,
        });
        if let Some(dest) = save_path {
            let saved = atlas::download_atlas_media(url, dest, project_root, app)?;
            result["saved"] = serde_json::from_str(&saved).unwrap_or(json!({ "raw": saved }));
        }
        return Ok(result.to_string());
    }

    let Some(b64) = parsed.pointer("/data/0/b64_json").and_then(|v| v.as_str()) else {
        return Err(format!(
            "OpenAI image response missing url/b64_json: {}",
            parsed.to_string().chars().take(400).collect::<String>()
        ));
    };

    let mut result = json!({
        "prediction_id": prediction_id,
        "provider": "openai",
        "status": "completed",
        "raw": { "note": "b64_json omitted from echo" },
    });
    if let Some(dest) = save_path {
        let saved = write_b64_image(b64, dest, project_root)?;
        result["saved"] = serde_json::from_str(&saved).unwrap_or(json!({ "raw": saved }));
        emit_progress(app, format!("Saved OpenAI image → {dest}"));
    } else {
        result["b64_json"] = json!(b64);
        result["note"] = json!("No save_path — returned base64. Prefer save_path on generate.");
    }
    Ok(result.to_string())
}

pub fn looks_like_openai_image_model(model: &str) -> bool {
    let id = model.to_ascii_lowercase();
    id.starts_with("gpt-image")
        || id.starts_with("dall-e")
        || id.contains("gpt-image")
        || id == "chatgpt-image-latest"
}
