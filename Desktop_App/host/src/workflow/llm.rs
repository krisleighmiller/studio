//! Chat-model calls and JSON extraction for workflow creative drafts.
//! Host owns IDs, paths, and mix. The model only fills typed creative fields.
use crate::chat;
use crate::ctx::HostCtx;
use crate::settings;
use serde_json::{json, Value};

pub fn resolve_chat<'a>(
    app: &'a HostCtx,
    api_id: Option<&str>,
    model: &str,
) -> Result<(settings::ApiEntry, String, String), String> {
    let model = model.trim();
    if model.is_empty() {
        return Err("Pick a chat model in the dropdown first.".into());
    }
    let chat_api_id = api_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            settings::list_apis(app).ok().and_then(|apis| {
                apis.into_iter()
                    .find(|api| {
                        api.enabled
                            && matches!(
                                api.kind.as_str(),
                                "atlas-cloud"
                                    | "openai-chat"
                                    | "anthropic-messages"
                                    | "gemini"
                                    | "ollama"
                            )
                    })
                    .map(|api| api.id)
            })
        })
        .ok_or_else(|| "Pick a chat model (with an API) in the dropdown first.".to_string())?;
    let (api, key) = settings::api_key_for(app, &chat_api_id)?;
    Ok((api, key, model.to_string()))
}

pub fn complete_json(
    app: &HostCtx,
    api_id: Option<&str>,
    model: &str,
    system: &str,
    user: &str,
) -> Result<(Value, String), String> {
    let (api, key, model) = resolve_chat(app, api_id, model)?;
    let messages = vec![
        json!({ "role": "system", "content": system }),
        json!({ "role": "user", "content": user }),
    ];
    let reply = chat::chat_completion(&api, &key, &model, &messages)?;
    let value = extract_json(&reply).ok_or_else(|| {
        format!(
            "the model did not return JSON.\n{}",
            reply.chars().take(800).collect::<String>()
        )
    })?;
    Ok((value, reply))
}

pub fn complete_json_with_correction(
    app: &HostCtx,
    api_id: Option<&str>,
    model: &str,
    system: &str,
    user: &str,
    correction: &str,
    previous_reply: &str,
) -> Result<(Value, String), String> {
    let (api, key, model) = resolve_chat(app, api_id, model)?;
    let messages = vec![
        json!({ "role": "system", "content": system }),
        json!({ "role": "user", "content": user }),
        json!({ "role": "assistant", "content": previous_reply }),
        json!({ "role": "user", "content": correction }),
    ];
    let reply = chat::chat_completion(&api, &key, &model, &messages)?;
    let value = extract_json(&reply).ok_or_else(|| {
        format!(
            "the model did not return JSON after correction.\n{}",
            reply.chars().take(800).collect::<String>()
        )
    })?;
    Ok((value, reply))
}

/// First JSON object in a model reply. Ignores fences and trailing prose.
pub fn extract_json(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        if v.is_object() || v.is_array() {
            return Some(v);
        }
    }
    let body = strip_fence(trimmed);
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        if v.is_object() || v.is_array() {
            return Some(v);
        }
    }
    let start = body.find('{')?;
    let slice = &body[start..];
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    for (i, ch) in slice.char_indices() {
        if in_str {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let end = i + ch.len_utf8();
                    if let Ok(v) = serde_json::from_str::<Value>(&slice[..end]) {
                        return Some(v);
                    }
                    break;
                }
            }
            _ => {}
        }
    }
    None
}

fn strip_fence(text: &str) -> &str {
    let t = text.trim();
    if !t.starts_with("```") {
        return t;
    }
    let rest = t.trim_start_matches('`');
    let rest = rest
        .strip_prefix("json")
        .or_else(|| rest.strip_prefix("JSON"))
        .unwrap_or(rest)
        .trim_start();
    if let Some(idx) = rest.rfind("```") {
        rest[..idx].trim()
    } else {
        rest.trim()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_fenced_and_prose_wrapped_objects() {
        let v = extract_json("```json\n{\"narrative_purpose\":\"stay warm\"}\n```").unwrap();
        assert_eq!(v["narrative_purpose"], "stay warm");
        let v = extract_json("Sure.\n{\n  \"cuts\": []\n}\nHope that helps.").unwrap();
        assert!(v.get("cuts").is_some());
    }
}
