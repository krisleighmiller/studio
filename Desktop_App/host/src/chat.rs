//! Multi-protocol chat adapters for Director turns.
//!
//! Each Settings API entry has a `kind` that selects one of these wire formats.

use crate::settings::ApiEntry;
use serde_json::{json, Value};

/// Chat is a separate, currently unpriced and unrecorded spend category.
pub const CHAT_SPEND_CATEGORY: &str = "llm.chat";

#[derive(Debug, PartialEq, Eq)]
pub enum ChatBudgetCoverage {
    NotAccountedUnknownCost,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ChatDispatch<'a> {
    pub category: &'static str,
    pub provider_kind: &'a str,
    pub model: &'a str,
    pub budget_coverage: ChatBudgetCoverage,
}

/// Classify every supported protocol, including Ollama (which may point at a
/// remote, billed endpoint). Never infer that a chat request is free.
pub fn classify_chat_dispatch<'a>(api: &'a ApiEntry, model: &'a str) -> Option<ChatDispatch<'a>> {
    if !matches!(
        api.kind.as_str(),
        "atlas-cloud" | "openai-chat" | "ollama" | "anthropic-messages" | "gemini"
    ) {
        return None;
    }
    Some(ChatDispatch {
        category: CHAT_SPEND_CATEGORY,
        provider_kind: &api.kind,
        model,
        budget_coverage: ChatBudgetCoverage::NotAccountedUnknownCost,
    })
}

/// Existing callers do not pass project identity, so a hard stop cannot be
/// checked. Fail closed rather than silently dispatch potentially paid chat.
/// A future project-aware host authorization must reserve spend before invoking
/// the private transport below and reconcile the actual charge afterward.
pub fn chat_completion(
    api: &ApiEntry,
    _api_key: &str,
    model: &str,
    _messages: &[Value],
) -> Result<String, String> {
    let Some(dispatch) = classify_chat_dispatch(api, model) else {
        return Err(format!(
            "API “{}” uses protocol “{}”, which does not support Director chat. Pick a chat-capable API (Atlas, OpenAI-compatible, Anthropic, Gemini, or Ollama).",
            api.name, api.kind
        ));
    };
    Err(format!(
        "{} request for {} / {} blocked: chat cost is unknown and project-budget enforcement is not wired to this route.",
        dispatch.category, dispatch.provider_kind, dispatch.model
    ))
}

// Keep wire-format implementations for a future, project-authorized dispatch.
// This function must not be made public without a host-owned reservation gate.
#[allow(dead_code)]
fn dispatch_chat(
    api: &ApiEntry,
    api_key: &str,
    model: &str,
    messages: &[Value],
) -> Result<String, String> {
    match api.kind.as_str() {
        "atlas-cloud" => openai_compatible_chat(
            &atlas_chat_base(&api.base_url),
            api_key,
            model,
            messages,
            true,
        ),
        "openai-chat" => openai_compatible_chat(
            &normalize_openai_base(&api.base_url),
            if api_key.is_empty() { "local" } else { api_key },
            model,
            messages,
            true,
        ),
        "ollama" => openai_compatible_chat_with_temp(
            &ollama_openai_base(&api.base_url),
            if api_key.is_empty() { "ollama" } else { api_key },
            model,
            messages,
            // Lower temp helps small local models keep director_action / file-tag shapes.
            0.2,
            true,
        ),
        "anthropic-messages" => anthropic_chat(&api.base_url, api_key, model, messages),
        "gemini" => gemini_chat(&api.base_url, api_key, model, messages),
        other => Err(format!(
            "API “{}” uses protocol “{other}”, which does not support Director chat. Pick a chat-capable API (Atlas, OpenAI-compatible, Anthropic, Gemini, or Ollama).",
            api.name
        )),
    }
}

fn atlas_chat_base(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return "https://api.atlascloud.ai/v1".into();
    }
    // Users may store https://api.atlascloud.ai — chat lives under /v1.
    if base.ends_with("/v1") {
        base.to_string()
    } else if base.contains("atlascloud.ai") {
        format!("{base}/v1")
    } else {
        normalize_openai_base(base)
    }
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

