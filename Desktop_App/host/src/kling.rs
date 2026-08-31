//! Kling AI Open Platform adapter (JWT AccessKey + SecretKey).
//!
//! Docs shape: HS256 JWT (`iss` = Access Key), then REST under
//! `https://api-singapore.klingai.com` (base URL overridable in Settings).

use crate::atlas;
use crate::ctx::HostCtx;
use crate::settings::ApiEntry;
use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{json, Value};
use sha2::Sha256;
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

fn emit_progress(app: Option<&HostCtx>, message: impl Into<String>) {
    if let Some(app) = app {
        app.emit("director-progress", message.into());
    }
}

fn b64url(data: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

/// Build a short-lived Bearer JWT for Kling Open Platform.
pub fn encode_jwt(access_key: &str, secret_key: &str) -> Result<String, String> {
    let access_key = access_key.trim();
    let secret_key = secret_key.trim();
    if access_key.is_empty() || secret_key.is_empty() {
        return Err("Kling requires Access Key and Secret Key in Settings.".into());
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let header = br#"{"alg":"HS256","typ":"JWT"}"#;
    let payload = json!({
        "iss": access_key,
        "exp": now + 1800,
        "nbf": now - 5,
    });
    let payload_bytes = serde_json::to_vec(&payload)
        .map_err(|error| format!("Could not encode Kling JWT payload: {error}"))?;
    let signing_input = format!("{}.{}", b64url(header), b64url(&payload_bytes));
    let mut mac = HmacSha256::new_from_slice(secret_key.as_bytes())
        .map_err(|error| format!("Invalid Kling Secret Key for HMAC: {error}"))?;
    mac.update(signing_input.as_bytes());
    let signature = b64url(&mac.finalize().into_bytes());
    Ok(format!("{signing_input}.{signature}"))
}

fn base_url(api: &ApiEntry) -> String {
    let base = api.base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        "https://api-singapore.klingai.com".into()
    } else {
        base.to_string()
    }
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent("VideoWorkbench/0.1 (desktop; kling-adapter)")
        .build()
        .map_err(|error| format!("Could not build HTTP client: {error}"))
}

fn auth_headers(api: &ApiEntry) -> Result<Vec<(String, String)>, String> {
    let token = encode_jwt(&api.api_key, &api.secret_key)?;
    Ok(vec![
        ("Authorization".into(), format!("Bearer {token}")),
        ("Content-Type".into(), "application/json".into()),
        ("Accept".into(), "application/json".into()),
    ])
}

fn request_json(
    method: reqwest::Method,
    url: &str,
    api: &ApiEntry,
    body: Option<&Value>,
) -> Result<Value, String> {
    let client = client()?;
    let mut req = client.request(method, url);
    for (key, value) in auth_headers(api)? {
        req = req.header(key, value);
    }
    if let Some(body) = body {
        req = req.json(body);
    }
    let response = req
        .send()
        .map_err(|error| format!("Kling request failed: {error}"))?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|error| format!("Could not read Kling response: {error}"))?;
    if !status.is_success() {
        return Err(format!("Kling request failed ({status}): {text}"));
    }
    let parsed: Value = serde_json::from_str(&text)
        .map_err(|error| format!("Could not parse Kling JSON: {error}\n{text}"))?;
    let code = parsed.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code != 0 {
        let message = parsed
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        return Err(format!("Kling API error ({code}): {message}"));
    }
    Ok(parsed)
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

fn param_num_as_string(params: &Value, keys: &[&str], default: &str) -> String {
    for key in keys {
        if let Some(value) = params.get(*key) {
            if let Some(n) = value.as_u64() {
                return n.to_string();
            }
            if let Some(n) = value.as_f64() {
                return (n.round() as i64).to_string();
            }
            if let Some(s) = value.as_str() {
                if !s.trim().is_empty() {
                    return s.trim().to_string();
                }
            }
        }
    }
    default.to_string()
}

fn resolve_image_input(
    raw: &str,
    project_root: Option<&str>,
) -> Result<String, String> {
    let raw = raw.trim();
    if raw.starts_with("http://") || raw.starts_with("https://") || raw.starts_with("data:") {
        return Ok(raw.to_string());
    }
    let path = if PathBuf::from(raw).is_absolute() {
        PathBuf::from(raw)
    } else if let Some(root) = project_root {
        PathBuf::from(root).join(raw)
    } else {
        PathBuf::from(raw)
    };
    if !path.is_file() {
        return Err(format!("Kling image path not found: {}", path.display()));
    }
    let bytes = fs::read(&path).map_err(|error| format!("Could not read image: {error}"))?;
    let mime = match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "image/png",
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

fn build_video_body(
    model: &str,
    params: &Value,
    project_root: Option<&str>,
) -> Result<(String, Value), String> {
    let prompt = param_str(params, &["prompt", "text"]).unwrap_or_default();
    let image = param_str(
        params,
        &[
            "image",
            "image_url",
            "imageUrl",
            "start_image",
            "startImage",
            "starting_visual_input",
            "video",
            "video_url",
            "videoUrl",
            "input_video",
        ],
    );
    let image_tail = param_str(params, &["image_tail", "imageTail", "end_image", "endImage"]);

    let mut body = serde_json::Map::new();
    body.insert("model_name".into(), json!(model));
    if !prompt.is_empty() {
        body.insert("prompt".into(), json!(prompt));
    }
    if let Some(neg) = param_str(params, &["negative_prompt", "negativePrompt"]) {
        body.insert("negative_prompt".into(), json!(neg));
    }
    body.insert(
        "aspect_ratio".into(),
        json!(param_str(params, &["aspect_ratio", "aspectRatio"]).unwrap_or_else(|| "16:9".into())),
    );
    body.insert(
        "duration".into(),
        json!(param_num_as_string(params, &["duration", "seconds"], "5")),
    );
    body.insert(
        "mode".into(),
        json!(param_str(params, &["mode", "quality"]).unwrap_or_else(|| "pro".into())),
    );
    if let Some(sound) = param_str(params, &["sound", "audio"]) {
        body.insert("sound".into(), json!(sound));
    }

    let endpoint = if let Some(image) = image {
        let image = resolve_image_input(&image, project_root)?;
        body.insert("image".into(), json!(image));
        if let Some(tail) = image_tail {
            body.insert(
                "image_tail".into(),
                json!(resolve_image_input(&tail, project_root)?),
            );
        }
        "image2video"
    } else {
        if prompt.is_empty() {
            return Err("Kling text2video requires params.prompt".into());
        }
        "text2video"
    };

    Ok((endpoint.to_string(), Value::Object(body)))
}

fn build_image_body(
    model: &str,
    params: &Value,
    project_root: Option<&str>,
) -> Result<Value, String> {
    let prompt = param_str(params, &["prompt", "text"])
        .ok_or_else(|| "Kling image generation requires params.prompt".to_string())?;
    let mut body = serde_json::Map::new();
    body.insert("model_name".into(), json!(model));
    body.insert("prompt".into(), json!(prompt));
    if let Some(neg) = param_str(params, &["negative_prompt", "negativePrompt"]) {
        body.insert("negative_prompt".into(), json!(neg));
    }
    body.insert(
        "aspect_ratio".into(),
        json!(param_str(params, &["aspect_ratio", "aspectRatio"]).unwrap_or_else(|| "1:1".into())),
    );
    body.insert(
        "resolution".into(),
        json!(param_str(params, &["resolution"]).unwrap_or_else(|| "1k".into())),
    );
    body.insert("n".into(), json!(1));
    if let Some(image) = param_str(
        params,
        &["image", "image_url", "imageUrl", "images", "input_image"],
    ) {
        // Single ref — if JSON array string, take first path-like entry later via resolve.
        let image = if image.starts_with('[') {
            serde_json::from_str::<Vec<String>>(&image)
                .ok()
                .and_then(|arr| arr.into_iter().next())
                .unwrap_or(image)
        } else {
            image
        };
        body.insert(
            "image".into(),
            json!(resolve_image_input(&image, project_root)?),
        );
        if let Some(reference) = param_str(params, &["image_reference", "imageReference"]) {
            body.insert("image_reference".into(), json!(reference));
        }
    }
    Ok(Value::Object(body))
}

pub fn encode_prediction_id(endpoint: &str, task_id: &str) -> String {
    format!("kling:{endpoint}:{task_id}")
}

pub fn parse_prediction_id(prediction_id: &str) -> Option<(String, String)> {
    let rest = prediction_id.strip_prefix("kling:")?;
    let (endpoint, task_id) = rest.split_once(':')?;
    if endpoint.is_empty() || task_id.is_empty() {
        return None;
    }
    Some((endpoint.to_string(), task_id.to_string()))
}

fn extract_task_id(parsed: &Value) -> Option<String> {
    parsed
        .pointer("/data/task_id")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

fn task_status(parsed: &Value) -> String {
    parsed
        .pointer("/data/task_status")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn extract_output_url(parsed: &Value) -> Option<String> {
    if let Some(url) = parsed
        .pointer("/data/task_result/videos/0/url")
        .and_then(|v| v.as_str())
    {
        return Some(url.to_string());
    }
    if let Some(url) = parsed
        .pointer("/data/task_result/images/0/url")
        .and_then(|v| v.as_str())
    {
        return Some(url.to_string());
    }
    None
}

pub fn generate_video(
    api: &ApiEntry,
    model: &str,
    params: &Value,
    project_root: Option<&str>,
    app: Option<&HostCtx>,
) -> Result<String, String> {
    let (endpoint, body) = build_video_body(model, params, project_root)?;
    emit_progress(app, format!("Submitting Kling {endpoint} ({model})…"));
    let url = format!("{}/v1/videos/{endpoint}", base_url(api));
    let parsed = request_json(reqwest::Method::POST, &url, api, Some(&body))?;
    let task_id = extract_task_id(&parsed)
        .ok_or_else(|| format!("Kling response missing task_id: {parsed}"))?;
    Ok(json!({
        "prediction_id": encode_prediction_id(&endpoint, &task_id),
        "provider": "kling",
        "endpoint": endpoint,
        "task_id": task_id,
        "status": task_status(&parsed),
        "raw": parsed,
    })
    .to_string())
}

pub fn generate_image(
    api: &ApiEntry,
    model: &str,
    params: &Value,
    project_root: Option<&str>,
    app: Option<&HostCtx>,
) -> Result<String, String> {
    let body = build_image_body(model, params, project_root)?;
    emit_progress(app, format!("Submitting Kling image generation ({model})…"));
    let url = format!("{}/v1/images/generations", base_url(api));
    let parsed = request_json(reqwest::Method::POST, &url, api, Some(&body))?;
    let task_id = extract_task_id(&parsed)
        .ok_or_else(|| format!("Kling response missing task_id: {parsed}"))?;
    Ok(json!({
        "prediction_id": encode_prediction_id("images/generations", &task_id),
        "provider": "kling",
        "endpoint": "images/generations",
        "task_id": task_id,
        "status": task_status(&parsed),
        "raw": parsed,
    })
    .to_string())
}

fn query_url(api: &ApiEntry, endpoint: &str, task_id: &str) -> String {
    if endpoint.starts_with("images/") {
        format!("{}/v1/{endpoint}/{task_id}", base_url(api))
    } else {
        format!("{}/v1/videos/{endpoint}/{task_id}", base_url(api))
    }
}

pub fn get_prediction(
    api: &ApiEntry,
    prediction_id: &str,
    wait_seconds: u64,
    app: Option<&HostCtx>,
) -> Result<String, String> {
    let (endpoint, task_id) = parse_prediction_id(prediction_id)
        .ok_or_else(|| format!("Not a Kling prediction id: {prediction_id}"))?;
    let url = query_url(api, &endpoint, &task_id);
    let deadline = Instant::now() + Duration::from_secs(wait_seconds.max(1));
    let mut last = request_json(reqwest::Method::GET, &url, api, None)?;
    loop {
        let status = task_status(&last);
        emit_progress(app, format!("Kling {task_id}: {status}"));
        if matches!(status.as_str(), "succeed" | "failed") {
            break;
        }
        if Instant::now() >= deadline {
            break;
        }
        thread::sleep(Duration::from_secs(3));
        last = request_json(reqwest::Method::GET, &url, api, None)?;
    }
    Ok(json!({
        "prediction_id": prediction_id,
        "provider": "kling",
        "status": task_status(&last),
        "output_url": extract_output_url(&last),
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
    let parsed: Value = serde_json::from_str(&polled)
        .map_err(|error| format!("Could not parse Kling poll result: {error}"))?;
    let status = parsed
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if status == "failed" {
        return Err(format!("Kling task failed: {parsed}"));
    }
    let Some(url) = parsed
        .get("output_url")
        .and_then(|v| v.as_str())
        .map(str::to_string)
    else {
        return Err(format!(
            "Kling task still processing or missing output URL. Resume with atlas_collect_media using prediction_id `{prediction_id}`."
        ));
    };
    atlas::download_atlas_media(&url, dest, project_root, app)
}
