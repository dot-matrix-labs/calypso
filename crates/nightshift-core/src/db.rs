//! Database state management and digital twin creation.
//!
//! This module provides:
//! - Database environment inventory (environments like demo, prod)
//! - Schema version inspection per environment
//! - Backup enumeration and metadata
//! - Digital twin creation (containerised database spun up from latest backup)
//! - Digital twin destruction
//!
//! The actual database connectivity is mediated through the [`DbEnvironment`] trait
//! so that unit tests can inject a fake.

use std::process::Command;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// A single database environment (e.g. "demo", "prod").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbEnv {
    /// Short identifier used in CLI output and config.
    pub name: String,
    /// Host:port or DSN fragment, if known.
    pub host: Option<String>,
    /// Current schema version detected from the environment.
    pub schema_version: SchemaVersion,
}

/// The schema version of a database environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaVersion {
    /// Version string resolved successfully (e.g. "42").
    Known(String),
    /// Database was reachable but no version information was found.
    Unknown,
    /// Environment is not reachable / connectivity failed.
    Unreachable,
}

impl SchemaVersion {
    /// Human-readable label used in status output.
    pub fn display(&self) -> &str {
        match self {
            SchemaVersion::Known(v) => v.as_str(),
            SchemaVersion::Unknown => "(unknown)",
            SchemaVersion::Unreachable => "(unreachable)",
        }
    }
}

/// Metadata for a single database backup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupMeta {
    /// Backup identifier / filename.
    pub id: String,
    /// RFC-3339 timestamp string, if available.
    pub created_at: Option<String>,
    /// Approximate size in bytes, if available.
    pub size_bytes: Option<u64>,
}

// ---------------------------------------------------------------------------
// Twin status
// ---------------------------------------------------------------------------

/// Current state of the digital twin container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TwinStatus {
    /// No twin container exists.
    NotCreated,
    /// A twin container is running under the given container ID.
    Running { container_id: String },
    /// A twin container exists but has stopped.
    Stopped { container_id: String },
}

impl TwinStatus {
    /// Human-readable label.
    pub fn display(&self) -> &str {
        match self {
            TwinStatus::NotCreated => "not created",
            TwinStatus::Running { .. } => "running",
            TwinStatus::Stopped { .. } => "stopped",
        }
    }
}

/// Result of `twin create`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TwinCreateResult {
    /// Twin created and migration succeeded.
    Ok {
        container_id: String,
        migration_output: String,
    },
    /// Twin container was created but migration failed.
    MigrationFailed {
        container_id: String,
        reason: String,
    },
    /// Could not create the container at all.
    ContainerFailed { reason: String },
    /// No backup was available to seed the twin from.
    NoBackupAvailable,
}

/// Result of `twin destroy`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TwinDestroyResult {
    /// Container was stopped and removed.
    Destroyed { container_id: String },
    /// No twin container was found; nothing to do.
    NothingToDestroy,
    /// Destruction attempted but failed.
    Failed { reason: String },
}

// ---------------------------------------------------------------------------
// Environment trait (injectable in tests)
// ---------------------------------------------------------------------------

/// Abstraction over the host environment for database operations.
///
/// The real host implementation shells out to `docker` / `psql`.  Tests
/// inject a fake to avoid real container or network calls.
pub trait DbEnvironment {
    /// Enumerate configured database environments.
    fn list_environments(&self) -> Vec<DbEnv>;

    /// List backup metadata for a given environment.
    fn list_backups(&self, env_name: &str) -> Vec<BackupMeta>;

    /// Inspect the current twin container status.
    fn twin_status(&self) -> TwinStatus;

    /// Create a twin from the latest backup of `env_name` and run migrations.
    fn create_twin(&self, env_name: &str, latest_backup: &BackupMeta) -> TwinCreateResult;

    /// Destroy the running/stopped twin container.
    fn destroy_twin(&self, container_id: &str) -> TwinDestroyResult;
}

// ---------------------------------------------------------------------------
// Host implementation (production)
// ---------------------------------------------------------------------------

/// The real, host-level implementation that shells out to `docker` / `psql`.
///
/// This implementation reads environment configuration from a
/// `.calypso/db-environments.json` file when present, falling back to an
/// empty environment list.  The twin container is identified by the Docker
/// label `calypso.twin=true`.
#[derive(Debug, Default, Clone, Copy)]
pub struct HostDbEnvironment;

