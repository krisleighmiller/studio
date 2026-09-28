//! Luma Dream Machine video API.

use crate::ctx::HostCtx;
use crate::media_util::{
    download_url, emit_progress, file_to_data_uri, http_client, param_str, param_u64,
};
use crate::settings::ApiEntry;
use serde_json::{json, Value};
use std::thread;
use std::time::{Duration, Instant};

fn base(api: &ApiEntry) -> String {
    let b = api.base_url.trim().trim_end_matches('/');
    if b.is_empty() {
        "https://api.lumalabs.ai/dream-machine/v1".into()
    } else if b.contains("dream-machine") {
        b.to_string()
    } else {
        format!("{b}/dream-machine/v1")
    }
}

fn auth(api: &ApiEntry) -> Result<String, String> {
    let key = api.api_key.trim();
    if key.is_empty() {
        return Err("Add a Luma API key in Settings first.".into());
    }
    Ok(format!("Bearer {key}"))
}

pub fn looks_like_model(model: &str) -> bool {
    let id = model.to_ascii_lowercase();
    id.starts_with("ray-") || id.starts_with("luma") || id == "dream-machine"
}

pub fn encode_id(id: &str) -> String {
    format!("luma:{id}")
}

pub fn parse_id(prediction_id: &str) -> Option<String> {
    prediction_id.strip_prefix("luma:").map(str::to_string)
}

pub fn generate_video(
    api: &ApiEntry,
    model: &str,
    params: &Value,
    project_root: Option<&str>,
    app: Option<&HostCtx>,
) -> Result<String, String> {
    let prompt = param_str(params, &["prompt", "text"])
        .ok_or_else(|| "Luma video generation requires params.prompt".to_string())?;
    let duration = param_u64(params, &["duration", "seconds"], 5);
    let duration_str = if duration >= 9 { "9s" } else { "5s" };
    let mut body = json!({
        "prompt": prompt,
        "model": if model == "dream-machine" || model == "luma" { "ray-2" } else { model },
        "aspect_ratio": param_str(params, &["aspect_ratio", "aspectRatio"]).unwrap_or_else(|| "16:9".into()),
        "resolution": param_str(params, &["resolution"]).unwrap_or_else(|| "720p".into()),
        "duration": duration_str,
    });
    if let Some(image) = param_str(
        params,
        &["image", "image_url", "start_image", "starting_visual_input"],
    ) {
        let url = file_to_data_uri(&image, project_root)?;
        body["keyframes"] = json!({
            "frame0": { "type": "image", "url": url }
        });
    }

    emit_progress(app, format!("Submitting Luma generation ({model})…"));
    let response = http_client(120)?
        .post(format!("{}/generations", base(api)))
        .header("Authorization", auth(api)?)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|error| format!("Luma submit failed: {error}"))?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|error| format!("Could not read Luma response: {error}"))?;
    if !status.is_success() {
        return Err(format!("Luma submit failed ({status}): {text}"));
    }
    let parsed: Value = serde_json::from_str(&text)
        .map_err(|error| format!("Could not parse Luma JSON: {error}\n{text}"))?;
    let id = parsed
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Luma response missing id: {parsed}"))?;
    Ok(json!({
        "prediction_id": encode_id(id),
        "provider": "luma",
        "status": parsed.get("state").cloned().unwrap_or(json!("queued")),
        "raw": parsed,
    })
    .to_string())
}

pub fn get_prediction(
    api: &ApiEntry,
    prediction_id: &str,
    wait_seconds: u64,
    app: Option<&HostCtx>,
) -> Result<String, String> {
    let id = parse_id(prediction_id).ok_or_else(|| format!("Not a Luma id: {prediction_id}"))?;
    let url = format!("{}/generations/{id}", base(api));
    let client = http_client(60)?;
    let auth = auth(api)?;
    let deadline = Instant::now() + Duration::from_secs(wait_seconds.max(1));
    let mut last: Value;
    loop {
        let response = client
            .get(&url)
            .header("Authorization", &auth)
            .header("Accept", "application/json")
            .send()
            .map_err(|error| format!("Luma poll failed: {error}"))?;
        let code = response.status();
        let text = response.text().unwrap_or_default();
        if !code.is_success() {
            return Err(format!("Luma poll failed ({code}): {text}"));
        }
        last = serde_json::from_str(&text)
            .map_err(|error| format!("Could not parse Luma poll: {error}"))?;
        let state = last
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        emit_progress(app, format!("Luma {id}: {state}"));
        if matches!(state.as_str(), "completed" | "failed") {
            break;
        }
        if Instant::now() >= deadline {
            break;
        }
        thread::sleep(Duration::from_secs(3));
    }
    let output_url = last
        .pointer("/assets/video")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    Ok(json!({
        "prediction_id": prediction_id,
        "provider": "luma",
        "status": last.get("state").cloned().unwrap_or(json!("unknown")),
        "output_url": output_url,
        "raw": last,
    })
    .to_string())
}

pub fn collect_media(
    api: &ApiEntry,
    prediction_id: &str,
    dest: &str,
    project_root: Option<&str>,
    wait_seconds: u64,
    app: Option<&HostCtx>,
) -> Result<String, String> {
    let polled = get_prediction(api, prediction_id, wait_seconds, app)?;
    let parsed: Value = serde_json::from_str(&polled).unwrap_or(json!({}));
    if parsed.get("status").and_then(|v| v.as_str()) == Some("failed") {
        return Err(format!("Luma generation failed: {parsed}"));
    }
    let url = parsed
        .get("output_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Luma output not ready. Resume with `{prediction_id}`."))?;
    download_url(url, dest, project_root, app)
}
