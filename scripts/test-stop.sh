#!/bin/bash

# Get the directory where this script is located
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

# Use absolute path to docker-compose files and stop all services with volume cleanup
docker-compose -f "$REPO_ROOT/infra/docker-compose.yml" \
               -f "$REPO_ROOT/infra/test-docker-compose.yml" down -v

echo "E2E infrastructure stopped and volumes cleaned"