//! Keep API key material out of host replies and error strings.

use crate::ctx::HostCtx;
use crate::settings;
use serde_json::Value;

pub fn redact_text(app: &HostCtx, text: &str) -> String {
    let mut out = text.to_string();
    for secret in known_secrets(app) {
        if secret.len() < 8 {
            continue;
        }
        if out.contains(&secret) {
            out = out.replace(&secret, "[redacted]");
        }
    }
    out
}

pub fn redact_json(app: &HostCtx, mut value: Value) -> Value {
    let secrets = known_secrets(app);
    redact_value(&mut value, &secrets);
    value
}

fn known_secrets(app: &HostCtx) -> Vec<String> {
    let Ok(settings) = settings::load_settings(app.clone()) else {
        return Vec::new();
    };
    let mut secrets = Vec::new();
    let Some(apis) = settings.get("apis").and_then(|v| v.as_array()) else {
        return secrets;
    };
    for api in apis {
        for key in ["apiKey", "secretKey"] {
            if let Some(secret) = api.get(key).and_then(|v| v.as_str()) {
                let secret = secret.trim();
                if secret.len() >= 8 {
                    secrets.push(secret.to_string());
                }
            }
        }
    }
    secrets
}

fn redact_value(value: &mut Value, secrets: &[String]) {
    match value {
        Value::String(text) => {
            for secret in secrets {
                if text.contains(secret) {
                    *text = text.replace(secret, "[redacted]");
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_value(item, secrets);
            }
        }
        Value::Object(map) => {
            for item in map.values_mut() {
                redact_value(item, secrets);
            }
        }
        _ => {}
    }
}
