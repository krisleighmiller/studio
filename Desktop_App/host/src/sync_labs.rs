//! Sync Labs (sync.so) lip-sync / talking-head adapter.
//!
//! Auth: `x-api-key`. Create via `POST /v2/generate`, poll `GET /v2/generate/{id}`.
//! Local files upload through the assets flow: presign → PUT → register.

use crate::ctx::HostCtx;
use crate::media_util::{download_url, emit_progress, http_client, param_str, resolve_path};
use crate::settings::{ApiEntry, CatalogModel};
use serde_json::{json, Value};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

fn base(api: &ApiEntry) -> String {
    let b = api.base_url.trim().trim_end_matches('/');
    if b.is_empty() {
        "https://api.sync.so".into()
    } else {
        b.to_string()
    }
}

fn api_key(api: &ApiEntry) -> Result<&str, String> {
    let key = api.api_key.trim();
    if key.is_empty() {
        return Err("Add a Sync Labs API key in Settings first.".into());
    }
    Ok(key)
}

pub fn looks_like_model(model: &str) -> bool {
    let id = model.trim().to_ascii_lowercase();
    matches!(
        id.as_str(),
        "sync-3"
            | "lipsync-2"
            | "lipsync-2-pro"
            | "lipsync-2-mini"
            | "lipsync-1.9.0-beta"
            | "react-1"
            | "appearence-1"
            | "appearance-1"
    ) || id.starts_with("lipsync-")
        || id.starts_with("sync-")
}

pub fn encode_id(generation_id: &str) -> String {
    format!("sync:{generation_id}")
}

pub fn parse_id(prediction_id: &str) -> Option<String> {
    prediction_id
        .strip_prefix("sync:")
        .map(str::to_string)
        .or_else(|| {
            // Bare UUID from Sync responses.
            let t = prediction_id.trim();
            if t.len() == 36 && t.chars().filter(|c| *c == '-').count() == 4 {
                Some(t.to_string())
            } else {
                None
            }
        })
}

fn asset_type_for_mime(mime: &str) -> Result<&'static str, String> {
    if mime.starts_with("video/") {
        Ok("VIDEO")
    } else if mime.starts_with("audio/") {
        Ok("AUDIO")
    } else if mime.starts_with("image/") {
        Ok("IMAGE")
    } else {
        Err(format!(
            "Sync Labs upload needs video/audio/image Content-Type; got {mime}"
        ))
    }
}

fn request_json(
    method: reqwest::Method,
    url: &str,
    api: &ApiEntry,
    body: Option<&Value>,
) -> Result<Value, String> {
    let key = api_key(api)?;
    let client = http_client(180)?;
    let mut req = client
        .request(method, url)
        .header("x-api-key", key)
        .header("Accept", "application/json");
    if let Some(body) = body {
        req = req.header("Content-Type", "application/json").json(body);
    }
    let response = req
        .send()
        .map_err(|error| format!("Sync Labs request failed: {error}"))?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|error| format!("Could not read Sync Labs response: {error}"))?;
    if !status.is_success() {
        return Err(format!("Sync Labs request failed ({status}): {text}"));
    }
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&text)
        .map_err(|error| format!("Could not parse Sync Labs JSON: {error}\n{text}"))
}

/// Upload a local file via Sync assets; returns a public HTTPS url.
fn upload_local_file(
    api: &ApiEntry,
    path: &Path,
    app: Option<&HostCtx>,
    role: crate::media_util::MediaRole,
) -> Result<String, String> {
    let (content_type, bytes) = crate::media_util::load_media_as(path, role)?;
    let size = bytes.len() as u64;
    if size == 0 {
        return Err(format!("File is empty: {}", path.display()));
    }
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("upload.bin")
        .to_string();
    if content_type == "application/octet-stream" {
        return Err(format!(
            "Unsupported Sync Labs media extension for {}: use mp4/webm/wav/mp3/png/jpg/…",
            path.display()
        ));
    }
    let asset_type = asset_type_for_mime(&content_type)?;

    emit_progress(
        app,
        format!("Sync Labs: requesting upload URL for {file_name} ({size} bytes)…"),
    );
    let presign = request_json(
        reqwest::Method::POST,
        &format!("{}/v2/assets/upload", base(api)),
        api,
        Some(&json!({
            "fileName": file_name,
            "contentType": content_type.clone(),
            "size": size,
        })),
    )?;
    let upload_url = presign
        .get("uploadUrl")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Sync Labs upload response missing uploadUrl: {presign}"))?;
    let public_url = presign
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Sync Labs upload response missing url: {presign}"))?
        .to_string();

    emit_progress(app, format!("Sync Labs: uploading {file_name}…"));
    let client = http_client(600)?;
    let put = client
        .put(upload_url)
        .header("Content-Type", content_type)
        .body(bytes)
        .send()
        .map_err(|error| format!("Sync Labs PUT upload failed: {error}"))?;
    let put_status = put.status();
    if !put_status.is_success() {
        let text = put.text().unwrap_or_default();
        return Err(format!(
            "Sync Labs PUT upload failed ({put_status}): {text}"
        ));
    }

    emit_progress(app, "Sync Labs: registering asset…");
    let _asset = request_json(
        reqwest::Method::POST,
        &format!("{}/v2/assets", base(api)),
        api,
        Some(&json!({
            "url": public_url,
            "type": asset_type,
            "inputType": "UPLOAD",
            "name": file_name,
        })),
    )?;
    Ok(public_url)
}

