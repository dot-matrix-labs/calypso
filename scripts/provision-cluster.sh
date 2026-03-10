#!/usr/bin/env bash
# provision-cluster.sh — Bootstrap a Calypso cluster from scratch.
#
# Accepts either a DigitalOcean API token (creates a Droplet + installs k3s)
# or an existing host IP (SSH in and installs k3s directly).
#
# On completion, writes .calypso-connect to the current directory with
# SSH connection details and IDE setup instructions.
#
# Usage (new Droplet):
#   DIGITALOCEAN_TOKEN=<tok> PROJECT=myapp ./scripts/provision-cluster.sh
#
# Usage (existing host):
#   HOST_IP=1.2.3.4 SSH_KEY=~/.ssh/id_ed25519 PROJECT=myapp ./scripts/provision-cluster.sh
#
# Required environment variables:
#   PROJECT          — project name (lowercase, no spaces)
#   GITHUB_USER      — GitHub username or org for the new private repo
#   GITHUB_TOKEN     — GitHub PAT with repo + packages scopes
#
# One of:
#   DIGITALOCEAN_TOKEN  — creates a new Droplet (requires doctl)
#   HOST_IP + SSH_KEY   — uses an existing host

set -euo pipefail

# ── Validate inputs ──────────────────────────────────────────────────────────
: "${PROJECT:?PROJECT must be set (e.g. PROJECT=myapp)}"
: "${GITHUB_USER:?GITHUB_USER must be set}"
: "${GITHUB_TOKEN:?GITHUB_TOKEN must be set}"

PROJECT_DIR="$(pwd)"
CONNECT_FILE="${PROJECT_DIR}/.calypso-connect"
K8S_DIR="${PROJECT_DIR}/k8s"
SCRIPTS_DIR="${PROJECT_DIR}/scripts"
NAMESPACE="calypso"
DROPLET_SIZE="${DROPLET_SIZE:-s-4vcpu-8gb-intel}"
DROPLET_REGION="${DROPLET_REGION:-nyc3}"

# ── Step 1: Acquire a host ───────────────────────────────────────────────────
if [[ -n "${DIGITALOCEAN_TOKEN:-}" ]]; then
  echo "▶ Creating DigitalOcean Droplet..."
  command -v doctl >/dev/null 2>&1 || { echo "doctl not found — install from https://docs.digitalocean.com/reference/doctl/how-to/install/"; exit 1; }

  doctl auth init --access-token "${DIGITALOCEAN_TOKEN}" --no-context >/dev/null

  # Get or upload SSH key
  SSH_KEY="${SSH_KEY:-${HOME}/.ssh/id_ed25519}"
  SSH_PUB="${SSH_KEY}.pub"
  [[ -f "${SSH_PUB}" ]] || { echo "SSH public key not found at ${SSH_PUB}"; exit 1; }

  KEY_FINGERPRINT=$(ssh-keygen -lf "${SSH_PUB}" | awk '{print $2}')
  KEY_ID=$(doctl compute ssh-key list --no-header --format FingerPrint,ID \
    | grep "${KEY_FINGERPRINT}" | awk '{print $2}' || true)

  if [[ -z "${KEY_ID}" ]]; then
    echo "  Uploading SSH key..."
    KEY_ID=$(doctl compute ssh-key import "calypso-${PROJECT}" \
      --public-key-file "${SSH_PUB}" --no-header --format ID)
  fi

  echo "  Creating droplet calypso-${PROJECT} (${DROPLET_SIZE}, ${DROPLET_REGION})..."
  DROPLET_ID=$(doctl compute droplet create "calypso-${PROJECT}" \
    --region "${DROPLET_REGION}" \
    --size "${DROPLET_SIZE}" \
    --image ubuntu-24-04-x64 \
    --ssh-keys "${KEY_ID}" \
    --wait \
    --no-header --format ID)

  HOST_IP=$(doctl compute droplet get "${DROPLET_ID}" --no-header --format PublicIPv4)
  echo "  Droplet ready: ${HOST_IP}"
elif [[ -n "${HOST_IP:-}" ]]; then
  SSH_KEY="${SSH_KEY:-${HOME}/.ssh/id_ed25519}"
  echo "▶ Using existing host: ${HOST_IP}"
else
  echo "Error: set either DIGITALOCEAN_TOKEN (new droplet) or HOST_IP (existing host)"
  exit 1
fi

SSH="ssh -i ${SSH_KEY} -o StrictHostKeyChecking=no -o ConnectTimeout=10 root@${HOST_IP}"

# Wait for SSH to be available
echo "▶ Waiting for SSH..."
for i in $(seq 1 30); do
  ${SSH} true 2>/dev/null && break || sleep 5
done

# ── Step 2: Install k3s ──────────────────────────────────────────────────────
echo "▶ Installing k3s..."
${SSH} "curl -sfL https://get.k3s.io | INSTALL_K3S_EXEC='--disable traefik' sh -"
${SSH} "until kubectl get nodes 2>/dev/null | grep -q Ready; do sleep 3; done"
echo "  k3s ready"

# ── Step 3: Copy k8s manifests and apply ────────────────────────────────────
echo "▶ Applying Calypso manifests..."
scp -i "${SSH_KEY}" -o StrictHostKeyChecking=no -r "${K8S_DIR}" root@${HOST_IP}:/tmp/calypso-k8s

