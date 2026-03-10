#!/usr/bin/env bash
# Generates a kubeconfig for the ci-deployer service account and prints it
# as a base64-encoded string suitable for storing in GitHub Secrets as KUBE_CONFIG.
#
# Prerequisites:
#   - kubectl configured with admin access to the target cluster
#   - k8s/rbac/ci-deployer.yaml already applied (kubectl apply -f k8s/rbac/ci-deployer.yaml)
#   - k8s/namespace.yaml already applied
#
# Usage: ./scripts/setup-ci-deployer.sh
# Output: base64-encoded kubeconfig — paste into GitHub → Settings → Secrets → KUBE_CONFIG
set -euo pipefail

NAMESPACE="${NAMESPACE:-calypso}"
SA_NAME="ci-deployer"
SECRET_NAME="ci-deployer-token"

echo "Creating long-lived token secret for service account ${SA_NAME}..."

# Create a long-lived token secret bound to the service account.
# K8s 1.24+ no longer auto-creates tokens; we create one explicitly.
kubectl apply -f - <<EOF
apiVersion: v1
kind: Secret
metadata:
  name: ${SECRET_NAME}
  namespace: ${NAMESPACE}
  annotations:
    kubernetes.io/service-account.name: ${SA_NAME}
type: kubernetes.io/service-account-token
EOF

# Wait for the token to be populated
echo "Waiting for token to be issued..."
for i in $(seq 1 10); do
  TOKEN=$(kubectl get secret "${SECRET_NAME}" -n "${NAMESPACE}" \
    -o jsonpath='{.data.token}' 2>/dev/null | base64 -d || true)
  if [[ -n "${TOKEN}" ]]; then
    break
  fi
  sleep 2
done

if [[ -z "${TOKEN}" ]]; then
  echo "Error: token was not issued after 20 seconds" >&2
  exit 1
fi

# Extract cluster info from current context
CLUSTER_SERVER=$(kubectl config view --minify -o jsonpath='{.clusters[0].cluster.server}')
CLUSTER_CA=$(kubectl config view --minify --raw -o jsonpath='{.clusters[0].cluster.certificate-authority-data}')
CLUSTER_NAME=$(kubectl config view --minify -o jsonpath='{.clusters[0].name}')

# Build a minimal kubeconfig for the service account
KUBECONFIG_CONTENT=$(cat <<EOF
apiVersion: v1
kind: Config
clusters:
  - name: ${CLUSTER_NAME}
    cluster:
      server: ${CLUSTER_SERVER}
      certificate-authority-data: ${CLUSTER_CA}
contexts:
  - name: ci-deployer
    context:
      cluster: ${CLUSTER_NAME}
      user: ci-deployer
      namespace: ${NAMESPACE}
current-context: ci-deployer
users:
  - name: ci-deployer
    user:
      token: ${TOKEN}
EOF
)

echo ""
echo "════════════════════════════════════════════════════════"
echo "  KUBE_CONFIG value for GitHub Secrets:"
echo "════════════════════════════════════════════════════════"
echo "${KUBECONFIG_CONTENT}" | base64 -w 0
echo ""
echo ""
echo "Copy the base64 string above and add it to:"
echo "  GitHub → Settings → Secrets and variables → Actions → KUBE_CONFIG"
echo ""
echo "To rotate this token in future, delete the secret and re-run this script:"
echo "  kubectl delete secret ${SECRET_NAME} -n ${NAMESPACE}"
