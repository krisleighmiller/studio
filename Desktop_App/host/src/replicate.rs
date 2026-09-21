//! Replicate predictions adapter.
//! Model ids: `owner/name` (official) or `owner/name:version`.

use crate::ctx::HostCtx;
use crate::media_util::{
    download_url, emit_progress, file_to_data_uri, first_url_in_value, http_client, param_str,
};
use crate::settings::ApiEntry;
use serde_json::{json, Value};
use std::thread;
use std::time::{Duration, Instant};

fn base(api: &ApiEntry) -> String {
    let b = api.base_url.trim().trim_end_matches('/');
    if b.is_empty() {
        "https://api.replicate.com/v1".into()
    } else if b.ends_with("/v1") {
        b.to_string()
    } else {
        format!("{b}/v1")
    }
}

fn auth(api: &ApiEntry) -> Result<String, String> {
    let key = api.api_key.trim();
    if key.is_empty() {
        return Err("Add a Replicate API token in Settings first.".into());
    }
    Ok(format!("Bearer {key}"))
}

fn build_input(params: &Value, project_root: Option<&str>) -> Result<Value, String> {
    let mut input = match params {
        Value::Object(map) => Value::Object(map.clone()),
        _ => json!({}),
    };
    if let Some(obj) = input.as_object_mut() {
        if !obj.contains_key("prompt") {
            if let Some(prompt) = param_str(params, &["prompt", "text"]) {
                obj.insert("prompt".into(), json!(prompt));
            }
        }
        for key in [
            "image",
            "image_url",
            "start_image",
            "mask",
            "video",
            "audio",
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
    }
    Ok(input)
}

pub fn looks_like_model(model: &str) -> bool {
    let id = model.trim();
    id.contains('/') && !id.starts_with("fal-ai/") && !id.starts_with("http")
}

pub fn encode_id(id: &str) -> String {
    format!("replicate:{id}")
}

pub fn parse_id(prediction_id: &str) -> Option<String> {
    prediction_id.strip_prefix("replicate:").map(str::to_string)
}

pub fn generate(
    api: &ApiEntry,
    model: &str,
    params: &Value,
    project_root: Option<&str>,
    app: Option<&HostCtx>,
) -> Result<String, String> {
    let input = build_input(params, project_root)?;
    let auth = auth(api)?;
    let client = http_client(120)?;
    emit_progress(app, format!("Submitting Replicate prediction ({model})…"));

    let (url, body) = if let Some((owner_model, version)) = model.split_once(':') {
        if version.len() >= 16 && !version.contains('/') {
            (
                format!("{}/predictions", base(api)),
                json!({ "version": version, "input": input }),
            )
        } else {
            (
                format!("{}/models/{owner_model}/predictions", base(api)),
                json!({ "input": input }),
            )
        }
    } else {
        (
            format!("{}/models/{model}/predictions", base(api)),
            json!({ "input": input }),
        )
    };

    let response = client
        .post(&url)
        .header("Authorization", auth)
        .header("Content-Type", "application/json")
        .header("Prefer", "wait=60")
        .json(&body)
        .send()
        .map_err(|error| format!("Replicate request failed: {error}"))?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|error| format!("Could not read Replicate response: {error}"))?;
    if !status.is_success() {
        return Err(format!("Replicate failed ({status}): {text}"));
    }
    let parsed: Value = serde_json::from_str(&text)
        .map_err(|error| format!("Could not parse Replicate JSON: {error}\n{text}"))?;
    let id = parsed
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Replicate response missing id: {parsed}"))?;
    Ok(json!({
        "prediction_id": encode_id(id),
        "provider": "replicate",
        "status": parsed.get("status").cloned().unwrap_or(json!("starting")),
        "output_url": first_url_in_value(parsed.get("output").unwrap_or(&Value::Null)),
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
    let id =
        parse_id(prediction_id).ok_or_else(|| format!("Not a Replicate id: {prediction_id}"))?;
    let url = format!("{}/predictions/{id}", base(api));
    let auth = auth(api)?;
    let client = http_client(60)?;
    let deadline = Instant::now() + Duration::from_secs(wait_seconds.max(1));
    let mut last = Value::Null;
    loop {
        let response = client
            .get(&url)
            .header("Authorization", &auth)
            .send()
            .map_err(|error| format!("Replicate poll failed: {error}"))?;
        let status_code = response.status();
        let text = response.text().unwrap_or_default();
        if !status_code.is_success() {
            return Err(format!("Replicate poll failed ({status_code}): {text}"));
        }
        last = serde_json::from_str(&text)
            .map_err(|error| format!("Could not parse Replicate poll JSON: {error}"))?;
        let status = last
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        emit_progress(app, format!("Replicate {id}: {status}"));
        if matches!(
            status.as_str(),
            "succeeded" | "failed" | "canceled" | "cancelled"
        ) {
            break;
        }
        if Instant::now() >= deadline {
            break;
        }
        thread::sleep(Duration::from_secs(2));
    }
    Ok(json!({
        "prediction_id": prediction_id,
        "provider": "replicate",
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
    if parsed.get("status").and_then(|v| v.as_str()) == Some("failed") {
        return Err(format!("Replicate prediction failed: {parsed}"));
    }
    let url = parsed
        .get("output_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            format!("Replicate output not ready. Resume with prediction_id `{prediction_id}`.")
        })?;
    download_url(url, dest, project_root, app)
}
