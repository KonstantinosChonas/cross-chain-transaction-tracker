# Flaky test tracking

This document tracks flaky tests in the CrossChainTransactionTracker project. Please update this file whenever a flaky test is discovered, fixed, or investigated.

## How to use

- **Add**: When a test is observed to be flaky (sometimes fails, sometimes passes), add it below.
- **Update**: When the root cause is found or fixed, update the entry.
- **Reference**: Link to issues, PRs, or CI runs as evidence.

## Marking flaky tests

**Rust:**

- Use `#[ignore]` to skip unstable tests, or add a comment `// flaky: ...` above the test.
- Example:
  ```rust
  #[test]
  #[ignore]
  // flaky: times out on CI, see issue #123
  fn test_something_flaky() { ... }
  ```

**Go:**

- Use `t.Skip("flaky: ...")` or add a comment above the test function.
- Example:
  ```go
  // flaky: fails randomly, see issue #123
  func TestFlaky(t *testing.T) {
  		if os.Getenv("CI") != "" { t.Skip("flaky: see issue #123") }
  		...
  }
  ```

## Setting retries in CI

GitHub Actions does not support per-job retries natively, but you can use the `retry` step action or wrap test commands in a retry loop. For critical jobs, consider using a third-party action or custom script.

**Example retry in a step:**

```yaml
		- name: Run Rust tests with retry
			run: |
				for i in 1 2 3; do
					cargo test && break || sleep 5
				done
```

Update this file and CI as you discover or fix flaky tests.

---

## Flaky tests

| Test Name / Path | Language | Symptoms | Frequency | Issue/PR | Status |
| ---------------- | -------- | -------- | --------- | -------- | ------ |
| _None yet_       |          |          |           |          |        |

---

**Legend:**

- **Symptoms**: e.g. "times out", "random assertion failure", "race condition"
- **Frequency**: e.g. "1/10 runs", "sporadic", "frequent"
- **Status**: Open, Investigating, Fixed, Ignored