/// Resolve a local path or http(s) URL into a Sync-usable URL (uploads locals).
fn resolve_media_url(
    api: &ApiEntry,
    raw: &str,
    project_root: Option<&str>,
    app: Option<&HostCtx>,
    role: crate::media_util::MediaRole,
) -> Result<String, String> {
    let raw = raw.trim();
    if raw.starts_with("http://") || raw.starts_with("https://") {
        return Ok(raw.to_string());
    }
    if raw.starts_with("data:") {
        return Err(
            "Sync Labs does not accept data: URIs — pass a local path or https URL.".into(),
        );
    }
    let path = resolve_path(raw, project_root)?;
    upload_local_file(api, &path, app, role)
}

fn build_input(
    api: &ApiEntry,
    params: &Value,
    project_root: Option<&str>,
    app: Option<&HostCtx>,
) -> Result<Vec<Value>, String> {
    if let Some(arr) = params.get("input").and_then(|v| v.as_array()) {
        if !arr.is_empty() {
            return Ok(arr.clone());
        }
    }

    let video = param_str(
        params,
        &[
            "video",
            "video_path",
            "videoPath",
            "video_url",
            "videoUrl",
            "source_video",
            "sourceVideo",
        ],
    );
    let image = param_str(
        params,
        &[
            "image",
            "image_path",
            "imagePath",
            "image_url",
            "imageUrl",
            "source_image",
            "sourceImage",
        ],
    );
    let audio = param_str(
        params,
        &[
            "audio",
            "audio_path",
            "audioPath",
            "audio_url",
            "audioUrl",
            "source_audio",
            "sourceAudio",
            "voice",
            "voice_path",
            "voicePath",
        ],
    );

    let visual = match (video, image) {
        (Some(v), _) => ("video", v),
        (None, Some(i)) => ("image", i),
        (None, None) => {
            return Err(
                "Sync Labs lip sync needs params.video (or video_path) or params.image, plus params.audio (or audio_path)."
                    .into(),
            );
        }
    };
    let audio = audio.ok_or_else(|| {
        "Sync Labs lip sync needs params.audio / audio_path / audio_url.".to_string()
    })?;

    let visual_role = if visual.0 == "video" {
        crate::media_util::MediaRole::Video
    } else {
        crate::media_util::MediaRole::Image
    };
    let visual_url = resolve_media_url(api, &visual.1, project_root, app, visual_role)?;
    let audio_url = resolve_media_url(
        api,
        &audio,
        project_root,
        app,
        crate::media_util::MediaRole::Audio,
    )?;
    Ok(vec![
        json!({ "type": visual.0, "url": visual_url }),
        json!({ "type": "audio", "url": audio_url }),
    ])
}