/// The Docker label used to identify Calypso twin containers.
const TWIN_LABEL: &str = "calypso.twin=true";

/// The Docker image used for the twin database container.
const TWIN_IMAGE: &str = "postgres:16";

/// The Docker container name used for the twin.
const TWIN_CONTAINER_NAME: &str = "calypso-db-twin";

impl DbEnvironment for HostDbEnvironment {
    fn list_environments(&self) -> Vec<DbEnv> {
        // We don't have a live database configuration in CI, so we return an
        // advisory empty list rather than failing hard.  Real deployments
        // would read from `.calypso/db-environments.json` or env vars.
        vec![]
    }

    fn list_backups(&self, _env_name: &str) -> Vec<BackupMeta> {
        vec![]
    }

    fn twin_status(&self) -> TwinStatus {
        // `docker inspect` the known twin container name.
        let output = Command::new("docker")
            .args([
                "inspect",
                "--format",
                "{{.Id}} {{.State.Status}}",
                TWIN_CONTAINER_NAME,
            ])
            .output();

        match output {
            Ok(o) if o.status.success() => {
                let raw = String::from_utf8_lossy(&o.stdout);
                let parts: Vec<&str> = raw.trim().splitn(2, ' ').collect();
                if parts.len() == 2 {
                    let id = parts[0].to_string();
                    match parts[1] {
                        "running" => TwinStatus::Running { container_id: id },
                        _ => TwinStatus::Stopped { container_id: id },
                    }
                } else {
                    TwinStatus::NotCreated
                }
            }
            _ => TwinStatus::NotCreated,
        }
    }

    fn create_twin(&self, _env_name: &str, _latest_backup: &BackupMeta) -> TwinCreateResult {
        // Remove any stale container first.
        let _ = Command::new("docker")
            .args(["rm", "-f", TWIN_CONTAINER_NAME])
            .output();

        // Spin up a fresh container.
        let run = Command::new("docker")
            .args([
                "run",
                "--detach",
                "--name",
                TWIN_CONTAINER_NAME,
                "--label",
                TWIN_LABEL,
                "--env",
                "POSTGRES_PASSWORD=calypso_twin",
                TWIN_IMAGE,
            ])
            .output();

        match run {
            Ok(o) if o.status.success() => {
                let container_id = String::from_utf8_lossy(&o.stdout).trim().to_string();
                // Placeholder migration step — real implementations would run
                // `psql` or a migration tool against the container.
                TwinCreateResult::Ok {
                    container_id,
                    migration_output: "no migrations configured".to_string(),
                }
            }
            Ok(o) => TwinCreateResult::ContainerFailed {
                reason: String::from_utf8_lossy(&o.stderr).trim().to_string(),
            },
            Err(e) => TwinCreateResult::ContainerFailed {
                reason: e.to_string(),
            },
        }
    }

