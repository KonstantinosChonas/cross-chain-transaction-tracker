#!/bin/bash
# Diagnostic script for Anvil connectivity issues

echo "=== Anvil Diagnostic ==="
echo ""

echo "1. Checking if Anvil container is running..."
docker ps --filter "name=infra-anvil-1" --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}"
echo ""

echo "2. Checking Anvil logs (last 20 lines)..."
docker logs infra-anvil-1 --tail 20
echo ""

echo "3. Checking if port 8545 is listening inside the container..."
docker exec infra-anvil-1 sh -c "netstat -tln 2>/dev/null || ss -tln 2>/dev/null || echo 'netstat/ss not available'"
echo ""

echo "4. Testing HTTP connection from host..."
curl -v -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
  http://127.0.0.1:8545 2>&1 | head -20
echo ""

echo "5. Testing from inside another container on same network..."
docker run --rm --network infra_default alpine/curl:latest \
  -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
  http://anvil:8545
echo ""

echo "=== End Diagnostic ==="
