#!/usr/bin/env bash
# Creates the worker-credentials secret (worker service token for API auth)
# and the worker-db-credentials secret (read-only DB role connection string).
#
# Usage: WORKER_SERVICE_TOKEN=<token> WORKER_DB_PASSWORD=<pw> ./worker-credentials.sh
set -euo pipefail

: "${WORKER_SERVICE_TOKEN:?WORKER_SERVICE_TOKEN must be set}"
: "${WORKER_DB_PASSWORD:?WORKER_DB_PASSWORD must be set}"
: "${NAMESPACE:=calypso}"
POSTGRES_DB="${POSTGRES_DB:-calypso}"
WORKER_DB_USER="${WORKER_DB_USER:-agent_worker}"

kubectl create secret generic worker-credentials \
  --from-literal=service-token="${WORKER_SERVICE_TOKEN}" \
  --namespace="${NAMESPACE}" \
  --dry-run=client -o yaml | kubectl apply -f -

kubectl create secret generic worker-db-credentials \
  --from-literal=url="postgres://${WORKER_DB_USER}:${WORKER_DB_PASSWORD}@postgres.${NAMESPACE}.svc.cluster.local:5432/${POSTGRES_DB}" \
  --namespace="${NAMESPACE}" \
  --dry-run=client -o yaml | kubectl apply -f -

echo "✓ worker-credentials and worker-db-credentials created in namespace ${NAMESPACE}"