fn ollama_openai_base(base_url: &str) -> String {
    let mut base = base_url.trim().trim_end_matches('/').to_string();
    if base.is_empty() {
        base = "http://127.0.0.1:11434".into();
    }
    // Accept either native root or OpenAI-compat `/v1` URL.
    if base.ends_with("/v1") {
        base
    } else {
        format!("{base}/v1")
    }
}

fn openai_compatible_chat(
    base: &str,
    api_key: &str,
    model: &str,
    messages: &[Value],
    allow_temp_retry: bool,
) -> Result<String, String> {
    openai_compatible_chat_with_temp(base, api_key, model, messages, 0.7, allow_temp_retry)
}

fn openai_compatible_chat_with_temp(
    base: &str,
    api_key: &str,
    model: &str,
    messages: &[Value],
    temperature: f64,
    allow_temp_retry: bool,
) -> Result<String, String> {
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));
    let body = json!({
        "model": model,
        "messages": messages,
        "temperature": temperature,
    });
    match post_json(&url, bearer_headers(api_key), &body) {
        Ok(parsed) => extract_openai_assistant_text(&parsed),
        Err(error) if allow_temp_retry && error.contains("400") => {
            let body_no_temp = json!({
                "model": model,
                "messages": messages,
            });
            let parsed = post_json(&url, bearer_headers(api_key), &body_no_temp)?;
            extract_openai_assistant_text(&parsed)
        }
        Err(error) => Err(error),
    }
}

fn anthropic_chat(
    base_url: &str,
    api_key: &str,
    model: &str,
    messages: &[Value],
) -> Result<String, String> {
    let base = base_url.trim().trim_end_matches('/');
    let base = if base.is_empty() {
        "https://api.anthropic.com"
    } else {
        base
    };
    let url = format!("{base}/v1/messages");

    let mut system_parts = Vec::new();
    let mut anth_messages = Vec::new();
    for message in messages {
        let role = message.get("role").and_then(|v| v.as_str()).unwrap_or("");
        let content = message
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        match role {
            "system" => system_parts.push(content),
            "user" | "assistant" => {
                anth_messages.push(json!({
                    "role": role,
                    "content": content,
                }));
            }
            _ => {}
        }
    }
    if anth_messages.is_empty() {
        return Err("No user/assistant messages for Anthropic chat.".into());
    }

    let mut body = json!({
        "model": model,
        // Scene plans / long scripts need headroom beyond a short chat reply.
        "max_tokens": 16384,
        "messages": anth_messages,
    });
    if !system_parts.is_empty() {
        body["system"] = json!(system_parts.join("\n\n"));
    }

    let headers = vec![
        ("x-api-key".into(), api_key.to_string()),
        ("anthropic-version".into(), "2023-06-01".into()),
        ("content-type".into(), "application/json".into()),
    ];
    let parsed = post_json(&url, headers, &body)?;
    extract_anthropic_text(&parsed)
}

fn gemini_chat(
    base_url: &str,
    api_key: &str,
    model: &str,
    messages: &[Value],
) -> Result<String, String> {
    let base = base_url.trim().trim_end_matches('/');
    let base = if base.is_empty() {
        "https://generativelanguage.googleapis.com/v1beta"
    } else {
        base
    };
    let model_id = model.strip_prefix("models/").unwrap_or(model);
    let url = format!("{base}/models/{model_id}:generateContent");

    let mut system_parts = Vec::new();
    let mut contents = Vec::new();
    for message in messages {
        let role = message.get("role").and_then(|v| v.as_str()).unwrap_or("");
        let content = message
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        match role {
            "system" => system_parts.push(content),
            "user" => contents.push(json!({
                "role": "user",
                "parts": [{ "text": content }],
            })),
            "assistant" => contents.push(json!({
                "role": "model",
                "parts": [{ "text": content }],
            })),
            _ => {}
        }
    }
    if contents.is_empty() {
        return Err("No user/assistant messages for Gemini chat.".into());
    }

    let mut body = json!({ "contents": contents });
    if !system_parts.is_empty() {
        body["systemInstruction"] = json!({
            "parts": [{ "text": system_parts.join("\n\n") }],
        });
    }

    let parsed = post_json(
        &url,
        vec![
            ("content-type".into(), "application/json".into()),
            ("x-goog-api-key".into(), api_key.to_string()),
        ],
        &body,
    )?;
    extract_gemini_text(&parsed)
}

