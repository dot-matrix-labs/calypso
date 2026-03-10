#!/usr/bin/env bash
# provision-cluster.sh — Bootstrap a Calypso app cluster from scratch.
#
# Installs k3s and deploys the three app containers (frontend, worker, db).
# Does NOT create a developer container — the agent runs directly on the host.
#
# Three modes:
#   Local (default) — installs k3s on this machine; run from the cloud host
#   Existing host   — SSH into HOST_IP and set up k3s there
#   New Droplet     — create a DigitalOcean Droplet and provision it
#
# Usage (local — run from the cloud host you want to use):
#   PROJECT=myapp GITHUB_USER=me GITHUB_TOKEN=<pat> ./scripts/provision-cluster.sh
#
# Usage (existing remote host):
#   HOST_IP=1.2.3.4 SSH_KEY=~/.ssh/id_ed25519 PROJECT=myapp ... ./scripts/provision-cluster.sh
#
# Usage (new DigitalOcean Droplet):
#   DIGITALOCEAN_TOKEN=<tok> PROJECT=myapp ... ./scripts/provision-cluster.sh
#
# Required environment variables:
#   PROJECT          — project name (lowercase, no spaces)
#   GITHUB_USER      — GitHub username or org for the new private repo
#   GITHUB_TOKEN     — GitHub PAT with repo + packages scopes

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

# ── Step 1: Acquire / identify host ─────────────────────────────────────────
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
  MODE="remote"

elif [[ -n "${HOST_IP:-}" ]]; then
  SSH_KEY="${SSH_KEY:-${HOME}/.ssh/id_ed25519}"
  echo "▶ Using existing host: ${HOST_IP}"
  MODE="remote"

else
  echo "▶ Local mode — provisioning k3s on this host"
  HOST_IP=$(curl -s ifconfig.me 2>/dev/null || hostname -I | awk '{print $1}')
  MODE="local"
fi

# ── Helper: run a command locally or via SSH ─────────────────────────────────
run() {
  if [[ "${MODE}" == "local" ]]; then
    bash -c "$1"
  else
    ssh -i "${SSH_KEY}" -o StrictHostKeyChecking=no -o ConnectTimeout=10 "root@${HOST_IP}" "$1"
  fi
}

copy_files() {
  local src="$1" dst="$2"
  if [[ "${MODE}" == "local" ]]; then
    cp -r "${src}" "${dst}"
  else
    scp -i "${SSH_KEY}" -o StrictHostKeyChecking=no -r "${src}" "root@${HOST_IP}:${dst}"
  fi
}

pipe_script() {
  local env_vars="$1"
  local script="$2"
  if [[ "${MODE}" == "local" ]]; then
    env ${env_vars} bash -s < "${script}"
  else
    ssh -i "${SSH_KEY}" -o StrictHostKeyChecking=no "root@${HOST_IP}" \
      "${env_vars} bash -s" < "${script}"
  fi
}

# ── Step 2: Wait for SSH (remote only) ───────────────────────────────────────
if [[ "${MODE}" == "remote" ]]; then
  echo "▶ Waiting for SSH..."
  for i in $(seq 1 30); do
    run true 2>/dev/null && break || sleep 5
  done
fi

# ── Step 3: Install k3s ──────────────────────────────────────────────────────
echo "▶ Installing k3s..."
run "curl -sfL https://get.k3s.io | INSTALL_K3S_EXEC='--disable traefik' sh -"
run "until kubectl get nodes 2>/dev/null | grep -q Ready; do sleep 3; done"
echo "  k3s ready"

# ── Step 4: Copy k8s manifests and apply ────────────────────────────────────
echo "▶ Applying Calypso manifests..."
copy_files "${K8S_DIR}" /tmp/calypso-k8s

run "bash" <<'REMOTE'
set -e
kubectl apply -f /tmp/calypso-k8s/namespace.yaml
kubectl apply -f /tmp/calypso-k8s/network-policy.yaml
kubectl apply -f /tmp/calypso-k8s/rbac/ci-deployer.yaml
kubectl apply -f /tmp/calypso-k8s/db/
kubectl apply -f /tmp/calypso-k8s/frontend/
kubectl apply -f /tmp/calypso-k8s/worker/
REMOTE

# ── Step 5: Create secrets ───────────────────────────────────────────────────
echo "▶ Creating cluster secrets..."

# Image pull (GHCR)
pipe_script "GITHUB_USER=${GITHUB_USER} GITHUB_TOKEN=${GITHUB_TOKEN} NAMESPACE=${NAMESPACE}" \
  "${K8S_DIR}/secrets/ghcr-credentials.sh"

