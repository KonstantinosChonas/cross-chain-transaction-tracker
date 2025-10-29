# CI/CD and Branch Protection Guide

## Overview

This document describes the Continuous Integration and Continuous Deployment (CI/CD) setup for the Cross-Chain Transaction Tracker project, including branch protection rules and required status checks.

## GitHub Actions Workflows

### 1. PR Quick Checks (`pr-checks.yml`)

**Trigger:** Pull requests and pushes to `main`, `develop`, and `release/**` branches

**Jobs:**

- `test-unit-rust` - Rust unit tests, formatting, and Clippy
- `test-unit-go` - Go unit tests (matrix: Go 1.21, 1.22)
- `test-go-race` - Go race detector
- `lint` - Code linting for both Rust and Go
- `build` - Build verification for both services

**Purpose:** Fast feedback loop for pull requests (5-10 minutes)

### 2. Integration and E2E Tests (`integration-e2e.yml`)

**Trigger:** Pushes to `main` and `release/**`, pull requests, manual dispatch

**Jobs:**

- `test-integration` - Integration tests with Docker infrastructure
- `test-e2e` - E2E tests (matrix: ethereum, solana, chaos)
- `test-e2e-full` - Complete E2E suite (main/release branches only)

**Purpose:** Comprehensive validation with real infrastructure (30-60 minutes)

### 3. Code Coverage (`coverage.yml`)

**Trigger:** Pushes to `main` and `develop`, pull requests, manual dispatch

**Jobs:**

- `coverage-rust` - Rust code coverage with cargo-llvm-cov
- `coverage-go` - Go code coverage
- `coverage-combined` - Combined coverage report
- `coverage-integration` - Integration test coverage (main branch only)

**Coverage Thresholds:**

- Minimum: 70% for both Rust and Go
- Reports uploaded to Codecov
- HTML reports available as artifacts

### 4. Nightly and Scheduled Tests (`nightly.yml`)

**Trigger:** Scheduled (2 AM UTC daily), pushes to `main`, manual dispatch

**Jobs:**

- `test-fuzzing-rust` - Property-based and fuzz testing
- `test-load` - Load testing with concurrent requests
- `test-stress` - Stress testing under high load
- `test-chaos` - Chaos engineering tests
- `security-scan` - Security vulnerability scanning (Trivy, cargo-audit, gosec)
- `notify` - Results notification

**Purpose:** Extensive testing and security scanning (60+ minutes)

## Branch Protection Rules

### Main Branch (`main`)

Configure the following protection rules in GitHub Settings → Branches → Add rule:

#### Required Status Checks

**Must pass before merging:**

- ✅ `test-unit-rust`
- ✅ `test-unit-go`
- ✅ `test-go-race`
- ✅ `lint`
- ✅ `build`
- ✅ `test-integration`
- ✅ `test-e2e`
- ✅ `coverage-rust`
- ✅ `coverage-go`

#### Additional Settings

```
☑ Require branches to be up to date before merging
☑ Require status checks to pass before merging
☑ Require conversation resolution before merging
☑ Require linear history
☑ Include administrators (recommended)
☐ Allow force pushes (disabled)
☐ Allow deletions (disabled)
```

#### Pull Request Requirements

```
☑ Require pull request reviews before merging
  - Required approving reviews: 1
  - Dismiss stale pull request approvals when new commits are pushed
  - Require review from Code Owners (if CODEOWNERS file exists)
```

### Release Branches (`release/**`)

Apply the same rules as `main` branch with the following additions:

**Additional Required Checks:**

- ✅ `test-e2e-full` - Complete E2E test suite

### Develop Branch (`develop`)

Lighter protection for development:

**Required Status Checks:**

- ✅ `test-unit-rust`
- ✅ `test-unit-go`
- ✅ `lint`
- ✅ `build`

## Setting Up Branch Protection

### Via GitHub Web UI

1. Navigate to your repository on GitHub
2. Go to **Settings** → **Branches**
3. Click **Add branch protection rule**
4. Enter branch name pattern (e.g., `main`, `release/**`)
5. Enable the following:
   - ✅ Require a pull request before merging
   - ✅ Require status checks to pass before merging
   - ✅ Require branches to be up to date before merging
6. Search and select required status checks from the list
7. Enable additional settings as listed above
8. Click **Create** or **Save changes**

### Via GitHub CLI

```bash
# Install GitHub CLI if not already installed
# https://cli.github.com/

# Protect main branch
gh api repos/:owner/:repo/branches/main/protection \
  --method PUT \
  -H "Accept: application/vnd.github+json" \
  -f required_status_checks='{"strict":true,"contexts":["test-unit-rust","test-unit-go","test-go-race","lint","build","test-integration","test-e2e","coverage-rust","coverage-go"]}' \
  -f enforce_admins=true \
  -f required_pull_request_reviews='{"required_approving_review_count":1,"dismiss_stale_reviews":true}' \
  -f restrictions=null \
  -f required_linear_history=true \
  -f allow_force_pushes=false \
  -f allow_deletions=false
```

### Via Terraform (Infrastructure as Code)

```hcl
resource "github_branch_protection" "main" {
  repository_id = github_repository.repo.node_id
  pattern       = "main"

  required_status_checks {
    strict   = true
    contexts = [
      "test-unit-rust",
      "test-unit-go",
      "test-go-race",
      "lint",
      "build",
      "test-integration",
      "test-e2e",
      "coverage-rust",
      "coverage-go"
    ]
  }

  required_pull_request_reviews {
    dismiss_stale_reviews           = true
    require_code_owner_reviews      = true
    required_approving_review_count = 1
  }

  enforce_admins        = true
  require_linear_history = true
  allow_force_pushes    = false
  allow_deletions       = false
}
```