${SSH} bash <<'REMOTE'
set -e
kubectl apply -f /tmp/calypso-k8s/namespace.yaml
kubectl apply -f /tmp/calypso-k8s/network-policy.yaml
kubectl apply -f /tmp/calypso-k8s/rbac/ci-deployer.yaml
kubectl apply -f /tmp/calypso-k8s/db/
kubectl apply -f /tmp/calypso-k8s/frontend/
kubectl apply -f /tmp/calypso-k8s/worker/
kubectl apply -f /tmp/calypso-k8s/dev/
REMOTE

# ── Step 4: Create secrets ───────────────────────────────────────────────────
echo "▶ Creating cluster secrets..."

# Image pull (GHCR)
${SSH} "GITHUB_USER=${GITHUB_USER} GITHUB_TOKEN=${GITHUB_TOKEN} NAMESPACE=${NAMESPACE} \
  bash -s" < "${SCRIPTS_DIR}/../k8s/secrets/ghcr-credentials.sh"

# Dev SSH keys — upload the operator's public key
SSH_PUB_CONTENT=$(cat "${SSH_KEY}.pub")
${SSH} "NAMESPACE=${NAMESPACE} AUTHORIZED_KEYS='${SSH_PUB_CONTENT}' bash -s" \
  < "${K8S_DIR}/secrets/dev-ssh-keys.sh"

# Postgres credentials (generated)
POSTGRES_PASSWORD=$(openssl rand -base64 32)
APP_DB_PASSWORD=$(openssl rand -base64 32)
${SSH} "POSTGRES_PASSWORD='${POSTGRES_PASSWORD}' APP_DB_PASSWORD='${APP_DB_PASSWORD}' \
  NAMESPACE=${NAMESPACE} bash -s" < "${K8S_DIR}/secrets/postgres-credentials.sh"

# Worker credentials (generated)
WORKER_SERVICE_TOKEN=$(openssl rand -base64 48)
WORKER_DB_PASSWORD=$(openssl rand -base64 32)
${SSH} "WORKER_SERVICE_TOKEN='${WORKER_SERVICE_TOKEN}' WORKER_DB_PASSWORD='${WORKER_DB_PASSWORD}' \
  NAMESPACE=${NAMESPACE} bash -s" < "${K8S_DIR}/secrets/worker-credentials.sh"

# Vendor API keys
if [[ -n "${CLAUDE_API_KEY:-}" ]]; then
  VENDOR_ARGS="CLAUDE_API_KEY='${CLAUDE_API_KEY}'"
  [[ -n "${GEMINI_API_KEY:-}" ]] && VENDOR_ARGS="${VENDOR_ARGS} GEMINI_API_KEY='${GEMINI_API_KEY}'"
  ${SSH} "NAMESPACE=${NAMESPACE} ${VENDOR_ARGS} bash -s" < "${K8S_DIR}/secrets/vendor-api-keys.sh"
fi

echo "  Secrets created"

# ── Step 5: Wait for dev container ──────────────────────────────────────────
echo "▶ Waiting for dev container to be ready..."
${SSH} "kubectl wait deployment/dev -n ${NAMESPACE} \
  --for=condition=Available --timeout=120s"

# Get the NodePort assigned to the dev SSH service
DEV_SSH_PORT=$(${SSH} "kubectl get svc dev -n ${NAMESPACE} \
  -o jsonpath='{.spec.ports[0].nodePort}'")
echo "  Dev container SSH port: ${DEV_SSH_PORT}"

# ── Step 6: Save connection file ─────────────────────────────────────────────
echo "▶ Writing connection info to .calypso-connect..."
cat > "${CONNECT_FILE}" <<EOF
# Calypso Project — Connection Info
# Generated: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
# Project: ${PROJECT}
#
# This file contains no secrets. Commit it to the project repo.

PROJECT=${PROJECT}
CLUSTER_HOST=${HOST_IP}
DEV_SSH_HOST=${HOST_IP}
DEV_SSH_PORT=${DEV_SSH_PORT}
DEV_SSH_USER=agent

# ── Connect to the dev container ──────────────────────────────────────────────
# SSH command:
#   ssh -i ${SSH_KEY} -p ${DEV_SSH_PORT} agent@${HOST_IP}
#
# To start/reattach a tmux session (run this after SSH):
#   tmux new-session -A -s main

# ── IDE remote setup (VS Code / Cursor / JetBrains) ──────────────────────────
# Add this to your local ~/.ssh/config:
#
# Host calypso-${PROJECT}
#   HostName ${HOST_IP}
#   Port ${DEV_SSH_PORT}
#   User agent
#   IdentityFile ${SSH_KEY}
#   ServerAliveInterval 30
#   ServerAliveCountMax 3
#
# Then in VS Code: Remote-SSH → Connect to Host → calypso-${PROJECT}
# Open folder: /workspace/${PROJECT}
# You will see all files the agent creates in real time.

# ── Kubernetes access (from host, not dev container) ─────────────────────────
# SSH to the host to run kubectl:
#   ssh -i ${SSH_KEY} root@${HOST_IP}
#   kubectl get pods -n ${NAMESPACE}
EOF

echo "  Written to .calypso-connect"
echo ""
echo "════════════════════════════════════════════════════"
echo "  Cluster ready."
echo "  SSH to dev container:"
echo "    ssh -i ${SSH_KEY} -p ${DEV_SSH_PORT} agent@${HOST_IP}"
echo ""
echo "  Add to ~/.ssh/config and connect via IDE:"
echo "    Host calypso-${PROJECT}"
echo "      HostName ${HOST_IP}"
echo "      Port ${DEV_SSH_PORT}"
echo "      User agent"
echo "      IdentityFile ${SSH_KEY}"
echo ""
echo "  Next: continue scaffold-task.md from Step 4 inside the dev container."
echo "════════════════════════════════════════════════════"
