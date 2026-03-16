#!/bin/bash

# Exit on any command failure
set -e

# Generate a unique 4-character hash for the test directory
HASH=$(openssl rand -hex 2)
TEST_PATH="$HOME/calypso-tests/${HASH}"

# Ensure parent directory exists
mkdir -p "$HOME/calypso-tests"

# Get the absolute path to the project root
REPO_ROOT="$(cd "$(dirname "$0")" && pwd)"

echo "--------------------------------------------------------"
echo "🧪 Calypso CLI Local Test Environment"
echo "📂 Test Path: $TEST_PATH"
echo "--------------------------------------------------------"

# Navigate to the CLI directory
cd "$REPO_ROOT/cli"

echo "📦 [1/2] Initializing Calypso..."
# Pass all arguments to the init command
cargo run -- init --path "$TEST_PATH" "$@"

echo -e "\n🩺 [2/2] Running Doctor... "
cargo run -- doctor --path "$TEST_PATH"

# Optional: If hello-world was passed, show that files were created
if [[ "$*" == *"--hello-world"* ]]; then
    echo -e "\n🌍 Hello-world detected! Verifying files exist:"
    ls -l "$TEST_PATH/.calypso/state-machine.yml"
    ls -l "$TEST_PATH/.github/workflows/hello-world.yml"
    ls -l "$TEST_PATH/.git/hooks/pre-commit"
fi

echo "--------------------------------------------------------"
echo "✅ Done! You can manually inspect the results at: $TEST_PATH"
