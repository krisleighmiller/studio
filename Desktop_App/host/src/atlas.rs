//! Atlas Cloud **direct API** adapter for Director tool calls (stills, model sheets, scene video/audio).
//! Atlas is the near-term test provider; tool names are adapter-local, not a product MCP dependency.
//! Model IDs should come from Settings `capabilityDefaults`, not marketplace discovery.

use crate::ctx::HostCtx;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const API_BASE: &str = "https://api.atlascloud.ai/api/v1";
const BALANCE_URL: &str = "https://api.atlascloud.ai/public/v1/balance";
const USER_AGENT: &str = "VideoWorkbench/0.1 (desktop; atlas-tools)";
/// Default patience for a single prediction poll/collect window. The media router
/// may run multiple windows while status stays `processing` (Seedream/edit, video).
const DEFAULT_WAIT_SECONDS: u64 = 240;

fn emit_progress(app: Option<&HostCtx>, message: impl Into<String>) {
    if let Some(app) = app {
        app.emit("director-progress", message.into());
    }
}

fn client(timeout_secs: u64) -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|error| format!("Could not build HTTP client: {error}"))
}

fn auth_get(api_key: &str, url: &str, timeout_secs: u64) -> Result<Value, String> {
    let response = crate::http_util::get_with_retry(&client(timeout_secs)?, url, Some(api_key), 3)?;
    parse_json_response(response)
}

fn auth_post_json(
    api_key: &str,
    url: &str,
    body: &Value,
    timeout_secs: u64,
) -> Result<Value, String> {
    let response =
        // Billable POSTs must not retry invisibly inside one spend reservation.
        crate::http_util::post_json_with_retry(&client(timeout_secs)?, url, api_key, body, 1)?;
    parse_json_response(response)
}

fn parse_json_response(response: reqwest::blocking::Response) -> Result<Value, String> {
    let status = response.status();
    let text = response
        .text()
        .map_err(|error| format!("Could not read response body: {error}"))?;
    if !status.is_success() {
        return Err(format!("Atlas request failed ({status}): {text}"));
    }
    serde_json::from_str(&text).map_err(|error| format!("Could not parse JSON: {error}\n{text}"))
}

fn fetch_models_catalog() -> Result<Vec<Value>, String> {
    let url = format!("{API_BASE}/models");
    let response = crate::http_util::get_with_retry(&client(60)?, &url, None, 3)?;
    let parsed = parse_json_response(response)?;
    let items = parsed
        .get("data")
        .and_then(|value| value.as_array())
        .ok_or_else(|| "Unexpected models catalog shape.".to_string())?;
    Ok(items
        .iter()
        .filter(|item| {
            item.get("display_console")
                .and_then(|v| v.as_bool())
                .unwrap_or(true)
        })
        .cloned()
        .collect())
}

fn model_id(item: &Value) -> &str {
    item.get("model").and_then(|v| v.as_str()).unwrap_or("")
}

fn format_model_line(item: &Value) -> String {
    let id = model_id(item);
    let name = item
        .get("displayName")
        .and_then(|v| v.as_str())
        .unwrap_or(id);
    let model_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("?");
    let org = item
        .get("organization")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let price = item
        .pointer("/price/actual/base_price")
        .and_then(|v| v.as_str())
        .or_else(|| {
            item.pointer("/price/actual/base_price")
                .and_then(|v| v.as_str())
        });
    let price_bit = price.map(|p| format!(" · ${p}")).unwrap_or_default();
    format!(
        "- `{id}` — {name} ({model_type}{org_bit}){price_bit}",
        org_bit = if org.is_empty() {
            String::new()
        } else {
            format!(" · {org}")
        }
    )
}

pub fn list_models(type_filter: Option<&str>) -> Result<String, String> {
    let catalog = fetch_models_catalog()?;
    let filter = type_filter.map(|t| t.trim()).filter(|t| !t.is_empty());
    let mut lines = vec![
        "# Atlas models".to_string(),
        filter
            .map(|t| format!("Filter: `{t}`"))
            .unwrap_or_else(|| "Filter: (all)".into()),
        String::new(),
    ];
    let mut count = 0usize;
    for item in &catalog {
        let model_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(wanted) = filter {
            if !model_type.eq_ignore_ascii_case(wanted) {
                continue;
            }
        }
        lines.push(format_model_line(item));
        count += 1;
        if count >= 200 {
            lines.push(
                "\n…truncated at 200 models. Narrow with type=Image or atlas_search_docs.".into(),
            );
            break;
        }
    }
    if count == 0 {
        lines.push("(no models matched)".into());
    } else {
        lines.insert(2, format!("Count: {count}"));
    }
    Ok(lines.join("\n"))
}

pub fn search_docs(query: &str) -> Result<String, String> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return Err("query is required".into());
    }
    let tokens: Vec<&str> = q.split_whitespace().collect();
    let catalog = fetch_models_catalog()?;
    let mut matches = Vec::new();
    for item in catalog {
        let hay = [
            model_id(&item),
            item.get("displayName")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            item.get("profile").and_then(|v| v.as_str()).unwrap_or(""),
            item.get("organization")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            item.get("familyName")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            item.get("familyDisplayName")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            &item
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|tags| {
                    tags.iter()
                        .filter_map(|t| t.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default(),
        ]
        .join(" ")
        .to_ascii_lowercase();
        if tokens.iter().all(|token| hay.contains(token)) {
            matches.push(item);
        }
    }
    if matches.is_empty() {
        return Ok(format!("No models matched query `{query}`."));
    }
    let mut lines = vec![
        format!("# Search: {query}"),
        format!("Matches: {}", matches.len()),
        String::new(),
    ];
    for item in matches.iter().take(40) {
        lines.push(format_model_line(item));
        if let Some(profile) = item.get("profile").and_then(|v| v.as_str()) {
            let short = if profile.len() > 180 {
                format!("{}…", &profile[..180])
            } else {
                profile.to_string()
            };
            lines.push(format!("  {short}"));
        }
    }
    if matches.len() > 40 {
        lines.push(format!(
            "\n…truncated; {} more matches.",
            matches.len() - 40
        ));
    }
    Ok(lines.join("\n"))
}

fn find_model_entry(model: &str) -> Result<Value, String> {
    let wanted = model.trim();
    let catalog = fetch_models_catalog()?;
    catalog
        .into_iter()
        .find(|item| model_id(item) == wanted)
        .ok_or_else(|| {
            format!(
                "Model not found in catalog: {wanted}. Use atlas_list_models / atlas_search_docs."
            )
        })
}

fn fetch_text(url: &str) -> Result<String, String> {
    let response = crate::http_util::get_with_retry(&client(60)?, url, None, 3)?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|error| format!("Could not read {url}: {error}"))?;
    if !status.is_success() {
        return Err(format!("Fetch failed ({status}) for {url}: {text}"));
    }
    Ok(text)
}

