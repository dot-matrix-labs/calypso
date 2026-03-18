//! Structured event log for webview trigger and cron-now actions.
//!
//! Entries are appended to `.calypso/event-log.json` and the file is capped at
//! [`MAX_ENTRIES`] entries — oldest entries are dropped when the limit is exceeded.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const LOG_FILE: &str = "event-log.json";
const MAX_ENTRIES: usize = 100;

// ── Public types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventKind {
    Trigger,
    Cron,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventLogEntry {
    /// Unique identifier: `{unix_ms}-{hex4}`.
    pub id: String,
    pub kind: EventKind,
    /// The event name (for triggers) or workflow name (for cron runs).
    pub name: String,
    /// ISO 8601 UTC timestamp.
    pub timestamp: String,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Append a new entry to the event log and return it.
///
/// Reads the existing log, prepends the new entry, truncates to [`MAX_ENTRIES`],
/// and atomically writes the result back.
pub fn append(calypso_dir: &Path, kind: EventKind, name: &str) -> EventLogEntry {
    let entry = EventLogEntry {
        id: generate_id(),
        kind,
        name: name.to_string(),
        timestamp: utc_now(),
    };

    let mut entries = read_log(calypso_dir);
    entries.insert(0, entry.clone());
    entries.truncate(MAX_ENTRIES);
    write_log(calypso_dir, &entries);

    entry
}

/// Read the current event log. Returns an empty vec if the file is missing or unparseable.
pub fn read_log(calypso_dir: &Path) -> Vec<EventLogEntry> {
    let path = calypso_dir.join(LOG_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    serde_json::from_str(&text).unwrap_or_default()
}

// ── Internals ─────────────────────────────────────────────────────────────────

fn write_log(calypso_dir: &Path, entries: &[EventLogEntry]) {
    let path = calypso_dir.join(LOG_FILE);
    let tmp = calypso_dir.join("event-log.json.tmp");
    if let Ok(json) = serde_json::to_string_pretty(entries) {
        let _ = std::fs::write(&tmp, json);
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// Generate a unique-enough ID without external dependencies.
///
/// Format: `{unix_ms}-{pseudo_random_hex4}` where the pseudo-random part is
/// derived from the nanosecond component of the current time — sufficient for
/// a sequential local log where writes are human-paced.
fn generate_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let ms = now.as_millis();
    let noise = (now.subsec_nanos() ^ (now.as_secs() as u32).wrapping_mul(0x9e37_79b9)) & 0xffff;
    format!("{ms:013}-{noise:04x}")
}

fn utc_now() -> String {
    use std::time::SystemTime;
    // Format as ISO 8601 / RFC 3339 without chrono to keep the impl minimal.
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (year, month, day, hour, min, sec) = epoch_secs_to_parts(secs);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
}

/// Convert a Unix epoch seconds value to (year, month, day, hour, min, sec).
/// Handles years 1970–2099 without any external date library.
fn epoch_secs_to_parts(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let sec = (secs % 60) as u32;
    let mins = secs / 60;
    let min = (mins % 60) as u32;
    let hours = mins / 60;
    let hour = (hours % 24) as u32;
    let days = hours / 24;

    // Determine year from day count since 1970-01-01.
    let mut year = 1970u32;
    let mut remaining = days;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }

    let month_days: [u64; 12] = [
        31,
        if is_leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u32;
    for &md in &month_days {
        if remaining < md {
            break;
        }
        remaining -= md;
        month += 1;
    }
    let day = remaining as u32 + 1;

    (year, month, day, hour, min, sec)
}

fn is_leap(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("calypso-eventlog-{name}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn append_creates_log_file() {
        let dir = tmp_dir("create");
        append(&dir, EventKind::Trigger, "planning-task-identified");
        assert!(dir.join("event-log.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_entry_has_correct_fields() {
        let dir = tmp_dir("fields");
        let entry = append(&dir, EventKind::Cron, "calypso-orchestrator-startup");
        assert_eq!(entry.kind, EventKind::Cron);
        assert_eq!(entry.name, "calypso-orchestrator-startup");
        assert!(!entry.id.is_empty());
        assert!(entry.timestamp.contains('T'));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_accumulates_entries_newest_first() {
        let dir = tmp_dir("order");
        append(&dir, EventKind::Trigger, "first");
        append(&dir, EventKind::Trigger, "second");
        append(&dir, EventKind::Trigger, "third");
        let log = read_log(&dir);
        assert_eq!(log.len(), 3);
        assert_eq!(log[0].name, "third");
        assert_eq!(log[2].name, "first");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_caps_at_max_entries() {
        let dir = tmp_dir("cap");
        for i in 0..=MAX_ENTRIES + 5 {
            append(&dir, EventKind::Trigger, &format!("event-{i}"));
        }
        let log = read_log(&dir);
        assert_eq!(log.len(), MAX_ENTRIES);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_log_returns_empty_when_file_missing() {
        let dir = tmp_dir("missing");
        let log = read_log(&dir);
        assert!(log.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ids_are_unique_across_sequential_appends() {
        let dir = tmp_dir("unique");
        let a = append(&dir, EventKind::Trigger, "a");
        let b = append(&dir, EventKind::Trigger, "b");
        // IDs may collide only if both nanos and ms are identical — extremely unlikely.
        // This is a smoke test, not a statistical guarantee.
        assert_ne!(a.id, b.id);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn timestamp_format_is_iso8601() {
        let dir = tmp_dir("ts");
        let entry = append(&dir, EventKind::Trigger, "ts-test");
        // Expect YYYY-MM-DDTHH:MM:SSZ
        let ts = &entry.timestamp;
        assert_eq!(ts.len(), 20);
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
        assert_eq!(&ts[19..20], "Z");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn epoch_secs_to_parts_known_date() {
        // 2024-01-15 12:30:45 UTC = 1705321845
        let (y, mo, d, h, mi, s) = epoch_secs_to_parts(1_705_321_845);
        assert_eq!(y, 2024);
        assert_eq!(mo, 1);
        assert_eq!(d, 15);
        assert_eq!(h, 12);
        assert_eq!(mi, 30);
        assert_eq!(s, 45);
    }

    #[test]
    fn epoch_secs_to_parts_leap_year_feb29() {
        // 2024-02-29 00:00:00 UTC = 1709164800
        let (y, mo, d, _h, _mi, _s) = epoch_secs_to_parts(1_709_164_800);
        assert_eq!(y, 2024);
        assert_eq!(mo, 2);
        assert_eq!(d, 29);
    }

    #[test]
    fn generate_id_has_expected_format() {
        let id = generate_id();
        // Format: {13-digit-ms}-{4-hex}
        let parts: Vec<&str> = id.splitn(2, '-').collect();
        assert_eq!(parts.len(), 2);
        assert!(parts[0].len() >= 10, "ms part too short: {id}");
        assert_eq!(parts[1].len(), 4, "hex suffix wrong length: {id}");
        assert!(
            parts[1].chars().all(|c| c.is_ascii_hexdigit()),
            "non-hex in suffix: {id}"
        );
    }
}
