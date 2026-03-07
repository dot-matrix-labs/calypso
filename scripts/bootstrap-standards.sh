#!/bin/bash
set -e

# Create standards directory
mkdir -p docs/standards

# List of standard files to download
FILES=(
  "calypso-blueprint.md"
  "development-standards.md"
  "documentation-standard.md"
  "git-standards.md"
  "product-owner-interview.md"
  "scaffold-task.md"
)

BASE_URL="https://raw.githubusercontent.com/dot-matrix-labs/calypso/main/prompts"

echo "Downloading Calypso standards..."

for FILE in "${FILES[@]}"; do
  echo "  - $FILE"
  curl -sSL "$BASE_URL/$FILE" -o "docs/standards/$FILE"
done

echo "Done. Standards saved to ./docs/standards/"

echo ""
echo "================================================================================"
echo "AGENT INSTRUCTION:"
echo "The agent MUST now read these local files in ./docs/standards/ as the first"
echo "action of the session. Do NOT proceed with any development or documentation"
echo "tasks until these standards are read; otherwise, you may fabricate"
echo "conventions that violate the Calypso architecture."
echo "================================================================================"
