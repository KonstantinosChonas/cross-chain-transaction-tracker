#!/bin/bash
# E2E test runner for CrossChainTransactionTracker
# Boots infra, runs E2E tests for Ethereum and Solana, checks DB and API
set -e

# Boot test infra (Anvil, Solana, DB, Rust, Go, API)
cd ../../infra
if [ -z "$E2E" ]; then
  echo "E2E env var not set, aborting."
  exit 1
fi

docker compose -f docker-compose.yml -f test-docker-compose.yml up -d

# Wait for services to be ready
echo "Waiting for services to be ready..."
sleep 10

# Check API health (Go API on port 3000)
echo "Checking API health..."
max_attempts=30
attempt=0
until curl -s http://127.0.0.1:3000/health > /dev/null 2>&1; do
  attempt=$((attempt + 1))
  if [ $attempt -ge $max_attempts ]; then
    echo "API failed to become healthy after $max_attempts attempts"
    docker compose -f docker-compose.yml -f test-docker-compose.yml logs
    exit 1
  fi
  echo "Waiting for API to be ready... (attempt $attempt/$max_attempts)"
  sleep 2
done

echo "API is healthy, proceeding with tests..."

cd ../tests/e2e

# Run E2E test suite (Python entrypoint)
pytest --maxfail=1 --disable-warnings -v

# Tear down infra with volume cleanup
cd ../../infra
docker compose -f docker-compose.yml -f test-docker-compose.yml down -v

