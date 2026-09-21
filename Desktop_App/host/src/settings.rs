use crate::ctx::HostCtx;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

const ATLAS_MODELS_URL: &str = "https://api.atlascloud.ai/api/v1/models";

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CatalogModel {
    pub id: String,
    pub display_name: String,
    #[serde(rename = "type")]
    pub model_type: String,
    pub organization: Option<String>,
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ApiEntry {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub api_key: String,
    pub secret_key: String,
    pub enabled: bool,
}

fn settings_path(app: &HostCtx) -> Result<PathBuf, String> {
    let dir = app.settings_dir();
    fs::create_dir_all(dir)
        .map_err(|error| format!("Could not create config directory: {error}"))?;
    Ok(dir.join("settings.json"))
}

fn empty_api(id: &str, name: &str, kind: &str, base_url: &str) -> Value {
    json!({
        "id": id,
        "name": name,
        "kind": kind,
        "baseUrl": base_url,
        "apiKey": "",
        "secretKey": "",
        "enabled": true,
        "catalog": [],
        "catalogFetchedAt": null,
        "pinnedLlmModelIds": []
    })
}

fn default_settings() -> Value {
    json!({
        "apis": [
            empty_api("api_atlas_default", "Atlas Cloud", "atlas-cloud", "https://api.atlascloud.ai")
        ],
        "capabilityDefaults": {},
        "selectedChatModel": null
    })
}

fn new_api_id(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("api_{prefix}_{millis}")
}

/// Convert legacy `{ providers: { atlas, elevenlabs } }` into `apis[]`.
fn migrate_legacy_providers(mut value: Value) -> Value {
    if value.get("apis").and_then(|v| v.as_array()).is_some() {
        return ensure_apis_nonempty(value);
    }

    let providers = value.get("providers").cloned().unwrap_or(json!({}));
    let mut apis = Vec::new();
    let mut id_map = serde_json::Map::new();

    for (legacy_id, kind, name, base) in [
        (
            "atlas",
            "atlas-cloud",
            "Atlas Cloud",
            "https://api.atlascloud.ai",
        ),
        (
            "elevenlabs",
            "elevenlabs",
            "ElevenLabs",
            "https://api.elevenlabs.io",
        ),
    ] {
        let Some(src) = providers.get(legacy_id) else {
            if legacy_id == "atlas" {
                let id = new_api_id("atlas");
                id_map.insert(legacy_id.to_string(), json!(id));
                apis.push(empty_api(&id, name, kind, base));
            }
            continue;
        };
        let id = new_api_id(legacy_id);
        id_map.insert(legacy_id.to_string(), json!(id));
        apis.push(json!({
            "id": id,
            "name": name,
            "kind": kind,
            "baseUrl": base,
            "apiKey": src.get("apiKey").and_then(|v| v.as_str()).unwrap_or(""),
            "secretKey": "",
            "enabled": src.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false),
            "catalog": src.get("catalog").cloned().unwrap_or(json!([])),
            "catalogFetchedAt": src.get("catalogFetchedAt").cloned().unwrap_or(Value::Null),
            "pinnedLlmModelIds": src.get("pinnedLlmModelIds").cloned().unwrap_or(json!([])),
        }));
    }

    if apis.is_empty() {
        return default_settings();
    }

    // Rewrite capabilityDefaults / selectedChatModel providerId → apiId
    if let Some(caps) = value
        .get_mut("capabilityDefaults")
        .and_then(|v| v.as_object_mut())
    {
        for (_slot, ref_val) in caps.iter_mut() {
            rewrite_model_ref(ref_val, &id_map, apis.first());
        }
    }
    if let Some(selected) = value.get_mut("selectedChatModel") {
        rewrite_model_ref(selected, &id_map, apis.first());
    }

    value["apis"] = Value::Array(apis);
    value.as_object_mut().map(|obj| obj.remove("providers"));
    value
}