## Workflow Matrix Strategy

### PR Workflow (Fast Checks)

```
┌─────────────────────────────────────┐
│ Pull Request Opened/Updated         │
└──────────────┬──────────────────────┘
               │
               ├─→ Rust Unit Tests (5 min)
               ├─→ Go Unit Tests - v1.21 (3 min)
               ├─→ Go Unit Tests - v1.22 (3 min)
               ├─→ Go Race Detector (4 min)
               ├─→ Linting (2 min)
               ├─→ Build (4 min)
               └─→ Coverage (6 min)
                    │
                    ▼
               All Checks Pass
                    │
                    ▼
           Merge Button Enabled
```

### Main Branch Workflow (Comprehensive)

```
┌─────────────────────────────────────┐
│ Merged to Main                      │
└──────────────┬──────────────────────┘
               │
               ├─→ PR Quick Checks
               ├─→ Integration Tests (15 min)
               ├─→ E2E Tests Matrix:
               │   ├─→ Ethereum (10 min)
               │   ├─→ Solana (10 min)
               │   └─→ Chaos (12 min)
               ├─→ Full E2E Suite (20 min)
               └─→ Coverage Reports
                    │
                    ▼
              Artifacts Published
```

### Nightly Workflow (Scheduled)

```
┌─────────────────────────────────────┐
│ 2 AM UTC Daily                      │
└──────────────┬──────────────────────┘
               │
               ├─→ Fuzzing Tests (30 min)
               ├─→ Load Tests (45 min)
               ├─→ Stress Tests (30 min)
               ├─→ Chaos Engineering (25 min)
               └─→ Security Scans (10 min)
                    │
                    ▼
           Results & Notifications
```

## Test Artifacts

All workflows upload artifacts on failure:

- **Test Logs:** Application and container logs
- **Coverage Reports:** HTML and LCOV/XML formats
- **Performance Metrics:** Load and stress test results
- **Security Reports:** SARIF format for GitHub Security tab

**Retention:**

- PR artifacts: 7 days
- Main branch artifacts: 14 days
- Coverage reports: 30 days
- Security scans: 90 days (automatic)

## Environment Variables and Secrets

### Required Secrets

Add these in GitHub Settings → Secrets and variables → Actions:

```bash
# Optional: Codecov integration
CODECOV_TOKEN=<your-codecov-token>

# Optional: Notification webhooks
SLACK_WEBHOOK_URL=<your-slack-webhook>
TEAMS_WEBHOOK_URL=<your-teams-webhook>
```

### Environment Variables

Set in workflow files or repository settings:

```yaml
CARGO_TERM_COLOR: always
RUST_BACKTRACE: 1
GO111MODULE: on
```

## Manual Workflow Dispatch

Some workflows can be triggered manually:

```bash
# Trigger nightly tests manually
gh workflow run nightly.yml \
  -f run-load-tests=true \
  -f run-fuzzing=true

# Run E2E tests on demand
gh workflow run integration-e2e.yml

# Generate coverage reports
gh workflow run coverage.yml
```

## Monitoring and Alerts

### Workflow Status Badge

Add to README.md:

```markdown
![CI](https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/workflows/PR%20Quick%20Checks/badge.svg)
![E2E](https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/workflows/Integration%20and%20E2E%20Tests/badge.svg)
![Coverage](https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/workflows/Code%20Coverage/badge.svg)
```

### Codecov Badge

```markdown
[![codecov](https://codecov.io/gh/KonstantinosChonas/cross-chain-transaction-tracker/branch/main/graph/badge.svg)](https://codecov.io/gh/KonstantinosChonas/cross-chain-transaction-tracker)
```

## Troubleshooting

### Common Issues

**1. Status checks don't appear**

- Ensure workflows have run at least once
- Check workflow YAML syntax
- Verify branch names match protection rules

**2. Tests timeout**

- Increase timeout values in workflow files
- Check Docker resource limits
- Review test infrastructure startup times

**3. Coverage fails to upload**

- Verify CODECOV_TOKEN is set
- Check network connectivity
- Ensure coverage files are generated

**4. Race detector finds issues**

- Review Go concurrency patterns
- Add proper synchronization
- Check for shared state access

## Best Practices

1. **Keep PR checks fast** (< 10 minutes)
2. **Run expensive tests on main/release branches only**
3. **Always collect artifacts on failure**
4. **Set appropriate timeouts**
5. **Use caching for dependencies**
6. **Matrix tests for different versions**
7. **Fail fast when possible**
8. **Provide clear error messages**

## Acceptance Criteria Summary

✅ **PRs cannot be merged if:**

- Unit tests fail (Rust or Go)
- Race detector finds issues
- Linting fails
- Build fails
- Code coverage drops below threshold
- Integration tests fail
- E2E tests fail

✅ **Main/Release branches require:**

- All PR checks passing
- Full E2E test suite passing
- Code review approval
- Up-to-date with base branch
- All conversations resolved

✅ **Additional validations:**

- Nightly security scans
- Regular fuzzing and property-based testing
- Load and stress testing
- Chaos engineering validation

## Further Reading

- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [Branch Protection Rules](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches)
- [Status Checks](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/collaborating-on-repositories-with-code-quality-features/about-status-checks)
- [Codecov Documentation](https://docs.codecov.com/)