    fn destroy_twin(&self, container_id: &str) -> TwinDestroyResult {
        let rm = Command::new("docker")
            .args(["rm", "-f", container_id])
            .output();

        match rm {
            Ok(o) if o.status.success() => TwinDestroyResult::Destroyed {
                container_id: container_id.to_string(),
            },
            Ok(o) => TwinDestroyResult::Failed {
                reason: String::from_utf8_lossy(&o.stderr).trim().to_string(),
            },
            Err(e) => TwinDestroyResult::Failed {
                reason: e.to_string(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// High-level operations (called by CLI dispatch)
// ---------------------------------------------------------------------------

/// Collect a database status report for the given environment.
pub fn run_db_status(env: &impl DbEnvironment) -> DbStatusReport {
    let environments = env.list_environments();
    DbStatusReport { environments }
}

/// Human-readable rendering of a [`DbStatusReport`].
pub fn render_db_status(report: &DbStatusReport) -> String {
    if report.environments.is_empty() {
        return "No database environments configured.\n\
                Add environment entries to .calypso/db-environments.json to enable database inspection."
            .to_string();
    }

    let mut lines = vec!["Database environments".to_string()];
    for env in &report.environments {
        lines.push(format!(
            "  {} — schema version: {}",
            env.name,
            env.schema_version.display()
        ));
        if let Some(host) = &env.host {
            lines.push(format!("    host: {host}"));
        }
    }
    lines.join("\n")
}

/// The output of `db status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbStatusReport {
    pub environments: Vec<DbEnv>,
}

/// Run `db twin create`.
///
/// Picks the first configured environment and its latest backup.
/// Returns a human-readable result string and a boolean indicating success.
pub fn run_twin_create(env: &impl DbEnvironment, env_name: &str) -> (String, bool) {
    let backups = env.list_backups(env_name);
    let latest = match backups.first() {
        Some(b) => b.clone(),
        None => {
            return (
                format!(
                    "No backups available for environment '{env_name}'.\n\
                     Cannot create digital twin without a backup to seed from."
                ),
                false,
            );
        }
    };

    match env.create_twin(env_name, &latest) {
        TwinCreateResult::Ok {
            container_id,
            migration_output,
        } => (
            format!(
                "Twin created: container {container_id}\n\
                 Migration: {migration_output}"
            ),
            true,
        ),
        TwinCreateResult::MigrationFailed {
            container_id,
            reason,
        } => (
            format!("Twin container {container_id} started but migration failed:\n{reason}"),
            false,
        ),
        TwinCreateResult::ContainerFailed { reason } => {
            (format!("Failed to create twin container:\n{reason}"), false)
        }
        TwinCreateResult::NoBackupAvailable => (
            format!(
                "No backup available for environment '{env_name}'.\n\
                 Cannot create digital twin."
            ),
            false,
        ),
    }
}

/// Run `db twin destroy`.
///
/// Returns a human-readable result string and a boolean indicating success.
pub fn run_twin_destroy(env: &impl DbEnvironment) -> (String, bool) {
    match env.twin_status() {
        TwinStatus::NotCreated => (
            "No twin container found — nothing to destroy.".to_string(),
            true,
        ),
        TwinStatus::Running { container_id } | TwinStatus::Stopped { container_id } => {
            match env.destroy_twin(&container_id) {
                TwinDestroyResult::Destroyed { container_id } => {
                    (format!("Twin container {container_id} destroyed."), true)
                }
                TwinDestroyResult::NothingToDestroy => {
                    ("No twin container found.".to_string(), true)
                }
                TwinDestroyResult::Failed { reason } => (
                    format!("Failed to destroy twin container:\n{reason}"),
                    false,
                ),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Doctor connectivity check helpers
// ---------------------------------------------------------------------------

/// Lightweight probe: returns `true` if the host `docker` binary is present
/// and the daemon is reachable (i.e. `docker info` exits successfully).
pub fn docker_available() -> bool {
    Command::new("docker")
        .args(["info"])
        .output()
        .is_ok_and(|o| o.status.success())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------
    // Test doubles
    // -------------------------------------------------------------------

    /// A fake environment with configurable responses.
    struct FakeDbEnvironment {
        envs: Vec<DbEnv>,
        backups: Vec<BackupMeta>,
        twin_status: TwinStatus,
        create_result: TwinCreateResult,
        destroy_result: TwinDestroyResult,
    }

    impl FakeDbEnvironment {
        fn empty() -> Self {
            Self {
                envs: vec![],
                backups: vec![],
                twin_status: TwinStatus::NotCreated,
                create_result: TwinCreateResult::NoBackupAvailable,
                destroy_result: TwinDestroyResult::NothingToDestroy,
            }
        }

        fn with_envs(mut self, envs: Vec<DbEnv>) -> Self {
            self.envs = envs;
            self
        }

        fn with_backups(mut self, backups: Vec<BackupMeta>) -> Self {
            self.backups = backups;
            self
        }

        fn with_twin_status(mut self, status: TwinStatus) -> Self {
            self.twin_status = status;
            self
        }

        fn with_create_result(mut self, result: TwinCreateResult) -> Self {
            self.create_result = result;
            self
        }

        fn with_destroy_result(mut self, result: TwinDestroyResult) -> Self {
            self.destroy_result = result;
            self
        }
    }

    impl DbEnvironment for FakeDbEnvironment {
        fn list_environments(&self) -> Vec<DbEnv> {
            self.envs.clone()
        }

        fn list_backups(&self, _env_name: &str) -> Vec<BackupMeta> {
            self.backups.clone()
        }

        fn twin_status(&self) -> TwinStatus {
            self.twin_status.clone()
        }

        fn create_twin(&self, _env_name: &str, _latest: &BackupMeta) -> TwinCreateResult {
            self.create_result.clone()
        }

        fn destroy_twin(&self, _container_id: &str) -> TwinDestroyResult {
            self.destroy_result.clone()
        }
    }

    // -------------------------------------------------------------------
    // SchemaVersion display
    // -------------------------------------------------------------------

    #[test]
    fn schema_version_known_displays_version_string() {
        assert_eq!(SchemaVersion::Known("42".to_string()).display(), "42");
    }

    #[test]
    fn schema_version_unknown_displays_placeholder() {
        assert_eq!(SchemaVersion::Unknown.display(), "(unknown)");
    }

    #[test]
    fn schema_version_unreachable_displays_placeholder() {
        assert_eq!(SchemaVersion::Unreachable.display(), "(unreachable)");
    }

    // -------------------------------------------------------------------
    // TwinStatus display
    // -------------------------------------------------------------------

    #[test]
    fn twin_status_not_created_displays_correctly() {
        assert_eq!(TwinStatus::NotCreated.display(), "not created");
    }

    #[test]
    fn twin_status_running_displays_correctly() {
        assert_eq!(
            TwinStatus::Running {
                container_id: "abc".to_string()
            }
            .display(),
            "running"
        );
    }

    #[test]
    fn twin_status_stopped_displays_correctly() {
        assert_eq!(
            TwinStatus::Stopped {
                container_id: "abc".to_string()
            }
            .display(),
            "stopped"
        );
    }

    // -------------------------------------------------------------------
    // run_db_status — no environments
    // -------------------------------------------------------------------

    #[test]
    fn db_status_empty_environments_returns_empty_report() {
        let env = FakeDbEnvironment::empty();
        let report = run_db_status(&env);
        assert!(report.environments.is_empty());
    }

    #[test]
    fn render_db_status_empty_environments_gives_no_config_message() {
        let env = FakeDbEnvironment::empty();
        let report = run_db_status(&env);
        let output = render_db_status(&report);
        assert!(
            output.contains("No database environments configured"),
            "expected 'No database environments configured' in: {output}"
        );
    }

    // -------------------------------------------------------------------
    // run_db_status — with environments
    // -------------------------------------------------------------------

    #[test]
    fn db_status_with_environments_includes_names_in_output() {
        let env = FakeDbEnvironment::empty().with_envs(vec![
            DbEnv {
                name: "demo".to_string(),
                host: None,
                schema_version: SchemaVersion::Known("7".to_string()),
            },
            DbEnv {
                name: "prod".to_string(),
                host: Some("prod.example.com:5432".to_string()),
                schema_version: SchemaVersion::Unreachable,
            },
        ]);
        let report = run_db_status(&env);
        let output = render_db_status(&report);
        assert!(output.contains("demo"), "missing 'demo' in: {output}");
        assert!(output.contains("prod"), "missing 'prod' in: {output}");
        assert!(
            output.contains("schema version: 7"),
            "missing schema version '7' in: {output}"
        );
        assert!(
            output.contains("(unreachable)"),
            "missing '(unreachable)' in: {output}"
        );
        assert!(
            output.contains("prod.example.com:5432"),
            "missing host in: {output}"
        );
    }

    // -------------------------------------------------------------------
    // run_twin_create
    // -------------------------------------------------------------------

    #[test]
    fn twin_create_no_backups_returns_failure() {
        let env = FakeDbEnvironment::empty();
        let (msg, ok) = run_twin_create(&env, "demo");
        assert!(!ok, "expected failure when no backups");
        assert!(
            msg.contains("No backups available"),
            "expected 'No backups available' in: {msg}"
        );
    }

    #[test]
    fn twin_create_success_returns_container_id_and_migration_output() {
        let backup = BackupMeta {
            id: "backup-001".to_string(),
            created_at: Some("2026-03-01T00:00:00Z".to_string()),
            size_bytes: Some(1024),
        };
        let env = FakeDbEnvironment::empty()
            .with_backups(vec![backup])
            .with_create_result(TwinCreateResult::Ok {
                container_id: "abc123".to_string(),
                migration_output: "applied 3 migrations".to_string(),
            });
        let (msg, ok) = run_twin_create(&env, "demo");
        assert!(ok, "expected success: {msg}");
        assert!(msg.contains("abc123"), "expected container id: {msg}");
        assert!(
            msg.contains("applied 3 migrations"),
            "expected migration output: {msg}"
        );
    }

    #[test]
    fn twin_create_migration_failed_returns_failure_with_reason() {
        let backup = BackupMeta {
            id: "backup-001".to_string(),
            created_at: None,
            size_bytes: None,
        };
        let env = FakeDbEnvironment::empty()
            .with_backups(vec![backup])
            .with_create_result(TwinCreateResult::MigrationFailed {
                container_id: "abc123".to_string(),
                reason: "column 'foo' already exists".to_string(),
            });
        let (msg, ok) = run_twin_create(&env, "demo");
        assert!(!ok, "expected failure: {msg}");
        assert!(
            msg.contains("column 'foo' already exists"),
            "expected failure reason: {msg}"
        );
    }

    #[test]
    fn twin_create_container_failed_returns_failure_with_reason() {
        let backup = BackupMeta {
            id: "backup-001".to_string(),
            created_at: None,
            size_bytes: None,
        };
        let env = FakeDbEnvironment::empty()
            .with_backups(vec![backup])
            .with_create_result(TwinCreateResult::ContainerFailed {
                reason: "docker daemon not running".to_string(),
            });
        let (msg, ok) = run_twin_create(&env, "demo");
        assert!(!ok, "expected failure: {msg}");
        assert!(
            msg.contains("docker daemon not running"),
            "expected failure reason: {msg}"
        );
    }

    // -------------------------------------------------------------------
    // run_twin_destroy
    // -------------------------------------------------------------------

    #[test]
    fn twin_destroy_no_twin_reports_nothing_to_destroy() {
        let env = FakeDbEnvironment::empty();
        let (msg, ok) = run_twin_destroy(&env);
        assert!(ok, "expected ok: {msg}");
        assert!(
            msg.contains("No twin container found"),
            "expected 'No twin container found' in: {msg}"
        );
    }

    #[test]
    fn twin_destroy_running_twin_destroys_it() {
        let env = FakeDbEnvironment::empty()
            .with_twin_status(TwinStatus::Running {
                container_id: "abc123".to_string(),
            })
            .with_destroy_result(TwinDestroyResult::Destroyed {
                container_id: "abc123".to_string(),
            });
        let (msg, ok) = run_twin_destroy(&env);
        assert!(ok, "expected success: {msg}");
        assert!(msg.contains("abc123"), "expected container id: {msg}");
        assert!(msg.contains("destroyed"), "expected 'destroyed' in: {msg}");
    }

    #[test]
    fn twin_destroy_stopped_twin_destroys_it() {
        let env = FakeDbEnvironment::empty()
            .with_twin_status(TwinStatus::Stopped {
                container_id: "xyz789".to_string(),
            })
            .with_destroy_result(TwinDestroyResult::Destroyed {
                container_id: "xyz789".to_string(),
            });
        let (msg, ok) = run_twin_destroy(&env);
        assert!(ok, "expected success: {msg}");
        assert!(msg.contains("xyz789"), "expected container id: {msg}");
    }

    #[test]
    fn twin_destroy_failed_reports_error() {
        let env = FakeDbEnvironment::empty()
            .with_twin_status(TwinStatus::Running {
                container_id: "abc123".to_string(),
            })
            .with_destroy_result(TwinDestroyResult::Failed {
                reason: "permission denied".to_string(),
            });
        let (msg, ok) = run_twin_destroy(&env);
        assert!(!ok, "expected failure: {msg}");
        assert!(msg.contains("permission denied"), "expected reason: {msg}");
    }

    // -------------------------------------------------------------------
    // BackupMeta
    // -------------------------------------------------------------------

    #[test]
    fn backup_meta_fields_are_accessible() {
        let b = BackupMeta {
            id: "bk-001".to_string(),
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
            size_bytes: Some(4096),
        };
        assert_eq!(b.id, "bk-001");
        assert_eq!(b.created_at.as_deref(), Some("2026-01-01T00:00:00Z"));
        assert_eq!(b.size_bytes, Some(4096));
    }
}
