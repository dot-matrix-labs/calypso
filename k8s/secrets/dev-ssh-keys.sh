#!/usr/bin/env bash
# Creates the dev-ssh-keys secret with SSH authorized_keys for the dev container.
# Accepts a path to an authorized_keys file, or reads from stdin.
#
# Usage: ./dev-ssh-keys.sh ~/.ssh/authorized_keys
#    or: cat ~/.ssh/id_ed25519.pub | ./dev-ssh-keys.sh
set -euo pipefail

: "${NAMESPACE:=calypso}"

if [[ -n "${AUTHORIZED_KEYS:-}" ]]; then
  : # already set via env var
elif [[ -n "${1:-}" ]]; then
  AUTHORIZED_KEYS="$(cat "$1")"
else
  AUTHORIZED_KEYS="$(cat)"
fi

if [[ -z "${AUTHORIZED_KEYS}" ]]; then
  echo "Error: no authorized_keys content provided" >&2
  exit 1
fi

kubectl create secret generic dev-ssh-keys \
  --from-literal=authorized_keys="${AUTHORIZED_KEYS}" \
  --namespace="${NAMESPACE}" \
  --dry-run=client -o yaml | kubectl apply -f -

echo "✓ dev-ssh-keys created in namespace ${NAMESPACE}"
