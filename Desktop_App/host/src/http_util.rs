//! Shared HTTP helpers for Atlas / ElevenLabs clients.

use std::thread;
use std::time::Duration;

/// Flatten a reqwest error chain so TLS/DNS/connect causes are visible in the UI.
pub fn format_reqwest_error(prefix: &str, error: reqwest::Error) -> String {
    let mut parts = vec![error.to_string()];
    let mut source = std::error::Error::source(&error);
    while let Some(err) = source {
        let text = err.to_string();
        if !parts.iter().any(|part| part.contains(&text)) {
            parts.push(text);
        }
        source = err.source();
    }
    format!("{prefix}: {}", parts.join(" — "))
}

fn is_transient(error: &reqwest::Error) -> bool {
    error.is_timeout()
        || error.is_connect()
        || error.is_request()
        || error.status().is_some_and(|status| status.is_server_error())
}

fn send_with_retry<F>(
    label: &str,
    attempts: u32,
    mut send: F,
) -> Result<reqwest::blocking::Response, String>
where
    F: FnMut() -> Result<reqwest::blocking::Response, reqwest::Error>,
{
    let attempts = attempts.max(1);
    let mut last_error = None;
    for attempt in 1..=attempts {
        match send() {
            Ok(response) => return Ok(response),
            Err(error) => {
                let transient = is_transient(&error);
                let formatted = format_reqwest_error(label, error);
                eprintln!(
                    "workbench-host: {formatted} (attempt {attempt}/{attempts}, transient={transient})"
                );
                last_error = Some(formatted);
                if !transient || attempt == attempts {
                    break;
                }
                thread::sleep(Duration::from_millis(250 * u64::from(attempt)));
            }
        }
    }
    Err(last_error.unwrap_or_else(|| label.to_string()))
}

/// POST JSON with a couple of retries for transient network failures.
pub fn post_json_with_retry(
    client: &reqwest::blocking::Client,
    url: &str,
    api_key: &str,
    body: &serde_json::Value,
    attempts: u32,
) -> Result<reqwest::blocking::Response, String> {
    send_with_retry("HTTP POST failed", attempts, || {
        client
            .post(url)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(body)
            .send()
    })
}

/// Authenticated GET with retries.
pub fn get_with_retry(
    client: &reqwest::blocking::Client,
    url: &str,
    api_key: Option<&str>,
    attempts: u32,
) -> Result<reqwest::blocking::Response, String> {
    send_with_retry("HTTP GET failed", attempts, || {
        let mut req = client.get(url).header("Accept", "application/json");
        if let Some(key) = api_key {
            req = req.header("Authorization", format!("Bearer {key}"));
        }
        req.send()
    })
}
