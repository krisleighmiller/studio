//! fal.ai queue adapter.
//! Model ids look like `fal-ai/flux/schnell` or `fal-ai/kling-video/v2.1/master`.

use crate::ctx::HostCtx;
use crate::media_util::{
    download_url, emit_progress, file_to_data_uri, first_url_in_value, http_client, param_str,
};
use crate::settings::ApiEntry;
use serde_json::{json, Value};
use std::thread;
use std::time::{Duration, Instant};

fn queue_base(api: &ApiEntry) -> String {
    let b = api.base_url.trim().trim_end_matches('/');
    if b.is_empty() || (b.contains("fal.ai") && !b.contains("queue")) {
        "https://queue.fal.run".into()
    } else {
        b.to_string()
    }
}

fn auth(api: &ApiEntry) -> Result<String, String> {
    let key = api.api_key.trim();
    if key.is_empty() {
        return Err("Add a fal.ai API key in Settings first.".into());
    }
    Ok(format!("Key {key}"))
}

pub fn looks_like_model(model: &str) -> bool {
    let id = model.to_ascii_lowercase();
    id.starts_with("fal-ai/") || id.starts_with("fal/") || id.contains("fal-ai/")
}

pub fn encode_id(model: &str, request_id: &str) -> String {
    format!("fal:{}::{request_id}", model.replace('/', "|"))
}

pub fn parse_id(prediction_id: &str) -> Option<(String, String)> {
    let rest = prediction_id.strip_prefix("fal:")?;
    let (model, request_id) = rest.rsplit_once("::")?;
    Some((model.replace('|', "/"), request_id.to_string()))
}

fn build_body(params: &Value, project_root: Option<&str>) -> Result<Value, String> {
    let mut body = match params {
        Value::Object(map) => Value::Object(map.clone()),
        _ => json!({}),
    };
    if let Some(obj) = body.as_object_mut() {
        if !obj.contains_key("prompt") {
            if let Some(prompt) = param_str(params, &["prompt", "text"]) {
                obj.insert("prompt".into(), json!(prompt));
            }
        }
        for key in [
            "image_url",
            "image_urls",
            "start_image_url",
            "end_image_url",
            "video_url",
            "audio_url",
        ] {
            if let Some(Value::String(path)) = obj.get(key).cloned() {
                if !path.starts_with("http") && !path.starts_with("data:") {
                    obj.insert(
                        key.to_string(),
                        json!(file_to_data_uri(&path, project_root)?),
                    );
                }
            }
        }
        // Common Atlas-ish aliases → fal fields
        if !obj.contains_key("image_url") {
            if let Some(image) = param_str(
                params,
                &["image", "image_url", "start_image", "starting_visual_input"],
            ) {
                obj.insert(
                    "image_url".into(),
                    json!(file_to_data_uri(&image, project_root)?),
                );
            }
        }
    }
    Ok(body)
}

pub fn generate(
    api: &ApiEntry,
    model: &str,
    params: &Value,
    project_root: Option<&str>,
    app: Option<&HostCtx>,
) -> Result<String, String> {
    let body = build_body(params, project_root)?;
    let auth = auth(api)?;
    let url = format!("{}/{model}", queue_base(api));
    emit_progress(app, format!("Submitting fal queue job ({model})…"));
    let response = http_client(120)?
        .post(&url)
        .header("Authorization", auth)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|error| format!("fal submit failed: {error}"))?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|error| format!("Could not read fal response: {error}"))?;
    if !status.is_success() {
        return Err(format!("fal submit failed ({status}): {text}"));
    }
    let parsed: Value = serde_json::from_str(&text)
        .map_err(|error| format!("Could not parse fal JSON: {error}\n{text}"))?;
    let request_id = parsed
        .get("request_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("fal response missing request_id: {parsed}"))?;
    Ok(json!({
        "prediction_id": encode_id(model, request_id),
        "provider": "fal",
        "status": "IN_QUEUE",
        "status_url": parsed.get("status_url").cloned(),
        "response_url": parsed.get("response_url").cloned(),
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
    let (model, request_id) =
        parse_id(prediction_id).ok_or_else(|| format!("Not a fal id: {prediction_id}"))?;
    let auth = auth(api)?;
    let client = http_client(60)?;
    let status_url = format!("{}/{model}/requests/{request_id}/status", queue_base(api));
    let response_url = format!("{}/{model}/requests/{request_id}", queue_base(api));
    let deadline = Instant::now() + Duration::from_secs(wait_seconds.max(1));
    let mut last_status = String::from("UNKNOWN");
    while Instant::now() < deadline {
        let response = client
            .get(&status_url)
            .header("Authorization", &auth)
            .send()
            .map_err(|error| format!("fal status failed: {error}"))?;
        let code = response.status();
        let text = response.text().unwrap_or_default();
        if !code.is_success() {
            return Err(format!("fal status failed ({code}): {text}"));
        }
        let parsed: Value = serde_json::from_str(&text)
            .map_err(|error| format!("Could not parse fal status: {error}"))?;
        last_status = parsed
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        emit_progress(app, format!("fal {request_id}: {last_status}"));
        if last_status.eq_ignore_ascii_case("COMPLETED")
            || last_status.eq_ignore_ascii_case("FAILED")
        {
            break;
        }
        thread::sleep(Duration::from_secs(2));
    }

    if !last_status.eq_ignore_ascii_case("COMPLETED") {
        return Ok(json!({
            "prediction_id": prediction_id,
            "provider": "fal",
            "status": last_status,
            "output_url": null,
        })
        .to_string());
    }

    let response = client
        .get(&response_url)
        .header("Authorization", &auth)
        .send()
        .map_err(|error| format!("fal result failed: {error}"))?;
    let code = response.status();
    let text = response.text().unwrap_or_default();
    if !code.is_success() {
        return Err(format!("fal result failed ({code}): {text}"));
    }
    let parsed: Value = serde_json::from_str(&text)
        .map_err(|error| format!("Could not parse fal result: {error}"))?;
    Ok(json!({
        "prediction_id": prediction_id,
        "provider": "fal",
        "status": "COMPLETED",
        "output_url": first_url_in_value(&parsed),
        "raw": parsed,
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
    if parsed
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .eq_ignore_ascii_case("FAILED")
    {
        return Err(format!("fal job failed: {parsed}"));
    }
    let url = parsed
        .get("output_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("fal output not ready. Resume with `{prediction_id}`."))?;
    download_url(url, dest, project_root, app)
}