pub fn get_model_info(model: &str) -> Result<String, String> {
    let entry = find_model_entry(model)?;
    let id = model_id(&entry).to_string();
    let mut sections = vec![
        format!("# {id}"),
        format!(
            "- Display name: {}",
            entry
                .get("displayName")
                .and_then(|v| v.as_str())
                .unwrap_or(id.as_str())
        ),
        format!(
            "- Type: {}",
            entry.get("type").and_then(|v| v.as_str()).unwrap_or("?")
        ),
        format!(
            "- Organization: {}",
            entry
                .get("organization")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
        ),
        format!("- Playground: https://www.atlascloud.ai/models/{id}"),
    ];
    if let Some(price) = entry.get("price") {
        sections.push(format!(
            "- Price JSON: {}",
            serde_json::to_string(price).unwrap_or_else(|_| "{}".into())
        ));
    }
    if let Some(profile) = entry.get("profile").and_then(|v| v.as_str()) {
        sections.push(String::new());
        sections.push(profile.to_string());
    }

    if let Some(schema_url) = entry.get("schema").and_then(|v| v.as_str()) {
        sections.push(String::new());
        sections.push("## Input schema (OpenAPI)".into());
        match fetch_text(schema_url) {
            Ok(raw) => match serde_json::from_str::<Value>(&raw) {
                Ok(schema) => {
                    let pretty = summarize_input_schema(&schema);
                    sections.push(pretty);
                    sections.push(String::new());
                    sections.push("### Raw schema (truncated)".into());
                    let dump = serde_json::to_string_pretty(&schema).unwrap_or(raw);
                    if dump.len() > 12_000 {
                        sections.push(format!("{}…\n[truncated]", &dump[..12_000]));
                    } else {
                        sections.push(dump);
                    }
                }
                Err(_) => sections.push(raw),
            },
            Err(error) => sections.push(format!("(could not fetch schema: {error})")),
        }
    }

    Ok(sections.join("\n"))
}

fn summarize_input_schema(schema: &Value) -> String {
    // Atlas schemas vary; try common OpenAPI shapes.
    let props = schema
        .pointer("/components/schemas/Input/properties")
        .or_else(|| schema.pointer("/components/schemas/input/properties"))
        .or_else(|| schema.pointer("/properties"))
        .and_then(|v| v.as_object());
    let required = schema
        .pointer("/components/schemas/Input/required")
        .or_else(|| schema.pointer("/components/schemas/input/required"))
        .or_else(|| schema.pointer("/required"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_else(|| "(see raw)".into());

    let mut lines = vec![format!("Required fields: {required}"), String::new()];
    if let Some(props) = props {
        for (name, prop) in props {
            let ty = prop.get("type").and_then(|v| v.as_str()).unwrap_or("any");
            let desc = prop
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let enums = prop
                .get("enum")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    let joined = arr
                        .iter()
                        .filter_map(|v| v.as_str())
                        .take(12)
                        .collect::<Vec<_>>()
                        .join(" | ");
                    if joined.is_empty() {
                        String::new()
                    } else {
                        format!(" — options: {joined}")
                    }
                })
                .unwrap_or_default();
            let default = prop
                .get("default")
                .map(|v| format!(" (default: {v})"))
                .unwrap_or_default();
            lines.push(format!(
                "- `{name}` ({ty}){default}{enums}{}",
                if desc.is_empty() {
                    String::new()
                } else {
                    format!(" — {desc}")
                }
            ));
        }
    } else {
        lines.push("(Could not locate properties; see raw schema.)".into());
    }
    lines.join("\n")
}

fn input_schema_props(schema: &Value) -> Option<&serde_json::Map<String, Value>> {
    schema
        .pointer("/components/schemas/Input/properties")
        .or_else(|| schema.pointer("/components/schemas/input/properties"))
        .or_else(|| schema.pointer("/properties"))
        .and_then(|v| v.as_object())
}

fn input_schema_required(schema: &Value) -> Vec<String> {
    schema
        .pointer("/components/schemas/Input/required")
        .or_else(|| schema.pointer("/components/schemas/input/required"))
        .or_else(|| schema.pointer("/required"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .filter(|name| *name != "model")
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Normalize a media value into an https URL. Accepts bare URLs or upload JSON blobs
/// that embed `download_url` (specialists / LLMs sometimes pass the whole upload result).
fn coerce_media_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Some(trimmed.to_string());
    }
    if trimmed.starts_with('{') {
        if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
            for pointer in ["/download_url", "/url", "/data/download_url", "/data/url"] {
                if let Some(url) = parsed.pointer(pointer).and_then(|v| v.as_str()) {
                    let u = url.trim();
                    if u.starts_with("http://") || u.starts_with("https://") {
                        return Some(u.to_string());
                    }
                }
            }
        }
    }
    None
}

