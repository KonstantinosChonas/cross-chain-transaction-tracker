#!/bin/bash
# E2E orchestrator for CrossChainTransactionTracker
# Usage: E2E=true ./scripts/e2e.sh
set -e

# Get the directory where this script is located
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

if [ -z "$E2E" ]; then
  echo "E2E env var not set, aborting."
  exit 1
fi

# Ensure cleanup happens even if tests fail
trap "$SCRIPT_DIR/test-stop.sh" EXIT

# Start test infra
"$SCRIPT_DIR/test-start.sh"

# Wait for services to be ready
echo "Waiting for services to be ready..."
sleep 10

# Check API health
echo "Checking API health..."
max_attempts=30
attempt=0
until curl -s http://127.0.0.1:8080/health > /dev/null 2>&1; do
  attempt=$((attempt + 1))
  if [ $attempt -ge $max_attempts ]; then
    echo "API failed to become healthy after $max_attempts attempts"
    docker-compose -f infra/docker-compose.yml -f infra/test-docker-compose.yml logs
    exit 1
  fi
  echo "Waiting for API to be ready... (attempt $attempt/$max_attempts)"
  sleep 2
done

echo "✓ API is healthy"

# Check Anvil
echo "Checking Anvil (Ethereum)..."
if curl -s -X POST -H "Content-Type: application/json" \
   --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
   http://127.0.0.1:8545 > /dev/null 2>&1; then
  echo "✓ Anvil is ready"
else
  echo "Warning: Anvil may not be ready"
fi

# Check Solana
echo "Checking Solana test validator..."
if curl -s -X POST -H "Content-Type: application/json" \
   --data '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' \
   http://127.0.0.1:8899 > /dev/null 2>&1; then
  echo "✓ Solana is ready"
else
  echo "Warning: Solana may not be ready"
fi

echo "All services ready, running E2E tests..."

# Run E2E Python suite
cd tests/e2e
pytest --maxfail=1 --disable-warnings -v
cd ../..

echo "✓ E2E tests completed successfully"

