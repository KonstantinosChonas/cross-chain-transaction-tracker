#!/bin/bash

# Get the directory where this script is located
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

# Stop all services with volume cleanup
docker compose -f "$REPO_ROOT/infra/docker-compose.yml" down -v || true
docker compose -f "$REPO_ROOT/infra/test-docker-compose.yml" down -v || true

echo "E2E infrastructure stopped and volumes cleaned"