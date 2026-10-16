//! Route generative media tools to the Settings API that owns the chosen model.

use crate::atlas;
use crate::ctx::HostCtx;
use crate::fal;
use crate::gemini_images;
use crate::kling;
use crate::luma;
use crate::openai_images;
use crate::replicate;
use crate::runway;
use crate::settings::{self, ApiEntry};
use crate::stability;
use crate::sync_labs;
use serde_json::{json, Value};

fn emit_progress(app: Option<&HostCtx>, message: impl Into<String>) {
    if let Some(app) = app {
        app.emit("director-progress", message.into());
    }
}

fn arg_str(args: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = args.get(*key).and_then(|v| v.as_str()) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Find which Settings API should run this model.
pub fn resolve_api_for_model(
    app: &HostCtx,
    model: &str,
    preferred_api_id: Option<&str>,
) -> Result<ApiEntry, String> {
    let apis = settings::list_apis(app)?;
    if let Some(api_id) = preferred_api_id {
        if let Some(api) = apis.iter().find(|api| api.id == api_id) {
            return Ok(api.clone());
        }
    }

    if let Ok(settings) = settings::load_settings(app.clone()) {
        if let Some(arr) = settings.get("apis").and_then(|v| v.as_array()) {
            for entry in arr {
                let id = entry.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let catalog = entry
                    .get("catalog")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let hit = catalog.iter().any(|m| {
                    m.get("id")
                        .and_then(|v| v.as_str())
                        .map(|mid| mid == model)
                        .unwrap_or(false)
                });
                if hit {
                    if let Some(api) = apis.iter().find(|a| a.id == id) {
                        return Ok(api.clone());
                    }
                }
            }
        }
    }

    if model.to_ascii_lowercase().starts_with("kling") {
        return settings::find_api_by_kind(app, "kling");
    }
    if openai_images::looks_like_openai_image_model(model) {
        return settings::find_api_by_kind(app, "openai-chat");
    }
    if gemini_images::looks_like_image_model(model) {
        return settings::find_api_by_kind(app, "gemini");
    }
    if fal::looks_like_model(model) {
        return settings::find_api_by_kind(app, "fal");
    }
    if stability::looks_like_model(model) {
        return settings::find_api_by_kind(app, "stability");
    }
    if runway::looks_like_model(model) {
        return settings::find_api_by_kind(app, "runway");
    }
    if luma::looks_like_model(model) {
        return settings::find_api_by_kind(app, "luma");
    }
    if sync_labs::looks_like_model(model) {
        return settings::find_api_by_kind(app, "sync-labs");
    }
    if replicate::looks_like_model(model) {
        return settings::find_api_by_kind(app, "replicate");
    }

    settings::find_api_by_kind(app, "atlas-cloud")
}

fn resolve_api_for_prediction(app: &HostCtx, prediction_id: &str) -> Result<ApiEntry, String> {
    if kling::parse_prediction_id(prediction_id).is_some() {
        return settings::find_api_by_kind(app, "kling");
    }
    if prediction_id.starts_with("openai:") {
        return settings::find_api_by_kind(app, "openai-chat");
    }
    if prediction_id.starts_with("gemini:") || prediction_id.starts_with("stability:") {
        // Sync jobs — collect shouldn't normally need these, but map for safety.
        if prediction_id.starts_with("gemini:") {
            return settings::find_api_by_kind(app, "gemini");
        }
        return settings::find_api_by_kind(app, "stability");
    }
    if replicate::parse_id(prediction_id).is_some() {
        return settings::find_api_by_kind(app, "replicate");
    }
    if fal::parse_id(prediction_id).is_some() {
        return settings::find_api_by_kind(app, "fal");
    }
    if runway::parse_id(prediction_id).is_some() {
        return settings::find_api_by_kind(app, "runway");
    }
    if luma::parse_id(prediction_id).is_some() {
        return settings::find_api_by_kind(app, "luma");
    }
    if sync_labs::parse_id(prediction_id).is_some() && prediction_id.starts_with("sync:") {
        return settings::find_api_by_kind(app, "sync-labs");
    }
    settings::find_api_by_kind(app, "atlas-cloud")
}

pub fn run_media_tool(
    name: &str,
    args: &Value,
    project_root: Option<&str>,
    app: &HostCtx,
) -> Option<Result<String, String>> {
    match name {
        "atlas_generate_image" | "atlas_generate_video" | "atlas_generate_audio" => {
            Some(generate(name, args, project_root, app))
        }
        "atlas_get_prediction" => Some(get_prediction(args, app)),
        "atlas_collect_image"
        | "atlas_collect_media"
        | "atlas_collect_video"
        | "atlas_collect_audio"
        | "watch_atlas_video" => Some(collect(name, args, project_root, app)),
        _ => None,
    }
}

fn generate(
    name: &str,
    args: &Value,
    project_root: Option<&str>,
    app: &HostCtx,
) -> Result<String, String> {
    let root = crate::session::command_root(project_root)?;
    let model = arg_str(args, &["model"]).ok_or_else(|| "missing model".to_string())?;
    let api_id = arg_str(args, &["api_id", "apiId"]);
    let api = resolve_api_for_model(app, &model, api_id.as_deref())?;
    // The selected API kind, not a caller-supplied provider or display label,
    // determines the registry price. Reservation precedes every adapter call.
    let capability = match name {
        "atlas_generate_image" => "image.text_to_image",
        "atlas_generate_video" => "video.text_only",
        "atlas_generate_audio" => "audio.tts_profile",
        _ => return Err(format!("unknown billable operation: {name}")),
    };
    let confirmation = arg_str(args, &["spend_confirmation", "spendConfirmation"]);
    // Fingerprint the resolved operation and request, excluding only the one-use
    // authorization token. No prompt or credential is persisted in the ledger.
    let mut request_args = args.clone();
    if let Some(object) = request_args.as_object_mut() {
        object.remove("spend_confirmation");
        object.remove("spendConfirmation");
    }
    let request_bytes = serde_json::to_vec(&json!([name, api.kind, request_args]))
        .map_err(|error| format!("cannot fingerprint media request: {error}"))?;
    let request_hash = crate::workflow::spend::fingerprint(&request_bytes)?;
    let numeric = |key: &str, ceiling: u64| {
        args.get("params")
            .and_then(|p| p.get(key))
            .and_then(Value::as_u64)
            .filter(|n| *n > 0 && *n <= ceiling)
            .map(|n| n as u32)
    };
    let metadata = crate::workflow::state::SpendRequestMetadata {
        request_hash: request_hash.clone(),
        disclosure: None,
        duration_seconds: numeric("duration", 3600),
        width: numeric("width", 16384),
        height: numeric("height", 16384),
        prediction_id: None,
    };
    let result = crate::workflow::spend::run_scoped_with_submission(
        &root,
        &api.kind,
        "media",
        name,
        capability,
        &model,
        confirmation.as_deref(),
        arg_str(args, &["retry_of", "retryOf"]).as_deref(),
        Some(metadata),
        None, // Registry estimates are not contractually verified maxima.
        |_, submitted| generate_unchecked(name, args, root.to_str(), app, &api, submitted),
    );
    // Only a host-confirmed soft-limit gate exposes a quote. A failed replay
    // must not open another prompt or mint another token automatically.
    if matches!(&result, Err(error) if error == "budget confirmation required; generation blocked")
        && confirmation.is_none()
    {
        if let Ok(quote) = crate::workflow::spend::quote_scoped(
            &root, &api.kind, "media", name, capability, &model,
        ) {
            app.emit(
                "spend-confirmation-required",
                json!({
                    "quote": quote,
                    "projectRoot": root,
                    "args": args,
                }),
            );
        }
    }
    if matches!(&result, Err(error) if error == "unbounded request approval required")
        && confirmation.is_none()
    {
        if let Ok(quote) = crate::workflow::spend::quote_unbounded_scoped(&root, &request_hash) {
            app.emit(
                "spend-confirmation-required",
                json!({ "quote": quote, "provider": api.kind, "category": "media", "operation": name,
                        "model": model, "projectRoot": root, "args": args }),
            );
        }
    }
    result
}

fn generate_unchecked(
    name: &str,
    args: &Value,
    project_root: Option<&str>,
    app: &HostCtx,
    api: &ApiEntry,
    submitted: &dyn Fn(&str) -> Result<(), String>,
) -> Result<String, String> {
    let model = arg_str(args, &["model"]).ok_or_else(|| "missing model".to_string())?;
    let params = args.get("params").cloned().unwrap_or(json!({}));
    let save_path = arg_str(args, &["save_path", "savePath"]);
    let kind = match name {
        "atlas_generate_video" => "video",
        "atlas_generate_audio" => "audio",
        _ => "image",
    };
    let default_wait = match kind {
        "video" => 300,
        "audio" => 180,
        _ => 240,
    };
    let wait = args
        .get("wait_seconds")
        .or_else(|| args.get("waitSeconds"))
        .and_then(|v| v.as_u64())
        .unwrap_or(default_wait);

    emit_progress(
        Some(app),
        format!(
            "Routing {kind} generate for `{model}` via {} ({})…",
            api.name, api.kind
        ),
    );

    let generated = match (api.kind.as_str(), kind) {
        ("kling", "video") => {
            kling::generate_video(&api, &model, &params, project_root, Some(app))?
        }
        ("kling", "image") => {
            kling::generate_image(&api, &model, &params, project_root, Some(app))?
        }
        ("openai-chat", "image") => openai_images::generate_image(
            &api,
            &model,
            &params,
            project_root,
            save_path.as_deref(),
            Some(app),
        )?,
        ("gemini", "image") => gemini_images::generate_image(
            &api,
            &model,
            &params,
            project_root,
            save_path.as_deref(),
            Some(app),
        )?,
        ("stability", "image") => stability::generate_image(
            &api,
            &model,
            &params,
            project_root,
            save_path.as_deref(),
            Some(app),
        )?,
        ("replicate", _) => replicate::generate(&api, &model, &params, project_root, Some(app))?,
        ("fal", _) => fal::generate(&api, &model, &params, project_root, Some(app))?,
        ("runway", "video") => {
            runway::generate_video(&api, &model, &params, project_root, Some(app))?
        }
        ("luma", "video") => luma::generate_video(&api, &model, &params, project_root, Some(app))?,
        ("sync-labs", "video") => {
            sync_labs::generate_video(&api, &model, &params, project_root, Some(app))?
        }
        ("atlas-cloud", _) => {
            let key = api.api_key.trim();
            if key.is_empty() {
                return Err("Add an Atlas Cloud API key in Settings first.".into());
            }
            match kind {
                "video" => atlas::generate_video(key, &model, &params)?,
                "audio" => atlas::generate_audio(key, &model, &params)?,
                _ => atlas::generate_image(key, &model, &params)?,
            }
        }
        (proto, media) => {
            return Err(format!(
                "Protocol “{proto}” on API “{}” does not support {media} generation.",
                api.name
            ));
        }
    };

    // Sync adapters already applied save_path.
    if matches!(api.kind.as_str(), "openai-chat" | "gemini" | "stability") {
        return Ok(generated);
    }

    let parsed: Value = serde_json::from_str(&generated).unwrap_or(json!({ "raw": generated }));
    if let Some(id) = parsed.get("prediction_id").and_then(Value::as_str) {
        // Recovery metadata must be durable before polling. If persistence fails,
        // stop after the accepted submission and require reconciliation; never regenerate.
        submitted(id)?;
    }
    let Some(save_path) = save_path else {
        return Ok(generated);
    };

    // Record as soon as the adapter returns, before a best-effort download or GET.
    // Replicate Prefer:wait may already include output_url
    if let Some(url) = parsed.get("output_url").and_then(|v| v.as_str()) {
        if !url.is_empty() {
            match crate::media_util::download_url(url, &save_path, project_root, Some(app)) {
                Ok(saved) => {
                    return Ok(json!({
                        "submitted": parsed,
                        "saved": serde_json::from_str::<Value>(&saved).unwrap_or(json!({ "raw": saved })),
                        "note": format!("Generated and downloaded via {}.", api.name),
                    })
                    .to_string());
                }
                Err(_) => { /* fall through to collect */ }
            }
        }
    }

    let Some(prediction_id) = parsed
        .get("prediction_id")
        .and_then(|v| v.as_str())
        .map(str::to_string)
    else {
        return Ok(format!(
            "{generated}\n\n(note: save_path was set but no prediction_id was returned)"
        ));
    };

    emit_progress(
        Some(app),
        format!("Waiting on {prediction_id}, then downloading…"),
    );

    // Seedream/edit and video jobs often exceed a single patience window. Keep
    // collecting while the prediction is still processing instead of bouncing
    // a half-finished job back to the chat as "finished".
    let max_windows = match kind {
        "video" => 3,
        "audio" => 2,
        _ => 3,
    };
    let mut last_error = String::new();
    for window in 1..=max_windows {
        match collect_by_api(&api, &prediction_id, &save_path, project_root, wait, app) {
            Ok(saved) => {
                return Ok(json!({
                    "submitted": {
                        "prediction_id": prediction_id,
                        "status": "accepted",
                    },
                    "saved": serde_json::from_str::<Value>(&saved).unwrap_or(json!({ "raw": saved })),
                    "note": format!(
                        "Generated and downloaded this {kind} asset via {} (wait window {window}/{max_windows}).",
                        api.name
                    ),
                })
                .to_string());
            }
            Err(error) => {
                last_error = error.clone();
                // A terminal provider failure is not "still processing":
                // stop polling and fail the operation instead of leaving the
                // workflow job pending on a prediction that will never finish.
                if error.starts_with("provider-failed:") {
                    break;
                }
                let lower = error.to_ascii_lowercase();
                let retriable = lower.contains("not ready")
                    || lower.contains("processing")
                    || lower.contains("patience")
                    || lower.contains("submitted");
                if !retriable || window == max_windows {
                    break;
                }
                emit_progress(
                    Some(app),
                    format!(
                        "Prediction {prediction_id} still processing after wait window {window}/{max_windows}; continuing to poll…"
                    ),
                );
            }
        }
    }

    // Terminal provider failure: report Err so the spend ledger records a
    // failed (unverified) attempt and callers can fail the workflow job,
    // rather than presenting a dead prediction as "submitted, resume later".
    if last_error.starts_with("provider-failed:") {
        return Err(last_error);
    }

    Ok(json!({
        "submitted": {
            "prediction_id": prediction_id,
            "status": parsed.get("status").cloned().unwrap_or(json!("processing")),
        },
        "saved": null,
        "error": last_error,
        "note": "Generation was submitted but download did not finish after extended polling. Resume with atlas_collect_media using this prediction_id — do not re-generate unless it failed.",
    })
    .to_string())
}

fn collect_by_api(
    api: &ApiEntry,
    prediction_id: &str,
    path: &str,
    project_root: Option<&str>,
    wait: u64,
    app: &HostCtx,
) -> Result<String, String> {
    match api.kind.as_str() {
        "kling" => kling::collect_media(api, prediction_id, path, project_root, wait, Some(app)),
        "replicate" => {
            replicate::collect_media(api, prediction_id, path, project_root, wait, Some(app))
        }
        "fal" => fal::collect_media(api, prediction_id, path, project_root, wait, Some(app)),
        "runway" => runway::collect_media(api, prediction_id, path, project_root, wait, Some(app)),
        "luma" => luma::collect_media(api, prediction_id, path, project_root, wait, Some(app)),
        "sync-labs" => {
            sync_labs::collect_media(api, prediction_id, path, project_root, wait, Some(app))
        }
        "openai-chat" | "gemini" | "stability" => Err(
            "This provider finishes during generate — re-run with save_path if the file is missing."
                .into(),
        ),
        _ => atlas::collect_media(
            api.api_key.trim(),
            prediction_id,
            path,
            project_root,
            wait,
            Some(app),
        ),
    }
}

/// Read-only provider status lookup by immutable spend job id. Never submits or downloads.
/// Arguments: project_root: Option<&str>, job_id: &str, app: &HostCtx.
/// Returns JSON {job_id, status}; no provider response body or credentials.
pub fn recovery_status(
    project_root: Option<&str>,
    job_id: &str,
    app: &HostCtx,
) -> Result<String, String> {
    let root = crate::session::command_root(project_root)?;
    recovery_status_with(&root, job_id, |provider, prediction_id| {
        let api = resolve_api_for_prediction(app, prediction_id)?;
        if api.kind != provider {
            return Err("stored provider and prediction id disagree".into());
        }
        get_prediction(
            &json!({"prediction_id": prediction_id, "wait_seconds": 0}),
            app,
        )
    })
}

pub(crate) fn recovery_status_with(
    root: &std::path::Path,
    job_id: &str,
    get: impl FnOnce(&str, &str) -> Result<String, String>,
) -> Result<String, String> {
    let (provider, prediction_id) = crate::workflow::spend::recovery_job(root, job_id)?;
    let response = get(&provider, &prediction_id)
        .map_err(|_| "provider status lookup failed; no generation was submitted".to_string())?;
    let body: Value = serde_json::from_str(&response)
        .map_err(|_| "provider returned invalid status JSON".to_string())?;
    let status = body
        .get("status")
        .and_then(Value::as_str)
        .filter(|s| {
            matches!(
                *s,
                "starting"
                    | "queued"
                    | "pending"
                    | "processing"
                    | "running"
                    | "succeeded"
                    | "completed"
                    | "failed"
                    | "canceled"
                    | "cancelled"
            )
        })
        .unwrap_or("unknown");
    Ok(json!({"job_id": job_id, "status": status}).to_string())
}

fn get_prediction(args: &Value, app: &HostCtx) -> Result<String, String> {
    let prediction_id = arg_str(args, &["prediction_id", "predictionId"])
        .ok_or_else(|| "missing prediction_id".to_string())?;
    let wait = args
        .get("wait_seconds")
        .or_else(|| args.get("waitSeconds"))
        .and_then(|v| v.as_u64())
        .unwrap_or(60);
    let api = resolve_api_for_prediction(app, &prediction_id)?;
    match api.kind.as_str() {
        "kling" => kling::get_prediction(&api, &prediction_id, wait, Some(app)),
        "replicate" => replicate::get_prediction(&api, &prediction_id, wait, Some(app)),
        "fal" => fal::get_prediction(&api, &prediction_id, wait, Some(app)),
        "runway" => runway::get_prediction(&api, &prediction_id, wait, Some(app)),
        "luma" => luma::get_prediction(&api, &prediction_id, wait, Some(app)),
        "sync-labs" => sync_labs::get_prediction(&api, &prediction_id, wait, Some(app)),
        "openai-chat" | "gemini" | "stability" => Ok(json!({
            "prediction_id": prediction_id,
            "provider": api.kind,
            "status": "completed",
            "note": "This provider completes inline during generate.",
        })
        .to_string()),
        _ => atlas::get_prediction(api.api_key.trim(), &prediction_id, wait, Some(app)),
    }
}

fn collect(
    name: &str,
    args: &Value,
    project_root: Option<&str>,
    app: &HostCtx,
) -> Result<String, String> {
    let prediction_id = arg_str(args, &["prediction_id", "predictionId"])
        .ok_or_else(|| "missing prediction_id".to_string())?;
    let path = arg_str(
        args,
        &["path", "destination", "dest", "output_path", "outputPath"],
    )
    .ok_or_else(|| "missing path".to_string())?;
    let wait = args
        .get("wait_seconds")
        .or_else(|| args.get("waitSeconds"))
        .and_then(|v| v.as_u64())
        .or_else(|| {
            args.get("timeout_minutes")
                .or_else(|| args.get("timeoutMinutes"))
                .and_then(|v| v.as_u64())
                .map(|m| (m * 60).min(300))
        })
        .unwrap_or(240);
    let api = resolve_api_for_prediction(app, &prediction_id)?;

    // Resume collects also get multi-window patience — Atlas edit models can sit
    // in `processing` well past a single 90s window.
    const MAX_WINDOWS: u32 = 3;
    let mut last_error = String::new();
    let mut result = Err("collect did not run".into());
    for window in 1..=MAX_WINDOWS {
        result = collect_by_api(&api, &prediction_id, &path, project_root, wait, app);
        match &result {
            Ok(_) => break,
            Err(error) => {
                last_error = error.clone();
                let lower = error.to_ascii_lowercase();
                let retriable = lower.contains("not ready")
                    || lower.contains("processing")
                    || lower.contains("patience")
                    || lower.contains("submitted");
                if !retriable || window == MAX_WINDOWS {
                    break;
                }
                emit_progress(
                    Some(app),
                    format!(
                        "Collect {prediction_id}: still processing after window {window}/{MAX_WINDOWS}; continuing…"
                    ),
                );
            }
        }
    }
    if result.is_err() && !last_error.is_empty() {
        result = Err(last_error);
    }

    if name == "watch_atlas_video" {
        result.map(|saved| {
            json!({
                "watcher": "mapped_to_collect_media",
                "note": "Host polled/downloaded via collect instead of OpenCode watcher.",
                "saved": serde_json::from_str::<Value>(&saved).unwrap_or(json!({ "raw": saved })),
            })
            .to_string()
        })
    } else {
        result
    }
}
