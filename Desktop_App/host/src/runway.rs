//! Runway ML developer API (text/image → video, async tasks).

use crate::ctx::HostCtx;
use crate::media_util::{
    download_url, emit_progress, file_to_data_uri, first_url_in_value, http_client, param_str,
    param_u64,
};
use crate::settings::ApiEntry;
use serde_json::{json, Value};
use std::thread;
use std::time::{Duration, Instant};

const RUNWAY_VERSION: &str = "2024-11-06";

fn base(api: &ApiEntry) -> String {
    let b = api.base_url.trim().trim_end_matches('/');
    if b.is_empty() {
        "https://api.dev.runwayml.com".into()
    } else {
        b.to_string()
    }
}

fn auth_headers(api: &ApiEntry) -> Result<Vec<(String, String)>, String> {
    let key = api.api_key.trim();
    if key.is_empty() {
        return Err("Add a Runway API key in Settings first.".into());
    }
    Ok(vec![
        ("Authorization".into(), format!("Bearer {key}")),
        ("Content-Type".into(), "application/json".into()),
        ("X-Runway-Version".into(), RUNWAY_VERSION.into()),
    ])
}

pub fn looks_like_model(model: &str) -> bool {
    let id = model.to_ascii_lowercase();
    id.starts_with("gen4")
        || id.starts_with("gen3")
        || id.starts_with("veo3")
        || id.starts_with("act_two")
        || id.starts_with("seedance")
        || id.starts_with("runway")
}

pub fn encode_id(task_id: &str) -> String {
    format!("runway:{task_id}")
}

pub fn parse_id(prediction_id: &str) -> Option<String> {
    prediction_id.strip_prefix("runway:").map(str::to_string)
}

fn map_ratio(params: &Value) -> String {
    match param_str(params, &["ratio", "aspect_ratio", "aspectRatio"])
        .unwrap_or_else(|| "16:9".into())
        .as_str()
    {
        "9:16" | "720:1280" => "720:1280".into(),
        "1:1" | "960:960" => "960:960".into(),
        "4:3" => "1104:832".into(),
        "3:4" => "832:1104".into(),
        _ => "1280:720".into(),
    }
}

pub fn generate_video(
    api: &ApiEntry,
    model: &str,
    params: &Value,
    project_root: Option<&str>,
    app: Option<&HostCtx>,
) -> Result<String, String> {
    let prompt = param_str(params, &["prompt", "promptText", "text"]).unwrap_or_default();
    let image = param_str(
        params,
        &[
            "image",
            "image_url",
            "promptImage",
            "start_image",
            "starting_visual_input",
        ],
    );
    let duration = param_u64(params, &["duration", "seconds"], 5).clamp(2, 10);
    let mut body = json!({
        "model": model,
        "ratio": map_ratio(params),
        "duration": duration,
    });
    if !prompt.is_empty() {
        body["promptText"] = json!(prompt);
    }

    let (endpoint, body) = if let Some(image) = image {
        body["promptImage"] = json!(file_to_data_uri(&image, project_root)?);
        ("image_to_video", body)
    } else {
        if prompt.is_empty() {
            return Err("Runway text_to_video requires params.prompt".into());
        }
        ("text_to_video", body)
    };

    emit_progress(app, format!("Submitting Runway {endpoint} ({model})…"));
    let url = format!("{}/v1/{endpoint}", base(api));
    let mut req = http_client(120)?.post(&url);
    for (k, v) in auth_headers(api)? {
        req = req.header(k, v);
    }
    let response = req
        .json(&body)
        .send()
        .map_err(|error| format!("Runway submit failed: {error}"))?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|error| format!("Could not read Runway response: {error}"))?;
    if !status.is_success() {
        return Err(format!("Runway submit failed ({status}): {text}"));
    }
    let parsed: Value = serde_json::from_str(&text)
        .map_err(|error| format!("Could not parse Runway JSON: {error}\n{text}"))?;
    let task_id = parsed
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Runway response missing id: {parsed}"))?;
    Ok(json!({
        "prediction_id": encode_id(task_id),
        "provider": "runway",
        "status": "PENDING",
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
    let task_id =
        parse_id(prediction_id).ok_or_else(|| format!("Not a Runway id: {prediction_id}"))?;
    let url = format!("{}/v1/tasks/{task_id}", base(api));
    let client = http_client(60)?;
    let deadline = Instant::now() + Duration::from_secs(wait_seconds.max(1));
    let mut last: Value;
    loop {
        let mut req = client.get(&url);
        for (k, v) in auth_headers(api)? {
            req = req.header(k, v);
        }
        let response = req
            .send()
            .map_err(|error| format!("Runway poll failed: {error}"))?;
        let code = response.status();
        let text = response.text().unwrap_or_default();
        if !code.is_success() {
            return Err(format!("Runway poll failed ({code}): {text}"));
        }
        last = serde_json::from_str(&text)
            .map_err(|error| format!("Could not parse Runway task: {error}"))?;
        let status = last
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_uppercase();
        emit_progress(app, format!("Runway {task_id}: {status}"));
        if matches!(status.as_str(), "SUCCEEDED" | "FAILED" | "CANCELLED") {
            break;
        }
        if Instant::now() >= deadline {
            break;
        }
        thread::sleep(Duration::from_secs(5));
    }
    Ok(json!({
        "prediction_id": prediction_id,
        "provider": "runway",
        "status": last.get("status").cloned().unwrap_or(json!("unknown")),
        "output_url": first_url_in_value(last.get("output").unwrap_or(&Value::Null)),
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
    let status = parsed
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_ascii_uppercase();
    if status == "FAILED" {
        return Err(format!("Runway task failed: {parsed}"));
    }
    let url = parsed
        .get("output_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Runway output not ready. Resume with `{prediction_id}`."))?;
    download_url(url, dest, project_root, app)
}
