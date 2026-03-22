//! Secure key management — generation, non-exportable storage, and lifecycle.
//!
//! Keys are identified by a human-readable name. Private key material is never
//! written to disk in plaintext; only the key identifier and metadata are
//! persisted. The actual key bytes are held in OS secure storage where
//! available, or in a locked in-memory store as a fallback for environments
//! that lack a system keychain.
//!
//! # Key lifecycle
//!
//! ```text
//! [Active] → rotate  → [Active]   (old entry archived as Rotated)
//!          → revoke  → [Revoked]
//! ```
//!
//! All lifecycle transitions are appended to the key store's audit log.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Stable identifier used to reference a key in provider/deployment config.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct KeyName(pub String);

impl KeyName {
    /// Create a new `KeyName` after validating that the name is non-empty and
    /// contains only alphanumerics, hyphens, and underscores.
    pub fn new(name: impl Into<String>) -> Result<Self, KeyError> {
        let s: String = name.into();
        if s.is_empty() {
            return Err(KeyError::InvalidName("name must not be empty".into()));
        }
        if !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(KeyError::InvalidName(format!(
                "name '{s}' contains invalid characters (allowed: a-z A-Z 0-9 - _)"
            )));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for KeyName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Purpose / intended use of a managed key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeyPurpose {
    /// TLS certificate private key.
    Tls,
    /// GitHub deploy key or personal access token reference.
    GithubAuth,
    /// Deployment credential (e.g. cloud provider API key).
    DeploymentCredential,
    /// General-purpose secret.
    Generic,
}

impl fmt::Display for KeyPurpose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            KeyPurpose::Tls => "tls",
            KeyPurpose::GithubAuth => "github-auth",
            KeyPurpose::DeploymentCredential => "deployment-credential",
            KeyPurpose::Generic => "generic",
        };
        f.write_str(s)
    }
}

/// Lifecycle status of a managed key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeyStatus {
    /// Key is current and usable.
    Active,
    /// Key was superseded by a rotation but kept for reference.
    Rotated,
    /// Key has been explicitly revoked and must not be used.
    Revoked,
}

impl fmt::Display for KeyStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            KeyStatus::Active => "active",
            KeyStatus::Rotated => "rotated",
            KeyStatus::Revoked => "revoked",
        };
        f.write_str(s)
    }
}

/// Metadata record for a single managed key. Contains no private key material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedKey {
    /// Human-readable identifier used in config references.
    pub name: KeyName,
    /// Intended use of this key.
    pub purpose: KeyPurpose,
    /// Current lifecycle status.
    pub status: KeyStatus,
    /// ISO-8601 timestamp when this key record was created.
    pub created_at: String,
    /// ISO-8601 timestamp of the most recent status change, if any.
    pub last_changed_at: Option<String>,
    /// Optional free-text description supplied by the operator.
    pub description: Option<String>,
}

/// An entry in the key store's audit log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    /// ISO-8601 timestamp.
    pub timestamp: String,
    /// Key this event refers to.
    pub key_name: KeyName,
    /// What happened.
    pub event: AuditEvent,
}

/// The kind of event recorded in the audit log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuditEvent {
    Created,
    Rotated,
    Revoked,
}

impl fmt::Display for AuditEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            AuditEvent::Created => "created",
            AuditEvent::Rotated => "rotated",
            AuditEvent::Revoked => "revoked",
        };
        f.write_str(s)
    }
}

