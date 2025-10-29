#!/bin/bash
set -euo pipefail

# Random chaos harness: stop/start services and verify basic invariants via API
# Usage: ./scripts/chaos.sh [iterations]

ITERATIONS=${1:-5}
REPO_ROOT=$(cd "$(dirname "$0")/.." && pwd)

compose() {
  docker-compose -f "$REPO_ROOT/infra/docker-compose.yml" -f "$REPO_ROOT/infra/test-docker-compose.yml" "$@"
}

api_health() {
  curl -sf http://127.0.0.1:8080/health >/dev/null
}

random_service() {
  local services=(anvil solana redis api rust)
  echo ${services[$RANDOM % ${#services[@]}]}
}

echo "Starting chaos with $ITERATIONS iterations"
for ((i=1;i<=ITERATIONS;i++)); do
  svc=$(random_service)
  echo "[$i/$ITERATIONS] Chaosing service: $svc"
  compose stop "$svc" || true
  sleep $(( (RANDOM % 5) + 3 ))
  compose start "$svc" || compose up -d "$svc"

  # basic invariant: API should recover within 60s
  attempt=0
  until api_health; do
    attempt=$((attempt+1))
    if [ $attempt -gt 60 ]; then
      echo "API failed to recover after chaos on $svc"
      compose logs --tail=200 "$svc" || true
      exit 1
    fi
    sleep 1
  done
  echo "API healthy after $svc restart"
done

echo "Chaos run completed"
