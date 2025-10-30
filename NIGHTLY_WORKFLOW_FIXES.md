# Nightly Workflow Bug Fixes

## Summary

Fixed potential bugs in Load Tests, Stress Tests, and Chaos Engineering Tests by aligning them with the working E2E and Integration test patterns.

## Bugs Found and Fixed

### 1. **Load Tests - Service Startup Order Issue**

**Problem:** Infrastructure and application services were started together in a single command, which could cause race conditions.

**Before:**

```yaml
- name: Start test infrastructure
  run: |
    docker compose -f infra/test-docker-compose.yml up -d --wait
    docker compose -f infra/docker-compose.yml build
    docker compose -f infra/docker-compose.yml up -d --wait
```

**After:** (Matching working e2e.yml pattern)

```yaml
- name: Start test infrastructure
  run: docker compose -f infra/test-docker-compose.yml up -d --wait

- name: Show infra container status
  run: docker compose -f infra/test-docker-compose.yml ps -a

- name: Verify infrastructure is ready
  run: |
    echo "Verifying Ethereum node..."
    curl -sf http://localhost:8545 ...
    echo "Verifying Solana node..."
    curl -sf http://localhost:8899 ...

- name: Build and start application services
  run: |
    docker compose -f infra/docker-compose.yml build
    docker compose -f infra/docker-compose.yml up -d --wait

- name: Show app container status
  run: docker compose -f infra/docker-compose.yml ps -a

- name: Verify application services
  run: |
    echo "Verifying Rust tracker..."
    curl -sf http://localhost:8080/health ...
```

**Fix:** Separated infrastructure and application startup with proper verification steps between them.

---

### 2. **Load Tests - Missing pip upgrade**

**Problem:** Direct `pip install` without upgrading pip first (unlike working tests).

**Before:**

```yaml
- name: Install load testing tools
  run: |
    pip install locust pytest pytest-benchmark
```

**After:**

```yaml
- name: Install load testing tools
  run: |
    python -m pip install --upgrade pip
    pip install locust pytest pytest-benchmark requests
```

**Fix:** Added `pip upgrade` and explicit `requests` dependency.

---

### 3. **Load Tests - Missing failure logging**

**Problem:** No detailed log collection on failure (working tests have this).

**Added:**

```yaml
- name: Collect logs on failure
  if: failure()
  run: |
    docker compose -f infra/docker-compose.yml logs > app-logs.txt || true
    docker compose -f infra/test-docker-compose.yml logs > infra-logs.txt || true

- name: Upload failure artifacts
  if: failure()
  uses: actions/upload-artifact@v4
  with:
    name: load-test-failure-artifacts
    path: |
      app-logs.txt
      infra-logs.txt
    retention-days: 7
```

---

### 4. **Stress Tests - Missing Infrastructure Health Checks**

**Problem:** Only checked app services, didn't verify Ethereum/Solana nodes were ready.

**Before:**

```yaml
- name: Verify services are ready
  run: |
    curl -sf http://localhost:8080/health || exit 1
    curl -sf http://localhost:3000/health || exit 1
```

**After:**

```yaml
- name: Verify infrastructure is ready
  run: |
    echo "Verifying Ethereum node..."
    curl -sf http://localhost:8545 -X POST ...
    echo "Verifying Solana node..."
    curl -sf http://localhost:8899 -X POST ...
    echo "Infrastructure ready!"

- name: Build and start application services
  run: |
    docker compose -f infra/docker-compose.yml build
    docker compose -f infra/docker-compose.yml up -d --wait

- name: Verify services are ready
  run: |
    echo "Verifying Rust tracker..."
    curl -sf http://localhost:8080/health || exit 1
    echo "Verifying Go API..."
    curl -sf http://localhost:3000/health || exit 1
```

---

### 5. **Stress Tests - Missing Container Status Visibility**

**Added:**

```yaml
- name: Show infra container status
  run: docker compose -f infra/test-docker-compose.yml ps -a

- name: Show app container status
  run: docker compose -f infra/docker-compose.yml ps -a
```

**Benefit:** Easier debugging when services fail to start.

---

### 6. **Stress Tests - Missing failure logging**

**Added:**

```yaml
- name: Collect logs on failure
  if: failure()
  run: |
    docker compose -f infra/docker-compose.yml logs > stress-app-logs.txt || true
    docker compose -f infra/test-docker-compose.yml logs > stress-infra-logs.txt || true

- name: Upload failure artifacts
  if: failure()
  uses: actions/upload-artifact@v4
  with:
    name: stress-test-failure-artifacts
    path: |
      stress-app-logs.txt
      stress-infra-logs.txt
    retention-days: 7
```

---

### 7. **Chaos Tests - pip cache misconfiguration**

**Problem:** Had `cache: "pip"` without `cache-dependency-path` (could cause cache misses).

**Before:**

```yaml
- name: Set up Python
  uses: actions/setup-python@v5
  with:
    python-version: "3.11"
    cache: "pip"
```

**After:**

```yaml
- name: Set up Python
  uses: actions/setup-python@v5
  with:
    python-version: "3.11"
```

**Fix:** Removed incomplete cache configuration (pip caching handled by actions automatically with requirements.txt).

---

### 8. **Chaos Tests - Missing pip upgrade**

**Before:**

```yaml
- name: Install Python dependencies
  working-directory: ./tests/e2e
  run: pip install -r requirements.txt
```

**After:**

```yaml
- name: Install Python dependencies
  working-directory: ./tests/e2e
  run: |
    python -m pip install --upgrade pip
    pip install -r requirements.txt
```

---

### 9. **Chaos Tests - Same service startup issues**

**Fixed:** Applied the same infrastructure-first, then app pattern with proper health checks.

---

## Why These Bugs Matter

1. **Race Conditions:** Starting all services together could cause app services to fail if infra isn't ready
2. **Debugging:** Without container status and logs, failures would be hard to diagnose
3. **Reliability:** Missing health checks could lead to false failures
4. **Consistency:** Following the same pattern as working tests reduces surprises

## Testing Recommendation

Since these tests are expensive (60+ minutes) and were skipped during development, recommend:

1. **Manual Test Run:** Trigger via workflow_dispatch to verify all fixes
2. **Wait for Nightly:** Let the scheduled run validate everything
3. **Monitor First Run:** Check artifacts if any failures occur

## Pattern Established

All test jobs now follow this proven pattern:

1. ✅ Start infrastructure with `--wait`
2. ✅ Show infra container status (`ps -a`)
3. ✅ Verify infrastructure health (curl checks)
4. ✅ Build and start app services
5. ✅ Show app container status
6. ✅ Verify app services health
7. ✅ Run tests
8. ✅ Collect logs on failure
9. ✅ Always cleanup with `down -v`
