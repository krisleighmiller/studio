//! Stability AI Stable Image API (v2beta multipart).

use crate::ctx::HostCtx;
use crate::media_util::{emit_progress, http_client, param_str, write_b64, write_bytes};
use crate::settings::ApiEntry;
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

fn base(api: &ApiEntry) -> String {
    let b = api.base_url.trim().trim_end_matches('/');
    if b.is_empty() {
        "https://api.stability.ai".into()
    } else {
        b.to_string()
    }
}

fn endpoint_for_model(model: &str) -> Result<&'static str, String> {
    match model.trim().to_ascii_lowercase().as_str() {
        "stable-image-ultra" | "ultra" | "stability-ultra" => {
            Ok("/v2beta/stable-image/generate/ultra")
        }
        "stable-image-core" | "core" | "stability-core" => Ok("/v2beta/stable-image/generate/core"),
        "sd3" | "stable-diffusion-3" | "sd3.5" | "stable-diffusion-3.5" => {
            Ok("/v2beta/stable-image/generate/sd3")
        }
        other => Err(format!(
            "Unknown Stability model `{other}`. Use stable-image-core, stable-image-ultra, or sd3."
        )),
    }
}

pub fn looks_like_model(model: &str) -> bool {
    let id = model.to_ascii_lowercase();
    id.starts_with("stable-image")
        || id.starts_with("stability-")
        || matches!(
            id.as_str(),
            "core" | "ultra" | "sd3" | "sd3.5" | "stable-diffusion-3"
        )
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
        return Err("Add a Stability API key in Settings first.".into());
    }
    let prompt = param_str(params, &["prompt", "text"])
        .ok_or_else(|| "Stability image generation requires params.prompt".to_string())?;
    let path = endpoint_for_model(model)?;
    let url = format!("{}{path}", base(api));
    emit_progress(app, format!("Submitting Stability image ({model})…"));

    let mut form = reqwest::blocking::multipart::Form::new()
        .text("prompt", prompt)
        .text(
            "output_format",
            param_str(params, &["output_format", "outputFormat"]).unwrap_or_else(|| "png".into()),
        )
        .text(
            "aspect_ratio",
            param_str(params, &["aspect_ratio", "aspectRatio"]).unwrap_or_else(|| "1:1".into()),
        );
    if let Some(neg) = param_str(params, &["negative_prompt", "negativePrompt"]) {
        form = form.text("negative_prompt", neg);
    }

    let response = http_client(180)?
        .post(&url)
        .header("Authorization", format!("Bearer {key}"))
        .header("Accept", "application/json")
        .multipart(form)
        .send()
        .map_err(|error| format!("Stability request failed: {error}"))?;
    let status = response.status();
    let bytes = response
        .bytes()
        .map_err(|error| format!("Could not read Stability response: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "Stability failed ({status}): {}",
            String::from_utf8_lossy(&bytes)
        ));
    }

    let prediction_id = format!(
        "stability:{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );

    // Prefer JSON { image: base64 } ; some configs return raw bytes.
    if let Ok(parsed) = serde_json::from_slice::<Value>(&bytes) {
        if let Some(b64) = parsed.get("image").and_then(|v| v.as_str()) {
            let mut result = json!({
                "prediction_id": prediction_id,
                "provider": "stability",
                "status": "completed",
            });
            if let Some(dest) = save_path {
                let saved = write_b64(dest, project_root, b64)?;
                result["saved"] = serde_json::from_str(&saved).unwrap_or(json!({ "raw": saved }));
                emit_progress(app, format!("Saved Stability image → {dest}"));
            }
            return Ok(result.to_string());
        }
    }

    let mut result = json!({
        "prediction_id": prediction_id,
        "provider": "stability",
        "status": "completed",
    });
    if let Some(dest) = save_path {
        let saved = write_bytes(dest, project_root, &bytes)?;
        result["saved"] = serde_json::from_str(&saved).unwrap_or(json!({ "raw": saved }));
        emit_progress(app, format!("Saved Stability image → {dest}"));
    } else {
        result["note"] = json!("No save_path — binary image not echoed. Pass save_path.");
    }
    Ok(result.to_string())
}