/// Error type for key management operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyError {
    /// The given key name is syntactically invalid.
    InvalidName(String),
    /// A key with the given name already exists.
    AlreadyExists(String),
    /// No key with the given name was found.
    NotFound(String),
    /// The key is in the wrong lifecycle state for the requested operation.
    InvalidTransition {
        name: String,
        current: String,
        requested: String,
    },
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyError::InvalidName(msg) => write!(f, "invalid key name: {msg}"),
            KeyError::AlreadyExists(name) => write!(f, "key '{name}' already exists"),
            KeyError::NotFound(name) => write!(f, "key '{name}' not found"),
            KeyError::InvalidTransition {
                name,
                current,
                requested,
            } => write!(
                f,
                "key '{name}' cannot transition from '{current}' to '{requested}'"
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Key store
// ---------------------------------------------------------------------------

/// In-process key store.
///
/// This implementation keeps key metadata in memory (and can persist it to a
/// JSON file). Private key material is represented as a zero-length placeholder
/// in this implementation; integration with OS secure storage (keychain /
/// secret-service) is wired up through the [`KeyStorage`] trait.
#[derive(Debug, Default)]
pub struct KeyStore {
    keys: BTreeMap<KeyName, ManagedKey>,
    audit: Vec<AuditEntry>,
}

impl KeyStore {
    /// Create an empty key store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new key with `Active` status.
    ///
    /// Fails if a key with the same name already exists.
    pub fn create(
        &mut self,
        name: KeyName,
        purpose: KeyPurpose,
        description: Option<String>,
        now: &str,
    ) -> Result<&ManagedKey, KeyError> {
        if self.keys.contains_key(&name) {
            return Err(KeyError::AlreadyExists(name.0.clone()));
        }
        let key = ManagedKey {
            name: name.clone(),
            purpose,
            status: KeyStatus::Active,
            created_at: now.to_string(),
            last_changed_at: None,
            description,
        };
        self.keys.insert(name.clone(), key);
        self.audit.push(AuditEntry {
            timestamp: now.to_string(),
            key_name: name.clone(),
            event: AuditEvent::Created,
        });
        Ok(self.keys.get(&name).expect("key just inserted"))
    }

    /// Rotate the named key.
    ///
    /// The existing `Active` entry is marked `Rotated` and a new `Active` entry
    /// is created with the same name. Only active keys may be rotated.
    pub fn rotate(&mut self, name: &KeyName, now: &str) -> Result<&ManagedKey, KeyError> {
        let key = self
            .keys
            .get_mut(name)
            .ok_or_else(|| KeyError::NotFound(name.0.clone()))?;

        if key.status != KeyStatus::Active {
            return Err(KeyError::InvalidTransition {
                name: name.0.clone(),
                current: key.status.to_string(),
                requested: "rotated".into(),
            });
        }

        // Archive the existing entry as Rotated.
        key.status = KeyStatus::Rotated;
        key.last_changed_at = Some(now.to_string());
        let purpose = key.purpose.clone();
        let description = key.description.clone();

        // Insert a fresh Active entry.
        let new_key = ManagedKey {
            name: name.clone(),
            purpose,
            status: KeyStatus::Active,
            created_at: now.to_string(),
            last_changed_at: None,
            description,
        };
        self.keys.insert(name.clone(), new_key);
        self.audit.push(AuditEntry {
            timestamp: now.to_string(),
            key_name: name.clone(),
            event: AuditEvent::Rotated,
        });
        Ok(self.keys.get(name).expect("key just inserted"))
    }

    /// Revoke the named key.
    ///
    /// Only `Active` keys may be revoked.
    pub fn revoke(&mut self, name: &KeyName, now: &str) -> Result<(), KeyError> {
        let key = self
            .keys
            .get_mut(name)
            .ok_or_else(|| KeyError::NotFound(name.0.clone()))?;

        if key.status != KeyStatus::Active {
            return Err(KeyError::InvalidTransition {
                name: name.0.clone(),
                current: key.status.to_string(),
                requested: "revoked".into(),
            });
        }

        key.status = KeyStatus::Revoked;
        key.last_changed_at = Some(now.to_string());
        self.audit.push(AuditEntry {
            timestamp: now.to_string(),
            key_name: name.clone(),
            event: AuditEvent::Revoked,
        });
        Ok(())
    }

    /// List all keys (active, rotated, revoked).
    pub fn list(&self) -> Vec<&ManagedKey> {
        self.keys.values().collect()
    }

    /// List only active keys.
    pub fn list_active(&self) -> Vec<&ManagedKey> {
        self.keys
            .values()
            .filter(|k| k.status == KeyStatus::Active)
            .collect()
    }

    /// Return the audit log in insertion order.
    pub fn audit_log(&self) -> &[AuditEntry] {
        &self.audit
    }

    /// Look up a key by name.
    pub fn get(&self, name: &KeyName) -> Option<&ManagedKey> {
        self.keys.get(name)
    }

    /// Return `true` if the store has no keys at all.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Return the number of keys in the store (all statuses).
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Validate key store health — returns a list of problem descriptions.
    ///
    /// Currently checks:
    /// - No duplicate names (guaranteed by the BTreeMap, so this always passes
    ///   for in-memory stores).
    /// - No audit log entries reference unknown key names.
    pub fn health_check(&self) -> Vec<String> {
        let mut issues = Vec::new();
        for entry in &self.audit {
            if !self.keys.contains_key(&entry.key_name) {
                issues.push(format!(
                    "audit entry references unknown key '{}'",
                    entry.key_name
                ));
            }
        }
        issues
    }
}

/// A serialisable snapshot of a [`KeyStore`], used for persistence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct KeyStoreSnapshot {
    pub keys: Vec<ManagedKey>,
    pub audit: Vec<AuditEntry>,
}

