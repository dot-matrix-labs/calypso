#!/usr/bin/env bash
# Creates the ghcr-credentials image pull secret.
# Required before any pod can pull images from ghcr.io.
#
# Usage: GITHUB_USER=<user> GITHUB_TOKEN=<pat> ./ghcr-credentials.sh
set -euo pipefail

: "${GITHUB_USER:?GITHUB_USER must be set}"
: "${GITHUB_TOKEN:?GITHUB_TOKEN must be set}"
: "${NAMESPACE:=calypso}"

kubectl create secret docker-registry ghcr-credentials \
  --docker-server=ghcr.io \
  --docker-username="${GITHUB_USER}" \
  --docker-password="${GITHUB_TOKEN}" \
  --namespace="${NAMESPACE}" \
  --dry-run=client -o yaml | kubectl apply -f -

echo "✓ ghcr-credentials created in namespace ${NAMESPACE}"
