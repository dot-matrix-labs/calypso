#!/bin/bash
set -e

# Create directory structure
mkdir -p docs/standards
mkdir -p docs/reference

# Standards files (operational — loaded by agents per the ontology)
STANDARDS=(
  "stack.md"
  "process.md"
  "testing.md"
  "deployment.md"
  "git-discipline.md"
  "security.md"
  "telemetry.md"
  "hardening.md"
  "multi-agent.md"
  "documentation.md"
)

# Reference files (one-shot or human-facing)
REFERENCE=(
  "scaffold-task.md"
  "product-owner-interview.md"
  "faq.md"
)

# Pin to a specific commit for reproducibility. Update this hash deliberately
# when upgrading standards — do not track main blindly.
COMMIT="main"
BASE_URL="https://raw.githubusercontent.com/dot-matrix-labs/calypso/${COMMIT}"

echo "Downloading Calypso standards (ref: ${COMMIT})..."

for FILE in "${STANDARDS[@]}"; do
  echo "  - standards/$FILE"
  curl -sSL "$BASE_URL/standards/$FILE" -o "docs/standards/$FILE"
done

for FILE in "${REFERENCE[@]}"; do
  echo "  - reference/$FILE"
  curl -sSL "$BASE_URL/reference/$FILE" -o "docs/reference/$FILE"
done

# Download the ontology (decision tree for agent context loading)
echo "  - calypso-ontology.ttl"
curl -sSL "$BASE_URL/calypso-ontology.ttl" -o "docs/calypso-ontology.ttl"

echo ""
echo "Done. Standards saved to ./docs/standards/ and ./docs/reference/"

echo ""
echo "================================================================================"
echo "IMPORTANT: Once bootstrapped, your local docs/ files are the source of truth."
echo "Update them deliberately — do not re-run this script on every session."
echo ""
echo "To pin to a specific version, edit COMMIT in this script to a commit hash."
echo "================================================================================"
echo ""
echo "AGENT INSTRUCTION:"
echo "Read docs/calypso-ontology.ttl to determine your current state, then load"
echo "only the standards required for that state. Start with docs/standards/stack.md."
echo "================================================================================"
