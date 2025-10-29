# E2E Test Suite

Comprehensive end-to-end tests for the Cross-Chain Transaction Tracker.

## Overview

This test suite validates the complete flow of cross-chain transaction tracking:

1. **Blockchain Events** - ERC20 transfers on Ethereum (Anvil), SOL transfers on Solana
2. **Event Normalization** - Rust service detects and normalizes events
3. **Message Bus** - Events published to Redis
4. **API Integration** - Go API consumes events and serves HTTP endpoints
5. **Event Verification** - Tests query API and validate event structure

## Quick Start

```bash
# From repository root
E2E=true ./scripts/e2e.sh
```

This will:

1. Start all infrastructure (Anvil, Solana validator, Redis, PostgreSQL, Rust tracker, Go API)
2. Run parameterized E2E tests for Ethereum and Solana (single + batch transfers)
3. Verify Rust logs, API responses, and event structure
4. Clean up all containers and volumes

## Test Structure

- `test_ethereum.py` - Ethereum E2E tests

  - Deploys ERC20 contract to Anvil
  - Sends token transfers (parameterized: 1 or 3 transfers)
  - Verifies Rust logs contain transaction hash
  - Polls API for transaction events
  - Validates event structure (chain, tx_hash, from, to, value, etc.)

- `test_solana.py` - Solana E2E tests

  - Generates keypairs in solana-test-validator
  - Airdrops SOL to sender
  - Sends SOL transfers (parameterized: 1 or 2 transfers)
  - Verifies Rust logs show Solana processing
  - Checks API health and queries for events

- `conftest.py` - Pytest fixtures
  - Session-level setup/teardown
  - Unique test ID generation for idempotency
  - Docker logs capture

## Infrastructure

### Services

- **anvil** (`infra-anvil-1`) - Ethereum emulator on port 8545
- **solana** (`infra-solana-1`) - Solana test validator on ports 8899/8900
- **redis** (`infra-redis-1`) - Message bus on port 6379
- **postgres** (`infra-postgres-1`) - Database on port 5432
- **rust** (`infra-rust-1`) - Blockchain tracker service
- **api** (`infra-api-1`) - Go HTTP API on port 8080

### Docker Compose

- `infra/docker-compose.yml` - Production service definitions
- `infra/test-docker-compose.yml` - Test-specific overrides (Anvil, Solana)

## Running Tests

### Full E2E Suite

```bash
E2E=true ./scripts/e2e.sh
```

### Individual Test Files

```bash
# Start infrastructure first
./scripts/test-start.sh

# Run specific test file
cd tests/e2e
pytest test_ethereum.py -v
pytest test_solana.py -v

# Run specific parameterization
pytest test_ethereum.py::test_ethereum_e2e[single] -v
pytest test_ethereum.py::test_ethereum_e2e[batch] -v

# Cleanup
cd ../..
./scripts/test-stop.sh
```

### CI Integration

GitHub Actions workflow (`.github/workflows/e2e.yml`) runs E2E tests on:

- Push to `main` or `develop` branches
- Pull requests to `main` or `develop`
- Manual workflow dispatch

## Test Parameterization

Tests are parameterized to run multiple scenarios:

**Ethereum:**

- `single` - 1 ERC20 transfer
- `batch` - 3 ERC20 transfers

**Solana:**

- `single` - 1 SOL transfer
- `batch` - 2 SOL transfers

## Idempotency

Tests are designed to be idempotent:

- Volume cleanup (`docker-compose down -v`) between runs
- Unique test IDs generated per run
- Fresh contract deployments for each test
- New keypairs generated for Solana tests

## Debugging

### View service logs

```bash
docker logs infra-api-1
docker logs infra-rust-1
docker logs infra-redis-1
docker logs infra-anvil-1
docker logs infra-solana-1
```

### Check service health

```bash
# API health check
curl http://127.0.0.1:8080/health

# Ethereum (Anvil)
curl -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
  http://127.0.0.1:8545

# Solana
curl -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' \
  http://127.0.0.1:8899
```

### Query API manually

```bash
# Get wallet transactions
curl http://127.0.0.1:8080/wallet/0x1234.../transactions

# Get recent transactions
curl http://127.0.0.1:8080/transactions?limit=10
```

## Requirements

- Docker & Docker Compose
- Python 3.12+
- Bash shell
- `curl` (for health checks)

Python dependencies (installed via `requirements.txt`):

- pytest
- web3
- py-solc-x
- requests
- psycopg2-binary

## Environment Variables

- `E2E=true` - Required to run E2E suite
- `ANVIL_RPC` - Ethereum RPC URL (default: `http://127.0.0.1:8545`)
- `SOLANA_RPC` - Solana RPC URL (default: `http://127.0.0.1:8899`)
- `API_URL` - API base URL (default: `http://127.0.0.1:8080`)

## Troubleshooting

**Services not starting:**

- Check Docker is running: `docker ps`
- View compose logs: `docker-compose -f infra/docker-compose.yml -f infra/test-docker-compose.yml logs`

**Tests timing out:**

- Increase wait times in `scripts/e2e.sh`
- Check Rust service is processing events: `docker logs infra-rust-1`

**API not receiving events:**

- Verify Redis connectivity: `docker logs infra-redis-1`
- Check Rust is publishing: Look for "Published event to Redis" in Rust logs
- Verify Go is subscribing: Look for "received event" in API logs

**Solana CLI commands failing:**

- Ensure solana-test-validator is running: `docker ps | grep solana`
- Check validator health endpoint
- Verify keypairs are generated in `/tmp` inside container