# Postgres credentials (generated)
POSTGRES_PASSWORD=$(openssl rand -base64 32)
APP_DB_PASSWORD=$(openssl rand -base64 32)
pipe_script "POSTGRES_PASSWORD='${POSTGRES_PASSWORD}' APP_DB_PASSWORD='${APP_DB_PASSWORD}' NAMESPACE=${NAMESPACE}" \
  "${K8S_DIR}/secrets/postgres-credentials.sh"

# Worker credentials (generated)
WORKER_SERVICE_TOKEN=$(openssl rand -base64 48)
WORKER_DB_PASSWORD=$(openssl rand -base64 32)
pipe_script "WORKER_SERVICE_TOKEN='${WORKER_SERVICE_TOKEN}' WORKER_DB_PASSWORD='${WORKER_DB_PASSWORD}' NAMESPACE=${NAMESPACE}" \
  "${K8S_DIR}/secrets/worker-credentials.sh"

# Vendor API keys (optional)
if [[ -n "${CLAUDE_API_KEY:-}" ]]; then
  VENDOR_ARGS="CLAUDE_API_KEY='${CLAUDE_API_KEY}'"
  [[ -n "${GEMINI_API_KEY:-}" ]] && VENDOR_ARGS="${VENDOR_ARGS} GEMINI_API_KEY='${GEMINI_API_KEY}'"
  pipe_script "NAMESPACE=${NAMESPACE} ${VENDOR_ARGS}" "${K8S_DIR}/secrets/vendor-api-keys.sh"
fi

echo "  Secrets created"

# ── Step 6: Wait for app containers ─────────────────────────────────────────
echo "▶ Waiting for app containers to be ready..."
run "kubectl wait deployment/frontend -n ${NAMESPACE} --for=condition=Available --timeout=120s"
run "kubectl wait deployment/worker   -n ${NAMESPACE} --for=condition=Available --timeout=120s"
run "kubectl wait statefulset/db      -n ${NAMESPACE} --for=condition=Ready      --timeout=120s" || \
  run "kubectl wait pod -l app=db -n ${NAMESPACE} --for=condition=Ready --timeout=120s"

FRONTEND_PORT=$(run "kubectl get svc frontend -n ${NAMESPACE} -o jsonpath='{.spec.ports[0].nodePort}'")
echo "  Frontend NodePort: ${FRONTEND_PORT}"

# ── Step 7: Save connection file ─────────────────────────────────────────────
SSH_KEY_PATH="${SSH_KEY:-${HOME}/.ssh/id_ed25519}"
echo "▶ Writing connection info to .calypso-connect..."
cat > "${CONNECT_FILE}" <<EOF
# Calypso Project — Connection Info
# Generated: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
# Project: ${PROJECT}
#
# This file contains no secrets. Commit it to the project repo.

PROJECT=${PROJECT}
HOST_IP=${HOST_IP}
FRONTEND_PORT=${FRONTEND_PORT}
NAMESPACE=${NAMESPACE}

# ── SSH to the cloud host ──────────────────────────────────────────────────
# The agent and developer work directly on the host OS.
#
# SSH command:
#   ssh -i ${SSH_KEY_PATH} root@${HOST_IP}
#
# To reattach the agent's tmux session:
#   tmux attach -t main

# ── IDE remote setup (VS Code / Cursor / JetBrains) ──────────────────────
# Add this to your local ~/.ssh/config:
#
# Host calypso-${PROJECT}
#   HostName ${HOST_IP}
#   User root
#   IdentityFile ${SSH_KEY_PATH}
#   ServerAliveInterval 30
#   ServerAliveCountMax 3
#
# Then in VS Code: Remote-SSH → Connect to Host → calypso-${PROJECT}
# Open folder: /workspace/${PROJECT}
# VS Code Remote SSH: https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-ssh

# ── Kubernetes access (from the host) ────────────────────────────────────
# kubectl get pods -n ${NAMESPACE}
EOF

echo "  Written to .calypso-connect"
echo ""
echo "════════════════════════════════════════════════════"
echo "  App cluster ready."
echo "  Frontend: http://${HOST_IP}:${FRONTEND_PORT}/health"
echo ""
echo "  SSH to the host:"
echo "    ssh -i ${SSH_KEY_PATH} root@${HOST_IP}"
echo ""
echo "  IDE remote (add to ~/.ssh/config):"
echo "    Host calypso-${PROJECT}"
echo "      HostName ${HOST_IP}"
echo "      User root"
echo "      IdentityFile ${SSH_KEY_PATH}"
echo ""
echo "  kubectl (run on the host):"
echo "    kubectl get pods -n ${NAMESPACE}"
echo "════════════════════════════════════════════════════"