pub fn generate_video(
    api: &ApiEntry,
    model: &str,
    params: &Value,
    project_root: Option<&str>,
    app: Option<&HostCtx>,
) -> Result<String, String> {
    let input = build_input(api, params, project_root, app)?;
    let mut body = json!({
        "model": model.trim(),
        "input": input,
    });
    if let Some(options) = params.get("options") {
        body["options"] = options.clone();
    }
    if let Some(segments) = params.get("segments") {
        body["segments"] = segments.clone();
    }
    if let Some(name) = param_str(params, &["outputFileName", "output_file_name", "filename"]) {
        body["outputFileName"] = json!(name);
    }
    if let Some(webhook) = param_str(params, &["webhookUrl", "webhook_url"]) {
        body["webhookUrl"] = json!(webhook);
    }
    if let Some(project_id) = param_str(params, &["projectId", "project_id"]) {
        body["projectId"] = json!(project_id);
    }

    emit_progress(
        app,
        format!("Sync Labs: submitting lip sync with model `{model}`…"),
    );
    let created = request_json(
        reqwest::Method::POST,
        &format!("{}/v2/generate", base(api)),
        api,
        Some(&body),
    )?;
    let id = created
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Sync Labs generate response missing id: {created}"))?;
    let status = created
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("PENDING");
    let output_url = created
        .get("outputUrl")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    Ok(json!({
        "prediction_id": encode_id(id),
        "provider": "sync-labs",
        "status": status,
        "output_url": output_url,
        "raw": created,
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
        parse_id(prediction_id).ok_or_else(|| format!("Not a Sync Labs id: {prediction_id}"))?;
    let url = format!("{}/v2/generate/{id}?include=progress", base(api));
    let deadline = Instant::now() + Duration::from_secs(wait_seconds.max(1));
    let last = loop {
        let current = request_json(reqwest::Method::GET, &url, api, None)?;
        let status = current
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("UNKNOWN")
            .to_ascii_uppercase();
        let progress = current
            .get("progress_percent")
            .and_then(|v| v.as_f64())
            .map(|p| format!(" ({p:.0}%)"))
            .unwrap_or_default();
        emit_progress(app, format!("Sync Labs {id}: {status}{progress}"));
        if matches!(status.as_str(), "COMPLETED" | "FAILED" | "REJECTED")
            || Instant::now() >= deadline
        {
            break current;
        }
        thread::sleep(Duration::from_secs(3));
    };

    let status = last.get("status").cloned().unwrap_or(json!("unknown"));
    let output_url = last
        .get("outputUrl")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let error = last.get("error").cloned();

    Ok(json!({
        "prediction_id": encode_id(&id),
        "provider": "sync-labs",
        "status": status,
        "output_url": output_url,
        "error": error,
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
    if matches!(status.as_str(), "FAILED" | "REJECTED") {
        let err = parsed
            .get("error")
            .cloned()
            .unwrap_or(json!("unknown error"));
        return Err(format!("Sync Labs generation {status}: {err}"));
    }
    let url = parsed
        .get("output_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            format!("Sync Labs output not ready. Resume with `{prediction_id}` when COMPLETED.")
        })?;
    download_url(url, dest, project_root, app)
}

pub fn fetch_catalog(api_key: &str, base_url: &str) -> Result<Vec<CatalogModel>, String> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err("Enter a Sync Labs API key before fetching the catalog.".into());
    }
    let base = {
        let b = base_url.trim().trim_end_matches('/');
        if b.is_empty() {
            "https://api.sync.so".to_string()
        } else {
            b.to_string()
        }
    };
    let client = http_client(60)?;
    let response = client
        .get(format!("{base}/v2/models"))
        .header("x-api-key", key)
        .header("Accept", "application/json")
        .send()
        .map_err(|error| format!("Sync Labs catalog request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|error| format!("Could not read Sync Labs catalog: {error}"))?;
    if !status.is_success() {
        return Err(format!("Sync Labs catalog failed ({status}): {body}"));
    }
    let parsed: Value = serde_json::from_str(&body)
        .map_err(|error| format!("Could not parse Sync Labs catalog JSON: {error}"))?;
    let items = if let Some(arr) = parsed.as_array() {
        arr.clone()
    } else if let Some(arr) = parsed.get("data").and_then(|v| v.as_array()) {
        arr.clone()
    } else if let Some(arr) = parsed.get("models").and_then(|v| v.as_array()) {
        arr.clone()
    } else {
        return Err(format!("Unexpected Sync Labs models response: {body}"));
    };

    let mut out = Vec::new();
    for item in items {
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .or_else(|| item.get("id").and_then(|v| v.as_str()))
            .unwrap_or("")
            .trim();
        if name.is_empty() {
            continue;
        }
        let display = item
            .get("description")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .map(|s| format!("{name} — {s}"))
            .unwrap_or_else(|| name.to_string());
        out.push(CatalogModel {
            id: name.to_string(),
            display_name: display,
            model_type: "Video".into(),
            organization: Some("sync".into()),
            profile: None,
            tags: Vec::new(),
        });
    }
    if out.is_empty() {
        return Err("Sync Labs returned no models.".into());
    }
    Ok(out)
}
