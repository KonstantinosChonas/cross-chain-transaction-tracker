# Cross Chain Transaction Tracker

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
