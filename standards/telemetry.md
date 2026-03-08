# Telemetry & Feedback Loop

## Architecture
Dedicated SQLite `telemetry.db`, separate from app DB. Different lifecycle (rotated), access pattern (analytical), permissions (agent=read-only). May remain SQLite at V1 or move to separate Postgres schema.

## Schema
```sql
CREATE TABLE errors (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    fingerprint TEXT UNIQUE NOT NULL,
    error_type TEXT NOT NULL,
    message TEXT NOT NULL,
    source_location TEXT,
    stack_trace TEXT,
    first_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
    occurrence_count INTEGER NOT NULL DEFAULT 1,
    resolved_at TEXT,
    resolved_by_commit TEXT
);

CREATE TABLE traces (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    trace_id TEXT UNIQUE NOT NULL,
    parent_trace_id TEXT,
    method TEXT, path TEXT, status_code INTEGER, duration_ms INTEGER,
    user_id TEXT, tenant_id TEXT,
    error_fingerprint TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    metadata TEXT
);

CREATE TABLE metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL, value REAL NOT NULL, tags TEXT,
    recorded_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE agent_observations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL, agent TEXT NOT NULL,
    observation_type TEXT NOT NULL, summary TEXT NOT NULL,
    query_used TEXT, action_taken TEXT, task_reference TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_errors_unresolved ON errors(resolved_at) WHERE resolved_at IS NULL;
CREATE INDEX idx_errors_last_seen ON errors(last_seen_at);
CREATE INDEX idx_traces_created ON traces(created_at);
CREATE INDEX idx_traces_error ON traces(error_fingerprint) WHERE error_fingerprint IS NOT NULL;
CREATE INDEX idx_traces_slow ON traces(duration_ms) WHERE duration_ms > 1000;
CREATE INDEX idx_metrics_name_time ON metrics(name, recorded_at);
```

## Ingestion
- **Errors:** fingerprint = `hash(type + message + location)`. UPSERT: increment count, update `last_seen_at`. Link to trace.
- **Traces:** middleware generates `trace_id` per request (propagate `X-Trace-Id`). Record method, path, status, duration, user, tenant.
- **Metrics:** `http_requests_total`, `http_request_duration_ms`, `db_query_duration_ms`, `active_sessions`.
- **Browser:** errors POST to `/api/telemetry` (error boundaries, unhandled rejections).
- **Retention:** traces/metrics: 14d. Errors: never auto-deleted (resolved >90d may archive). Observations: indefinite.

## Agent Queries (run at session start)
```sql
-- Top unresolved errors
SELECT error_type, message, source_location, occurrence_count, last_seen_at
FROM errors WHERE resolved_at IS NULL ORDER BY occurrence_count DESC LIMIT 10;

-- Error rate (24h hourly)
SELECT strftime('%Y-%m-%d %H:00', created_at) AS hour, COUNT(*) AS n
FROM traces WHERE error_fingerprint IS NOT NULL
  AND created_at > datetime('now', '-24 hours') GROUP BY hour ORDER BY hour;

-- Slow endpoints (P95 24h)
SELECT path, COUNT(*) AS reqs, CAST(duration_ms AS INTEGER) AS p95_ms
FROM (SELECT path, duration_ms, NTILE(20) OVER (PARTITION BY path ORDER BY duration_ms) AS pct
      FROM traces WHERE created_at > datetime('now', '-24 hours'))
WHERE pct = 19 GROUP BY path ORDER BY p95_ms DESC LIMIT 10;
```

## Feedback Loop
App → telemetry.db → agent queries → identifies issue → records `agent_observations` → creates task or fixes → PR → deploy → marks error resolved → next session verifies.
`agent_observations` prevents duplicate triage across sessions.

## Invariant
Agents read telemetry, never write to app DB. App modifications only via code changes through normal PR flow.