fn bearer_headers(api_key: &str) -> Vec<(String, String)> {
    vec![
        ("Authorization".into(), format!("Bearer {api_key}")),
        ("content-type".into(), "application/json".into()),
        ("Accept".into(), "application/json".into()),
    ]
}

fn post_json(url: &str, headers: Vec<(String, String)>, body: &Value) -> Result<Value, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|error| format!("Could not build HTTP client: {error}"))?;
    let mut request = client.post(url).json(body);
    for (key, value) in headers {
        request = request.header(key, value);
    }
    let response = request
        .send()
        .map_err(|error| format!("Chat request failed: {error}"))?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|error| format!("Could not read chat response: {error}"))?;
    if !status.is_success() {
        return Err(format!("Chat request failed ({status}): {text}"));
    }
    serde_json::from_str(&text)
        .map_err(|error| format!("Could not parse chat JSON: {error}\n{text}"))
}

fn extract_openai_assistant_text(parsed: &Value) -> Result<String, String> {
    let choice = parsed
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .ok_or_else(|| "Chat returned no choices.".to_string())?;
    let message = choice
        .get("message")
        .ok_or_else(|| "Chat choice missing message.".to_string())?;
    Ok(extract_content_field(message))
}

fn extract_content_field(message: &Value) -> String {
    match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| {
                if part.get("type").and_then(|t| t.as_str()) == Some("text") {
                    part.get("text")
                        .and_then(|t| t.as_str())
                        .map(str::to_string)
                } else {
                    part.get("text")
                        .and_then(|t| t.as_str())
                        .map(str::to_string)
                }
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn extract_anthropic_text(parsed: &Value) -> Result<String, String> {
    let parts = parsed
        .get("content")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Anthropic response missing content[].".to_string())?;
    let text = parts
        .iter()
        .filter_map(|part| {
            if part.get("type").and_then(|t| t.as_str()) == Some("text") {
                part.get("text")
                    .and_then(|t| t.as_str())
                    .map(str::to_string)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("");
    if text.is_empty() {
        Err("Anthropic response had no text blocks.".into())
    } else {
        Ok(text)
    }
}

fn extract_gemini_text(parsed: &Value) -> Result<String, String> {
    let text = parsed
        .pointer("/candidates/0/content/parts")
        .and_then(|v| v.as_array())
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();
    if text.is_empty() {
        Err(format!(
            "Gemini response had no text. Raw: {}",
            parsed.to_string().chars().take(500).collect::<String>()
        ))
    } else {
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api(kind: &str) -> ApiEntry {
        ApiEntry {
            id: "test".into(),
            name: "Test".into(),
            kind: kind.into(),
            base_url: "http://127.0.0.1:1".into(),
            api_key: String::new(),
            secret_key: String::new(),
            enabled: true,
        }
    }

    #[test]
    fn every_chat_protocol_is_unknown_cost_and_blocked_before_network() {
        for kind in [
            "atlas-cloud",
            "openai-chat",
            "ollama",
            "anthropic-messages",
            "gemini",
        ] {
            let api = api(kind);
            let dispatch = classify_chat_dispatch(&api, "test-model").unwrap();
            assert_eq!(dispatch.category, "llm.chat");
            assert_eq!(dispatch.provider_kind, kind);
            assert_eq!(dispatch.model, "test-model");
            assert_eq!(
                dispatch.budget_coverage,
                ChatBudgetCoverage::NotAccountedUnknownCost
            );
            // Port 1 is not a provider. This must return the budget guard error,
            // not a connection refusal, even with a valid user message.
            let error = chat_completion(
                &api,
                "key",
                "test-model",
                &[json!({"role":"user","content":"hi"})],
            )
            .unwrap_err();
            assert!(
                error.contains("blocked: chat cost is unknown"),
                "{kind}: {error}"
            );
        }
    }

    #[test]
    fn unsupported_protocol_is_not_misclassified() {
        let api = api("elevenlabs");
        assert!(classify_chat_dispatch(&api, "model").is_none());
        assert!(chat_completion(&api, "", "model", &[])
            .unwrap_err()
            .contains("does not support Director chat"));
    }
}
