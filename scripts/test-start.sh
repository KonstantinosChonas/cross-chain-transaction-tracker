#!/bin/bash

# Get the directory where this script is located
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

# Start test infrastructure first
docker compose -f "$REPO_ROOT/infra/test-docker-compose.yml" up -d --wait

# Build and start application services
docker compose -f "$REPO_ROOT/infra/docker-compose.yml" build
docker compose -f "$REPO_ROOT/infra/docker-compose.yml" up -d --wait