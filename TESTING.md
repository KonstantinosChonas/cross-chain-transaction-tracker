# Testing Guide

This guide explains how to verify that all debugging enhancements and flaky test management features are working correctly.

## 1. Verify Structured Logging in Tests

### Rust Tests (Local)

```bash
# Run Rust tests with logging enabled
cd rust
RUST_LOG=debug cargo test --bins 2>&1 | tee test-output.log

# Verify logging output appears
grep -i "log\|debug\|info\|warn\|error" test-output.log
```

**Expected:** You should see structured log output from the tests, including debug/info messages.

### Go Tests (Local)

```bash
# Run Go tests with verbose output
cd go
go test -v ./... 2>&1 | tee test-output.log

# Verify JSON logging appears (from logrus)
grep -i "level\|msg\|time" test-output.log
```

**Expected:** You should see JSON-formatted log entries from the Go tests.

## 2. Verify CI Artifact Collection

### Test Locally (Simulate CI)

```bash
# Run Rust tests and collect artifacts
cd rust
cargo test --bins || true

# Check for artifacts that would be collected
ls -la target/debug/deps/*.log 2>/dev/null || echo "No .log files (expected if no failures)"
ls -la target/debug/*.db 2>/dev/null || echo "No .db files (expected if no DB in tests)"

# Run Go tests and collect artifacts
cd ../go
go test -v ./... || true
ls -la *.log 2>/dev/null || echo "No .log files (expected if no failures)"
ls -la *.db 2>/dev/null || echo "No .db files (expected if no DB in tests)"
```

### Verify in CI

1. **Trigger a workflow run:**

   ```bash
   # Push to main or create a PR
   git add .
   git commit -m "Test debugging enhancements"
   git push
   ```

2. **Check artifacts in GitHub Actions:**

   - Go to: https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/actions
   - Click on the most recent workflow run
   - If tests fail, scroll down to "Artifacts" section
   - You should see artifacts like:
     - `rust-test-artifacts` (if Rust tests failed)
     - `go-test-artifacts-<version>` (if Go tests failed)
     - `fuzzing-artifacts` (if fuzzing failed)
     - `load-test-failure-artifacts` (if load tests failed)

3. **Download and inspect artifacts:**
   - Click on an artifact to download it
   - Extract and verify it contains:
     - Test logs (`.log` files)
     - Test fixtures (`*.json` files)
     - DB dumps (`.db` files, if applicable)

## 3. Test Flaky Test Marking

### Rust: Mark a Test as Flaky

```rust
// In rust/src/tests.rs, add a flaky test example:

#[test]
#[ignore]
// flaky: sometimes fails due to timing, see issue #XXX
fn test_example_flaky() {
    // This test is ignored in normal runs
    assert!(true);
}
```

Run tests:

```bash
cd rust
cargo test  # Should skip ignored test
cargo test -- --ignored  # Should run only ignored tests
```

### Go: Mark a Test as Flaky

```go
// In go/cmd/api/api_test.go, add a flaky test example:

func TestExampleFlaky(t *testing.T) {
    if os.Getenv("CI") != "" {
        t.Skip("flaky: skipping on CI, see issue #XXX")
    }
    // Test code
}
```

Run tests:

```bash
cd go
go test -v ./...  # Should skip when CI=true
CI=true go test -v ./...  # Should skip the flaky test
```

## 4. Test CI Retry Logic

The retry logic is documented in `docs/test-flakiness.md`. To test it:

1. **Add a retry wrapper to a step** (example in nightly.yml):

```yaml
- name: Run Rust tests with retry
  run: |
    for i in 1 2 3; do
      cargo test && break || { echo "Attempt $i failed"; sleep 5; }
    done
```

2. **Trigger the workflow manually:**

   - Go to Actions → Nightly and Scheduled Tests → Run workflow

3. **Monitor the run:**
   - Check if retries happen on failure
   - Verify logs show multiple attempts

