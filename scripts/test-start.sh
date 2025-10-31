#!/bin/bash

# Get the directory where this script is located
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

docker compose -f "$REPO_ROOT/infra/test-docker-compose.yml" up -d
# Use absolute path to docker-compose files; include main infra and test overrides so
# api and rust services are started alongside test services (postgres, anvil, solana).
# Force rebuild to ensure latest code changes are included
docker compose -f "$REPO_ROOT/infra/docker-compose.yml" -f "$REPO_ROOT/infra/test-docker-compose.yml" up -d --build