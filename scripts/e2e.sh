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

# Services should be ready after --wait, but let's verify
echo "Verifying services are ready..."

# Check Rust service health
echo "Checking Rust service health..."
max_attempts=30
attempt=0
until curl -sf http://127.0.0.1:8080/health > /dev/null 2>&1; do
  attempt=$((attempt + 1))
  if [ $attempt -ge $max_attempts ]; then
    echo "Rust service failed to become healthy after $max_attempts attempts"
    docker compose -f "$REPO_ROOT/infra/docker-compose.yml" logs rust
    exit 1
  fi
  echo "Waiting for Rust service to be ready... (attempt $attempt/$max_attempts)"
  sleep 2
done
echo "✓ Rust service is healthy"

# Check Go API health
echo "Checking Go API health..."
attempt=0
until curl -sf http://127.0.0.1:3000/health > /dev/null 2>&1; do
  attempt=$((attempt + 1))
  if [ $attempt -ge $max_attempts ]; then
    echo "Go API failed to become healthy after $max_attempts attempts"
    docker compose -f "$REPO_ROOT/infra/docker-compose.yml" logs api
    exit 1
  fi
  echo "Waiting for Go API to be ready... (attempt $attempt/$max_attempts)"
  sleep 2
done
echo "✓ Go API is healthy"

# Check Anvil
echo "Checking Anvil (Ethereum)..."
if curl -sf -X POST -H "Content-Type: application/json" \
   --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
   http://127.0.0.1:8545 > /dev/null 2>&1; then
  echo "✓ Anvil is ready"
else
  echo "Warning: Anvil may not be ready"
  docker compose -f "$REPO_ROOT/infra/test-docker-compose.yml" logs anvil
fi

# Check Solana
echo "Checking Solana test validator..."
if curl -sf -X POST -H "Content-Type: application/json" \
   --data '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' \
   http://127.0.0.1:8899 > /dev/null 2>&1; then
  echo "✓ Solana is ready"
else
  echo "Warning: Solana may not be ready"
  docker compose -f "$REPO_ROOT/infra/test-docker-compose.yml" logs solana
fi

echo "All services ready, running E2E tests..."

# Run E2E Python suite
cd tests/e2e
pytest --maxfail=1 --disable-warnings -v
cd ../..

echo "✓ E2E tests completed successfully"

