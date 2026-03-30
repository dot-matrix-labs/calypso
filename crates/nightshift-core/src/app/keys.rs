use std::path::Path;

use crate::error::CalypsoError;
use crate::keys::{KeyName, KeyStore, KeyStoreSnapshot, render_keys_list};
use crate::timefmt;

/// Path to the key store snapshot file, relative to `.calypso/`.
const KEY_STORE_FILE: &str = "keys.json";

fn load_key_store(cwd: &Path) -> Result<KeyStore, CalypsoError> {
    let path = cwd.join(".calypso").join(KEY_STORE_FILE);
    if !path.exists() {
        return Ok(KeyStore::new());
    }
    let json = std::fs::read_to_string(&path)
        .map_err(|e| CalypsoError::state_load(format!("cannot read {}: {e}", path.display())))?;
    let snapshot: KeyStoreSnapshot = serde_json::from_str(&json)
        .map_err(|e| CalypsoError::state_load(format!("key store JSON invalid: {e}")))?;
    Ok(snapshot.into_store())
}

fn save_key_store(store: &KeyStore, cwd: &Path) -> Result<(), CalypsoError> {
    let calypso_dir = cwd.join(".calypso");
    std::fs::create_dir_all(&calypso_dir)
        .map_err(|e| CalypsoError::state_load(format!("cannot create .calypso/: {e}")))?;
    let path = calypso_dir.join(KEY_STORE_FILE);
    let tmp = path.with_extension("tmp");
    let snapshot = KeyStoreSnapshot::from(store);
    let json = serde_json::to_string_pretty(&snapshot)
        .map_err(|e| CalypsoError::state_load(format!("serialization error: {e}")))?;
    std::fs::write(&tmp, json)
        .map_err(|e| CalypsoError::state_load(format!("write error: {e}")))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| CalypsoError::state_load(format!("rename error: {e}")))?;
    Ok(())
}

fn now_iso8601() -> String {
    timefmt::now_iso8601()
}

/// `calypso keys list` — list all managed keys with metadata.
pub fn run_keys_list(cwd: &Path) -> Result<String, CalypsoError> {
    let store = load_key_store(cwd)?;
    let keys = store.list();
    Ok(render_keys_list(&keys))
}

/// `calypso keys list --json` — list all managed keys as JSON.
pub fn run_keys_list_json(cwd: &Path) -> Result<String, CalypsoError> {
    let store = load_key_store(cwd)?;
    let snapshot = KeyStoreSnapshot::from(&store);
    serde_json::to_string_pretty(&snapshot.keys)
        .map_err(|e| CalypsoError::state_load(format!("serialization error: {e}")))
}

/// `calypso keys rotate <name>` — rotate the named key.
pub fn run_keys_rotate(cwd: &Path, name: &str) -> Result<String, CalypsoError> {
    let key_name = KeyName::new(name).map_err(|e| CalypsoError::state_load(e.to_string()))?;
    let mut store = load_key_store(cwd)?;
    let now = now_iso8601();
    store
        .rotate(&key_name, &now)
        .map_err(|e| CalypsoError::state_load(e.to_string()))?;
    save_key_store(&store, cwd)?;
    Ok(format!("Key '{name}' rotated successfully."))
}

/// `calypso keys revoke <name>` — revoke the named key.
pub fn run_keys_revoke(cwd: &Path, name: &str) -> Result<String, CalypsoError> {
    let key_name = KeyName::new(name).map_err(|e| CalypsoError::state_load(e.to_string()))?;
    let mut store = load_key_store(cwd)?;
    let now = now_iso8601();
    store
        .revoke(&key_name, &now)
        .map_err(|e| CalypsoError::state_load(e.to_string()))?;
    save_key_store(&store, cwd)?;
    Ok(format!("Key '{name}' revoked successfully."))
}
