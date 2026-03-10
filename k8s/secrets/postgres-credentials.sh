#!/usr/bin/env bash
# Creates the postgres-credentials secret (superuser credentials for the StatefulSet)
# and the app-db-credentials secret (application user, used by the frontend/API).
#
# Usage: POSTGRES_PASSWORD=<pw> APP_DB_PASSWORD=<pw> ./postgres-credentials.sh
set -euo pipefail

: "${POSTGRES_PASSWORD:?POSTGRES_PASSWORD must be set}"
: "${APP_DB_PASSWORD:?APP_DB_PASSWORD must be set}"
: "${NAMESPACE:=calypso}"
POSTGRES_USER="${POSTGRES_USER:-postgres}"
POSTGRES_DB="${POSTGRES_DB:-calypso}"
APP_DB_USER="${APP_DB_USER:-app}"

kubectl create secret generic postgres-credentials \
  --from-literal=username="${POSTGRES_USER}" \
  --from-literal=password="${POSTGRES_PASSWORD}" \
  --from-literal=database="${POSTGRES_DB}" \
  --namespace="${NAMESPACE}" \
  --dry-run=client -o yaml | kubectl apply -f -

kubectl create secret generic app-db-credentials \
  --from-literal=url="postgres://${APP_DB_USER}:${APP_DB_PASSWORD}@postgres.${NAMESPACE}.svc.cluster.local:5432/${POSTGRES_DB}" \
  --namespace="${NAMESPACE}" \
  --dry-run=client -o yaml | kubectl apply -f -

echo "✓ postgres-credentials and app-db-credentials created in namespace ${NAMESPACE}"
