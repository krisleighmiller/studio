//! API secrets live in the system Secret Service, not in settings.json.
//!
//! Tests use a process-local map so they never touch the login keyring.

#[cfg(test)]
use std::collections::BTreeMap;
use std::path::Path;
#[cfg(test)]
use std::sync::Mutex;

#[cfg(not(test))]
const SERVICE: &str = "video-workbench";

#[cfg(test)]
static MEMORY: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

pub fn get(dir: &Path, api_id: &str, field: &str) -> Result<Option<String>, String> {
    #[cfg(test)]
    {
        let key = memory_key(dir, api_id, field);
        let guard = MEMORY.lock().map_err(|_| "secret store lock".to_string())?;
        return Ok(guard.get(&key).cloned());
    }
    #[cfg(not(test))]
    {
        let _ = dir;
        match os_entry(api_id, field)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(error) if is_missing(&error) => Ok(None),
            Err(error) => Err(format!("Could not read a saved API secret: {error}")),
        }
    }
}

pub fn set(dir: &Path, api_id: &str, field: &str, secret: &str) -> Result<(), String> {
    let secret = secret.trim();
    if secret.is_empty() {
        return delete(dir, api_id, field);
    }
    #[cfg(test)]
    {
        let key = memory_key(dir, api_id, field);
        let mut guard = MEMORY.lock().map_err(|_| "secret store lock".to_string())?;
        guard.insert(key, secret.to_string());
        return Ok(());
    }
    #[cfg(not(test))]
    {
        let _ = dir;
        os_entry(api_id, field)?
            .set_password(secret)
            .map_err(|error| format!("Could not save an API secret to the system keyring: {error}"))
    }
}

pub fn delete(dir: &Path, api_id: &str, field: &str) -> Result<(), String> {
    #[cfg(test)]
    {
        let key = memory_key(dir, api_id, field);
        let mut guard = MEMORY.lock().map_err(|_| "secret store lock".to_string())?;
        guard.remove(&key);
        return Ok(());
    }
    #[cfg(not(test))]
    {
        let _ = dir;
        match os_entry(api_id, field)?.delete_credential() {
            Ok(()) => Ok(()),
            Err(error) if is_missing(&error) => Ok(()),
            Err(error) => Err(format!("Could not remove an API secret: {error}")),
        }
    }
}

#[cfg(test)]
fn memory_key(dir: &Path, api_id: &str, field: &str) -> String {
    format!("{}::{api_id}::{field}", dir.display())
}

#[cfg(not(test))]
fn os_entry(api_id: &str, field: &str) -> Result<keyring_core::Entry, String> {
    ensure_store()?;
    keyring_core::Entry::new(SERVICE, &format!("{api_id}::{field}"))
        .map_err(|error| format!("Could not open the system keyring: {error}"))
}

#[cfg(not(test))]
fn ensure_store() -> Result<(), String> {
    use std::sync::OnceLock;
    static READY: OnceLock<Result<(), String>> = OnceLock::new();
    READY
        .get_or_init(|| {
            let store = zbus_secret_service_keyring_store::Store::new()
                .map_err(|error| format!("Could not open the system keyring: {error}"))?;
            keyring_core::set_default_store(store);
            Ok(())
        })
        .clone()
}

#[cfg(not(test))]
fn is_missing(error: &keyring_core::Error) -> bool {
    matches!(error, keyring_core::Error::NoEntry)
}
