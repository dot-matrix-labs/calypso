#!/usr/bin/env bash
# Creates the vendor-api-keys secret for AI vendor API keys consumed by workers.
#
# Usage: CLAUDE_API_KEY=<key> GEMINI_API_KEY=<key> ./vendor-api-keys.sh
set -euo pipefail

: "${CLAUDE_API_KEY:?CLAUDE_API_KEY must be set}"
: "${NAMESPACE:=calypso}"

ARGS=(
  --from-literal=claude-api-key="${CLAUDE_API_KEY}"
)

if [[ -n "${GEMINI_API_KEY:-}" ]]; then
  ARGS+=(--from-literal=gemini-api-key="${GEMINI_API_KEY}")
fi

kubectl create secret generic vendor-api-keys \
  "${ARGS[@]}" \
  --namespace="${NAMESPACE}" \
  --dry-run=client -o yaml | kubectl apply -f -

echo "✓ vendor-api-keys created in namespace ${NAMESPACE}"