fn rewrite_model_ref(
    ref_val: &mut Value,
    id_map: &serde_json::Map<String, Value>,
    fallback_api: Option<&Value>,
) {
    if ref_val.is_null() {
        return;
    }
    let Some(obj) = ref_val.as_object_mut() else {
        return;
    };
    if obj.get("apiId").and_then(|v| v.as_str()).is_some() {
        obj.remove("providerId");
        return;
    }
    let legacy = obj
        .get("providerId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let api_id = id_map
        .get(&legacy)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| {
            fallback_api
                .and_then(|v| v.get("id"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        });
    if let Some(api_id) = api_id {
        obj.insert("apiId".into(), json!(api_id));
    }
    obj.remove("providerId");
}

fn ensure_apis_nonempty(mut value: Value) -> Value {
    let empty = value
        .get("apis")
        .and_then(|v| v.as_array())
        .map(|arr| arr.is_empty())
        .unwrap_or(true);
    if empty {
        return default_settings();
    }
    // Normalize model refs that still use providerId
    let first_id = value
        .pointer("/apis/0/id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let id_map = serde_json::Map::new();
    let fallback = json!({ "id": first_id });
    if let Some(caps) = value
        .get_mut("capabilityDefaults")
        .and_then(|v| v.as_object_mut())
    {
        for (_slot, ref_val) in caps.iter_mut() {
            rewrite_model_ref(ref_val, &id_map, Some(&fallback));
        }
    }
    if let Some(selected) = value.get_mut("selectedChatModel") {
        rewrite_model_ref(selected, &id_map, Some(&fallback));
    }
    value
}

pub fn load_settings(app: HostCtx) -> Result<Value, String> {
    let path = settings_path(&app)?;
    if !path.exists() {
        return Ok(default_settings());
    }
    let raw =
        fs::read_to_string(&path).map_err(|error| format!("Could not read settings: {error}"))?;
    let value: Value =
        serde_json::from_str(&raw).map_err(|error| format!("Could not parse settings: {error}"))?;
    let had_legacy = value.get("providers").is_some() && value.get("apis").is_none();
    let migrated = migrate_legacy_providers(value);
    // Persist conversion once so keys are not stuck in the old shape.
    if had_legacy {
        let _ = fs::write(
            &path,
            serde_json::to_string_pretty(&migrated).unwrap_or_default(),
        );
    }
    Ok(migrated)
}

pub fn save_settings(app: HostCtx, settings: Value) -> Result<(), String> {
    let migrated = migrate_legacy_providers(settings);
    if migrated
        .get("apis")
        .and_then(|v| v.as_array())
        .map(|arr| arr.is_empty())
        .unwrap_or(true)
    {
        return Err("Settings must include at least one API.".into());
    }
    let path = settings_path(&app)?;
    let raw = serde_json::to_string_pretty(&migrated)
        .map_err(|error| format!("Could not serialize settings: {error}"))?;
    fs::write(&path, raw).map_err(|error| format!("Could not write settings: {error}"))
}

fn parse_api_entry(value: &Value) -> Option<ApiEntry> {
    Some(ApiEntry {
        id: value.get("id")?.as_str()?.to_string(),
        name: value
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("API")
            .to_string(),
        kind: value.get("kind")?.as_str()?.to_string(),
        base_url: value
            .get("baseUrl")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim_end_matches('/')
            .to_string(),
        api_key: value
            .get("apiKey")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        secret_key: value
            .get("secretKey")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        enabled: value
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
    })
}

pub fn list_apis(app: &HostCtx) -> Result<Vec<ApiEntry>, String> {
    let settings = load_settings(app.clone())?;
    let arr = settings
        .get("apis")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(arr.iter().filter_map(parse_api_entry).collect())
}

pub fn find_api(app: &HostCtx, api_id: &str) -> Result<ApiEntry, String> {
    list_apis(app)?
        .into_iter()
        .find(|api| api.id == api_id)
        .ok_or_else(|| format!("Unknown API id in Settings: {api_id}"))
}

pub fn find_api_by_kind(app: &HostCtx, kind: &str) -> Result<ApiEntry, String> {
    let apis = list_apis(app)?;
    apis.iter()
        .find(|api| {
            api.enabled && api.kind == kind && (!api.api_key.trim().is_empty() || kind == "ollama")
        })
        .cloned()
        .or_else(|| apis.into_iter().find(|api| api.kind == kind))
        .ok_or_else(|| format!("Add a {kind} API in Settings first."))
}

/// Atlas API key from the first enabled atlas-cloud entry (media tools).
pub fn atlas_api_key(app: &HostCtx) -> Result<String, String> {
    let api = find_api_by_kind(app, "atlas-cloud")?;
    let key = api.api_key.trim();
    if key.is_empty() {
        return Err("Add an Atlas Cloud API key in Settings first.".into());
    }
    Ok(key.to_string())
}

/// ElevenLabs API key from the first enabled elevenlabs entry.
pub fn elevenlabs_api_key(app: &HostCtx) -> Result<String, String> {
    let api = find_api_by_kind(app, "elevenlabs")?;
    let key = api.api_key.trim();
    if key.is_empty() {
        return Err("Add an ElevenLabs API key in Settings first.".into());
    }
    Ok(key.to_string())
}

pub fn is_local_llm_base(base_url: &str) -> bool {
    let raw = base_url.trim().to_ascii_lowercase();
    if raw.is_empty() {
        return false;
    }
    let host = raw
        .split("://")
        .nth(1)
        .unwrap_or(raw.as_str())
        .split('/')
        .next()
        .unwrap_or("")
        .split('@')
        .next_back()
        .unwrap_or("")
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(':')
        .next()
        .unwrap_or("");
    matches!(host, "localhost" | "127.0.0.1" | "0.0.0.0" | "::1") || host.ends_with(".local")
}

pub fn api_key_for(app: &HostCtx, api_id: &str) -> Result<(ApiEntry, String), String> {
    let api = find_api(app, api_id)?;
    let key = api.api_key.trim().to_string();
    let key_optional = api.kind == "ollama" || is_local_llm_base(&api.base_url);
    if key.is_empty() && !key_optional {
        return Err(format!(
            "Add an API key for “{}” in Settings first.",
            api.name
        ));
    }
    Ok((api, key))
}

pub fn fetch_api_catalog(
    kind: String,
    api_key: String,
    base_url: String,
) -> Result<Vec<CatalogModel>, String> {
    match kind.as_str() {
        "atlas-cloud" => fetch_atlas_catalog(api_key),
        "openai-chat" => fetch_openai_catalog(api_key, base_url),
        "ollama" => fetch_ollama_catalog(base_url),
        "gemini" => fetch_gemini_catalog(api_key, base_url),
        "sync-labs" => crate::sync_labs::fetch_catalog(&api_key, &base_url),
        other => Err(format!(
            "Catalog fetch is not supported for protocol “{other}”."
        )),
    }
}

pub fn fetch_atlas_catalog(api_key: String) -> Result<Vec<CatalogModel>, String> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err("Enter an Atlas API key before fetching the catalog.".into());
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|error| format!("Could not build HTTP client: {error}"))?;

    let response = client
        .get(ATLAS_MODELS_URL)
        .header("Authorization", format!("Bearer {key}"))
        .header("Accept", "application/json")
        .send()
        .map_err(|error| format!("Catalog request failed: {error}"))?;

    let status = response.status();
    let body = response
        .text()
        .map_err(|error| format!("Could not read catalog response: {error}"))?;

    if !status.is_success() {
        return Err(format!("Atlas catalog request failed ({status}): {body}"));
    }

    let parsed: Value = serde_json::from_str(&body)
        .map_err(|error| format!("Could not parse catalog JSON: {error}"))?;
    let items = parsed
        .get("data")
        .and_then(|value| value.as_array())
        .ok_or_else(|| "Unexpected Atlas catalog response shape.".to_string())?;

    let mut models = Vec::new();
    for item in items {
        let id = item
            .get("model")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        if id.is_empty() {
            continue;
        }
        let tags = item
            .get("tags")
            .and_then(|value| value.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        models.push(CatalogModel {
            id,
            display_name: item
                .get("displayName")
                .and_then(|value| value.as_str())
                .unwrap_or("Untitled")
                .to_string(),
            model_type: item
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or("Unknown")
                .to_string(),
            organization: item
                .get("organization")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            profile: item
                .get("profile")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            tags,
        });
    }

    sort_models(&mut models);
    Ok(models)
}

fn normalize_openai_base(base_url: &str) -> String {
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

fn fetch_openai_catalog(api_key: String, base_url: String) -> Result<Vec<CatalogModel>, String> {
    let key = api_key.trim();
    let local = is_local_llm_base(&base_url);
    if key.is_empty() && !local {
        return Err("Enter an API key before fetching the catalog.".into());
    }
    let bearer = if key.is_empty() { "local" } else { key };
    let base = normalize_openai_base(&base_url);
    let url = format!("{base}/models");
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|error| format!("Could not build HTTP client: {error}"))?;
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {bearer}"))
        .header("Accept", "application/json")
        .send()
        .map_err(|error| format!("Catalog request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|error| format!("Could not read catalog response: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "OpenAI-compatible catalog failed ({status}): {body}"
        ));
    }
    let parsed: Value = serde_json::from_str(&body)
        .map_err(|error| format!("Could not parse catalog JSON: {error}"))?;
    let items = parsed
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Unexpected models response (expected data[]).".to_string())?;
    let mut models = Vec::new();
    for item in items {
        let id = item
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if id.is_empty() {
            continue;
        }
        models.push(CatalogModel {
            id: id.clone(),
            display_name: id,
            model_type: "Text".into(),
            organization: item
                .get("owned_by")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            profile: None,
            tags: Vec::new(),
        });
    }
    sort_models(&mut models);
    Ok(models)
}

/// Native Ollama root (for /api/tags). Strips a trailing `/v1` if the user pasted the OpenAI URL.
fn normalize_ollama_native_base(base_url: &str) -> String {
    let mut base = base_url.trim().trim_end_matches('/').to_string();
    if base.is_empty() {
        return "http://127.0.0.1:11434".into();
    }
    if base.ends_with("/v1") {
        base.truncate(base.len() - 3);
        base = base.trim_end_matches('/').to_string();
    }
    if base.is_empty() {
        "http://127.0.0.1:11434".into()
    } else {
        base
    }
}

fn fetch_ollama_catalog(base_url: String) -> Result<Vec<CatalogModel>, String> {
    let base = normalize_ollama_native_base(&base_url);
    let url = format!("{base}/api/tags");
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| format!("Could not build HTTP client: {error}"))?;
    let response = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .map_err(|error| format!("Ollama catalog request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|error| format!("Could not read Ollama response: {error}"))?;
    if !status.is_success() {
        return Err(format!("Ollama catalog failed ({status}): {body}"));
    }
    let parsed: Value = serde_json::from_str(&body)
        .map_err(|error| format!("Could not parse Ollama JSON: {error}"))?;
    let items = parsed
        .get("models")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Unexpected Ollama /api/tags shape.".to_string())?;
    let mut models = Vec::new();
    for item in items {
        let id = item
            .get("name")
            .or_else(|| item.get("model"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if id.is_empty() {
            continue;
        }
        models.push(CatalogModel {
            id: id.clone(),
            display_name: id,
            model_type: "Text".into(),
            organization: Some("ollama".into()),
            profile: None,
            tags: Vec::new(),
        });
    }
    sort_models(&mut models);
    Ok(models)
}

fn fetch_gemini_catalog(api_key: String, base_url: String) -> Result<Vec<CatalogModel>, String> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err("Enter a Gemini API key before fetching the catalog.".into());
    }
    let base = base_url.trim().trim_end_matches('/');
    let base = if base.is_empty() {
        "https://generativelanguage.googleapis.com/v1beta"
    } else {
        base
    };
    let url = format!("{base}/models?key={key}");
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|error| format!("Could not build HTTP client: {error}"))?;
    let response = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .map_err(|error| format!("Gemini catalog request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|error| format!("Could not read Gemini response: {error}"))?;
    if !status.is_success() {
        return Err(format!("Gemini catalog failed ({status}): {body}"));
    }
    let parsed: Value = serde_json::from_str(&body)
        .map_err(|error| format!("Could not parse Gemini JSON: {error}"))?;
    let items = parsed
        .get("models")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Unexpected Gemini models response.".to_string())?;
    let mut models = Vec::new();
    for item in items {
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
        // names look like "models/gemini-2.0-flash"
        let id = name.strip_prefix("models/").unwrap_or(name).to_string();
        if id.is_empty() {
            continue;
        }
        let methods = item
            .get("supportedGenerationMethods")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let supports_chat = methods.iter().any(|m| {
            m.as_str()
                .map(|s| s == "generateContent" || s == "generateContentStream")
                .unwrap_or(false)
        });
        if !supports_chat {
            continue;
        }
        models.push(CatalogModel {
            id: id.clone(),
            display_name: item
                .get("displayName")
                .and_then(|v| v.as_str())
                .unwrap_or(&id)
                .to_string(),
            model_type: "Text".into(),
            organization: Some("google".into()),
            profile: None,
            tags: Vec::new(),
        });
    }
    sort_models(&mut models);
    Ok(models)
}

fn sort_models(models: &mut [CatalogModel]) {
    models.sort_by(|a, b| {
        (
            a.model_type.to_lowercase(),
            a.display_name.to_lowercase(),
            a.id.to_lowercase(),
        )
            .cmp(&(
                b.model_type.to_lowercase(),
                b.display_name.to_lowercase(),
                b.id.to_lowercase(),
            ))
    });
}
