# Kubernetes Secrets

Secrets are never committed to version control. This directory contains creation
scripts that generate K8s secrets from environment variables or prompted input.

Run each script once when provisioning a new cluster, or when rotating a secret.
All scripts are idempotent: they use `kubectl apply` with `--dry-run=client` to
validate before applying, and `kubectl create secret --save-config` so re-runs
update rather than error.

## Required secrets

| Secret name | Consumed by | Created by |
|---|---|---|
| `ghcr-credentials` | all containers (image pull) | `ghcr-credentials.sh` |
| `postgres-credentials` | postgres, frontend (app user) | `postgres-credentials.sh` |
| `worker-credentials` | worker (service token) | `worker-credentials.sh` |
| `worker-db-credentials` | worker (read-only DB role) | `worker-db-credentials.sh` |
| `vendor-api-keys` | worker (Claude, Gemini API keys) | `vendor-api-keys.sh` |
| `dev-ssh-keys` | dev container (SSH authorized keys) | `dev-ssh-keys.sh` |

## Bootstrap order

1. `ghcr-credentials.sh` — must exist before any pod can pull an image
2. `postgres-credentials.sh` — must exist before postgres StatefulSet starts
3. `worker-db-credentials.sh` — must exist before worker Deployment starts
4. `worker-credentials.sh` — must exist before worker Deployment starts
5. `vendor-api-keys.sh` — must exist before worker Deployment starts
6. `dev-ssh-keys.sh` — must exist before dev Deployment starts

The provisioning script (`scripts/provision-cluster.sh`) calls each of these
in order. Do not apply secrets manually unless debugging a specific secret.
