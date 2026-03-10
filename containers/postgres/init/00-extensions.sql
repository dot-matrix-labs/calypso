-- Calypso PostgreSQL init: standard extensions
-- This file runs once on a virgin volume mount.
-- Project-specific schema migrations are managed by the application's migration runner.

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
