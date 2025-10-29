# Cross Chain Transaction Tracker

[![CI](https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/workflows/PR%20Quick%20Checks/badge.svg)](https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/actions/workflows/pr-checks.yml)
[![E2E Tests (manual)](https://img.shields.io/badge/E2E%20Tests-manual-blue)](https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/actions/workflows/integration-e2e.yml)
[![Coverage](https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/workflows/Code%20Coverage/badge.svg)](https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/actions/workflows/coverage.yml)
[![Nightly](https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/workflows/Nightly%20and%20Scheduled%20Tests/badge.svg)](https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/actions/workflows/nightly.yml)

A service that tracks transactions across multiple blockchains and normalizes them into a common format.

## Components

- Rust listener service: Monitors blockchain events and publishes to message bus
- Go API service: Provides HTTP API for querying normalized transaction events

## Development Setup

### Prerequisites

- Docker and docker-compose
- Rust toolchain
- Go toolchain
- Python 3.8+ (for integration tests)
- Solidity compiler (solc)

### Quick Start

1. Start development infrastructure:

```bash
docker-compose -f infra/docker-compose.yml up -d
```

2. Run Rust listener:

```bash
cd rust
cargo run
```

3. Run Go API:

```bash
cd go/cmd/api
go run main.go
```

## Testing

### Continuous Integration

This project uses GitHub Actions for comprehensive CI/CD. See [.github/README.md](.github/README.md) for details.

**PR Requirements:**

- ✅ All unit tests pass (Rust + Go)
- ✅ Go race detector clean
- ✅ Linting passes
- ✅ Code coverage ≥ 70%
- ✅ Integration and E2E tests (optional manual run)

**Branch Protection:** Main and release branches are protected and require all CI checks to pass before merging.

For detailed CI/CD documentation, see [.github/CI_BRANCH_PROTECTION.md](.github/CI_BRANCH_PROTECTION.md).

### Integration Tests

The integration test suite verifies end-to-end event delivery from the Rust listener to the Go API service.

1. Install test dependencies:

```bash
cd tests/integration
pip install -r requirements.txt
```

2. Run integration tests:

```bash
python test_event_delivery.py
```

The integration test:

- Spins up test infrastructure (Anvil, Redis, etc.)
- Deploys a test ERC20 contract
- Sends a test transfer transaction
- Verifies the event is received and normalized by the Go API

### E2E Tests

End-to-end tests validate the complete system with real blockchain nodes:

```bash
cd tests/e2e
pip install -r requirements.txt

# Run all E2E tests
pytest -v

# Run specific test suites
pytest test_ethereum.py -v
pytest test_solana.py -v
pytest test_chaos.py -v
```

### Unit Tests

Run Rust tests:

```bash
cd rust
cargo test
```

Run Go tests:

```bash
cd go
go test ./...
```

Run with race detection:

```bash
cd go
go test -race ./...
```

### Coverage Reports

Generate coverage locally:

**Rust:**

```bash
cd rust
cargo install cargo-llvm-cov
cargo llvm-cov --html
# Open target/llvm-cov/html/index.html
```

**Go:**

```bash
cd go
go test -coverprofile=coverage.out ./...
go tool cover -html=coverage.out
```
