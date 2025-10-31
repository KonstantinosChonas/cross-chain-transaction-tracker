# Cross-Chain Transaction Tracker

[![CI](https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/workflows/PR%20Quick%20Checks/badge.svg)](https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/actions/workflows/pr-checks.yml)
[![E2E Tests (manual)](https://img.shields.io/badge/E2E%20Tests-manual-blue)](https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/actions/workflows/integration-e2e.yml)
[![Coverage](https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/workflows/Code%20Coverage/badge.svg)](https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/actions/workflows/coverage.yml)
[![Nightly](https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/workflows/Nightly%20and%20Scheduled%20Tests/badge.svg)](https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/actions/workflows/nightly.yml)

A production-ready service that tracks transactions across multiple blockchains (Ethereum, Solana, etc.) and normalizes them into a common event format exposed via an HTTP API and Server-Sent Events (SSE).

## Overview

- Rust listener: subscribes to on-chain activity (WS or HTTP polling), normalizes events, and publishes them to a message bus (Redis Pub/Sub).
- Go API: persists events to Postgres (optional), serves query APIs, and streams live events over SSE.

See `docs/api.md` for the API contract and normalized event schema.

## Architecture

- Ingest: Rust listener fetches Ethereum (native + ERC‑20) and Solana transactions for watched addresses.
- Normalize: Listener emits a consistent JSON schema for all chains.
- Transport: Redis Pub/Sub channel `cross_chain_events` (Phase A). Future options include NATS or gRPC.
- Serve: Go API ingests from Redis, optionally persists to Postgres, and exposes REST + SSE.

## Configuration

Both services use environment variables. You can copy `.env.example` and adjust values.

Required (listener):

- ETH_RPC_URL: Ethereum RPC endpoint (wss://… or https://…)
- SOL_RPC_URL: Solana RPC endpoint (wss://… or https://…)
- REDIS_URL: Redis connection string (e.g., redis://localhost:6379)
- ETH_NETWORK: e.g., mainnet, sepolia
- SOL_NETWORK: e.g., mainnet, devnet

Optional (listener):

- WATCHED_ADDRESSES_ETH: comma-separated list of 0x addresses
- WATCHED_ADDRESSES_SOL: comma-separated list of base58 pubkeys
- POLL_INTERVAL_SECS: HTTP poll interval (default 10)
- LOG_LEVEL: tracing filter, e.g., info, debug

API service:

- REDIS_URL: same as above
- POSTGRES_DSN: optional, to persist events (e.g., postgres://user:pass@localhost:5432/db?sslmode=disable)
- BIND_ADDR: API bind address (default 0.0.0.0:8080)

## Quick start (Docker Compose)

From the repo root:

```bash
docker compose -f infra/docker-compose.yml up -d
```

This brings up Redis, Postgres, and the API. Point the Rust listener at the same Redis and run it locally (see next section).

## Local development

Rust listener:

```bash
cd rust
cargo run
```

Go API:

```bash
cd go/cmd/api
go run main.go
```

Windows notes:

- The above commands work in PowerShell or Command Prompt if Rust, Go, and Docker are installed and in PATH.
- If using WSL, run the Linux equivalents inside your distro.

## API quick tour

- Health: `GET /health` → 200 OK
- Recent events: `GET /transactions?limit=50&offset=0`
- Wallet history: `GET /wallet/{address}/transactions?chain=ethereum&token=USDC`
- Live stream: `GET /events/subscribe` (SSE)

Example:

```bash
curl http://localhost:8080/transactions?limit=5
```

See `docs/api.md` for full request/response examples.

## Testing

This project uses GitHub Actions for comprehensive CI/CD. See `TESTING.md` and `docs/test-flakiness.md`.

PR requirements:

- All unit tests pass (Rust + Go)
- Go race detector clean
- Linting passes
- Code coverage is tracked (goal ≥ 70% over time; not enforced yet)
- Integration and E2E tests (optional manual run)

### Integration tests

```bash
cd tests/integration
pip install -r requirements.txt
python test_event_delivery.py
```

Validates end‑to‑end delivery from Rust → Redis → Go API.

### E2E tests

```bash
cd tests/e2e
pip install -r requirements.txt
pytest -v
# or subsets:
pytest test_ethereum.py -v
pytest test_solana.py -v
pytest test_chaos.py -v
```

### Unit tests

Rust:

```bash
cd rust
cargo test
```

Go:

```bash
cd go
go test ./...
go test -race ./...
```

### Coverage

Rust:

```bash
cd rust
cargo install cargo-llvm-cov
cargo llvm-cov --html
# Open target/llvm-cov/html/index.html
```

Go:

```bash
cd go
go test -coverprofile=coverage.out ./...
go tool cover -html=coverage.out
```

## Security & reporting

If you discover a security issue, please open a private report or contact the maintainer. See `SECURITY_FIXES.md` for historical notes.

## License

This project is licensed under the terms of the `LICENSE` file in this repository.
