# GitHub Actions CI/CD Configuration

This directory contains the GitHub Actions workflows and configuration for the Cross-Chain Transaction Tracker project.

## 📁 Directory Structure

```
.github/
├── workflows/
│   ├── pr-checks.yml           # Fast PR validation
│   ├── integration-e2e.yml     # Integration and E2E tests
│   ├── coverage.yml            # Code coverage reporting
│   └── nightly.yml             # Scheduled comprehensive tests
├── CI_BRANCH_PROTECTION.md     # Detailed CI/CD documentation
├── setup-branch-protection.sh  # Branch protection setup (Bash)
└── setup-branch-protection.ps1 # Branch protection setup (PowerShell)
```

## 🚀 Quick Start

### 1. Enable GitHub Actions

GitHub Actions should be enabled by default. Verify in: `Settings → Actions → General`

### 2. Set Up Branch Protection

Choose your platform and run the setup script:

**Linux/macOS:**

```bash
chmod +x .github/setup-branch-protection.sh
.github/setup-branch-protection.sh
```

**Windows (PowerShell):**

```powershell
.\.github\setup-branch-protection.ps1
```

**Manual Setup:**
Follow the guide in [CI_BRANCH_PROTECTION.md](./CI_BRANCH_PROTECTION.md#setting-up-branch-protection)

### 3. Configure Secrets (Optional)

Add these secrets in `Settings → Secrets and variables → Actions`:

- `CODECOV_TOKEN` - For coverage reporting to Codecov
- `SLACK_WEBHOOK_URL` - For Slack notifications (optional)

## 📊 Workflows Overview

### PR Quick Checks

**File:** `pr-checks.yml`  
**Triggers:** Pull requests, pushes to main/develop  
**Duration:** ~1 minute

**What it does:**

- ✅ Rust unit tests + formatting + Clippy
- ✅ Go unit tests (Go 1.21 & 1.22)
- ✅ Go race detector
- ✅ Linting (both languages)
- ✅ Build verification

**When to use:** Every PR must pass these checks before merging.

### Integration and E2E Tests

**File:** `integration-e2e.yml`  
**Triggers:** Pushes to main/release, PRs, manual  
**Duration:** ~20 minutes

**What it does:**

- 🔧 Integration tests with real infrastructure
- 🌐 E2E tests for Ethereum transactions
- 🌐 E2E tests for Solana transactions
- 💥 Chaos engineering tests
- 📦 Full E2E suite (main branch only)

**When to use:** Required for merging to main or release branches.

### Code Coverage

**File:** `coverage.yml`  
**Triggers:** Pushes, PRs, manual  
**Duration:** ~1 minute

**What it does:**

- 📈 Rust coverage with cargo-llvm-cov
- 📈 Go coverage with built-in tools
- 📊 Combined HTML reports
- 📤 Upload to Codecov
- ⚠️ Threshold checking (70% minimum)

**When to use:** Automatically runs on all PRs and main pushes.

### Nightly Tests

**File:** `nightly.yml`  
**Triggers:** Daily at 2 AM UTC, manual  
**Duration:** ~10 minutes

**What it does:**

- 🎲 Fuzzing and property-based tests
- 📊 Load testing
- 💪 Stress testing
- 🔥 Chaos engineering
- 🔒 Security scanning (Trivy, cargo-audit, gosec)

**When to use:** Automatically scheduled; can be triggered manually for release validation.

## 🎯 Merge Requirements

### For Pull Requests

All PRs must pass:

- ✅ Rust unit tests
- ✅ Go unit tests
- ✅ Race detector
- ✅ Linting
- ✅ Build
- ✅ Integration tests
- ✅ E2E tests
- ✅ Code coverage (70%+ threshold)

### For Main Branch

Additional requirements:

- ✅ Full E2E test suite
- ✅ At least 1 approving review
- ✅ All conversations resolved
- ✅ Branch up to date with main

### For Release Branches

Same as main branch, plus:

- ✅ Manual approval from maintainers
- ✅ Nightly tests should be passing

## 🔍 Viewing Test Results

### In Pull Requests

1. Scroll to the bottom of the PR
2. Check "All checks have passed" status
3. Click "Details" on any check to view logs

### Build Artifacts

1. Go to Actions tab
2. Select the workflow run
3. Scroll to "Artifacts" section
4. Download:
   - Test logs
   - Coverage reports (HTML)
   - Container logs (on failure)

### Coverage Reports

- **Codecov:** Automatic comment on PRs with coverage diff
- **Artifacts:** Download HTML reports from workflow runs
- **Main branch:** Coverage badge in README

## 🛠️ Manual Workflow Triggers

Run workflows manually via GitHub CLI:

```bash
# Run full E2E tests on demand
gh workflow run integration-e2e.yml

# Run coverage analysis
gh workflow run coverage.yml

# Run nightly tests with specific options
gh workflow run nightly.yml \
  -f run-load-tests=true \
  -f run-fuzzing=true
```

Or via GitHub UI:

1. Go to Actions tab
2. Select workflow on the left
3. Click "Run workflow" button
4. Choose options and confirm

## 📈 Status Badges

Add these to your README.md:

```markdown
![CI](https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/workflows/PR%20Quick%20Checks/badge.svg)
![E2E](https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/workflows/Integration%20and%20E2E%20Tests/badge.svg)
![Coverage](https://github.com/KonstantinosChonas/cross-chain-transaction-tracker/workflows/Code%20Coverage/badge.svg)
[![codecov](https://codecov.io/gh/KonstantinosChonas/cross-chain-transaction-tracker/branch/main/graph/badge.svg)](https://codecov.io/gh/KonstantinosChonas/cross-chain-transaction-tracker)
```

## 🐛 Troubleshooting

### Workflow doesn't start

- Check if Actions are enabled in repo settings
- Verify workflow YAML syntax
- Ensure you have proper permissions

### Status checks not showing

- Workflows must run at least once to appear
- Check branch name matches protection rules
- Wait a few minutes for GitHub to register checks

### Tests timeout

- Review timeout settings in workflow files
- Check Docker resource allocation
- Ensure test infrastructure starts properly

### Coverage upload fails

- Verify CODECOV_TOKEN is set correctly
- Check network connectivity
- Ensure coverage files are generated

## 📚 Additional Resources

- [Detailed CI/CD Guide](./CI_BRANCH_PROTECTION.md) - Comprehensive documentation
- [GitHub Actions Docs](https://docs.github.com/en/actions)
- [Branch Protection](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches)
- [Codecov Documentation](https://docs.codecov.com/)

## 🤝 Contributing

When adding new workflows:

1. Test locally first using [act](https://github.com/nektos/act)
2. Document the workflow purpose and triggers
3. Add appropriate timeouts
4. Include artifact uploads for debugging
5. Update this README and CI_BRANCH_PROTECTION.md
6. Consider impact on PR merge time

## 📝 Maintenance

### Weekly

- Review failed nightly tests
- Check security scan results
- Monitor coverage trends

### Monthly

- Update GitHub Actions versions
- Review and optimize workflow performance
- Clean up old workflow runs

### Quarterly

- Update Go/Rust toolchain versions
- Review branch protection rules
- Audit secrets and permissions
