.PHONY: dev rust go clean test test-update-golden

# Run both services for dev. Adjust commands as you implement them.
dev:
	@echo "Starting dev: Rust + Go (stubs) - logs will stream to terminals"
	# Run rust and go concurrently in the background of this shell. Use CTRL+C to stop.
	( cd rust && cargo run ) & ( cd go/cmd/api && go run . )

rust:
	cd rust && cargo run

go:
	cd go/cmd/api && go run .

clean:
	@echo "Cleaning rust target and go bin"
	cd rust && cargo clean || true
	rm -rf go/bin || true

test:
	@echo "Running tests with existing golden files..."
	cd go/cmd/api && go test ./...
	cd rust && cargo test

test-update-golden:
	@echo "Updating golden test files..."
	cd go/cmd/api && go test ./... -update
	cd rust && cargo test -- --update-goldens