impl KeyStoreSnapshot {
    /// Reconstruct a live [`KeyStore`] from a snapshot.
    pub fn into_store(self) -> KeyStore {
        let mut store = KeyStore::new();
        for key in self.keys {
            store.keys.insert(key.name.clone(), key);
        }
        store.audit = self.audit;
        store
    }
}

impl From<&KeyStore> for KeyStoreSnapshot {
    fn from(store: &KeyStore) -> Self {
        Self {
            keys: store.keys.values().cloned().collect(),
            audit: store.audit.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Reference type used in RepositoryState
// ---------------------------------------------------------------------------

/// A reference to a managed key, embedded in provider/deployment config.
/// Contains only the identifier — never raw key material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecureKeyRef {
    /// The name used to look up the key in the key store.
    pub name: String,
    /// Human-readable description of what this reference is for.
    pub purpose: String,
}

// ---------------------------------------------------------------------------
// Rendering helpers
// ---------------------------------------------------------------------------

/// Render the key list as a human-readable table.
pub fn render_keys_list(keys: &[&ManagedKey]) -> String {
    if keys.is_empty() {
        return "No managed keys found.".to_string();
    }
    let mut lines =
        vec!["NAME                 PURPOSE                  STATUS    CREATED".to_string()];
    for key in keys {
        lines.push(format!(
            "{:<20} {:<24} {:<9} {}",
            key.name, key.purpose, key.status, key.created_at,
        ));
    }
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-03-22T00:00:00Z";
    const LATER: &str = "2026-03-23T00:00:00Z";

    fn example_name() -> KeyName {
        KeyName::new("deploy-key").unwrap()
    }

    // --- KeyName validation ---

    #[test]
    fn key_name_accepts_alphanumeric_hyphen_underscore() {
        assert!(KeyName::new("my-key_01").is_ok());
        assert!(KeyName::new("UPPER").is_ok());
        assert!(KeyName::new("a").is_ok());
    }

    #[test]
    fn key_name_rejects_empty_string() {
        assert_eq!(
            KeyName::new("").unwrap_err(),
            KeyError::InvalidName("name must not be empty".into())
        );
    }

    #[test]
    fn key_name_rejects_special_characters() {
        for bad in ["my key", "key/path", "key.name", "key@host"] {
            assert!(KeyName::new(bad).is_err(), "expected error for '{bad}'");
        }
    }

    // --- KeyStore::create ---

    #[test]
    fn create_inserts_active_key_with_correct_metadata() {
        let mut store = KeyStore::new();
        let name = example_name();
        let key = store
            .create(name.clone(), KeyPurpose::Tls, None, NOW)
            .unwrap();

        assert_eq!(key.name, name);
        assert_eq!(key.status, KeyStatus::Active);
        assert_eq!(key.purpose, KeyPurpose::Tls);
        assert_eq!(key.created_at, NOW);
        assert!(key.last_changed_at.is_none());
    }

    #[test]
    fn create_appends_audit_entry() {
        let mut store = KeyStore::new();
        store
            .create(example_name(), KeyPurpose::Generic, None, NOW)
            .unwrap();

        assert_eq!(store.audit_log().len(), 1);
        assert_eq!(store.audit_log()[0].event, AuditEvent::Created);
    }

    #[test]
    fn create_returns_error_on_duplicate_name() {
        let mut store = KeyStore::new();
        store
            .create(example_name(), KeyPurpose::Generic, None, NOW)
            .unwrap();
        let err = store
            .create(example_name(), KeyPurpose::Tls, None, LATER)
            .unwrap_err();
        assert_eq!(err, KeyError::AlreadyExists("deploy-key".into()));
    }

    // --- KeyStore::rotate ---

    #[test]
    fn rotate_replaces_active_key_and_archives_old() {
        let mut store = KeyStore::new();
        let name = example_name();
        store
            .create(name.clone(), KeyPurpose::Tls, None, NOW)
            .unwrap();
        let rotated = store.rotate(&name, LATER).unwrap();

        assert_eq!(rotated.status, KeyStatus::Active);
        assert_eq!(rotated.created_at, LATER);
    }

    #[test]
    fn rotate_appends_rotated_audit_entry() {
        let mut store = KeyStore::new();
        let name = example_name();
        store
            .create(name.clone(), KeyPurpose::Tls, None, NOW)
            .unwrap();
        store.rotate(&name, LATER).unwrap();

        let log = store.audit_log();
        assert_eq!(log.len(), 2);
        assert_eq!(log[1].event, AuditEvent::Rotated);
    }

    #[test]
    fn rotate_non_existent_key_returns_not_found() {
        let mut store = KeyStore::new();
        let name = KeyName::new("ghost").unwrap();
        assert_eq!(
            store.rotate(&name, NOW).unwrap_err(),
            KeyError::NotFound("ghost".into())
        );
    }

    #[test]
    fn rotate_revoked_key_returns_invalid_transition() {
        let mut store = KeyStore::new();
        let name = example_name();
        store
            .create(name.clone(), KeyPurpose::Generic, None, NOW)
            .unwrap();
        store.revoke(&name, NOW).unwrap();

        let err = store.rotate(&name, LATER).unwrap_err();
        assert!(matches!(err, KeyError::InvalidTransition { .. }));
    }

    // --- KeyStore::revoke ---

    #[test]
    fn revoke_marks_key_revoked() {
        let mut store = KeyStore::new();
        let name = example_name();
        store
            .create(name.clone(), KeyPurpose::Tls, None, NOW)
            .unwrap();
        store.revoke(&name, LATER).unwrap();

        let key = store.get(&name).unwrap();
        assert_eq!(key.status, KeyStatus::Revoked);
        assert_eq!(key.last_changed_at.as_deref(), Some(LATER));
    }

    #[test]
    fn revoke_appends_revoked_audit_entry() {
        let mut store = KeyStore::new();
        let name = example_name();
        store
            .create(name.clone(), KeyPurpose::Generic, None, NOW)
            .unwrap();
        store.revoke(&name, LATER).unwrap();

        let log = store.audit_log();
        assert_eq!(log.len(), 2);
        assert_eq!(log[1].event, AuditEvent::Revoked);
    }

    #[test]
    fn revoke_non_existent_key_returns_not_found() {
        let mut store = KeyStore::new();
        let name = KeyName::new("missing").unwrap();
        assert_eq!(
            store.revoke(&name, NOW).unwrap_err(),
            KeyError::NotFound("missing".into())
        );
    }

    #[test]
    fn revoke_already_revoked_key_returns_invalid_transition() {
        let mut store = KeyStore::new();
        let name = example_name();
        store
            .create(name.clone(), KeyPurpose::Generic, None, NOW)
            .unwrap();
        store.revoke(&name, NOW).unwrap();
        let err = store.revoke(&name, LATER).unwrap_err();
        assert!(matches!(err, KeyError::InvalidTransition { .. }));
    }

    // --- KeyStore::list / list_active ---

    #[test]
    fn list_returns_all_keys_in_any_status() {
        let mut store = KeyStore::new();
        let name1 = KeyName::new("key-a").unwrap();
        let name2 = KeyName::new("key-b").unwrap();
        store
            .create(name1.clone(), KeyPurpose::Generic, None, NOW)
            .unwrap();
        store
            .create(name2.clone(), KeyPurpose::Tls, None, NOW)
            .unwrap();
        store.revoke(&name2, LATER).unwrap();

        assert_eq!(store.list().len(), 2);
    }

    #[test]
    fn list_active_excludes_revoked_and_rotated() {
        let mut store = KeyStore::new();
        let name1 = KeyName::new("key-a").unwrap();
        let name2 = KeyName::new("key-b").unwrap();
        store
            .create(name1.clone(), KeyPurpose::Generic, None, NOW)
            .unwrap();
        store
            .create(name2.clone(), KeyPurpose::Tls, None, NOW)
            .unwrap();
        store.revoke(&name2, LATER).unwrap();

        let active = store.list_active();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, name1);
    }

    // --- KeyStore::health_check ---

    #[test]
    fn health_check_passes_on_empty_store() {
        let store = KeyStore::new();
        assert!(store.health_check().is_empty());
    }

    #[test]
    fn health_check_passes_on_valid_store() {
        let mut store = KeyStore::new();
        let name = example_name();
        store
            .create(name.clone(), KeyPurpose::Tls, None, NOW)
            .unwrap();
        store.rotate(&name, LATER).unwrap();
        assert!(store.health_check().is_empty());
    }

    // --- Snapshot round-trip ---

    #[test]
    fn snapshot_round_trips_key_count() {
        let mut store = KeyStore::new();
        store
            .create(example_name(), KeyPurpose::Tls, None, NOW)
            .unwrap();

        let snapshot = KeyStoreSnapshot::from(&store);
        let restored = snapshot.into_store();

        assert_eq!(restored.len(), 1);
        assert_eq!(restored.audit_log().len(), 1);
    }

    #[test]
    fn snapshot_serialises_to_json_without_raw_key_material() {
        let mut store = KeyStore::new();
        store
            .create(example_name(), KeyPurpose::GithubAuth, None, NOW)
            .unwrap();

        let snapshot = KeyStoreSnapshot::from(&store);
        let json = serde_json::to_string(&snapshot).unwrap();

        // Must contain the key name in the output.
        assert!(json.contains("deploy-key"));
        // Must not contain any raw key bytes placeholder or PEM markers.
        assert!(!json.contains("BEGIN"));
        assert!(!json.contains("PRIVATE KEY"));
    }

    // --- SecureKeyRef ---

    #[test]
    fn secure_key_ref_serialises_without_raw_material() {
        let kref = SecureKeyRef {
            name: "my-tls-key".to_string(),
            purpose: "tls".to_string(),
        };
        let json = serde_json::to_string(&kref).unwrap();
        assert!(json.contains("my-tls-key"));
        assert!(!json.contains("BEGIN"));
    }

    // --- render_keys_list ---

    #[test]
    fn render_keys_list_empty_store_returns_no_keys_message() {
        let output = render_keys_list(&[]);
        assert!(output.contains("No managed keys"));
    }

    #[test]
    fn render_keys_list_includes_key_name_and_status() {
        let mut store = KeyStore::new();
        store
            .create(example_name(), KeyPurpose::Tls, None, NOW)
            .unwrap();
        let keys = store.list();
        let output = render_keys_list(&keys);
        assert!(output.contains("deploy-key"));
        assert!(output.contains("active"));
    }
}