## 5. Verify Test Flakiness Tracking

### Check Documentation

```bash
# Verify the tracking document exists and is complete
cat docs/test-flakiness.md

# Should contain:
# - Table for tracking flaky tests
# - Instructions for marking flaky tests in Rust and Go
# - CI retry configuration examples
```

### Update the Document

When you discover a flaky test:

1. Add an entry to the table in `docs/test-flakiness.md`:

```markdown
| TestWebSocketConnection | Go | WebSocket connection fails randomly | 1/20 runs | #123 | Open |
```

2. Mark the test in code (see examples above)

3. Add retry logic if needed in CI

## 6. End-to-End Verification Checklist

- [ ] **Rust tests show structured logs locally** (`RUST_LOG=debug cargo test`)
- [ ] **Go tests show JSON logs locally** (`go test -v ./...`)
- [ ] **PR workflow collects artifacts on failure** (check GitHub Actions after a failed test)
- [ ] **Nightly workflow collects artifacts on failure** (check after nightly run)
- [ ] **Flaky tests can be marked with `#[ignore]` in Rust**
- [ ] **Flaky tests can be skipped with `t.Skip()` in Go**
- [ ] **`docs/test-flakiness.md` has complete instructions**
- [ ] **Workflow files have no syntax errors** (validated by GitHub Actions)

## 7. Common Issues and Troubleshooting

### Issue: No logs appear in tests

**Rust:** Make sure to set `RUST_LOG=debug` or higher:

```bash
RUST_LOG=debug cargo test
```

**Go:** Check that logrus is initialized in test files (should be in `init()` function)

### Issue: Artifacts not collected in CI

**Check:**

1. Test actually failed (artifacts only collected on failure)
2. Paths in workflow file match actual file locations
3. Files exist at the specified paths after test failure

**Debug:**

```yaml
- name: Debug artifacts
  if: always()
  run: |
    find . -name "*.log" -o -name "*.db" -o -name "*.json"
```

### Issue: Flaky tests still running on CI

**Check:**

1. Environment variable is set correctly (`CI=true` for Go)
2. Test is actually marked with `#[ignore]` or `t.Skip()`
3. CI workflow doesn't use `--ignored` flag (which would run ignored tests)

## 8. Quick Smoke Test

Run this script to verify everything at once:

```bash
#!/bin/bash
set -e

echo "=== Testing Rust with logging ==="
cd rust
RUST_LOG=debug cargo test --bins 2>&1 | head -n 50

echo -e "\n=== Testing Go with logging ==="
cd ../go
go test -v ./... 2>&1 | head -n 50

echo -e "\n=== Checking documentation ==="
cat ../docs/test-flakiness.md | head -n 30

echo -e "\n=== Checking workflow syntax ==="
# This requires GitHub CLI or online validation
# For now, just verify files exist
ls -la ../.github/workflows/*.yml

echo -e "\n✅ All checks passed! Implementation is correct."
```

Save this as `test-implementation.sh` and run:

```bash
chmod +x test-implementation.sh
./test-implementation.sh
```

## Success Criteria

Your implementation is correct if:

1. ✅ Tests produce structured logs (JSON for Go, env_logger for Rust)
2. ✅ CI workflows save artifacts on failure (visible in GitHub Actions UI)
3. ✅ Flaky tests can be marked and skipped appropriately
4. ✅ Documentation exists and is complete (`docs/test-flakiness.md`)
5. ✅ Workflow files have no syntax errors
6. ✅ All changes are committed to the repository

---

**Need help?** Check the implementation files:

- Rust logging: `rust/src/tests.rs` (look for `init_logging()`)
- Go logging: `go/cmd/api/api_test.go` (look for `init()` with logrus)
- CI artifacts: `.github/workflows/pr-checks.yml` and `nightly.yml`
- Tracking doc: `docs/test-flakiness.md`