fn collect_url_list(params: &Map<String, Value>, keys: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let push = |out: &mut Vec<String>, raw: &str| {
        if let Some(url) = coerce_media_url(raw) {
            if !out.iter().any(|x| x == &url) {
                out.push(url);
            }
        }
    };
    for key in keys {
        match params.get(*key) {
            Some(Value::String(s)) => push(&mut out, s),
            Some(Value::Array(items)) => {
                for item in items {
                    if let Some(s) = item.as_str() {
                        push(&mut out, s);
                    } else if let Some(obj) = item.as_object() {
                        for k in ["download_url", "url"] {
                            if let Some(s) = obj.get(k).and_then(|v| v.as_str()) {
                                push(&mut out, s);
                            }
                        }
                    }
                }
            }
            Some(Value::Object(obj)) => {
                for k in ["download_url", "url"] {
                    if let Some(s) = obj.get(k).and_then(|v| v.as_str()) {
                        push(&mut out, s);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn parse_aspect_ratio(raw: &str) -> Option<(u32, u32)> {
    let raw = raw.trim().trim_matches('"').to_ascii_lowercase();
    let raw = match raw.as_str() {
        "vertical" | "portrait" => "9:16",
        "horizontal" | "landscape" => "16:9",
        "square" => "1:1",
        other => other,
    };
    let mut parts = raw.split(|c: char| c == ':' || c == '/' || c == 'x' || c == '*');
    let width: u32 = parts.next()?.trim().parse().ok()?;
    let height: u32 = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() || width == 0 || height == 0 {
        return None;
    }
    Some((width, height))
}

fn parse_pixel_size(raw: &str) -> Option<(u32, u32)> {
    let raw = raw.trim().to_ascii_lowercase().replace('×', "*");
    parse_aspect_ratio(&raw)
}

fn push_size_choices(choices: &mut Vec<String>, value: &Value) {
    let Some(items) = value.as_array() else {
        return;
    };
    for item in items {
        if let Some(text) = item.as_str() {
            if parse_pixel_size(text).is_some() {
                choices.push(text.to_string());
            }
        }
    }
}

fn size_choices(prop: &Value) -> Vec<String> {
    let mut choices = Vec::new();
    if let Some(value) = prop.get("enum") {
        push_size_choices(&mut choices, value);
    }
    if choices.is_empty() {
        if let Some(value) = prop.pointer("/items/enum") {
            push_size_choices(&mut choices, value);
        }
    }
    choices
}

fn synthesized_size(aspect: (u32, u32)) -> String {
    let long = 2048u32;
    let (width, height) = if aspect.0 >= aspect.1 {
        let height = ((long as u64 * aspect.1 as u64) / aspect.0 as u64) as u32;
        (long, height.max(16))
    } else {
        let width = ((long as u64 * aspect.0 as u64) / aspect.1 as u64) as u32;
        (width.max(16), long)
    };
    format!("{}*{}", (width / 16).max(1) * 16, (height / 16).max(1) * 16)
}

/// Closest allowed `WIDTH*HEIGHT`. Equal ratios prefer the larger preset.
fn best_size_for_aspect(aspect: (u32, u32), options: &[String]) -> String {
    let target = aspect.0 as f64 / aspect.1 as f64;
    let mut best: Option<(f64, u64, String)> = None;
    for option in options {
        let Some((width, height)) = parse_pixel_size(option) else {
            continue;
        };
        let error = ((width as f64 / height as f64) - target).abs();
        let pixels = width as u64 * height as u64;
        let replace = match &best {
            None => true,
            Some((best_error, best_pixels, _)) => {
                error + 1e-6 < *best_error
                    || ((error - best_error).abs() < 1e-4 && pixels > *best_pixels)
            }
        };
        if replace {
            best = Some((error, pixels, format!("{width}*{height}")));
        }
    }
    best.map(|(_, _, size)| size)
        .unwrap_or_else(|| synthesized_size(aspect))
}

/// Map common aliases (`image`, `media_files`, …) onto the model's real schema fields
/// and drop keys the schema does not accept. Host specialists and LLMs often shotgun
/// aliases; Grok reference-to-video wants `image_urls`, not `image`/`media_files`.
fn remap_params_to_schema(
    params: &Map<String, Value>,
    props: &Map<String, Value>,
) -> Map<String, Value> {
    let mut out = Map::new();

    for (key, value) in params {
        if key == "model" {
            continue;
        }
        if props.contains_key(key) {
            out.insert(key.clone(), value.clone());
        }
    }

    let image_urls = collect_url_list(
        params,
        &[
            "image_urls",
            "images",
            "media_files",
            "image",
            "image_url",
            "reference_image",
            "reference_images",
            "start_image",
            "start_image_url",
            "starting_visual_input",
        ],
    );
    let video_urls = collect_url_list(
        params,
        &["video_urls", "videos", "video", "video_url", "source_video"],
    );
    let audio_urls = collect_url_list(params, &["audio_urls", "audios", "audio", "audio_url"]);

    if !image_urls.is_empty() {
        if props.contains_key("image_urls") {
            out.insert("image_urls".into(), json!(image_urls.clone()));
        } else if props.contains_key("images") {
            out.insert("images".into(), json!(image_urls.clone()));
        } else if props.contains_key("media_files") {
            out.insert("media_files".into(), json!(image_urls.clone()));
        }
        let first = &image_urls[0];
        for key in [
            "image",
            "image_url",
            "reference_image",
            "start_image",
            "start_image_url",
            "starting_visual_input",
        ] {
            if props.contains_key(key) && !out.contains_key(key) {
                out.insert(key.into(), json!(first));
            }
        }
    }

    if !video_urls.is_empty() {
        if props.contains_key("video_urls") {
            out.insert("video_urls".into(), json!(video_urls.clone()));
        }
        for key in ["video", "video_url"] {
            if props.contains_key(key) && !out.contains_key(key) {
                out.insert(key.into(), json!(&video_urls[0]));
            }
        }
    }

    if !audio_urls.is_empty() {
        if props.contains_key("audio_urls") {
            out.insert("audio_urls".into(), json!(audio_urls.clone()));
        }
        for key in ["audio", "audio_url"] {
            if props.contains_key(key) && !out.contains_key(key) {
                out.insert(key.into(), json!(&audio_urls[0]));
            }
        }
    }

    // Edit models such as Seedream and Flux take `size` (`WIDTH*HEIGHT`) and have
    // no `aspect_ratio` field. Leaving the ratio unmapped makes the model pick
    // its own canvas, usually 16:9 or square, even when the project is 9:16.
    if params.contains_key("aspect_ratio")
        && !props.contains_key("aspect_ratio")
        && props.contains_key("size")
        && !out.contains_key("size")
    {
        if let Some(aspect) = params
            .get("aspect_ratio")
            .and_then(|value| value.as_str())
            .and_then(parse_aspect_ratio)
        {
            let options = props.get("size").map(size_choices).unwrap_or_default();
            let size = best_size_for_aspect(aspect, &options);
            out.insert("size".into(), json!(size));
        }
    }

    // Grok reference-to-video documents <IMAGE_N> tags; add them if absent.
    if props.contains_key("image_urls") && !image_urls.is_empty() {
        if let Some(Value::String(prompt)) = out.get("prompt").cloned() {
            let lower = prompt.to_ascii_lowercase();
            if !lower.contains("<image_") {
                let tags = (1..=image_urls.len())
                    .map(|i| format!("<IMAGE_{i}>"))
                    .collect::<Vec<_>>()
                    .join(" ");
                out.insert(
                    "prompt".into(),
                    json!(format!("{prompt}\n\nUse reference images {tags}.")),
                );
            }
        }
    }

    out
}

fn normalize_generate_params(model: &str, params: &Value) -> Result<Value, String> {
    let obj = params
        .as_object()
        .ok_or_else(|| "params must be a JSON object".to_string())?;
    let entry = match find_model_entry(model) {
        Ok(entry) => entry,
        Err(error) => {
            eprintln!("workbench-host: skip param remap for {model}: {error}");
            return Ok(params.clone());
        }
    };
    let Some(schema_url) = entry.get("schema").and_then(|v| v.as_str()) else {
        return Ok(params.clone());
    };
    let raw = match fetch_text(schema_url) {
        Ok(raw) => raw,
        Err(error) => {
            eprintln!("workbench-host: skip param remap (schema fetch failed): {error}");
            return Ok(params.clone());
        }
    };
    let schema: Value = match serde_json::from_str(&raw) {
        Ok(schema) => schema,
        Err(error) => {
            eprintln!("workbench-host: skip param remap (parse failed): {error}");
            return Ok(params.clone());
        }
    };
    let Some(props) = input_schema_props(&schema) else {
        return Ok(params.clone());
    };
    Ok(Value::Object(remap_params_to_schema(obj, props)))
}

/// Returns Ok(None) when schema could not be loaded (caller may still submit).
/// Returns Ok(Some(())) when schema checks passed.
/// Returns Err when params are definitely invalid for the model.
fn validate_generate_params(model: &str, params: &Value) -> Result<Option<()>, String> {
    let entry = match find_model_entry(model) {
        Ok(entry) => entry,
        Err(error) => {
            // Catalog fetch / missing model should not be narrated as "tools disabled".
            eprintln!("workbench-host: skip local schema check for {model}: {error}");
            return Ok(None);
        }
    };
    let Some(schema_url) = entry.get("schema").and_then(|v| v.as_str()) else {
        return Ok(Some(()));
    };
    let raw = match fetch_text(schema_url) {
        Ok(raw) => raw,
        Err(error) => {
            eprintln!("workbench-host: skip local schema check (schema fetch failed): {error}");
            return Ok(None);
        }
    };
    let schema: Value = match serde_json::from_str(&raw) {
        Ok(schema) => schema,
        Err(error) => {
            eprintln!("workbench-host: skip local schema check (parse failed): {error}");
            return Ok(None);
        }
    };
    let Some(props) = input_schema_props(&schema) else {
        return Ok(Some(()));
    };
    let obj = params
        .as_object()
        .ok_or_else(|| "params must be a JSON object".to_string())?;

    let mut unknown = Vec::new();
    for key in obj.keys() {
        if key == "model" {
            continue;
        }
        if !props.contains_key(key) {
            unknown.push(key.clone());
        }
    }
    if !unknown.is_empty() {
        return Err(format!(
            "HOST PRE-SUBMIT VALIDATION: unknown field(s) for {model}: {}. \
Call atlas_get_model_info, fix params, and emit the same generate tool again \
(atlas_generate_image / atlas_generate_video / atlas_generate_audio). \
Media tools ARE available — credits were NOT spent.",
            unknown.join(", ")
        ));
    }

    for req in input_schema_required(&schema) {
        if !obj.contains_key(&req) {
            let hint = if req == "video_clips" {
                " This model requires video input and is unsuitable for still model sheets. Change Settings → image.character_reference / image.image_to_image to a still edit model (e.g. google/nano-banana-2/edit), or for text-only packs use image.text_to_image."
            } else {
                ""
            };
            return Err(format!(
                "HOST PRE-SUBMIT VALIDATION: missing required field `{req}` for {model}.{hint} \
Media tools ARE available — fix params and emit the generate tool again. Credits were NOT spent."
            ));
        }
    }
    Ok(Some(()))
}

fn generate_media(
    api_key: &str,
    endpoint: &str,
    model: &str,
    params: &Value,
) -> Result<String, String> {
    let model = model.trim();
    if model.is_empty() {
        return Err("model is required".into());
    }
    // Remap aliases → schema fields and drop unsupported keys before validate/submit.
    let params = normalize_generate_params(model, params)?;
    // Hard-fail only on definite param mistakes. If catalog/schema is unreachable,
    // still submit — Atlas validates server-side.
    let _ = validate_generate_params(model, &params)?;

    let mut body = match params {
        Value::Object(map) => Value::Object(map.clone()),
        _ => return Err("params must be a JSON object".into()),
    };
    if let Some(obj) = body.as_object_mut() {
        obj.insert("model".into(), Value::String(model.to_string()));
    }

    let parsed = auth_post_json(
        api_key,
        &format!("{API_BASE}/model/{endpoint}"),
        &body,
        120,
    ).map_err(|error| {
        format!(
            "HOST SUBMIT FAILED for {endpoint} ({model}): {error}. \
Media tools ARE available — retry atlas_generate_image / atlas_generate_video / atlas_generate_audio. \
Credits may or may not have been spent; check Atlas prediction history before re-submitting the same job."
        )
    })?;
    let id = parsed
        .pointer("/data/id")
        .and_then(|v| v.as_str())
        .or_else(|| parsed.get("id").and_then(|v| v.as_str()))
        .unwrap_or("");
    if id.is_empty() {
        return Ok(serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| parsed.to_string()));
    }
    Ok(json!({
        "prediction_id": id,
        "status": parsed.pointer("/data/status").cloned().unwrap_or(Value::String("starting".into())),
        "raw": parsed,
    })
    .to_string())
}

pub fn generate_image(api_key: &str, model: &str, params: &Value) -> Result<String, String> {
    generate_media(api_key, "generateImage", model, params)
}

pub fn generate_video(api_key: &str, model: &str, params: &Value) -> Result<String, String> {
    generate_media(api_key, "generateVideo", model, params)
}

pub fn generate_audio(api_key: &str, model: &str, params: &Value) -> Result<String, String> {
    generate_media(api_key, "generateAudio", model, params)
}

fn prediction_status(parsed: &Value) -> String {
    parsed
        .pointer("/data/status")
        .and_then(|v| v.as_str())
        .or_else(|| parsed.get("status").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn is_terminal_status(status: &str) -> bool {
    matches!(
        status,
        "completed"
            | "succeeded"
            | "success"
            | "failed"
            | "canceled"
            | "cancelled"
            | "timeout"
            | "error"
    )
}

fn is_success_status(status: &str) -> bool {
    matches!(status, "completed" | "succeeded" | "success")
}

/// Pull the first usable output URL from varied Atlas prediction shapes.
fn extract_output_url(parsed: &Value) -> Option<String> {
    let candidates = [
        parsed.pointer("/data/outputs/0"),
        parsed.pointer("/outputs/0"),
        parsed.pointer("/data/output"),
        parsed.pointer("/output"),
        parsed.pointer("/data/urls/get"),
    ];
    for candidate in candidates {
        match candidate {
            Some(Value::String(url)) if url.starts_with("http") => return Some(url.clone()),
            Some(Value::Object(map)) => {
                for key in ["url", "download_url", "href", "image"] {
                    if let Some(Value::String(url)) = map.get(key) {
                        if url.starts_with("http") {
                            return Some(url.clone());
                        }
                    }
                }
            }
            Some(Value::Array(arr)) => {
                if let Some(Value::String(url)) = arr.first() {
                    if url.starts_with("http") {
                        return Some(url.clone());
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn shape_prediction(prediction_id: &str, parsed: &Value) -> Value {
    let status = prediction_status(parsed);
    let output_url = extract_output_url(parsed);
    json!({
        "prediction_id": prediction_id,
        "status": status,
        "output_url": output_url,
        "ready_to_download": output_url.is_some(),
        "raw": parsed,
    })
}

pub fn get_prediction(
    api_key: &str,
    prediction_id: &str,
    wait_seconds: u64,
    app: Option<&HostCtx>,
) -> Result<String, String> {
    let prediction_id = prediction_id.trim();
    if prediction_id.is_empty() {
        return Err("prediction_id is required".into());
    }
    let url = format!("{API_BASE}/model/prediction/{prediction_id}");
    let poll_every = Duration::from_secs(8);
    let deadline = std::time::Instant::now() + Duration::from_secs(wait_seconds);
    let mut consecutive_errors = 0u32;
    let mut last: Value = Value::Null;
    let mut polls = 0u32;

    emit_progress(
        app,
        format!("Polling prediction {prediction_id} (wait up to {wait_seconds}s)…"),
    );

    loop {
        match auth_get(api_key, &url, 60) {
            Ok(parsed) => {
                consecutive_errors = 0;
                last = parsed;
                polls += 1;
                let status = prediction_status(&last);
                if polls == 1 || polls % 3 == 0 || is_terminal_status(&status) {
                    emit_progress(
                        app,
                        format!("Prediction {prediction_id}: {status} (poll {polls})"),
                    );
                }
                if is_terminal_status(&status)
                    || wait_seconds == 0
                    || std::time::Instant::now() >= deadline
                {
                    return Ok(serde_json::to_string_pretty(&shape_prediction(
                        prediction_id,
                        &last,
                    ))
                    .unwrap_or_else(|_| last.to_string()));
                }
            }
            Err(error) => {
                consecutive_errors += 1;
                emit_progress(
                    app,
                    format!("Prediction poll error ({consecutive_errors}/3): {error}"),
                );
                if consecutive_errors >= 3 || wait_seconds == 0 {
                    return Err(error);
                }
            }
        }
        if std::time::Instant::now() >= deadline {
            let shaped = if last.is_null() {
                json!({
                    "prediction_id": prediction_id,
                    "status": "submitted",
                    "output_url": null,
                    "ready_to_download": false,
                    "note": "Patience window expired while still non-terminal. Call atlas_collect_image / atlas_get_prediction again to resume.",
                })
            } else {
                let mut shaped = shape_prediction(prediction_id, &last);
                if let Some(obj) = shaped.as_object_mut() {
                    obj.insert(
                        "note".into(),
                        Value::String(
                            "Patience window expired while still non-terminal. Resume with atlas_collect_image."
                                .into(),
                        ),
                    );
                }
                shaped
            };
            return Ok(serde_json::to_string_pretty(&shaped).unwrap_or_else(|_| shaped.to_string()));
        }
        thread::sleep(poll_every);
    }
}

/// Poll a prediction until ready, then download the output to `path` in one host step.
pub fn collect_media(
    api_key: &str,
    prediction_id: &str,
    dest: &str,
    project_root: Option<&str>,
    wait_seconds: u64,
    app: Option<&HostCtx>,
) -> Result<String, String> {
    let prediction_id = prediction_id.trim();
    if prediction_id.is_empty() {
        return Err("prediction_id is required".into());
    }

    // Resume: nonempty local file already present.
    if let Ok(path) = resolve_dest_path(dest, project_root) {
        if path.is_file() {
            if let Ok(meta) = fs::metadata(&path) {
                if meta.len() > 0 {
                    emit_progress(app, format!("Already on disk: {}", path.display()));
                    return Ok(json!({
                        "saved": true,
                        "already_existed": true,
                        "path": path.to_string_lossy(),
                        "bytes": meta.len(),
                        "prediction_id": prediction_id,
                    })
                    .to_string());
                }
            }
        }
    }

    let polled = get_prediction(api_key, prediction_id, wait_seconds, app)?;
    let parsed: Value = serde_json::from_str(&polled)
        .map_err(|error| format!("Could not parse prediction result: {error}"))?;
    let status = parsed
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if !is_success_status(&status) {
        return Err(format!(
            "Prediction {prediction_id} not ready to download (status={status}). Resume later with the same prediction_id."
        ));
    }
    let Some(url) = parsed
        .get("output_url")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|u| !u.is_empty())
    else {
        return Err(format!(
            "Prediction {prediction_id} completed but no output_url was found. Raw was returned to chat — inspect and call download_atlas_media with the URL."
        ));
    };

    // Match destination extension to returned asset when needed.
    let mut dest = dest.to_string();
    let url_ext = Path::new(&url)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let dest_ext = Path::new(&dest)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let swappable = matches!(
        url_ext.as_str(),
        "jpg" | "jpeg" | "png" | "webp" | "mp4" | "webm" | "mov" | "wav" | "mp3" | "m4a" | "ogg"
    );
    if swappable
        && !dest_ext.is_empty()
        && dest_ext != url_ext
        && !(dest_ext == "jpg" && url_ext == "jpeg")
        && !(dest_ext == "jpeg" && url_ext == "jpg")
    {
        if let Some(stripped) = dest.rsplit_once('.') {
            dest = format!("{}.{}", stripped.0, url_ext);
            emit_progress(
                app,
                format!("Adjusted destination extension to .{url_ext} for Atlas output"),
            );
        }
    }

    emit_progress(app, format!("Downloading {prediction_id} → {dest}"));
    let saved = download_atlas_media(&url, &dest, project_root, app)?;
    let mut saved_value: Value =
        serde_json::from_str(&saved).unwrap_or_else(|_| json!({ "saved": true, "path": dest }));
    if let Some(obj) = saved_value.as_object_mut() {
        obj.insert("prediction_id".into(), Value::String(prediction_id.into()));
        obj.insert("output_url".into(), Value::String(url));
        obj.insert("status".into(), Value::String(status));
    }
    Ok(serde_json::to_string_pretty(&saved_value).unwrap_or_else(|_| saved))
}

#[allow(dead_code)]
pub fn collect_image(
    api_key: &str,
    prediction_id: &str,
    dest: &str,
    project_root: Option<&str>,
    wait_seconds: u64,
    app: Option<&HostCtx>,
) -> Result<String, String> {
    collect_media(
        api_key,
        prediction_id,
        dest,
        project_root,
        wait_seconds,
        app,
    )
}

pub fn upload_media(
    api_key: &str,
    file_path: &str,
    project_root: Option<&str>,
    role: crate::media_util::MediaRole,
) -> Result<String, String> {
    let path = resolve_existing_file(file_path, project_root, role)?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("upload.bin")
        .to_string();
    let mime = mime_for_path(&path);
    let bytes =
        fs::read(&path).map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    if bytes.is_empty() {
        return Err(format!("File is empty: {}", path.display()));
    }

    let part = reqwest::blocking::multipart::Part::bytes(bytes)
        .file_name(file_name.clone())
        .mime_str(mime)
        .map_err(|error| format!("Could not build multipart part: {error}"))?;
    let form = reqwest::blocking::multipart::Form::new().part("file", part);

    let response = client(180)?
        .post(format!("{API_BASE}/model/uploadMedia"))
        .header("Authorization", format!("Bearer {api_key}"))
        .multipart(form)
        .send()
        .map_err(|error| format!("Upload failed: {error}"))?;
    let parsed = parse_json_response(response)?;
    let url = parsed
        .pointer("/data/download_url")
        .and_then(|v| v.as_str())
        .or_else(|| parsed.pointer("/data/url").and_then(|v| v.as_str()))
        .unwrap_or("");
    if url.is_empty() {
        return Ok(serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| parsed.to_string()));
    }
    Ok(json!({
        "download_url": url,
        "filename": parsed.pointer("/data/filename").cloned().unwrap_or(Value::String(file_name)),
        "size": parsed.pointer("/data/size").cloned().unwrap_or(Value::Null),
        "local_path": path.to_string_lossy(),
        "note": "Temporary Atlas upload URL — copy exactly into generation params; do not rewrite.",
    })
    .to_string())
}

pub fn get_balance(api_key: &str) -> Result<String, String> {
    let parsed = auth_get(api_key, BALANCE_URL, 30)?;
    Ok(serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| parsed.to_string()))
}

pub fn download_atlas_media(
    url: &str,
    dest: &str,
    project_root: Option<&str>,
    app: Option<&HostCtx>,
) -> Result<String, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("url is required".into());
    }
    if !url.starts_with("https://") {
        return Err("Downloads must use https.".into());
    }
    let path = resolve_dest_path(dest, project_root)?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(
        ext.as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "webp"
            | "mp4"
            | "webm"
            | "mov"
            | "mkv"
            | "wav"
            | "mp3"
            | "m4a"
            | "ogg"
            | "flac"
            | "aac"
    ) {
        return Err(
            "Allowed extensions: .png, .jpg, .jpeg, .webp, .mp4, .webm, .mov, .mkv, .wav, .mp3, .m4a, .ogg, .flac, .aac"
                .into(),
        );
    }
    if path.exists() {
        if let Ok(meta) = fs::metadata(&path) {
            if meta.len() > 0 {
                return Ok(json!({
                    "saved": true,
                    "already_existed": true,
                    "path": path.to_string_lossy(),
                    "bytes": meta.len(),
                    "note": "File already on disk; left unchanged.",
                })
                .to_string());
            }
        }
        // Empty leftover from a failed prior attempt — replace it.
        let _ = fs::remove_file(&path);
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create parent dirs: {error}"))?;
        }
    }

    emit_progress(app, format!("Downloading media → {}", path.display()));
    let response = client(300)?
        .get(url)
        .header("Accept", "*/*")
        .send()
        .map_err(|error| crate::http_util::format_reqwest_error("Download failed", error))?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().unwrap_or_default();
        return Err(format!("Download failed ({status}): {text}"));
    }
    let tmp = path.with_extension(format!(
        "{}.partial",
        path.extension().and_then(|e| e.to_str()).unwrap_or("bin")
    ));
    let written = crate::http_util::write_response_limited(
        response,
        &tmp,
        crate::http_util::max_download_bytes(),
    )?;
    if written == 0 {
        let _ = fs::remove_file(&tmp);
        return Err("Download returned empty body".into());
    }
    fs::rename(&tmp, &path).map_err(|error| {
        let _ = fs::remove_file(&tmp);
        format!("Could not finalize {}: {error}", path.display())
    })?;
    let meta = fs::metadata(&path).map_err(|error| format!("Saved but could not stat: {error}"))?;
    if meta.len() == 0 {
        let _ = fs::remove_file(&path);
        return Err("Saved file was empty; removed.".into());
    }
    emit_progress(
        app,
        format!("Saved {} ({} bytes)", path.display(), meta.len()),
    );
    Ok(json!({
        "saved": true,
        "path": path.to_string_lossy(),
        "bytes": meta.len(),
    })
    .to_string())
}

#[allow(dead_code)]
pub fn download_atlas_image(
    url: &str,
    dest: &str,
    project_root: Option<&str>,
    app: Option<&HostCtx>,
) -> Result<String, String> {
    download_atlas_media(url, dest, project_root, app)
}

fn resolve_existing_file(
    raw: &str,
    project_root: Option<&str>,
    role: crate::media_util::MediaRole,
) -> Result<PathBuf, String> {
    let path = crate::media_util::resolve_path(raw, project_root)?;
    crate::media_util::ensure_media_file_as(&path, role)?;
    Ok(path)
}

fn resolve_dest_path(raw: &str, project_root: Option<&str>) -> Result<PathBuf, String> {
    crate::media_util::resolve_write_path(raw, project_root)
}

fn mime_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "mov" => "video/quicktime",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "aac" => "audio/aac",
        _ => "application/octet-stream",
    }
}

/// Dispatch an Atlas-named tool. Returns Ok(result) or Err(message).
pub fn run_atlas_tool(
    name: &str,
    args: &Value,
    api_key: &str,
    project_root: Option<&str>,
    app: Option<&HostCtx>,
) -> Option<Result<String, String>> {
    match name {
        "atlas_list_models" => {
            let ty = args.get("type").and_then(|v| v.as_str());
            Some(list_models(ty))
        }
        "atlas_search_docs" => {
            let Some(query) = args.get("query").and_then(|v| v.as_str()) else {
                return Some(Err("missing query".into()));
            };
            Some(search_docs(query))
        }
        "atlas_get_model_info" => {
            let Some(model) = args.get("model").and_then(|v| v.as_str()) else {
                return Some(Err("missing model".into()));
            };
            Some(get_model_info(model))
        }
        "atlas_generate_image" | "atlas_generate_video" | "atlas_generate_audio" => {
            let Some(model) = args.get("model").and_then(|v| v.as_str()) else {
                return Some(Err("missing model".into()));
            };
            let params = args.get("params").cloned().unwrap_or(json!({}));
            let kind = match name {
                "atlas_generate_video" => "video",
                "atlas_generate_audio" => "audio",
                _ => "image",
            };
            // Keep waits short so the desk can return progress; resume with
            // atlas_collect_media for long video jobs instead of freezing the UI.
            let default_wait = match kind {
                "video" => 90,
                "audio" => 90,
                _ => DEFAULT_WAIT_SECONDS,
            };
            emit_progress(app, format!("Submitting {kind} generation ({model})…"));
            let generated = match name {
                "atlas_generate_video" => generate_video(api_key, model, &params),
                "atlas_generate_audio" => generate_audio(api_key, model, &params),
                _ => generate_image(api_key, model, &params),
            };
            let generated = match generated {
                Ok(text) => text,
                Err(error) => return Some(Err(error)),
            };
            // Optional save_path: download THIS prediction as soon as it completes.
            let save_path = args
                .get("save_path")
                .or_else(|| args.get("savePath"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let Some(save_path) = save_path else {
                return Some(Ok(generated));
            };
            let parsed: Value = match serde_json::from_str(&generated) {
                Ok(value) => value,
                Err(_) => {
                    return Some(Ok(format!(
                        "{generated}\n\n(note: save_path was set but could not parse prediction_id; call atlas_collect_media manually)"
                    )));
                }
            };
            let Some(prediction_id) = parsed
                .get("prediction_id")
                .and_then(|v| v.as_str())
                .map(str::to_string)
            else {
                return Some(Ok(format!(
                    "{generated}\n\n(note: save_path was set but no prediction_id was returned)"
                )));
            };
            let wait = args
                .get("wait_seconds")
                .or_else(|| args.get("waitSeconds"))
                .and_then(|v| v.as_u64())
                .unwrap_or(default_wait);
            emit_progress(
                app,
                format!("Waiting on {prediction_id}, then downloading immediately…"),
            );
            match collect_media(
                api_key,
                &prediction_id,
                save_path,
                project_root,
                wait,
                app,
            ) {
                Ok(saved) => Some(Ok(json!({
                    "submitted": parsed,
                    "saved": serde_json::from_str::<Value>(&saved).unwrap_or(json!({ "raw": saved })),
                    "note": format!("Generated and downloaded this {kind} asset before starting another paid job."),
                })
                .to_string())),
                Err(error) => Some(Ok(json!({
                    "submitted": parsed,
                    "saved": null,
                    "error": error,
                    "note": "Generation was submitted. Resume with atlas_collect_media / atlas_collect_image using this prediction_id — do not re-generate unless it failed.",
                })
                .to_string())),
            }
        }
        "atlas_get_prediction" => {
            let Some(prediction_id) = args
                .get("prediction_id")
                .or_else(|| args.get("predictionId"))
                .and_then(|v| v.as_str())
            else {
                return Some(Err("missing prediction_id".into()));
            };
            let wait = args
                .get("wait_seconds")
                .or_else(|| args.get("waitSeconds"))
                .and_then(|v| v.as_u64())
                .unwrap_or(DEFAULT_WAIT_SECONDS);
            Some(get_prediction(api_key, prediction_id, wait, app))
        }
        "atlas_collect_image"
        | "atlas_collect_media"
        | "atlas_collect_video"
        | "atlas_collect_audio"
        | "watch_atlas_video" => {
            // watch_atlas_video is the OpenCode detached-watcher name; map it to
            // in-process collect so pasted specialist contracts still work.
            let Some(prediction_id) = args
                .get("prediction_id")
                .or_else(|| args.get("predictionId"))
                .and_then(|v| v.as_str())
            else {
                return Some(Err("missing prediction_id".into()));
            };
            let Some(path) = args
                .get("path")
                .or_else(|| args.get("destination"))
                .or_else(|| args.get("dest"))
                .or_else(|| args.get("output_path"))
                .or_else(|| args.get("outputPath"))
                .and_then(|v| v.as_str())
            else {
                return Some(Err("missing path".into()));
            };
            let wait = args
                .get("wait_seconds")
                .or_else(|| args.get("waitSeconds"))
                .and_then(|v| v.as_u64())
                .or_else(|| {
                    // OpenCode watcher timeout is often minutes; convert when present.
                    args.get("timeout_minutes")
                        .or_else(|| args.get("timeoutMinutes"))
                        .and_then(|v| v.as_u64())
                        .map(|m| (m * 60).min(120))
                })
                .unwrap_or(DEFAULT_WAIT_SECONDS);
            let result = collect_media(
                api_key,
                prediction_id,
                path,
                project_root,
                wait,
                app,
            );
            if name == "watch_atlas_video" {
                Some(result.map(|saved| {
                    json!({
                        "watcher": "mapped_to_atlas_collect_media",
                        "note": "OpenCode watch_atlas_video is not used on this desk. Host polled/downloaded via atlas_collect_media instead. Prefer atlas_generate_video with save_path next time.",
                        "saved": serde_json::from_str::<Value>(&saved).unwrap_or(json!({ "raw": saved })),
                    })
                    .to_string()
                }))
            } else {
                Some(result)
            }
        }
        "atlas_upload_media" => {
            let Some(file_path) = args
                .get("file_path")
                .or_else(|| args.get("filePath"))
                .or_else(|| args.get("path"))
                .and_then(|v| v.as_str())
            else {
                return Some(Err("missing file_path".into()));
            };
            emit_progress(app, format!("Uploading media {file_path}…"));
            Some(upload_media(
                api_key,
                file_path,
                project_root,
                crate::media_util::MediaRole::AnyMedia,
            ))
        }
        "atlas_get_balance" => Some(get_balance(api_key)),
        "download_atlas_image" | "download_atlas_media" | "download_media" => {
            let Some(url) = args.get("url").and_then(|v| v.as_str()) else {
                return Some(Err("missing url".into()));
            };
            let Some(path) = args
                .get("path")
                .or_else(|| args.get("destination"))
                .or_else(|| args.get("dest"))
                .and_then(|v| v.as_str())
            else {
                return Some(Err("missing path".into()));
            };
            if name == "download_atlas_image" {
                Some(download_atlas_image(url, path, project_root, app))
            } else {
                Some(download_atlas_media(url, path, project_root, app))
            }
        }
        "atlas_quick_generate" => Some(Err(
            "atlas_quick_generate is not used for handoff execution. Use atlas_generate_video / atlas_generate_audio with schema inspection.".into(),
        )),
        "atlas_get_model_usage" | "atlas_get_model_costs" => Some(Err(format!(
            "{name} is not wired in the workbench yet. Use atlas_get_balance for credit checks."
        ))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaps_shotgun_aliases_to_grok_image_urls() {
        let mut props = Map::new();
        props.insert("prompt".into(), json!({"type": "string"}));
        props.insert("image_urls".into(), json!({"type": "array"}));
        props.insert("duration".into(), json!({"type": "integer"}));
        props.insert("aspect_ratio".into(), json!({"type": "string"}));

        let upload_blob = json!({
            "download_url": "https://cdn.example/from-upload.png",
            "filename": "x.png",
            "note": "Temporary Atlas upload URL",
        })
        .to_string();
        let params = json!({
            "prompt": "kids run to the boxcar",
            "duration": 8,
            "aspect_ratio": "16:9",
            "negative_prompt": "blur",
            "image": "https://cdn.example/a.png",
            "image_url": "https://cdn.example/a.png",
            "reference_image": "https://cdn.example/a.png",
            "media_files": ["https://cdn.example/a.png", "https://cdn.example/b.png"],
            "image_urls": [upload_blob],
        })
        .as_object()
        .unwrap()
        .clone();

        let out = remap_params_to_schema(&params, &props);
        assert_eq!(out.get("duration"), Some(&json!(8)));
        assert_eq!(out.get("aspect_ratio"), Some(&json!("16:9")));
        assert!(out.get("negative_prompt").is_none());
        assert!(out.get("image").is_none());
        assert!(out.get("media_files").is_none());
        assert_eq!(
            out.get("image_urls"),
            Some(&json!([
                "https://cdn.example/from-upload.png",
                "https://cdn.example/a.png",
                "https://cdn.example/b.png"
            ]))
        );
        let prompt = out.get("prompt").and_then(|v| v.as_str()).unwrap();
        assert!(prompt.contains("<IMAGE_1>"));
        assert!(prompt.contains("<IMAGE_2>"));
    }

    #[test]
    fn reference_edit_size_follows_aspect_ratio_when_schema_has_no_aspect_field() {
        let mut props = Map::new();
        props.insert("prompt".into(), json!({"type": "string"}));
        props.insert("images".into(), json!({"type": "array"}));
        props.insert(
            "size".into(),
            json!({
                "type": "string",
                "enum": [
                    "2048*2048",
                    "2720*1530",
                    "1530*2720",
                    "2048*1152",
                    "1152*2048",
                    "1728*2304"
                ]
            }),
        );
        let params = json!({
            "prompt": "rainy train compartment",
            "aspect_ratio": "9:16",
            "images": ["https://cdn.example/tom.png"]
        })
        .as_object()
        .unwrap()
        .clone();
        let out = remap_params_to_schema(&params, &props);
        assert!(out.get("aspect_ratio").is_none());
        assert_eq!(out.get("size"), Some(&json!("1530*2720")));
    }

    #[test]
    fn freeform_size_is_synthesized_for_vertical_aspect() {
        let mut props = Map::new();
        props.insert("prompt".into(), json!({"type": "string"}));
        props.insert("size".into(), json!({"type": "string"}));
        let params = json!({
            "prompt": "platform at night",
            "aspect_ratio": "9:16"
        })
        .as_object()
        .unwrap()
        .clone();
        let out = remap_params_to_schema(&params, &props);
        assert_eq!(out.get("size"), Some(&json!("1152*2048")));
    }
}
