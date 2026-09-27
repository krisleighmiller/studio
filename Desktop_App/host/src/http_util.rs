//! Shared HTTP helpers for Atlas / ElevenLabs clients.

use std::thread;
use std::time::Duration;

const DEFAULT_DOWNLOAD_CAP: u64 = 512_000_000;

pub fn read_limited(
    mut response: reqwest::blocking::Response,
    max_bytes: u64,
) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut body = Vec::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let read = response
            .read(&mut buf)
            .map_err(|error| format!("Could not read response: {error}"))?;
        if read == 0 {
            break;
        }
        let next = body.len() as u64 + read as u64;
        if next > max_bytes {
            return Err(format!("Download is larger than {max_bytes} bytes."));
        }
        body.extend_from_slice(&buf[..read]);
    }
    Ok(body)
}

pub fn write_response_limited(
    response: reqwest::blocking::Response,
    dest: &std::path::Path,
    max_bytes: u64,
) -> Result<u64, String> {
    use std::io::{Read, Write};
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create parent dirs: {error}"))?;
        }
    }
    let mut file = std::fs::File::create(dest)
        .map_err(|error| format!("Could not write {}: {error}", dest.display()))?;
    let mut response = response;
    let mut written = 0u64;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let read = response
            .read(&mut buf)
            .map_err(|error| format!("Could not read download: {error}"))?;
        if read == 0 {
            break;
        }
        written += read as u64;
        if written > max_bytes {
            let _ = std::fs::remove_file(dest);
            return Err(format!("Download is larger than {max_bytes} bytes."));
        }
        file.write_all(&buf[..read])
            .map_err(|error| format!("Could not write {}: {error}", dest.display()))?;
    }
    Ok(written)
}

pub fn max_download_bytes() -> u64 {
    DEFAULT_DOWNLOAD_CAP
}

/// Flatten a reqwest error chain so TLS/DNS/connect causes are visible in the UI.
pub fn format_reqwest_error(prefix: &str, error: reqwest::Error) -> String {
    let error = error.without_url();
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
        || error
            .status()
            .is_some_and(|status| status.is_server_error())
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
