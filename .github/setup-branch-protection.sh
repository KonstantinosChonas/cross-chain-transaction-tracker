#!/bin/bash

# Branch Protection Setup Script
# This script helps configure branch protection rules via GitHub CLI

set -e

REPO_OWNER="KonstantinosChonas"
REPO_NAME="cross-chain-transaction-tracker"

echo "=========================================="
echo "Branch Protection Setup"
echo "=========================================="
echo ""

# Check if gh CLI is installed
if ! command -v gh &> /dev/null; then
    echo "❌ GitHub CLI (gh) is not installed."
    echo "Please install it from: https://cli.github.com/"
    exit 1
fi

# Check authentication
if ! gh auth status &> /dev/null; then
    echo "❌ Not authenticated with GitHub CLI."
    echo "Please run: gh auth login"
    exit 1
fi

echo "✅ GitHub CLI is installed and authenticated"
echo ""

# Function to set up branch protection
setup_branch_protection() {
    local branch=$1
    shift
    local status_checks=("$@")
    
    echo "Setting up protection for branch: $branch"
    
    # Convert array to JSON array
    local checks_json=$(printf '%s\n' "${status_checks[@]}" | jq -R . | jq -s .)
    
    # Create protection rule
    gh api \
        --method PUT \
        -H "Accept: application/vnd.github+json" \
        "/repos/$REPO_OWNER/$REPO_NAME/branches/$branch/protection" \
        -f required_status_checks="$(cat <<EOF
{
  "strict": true,
  "contexts": $checks_json
}
EOF
)" \
        -f enforce_admins=true \
        -f required_pull_request_reviews='{"dismiss_stale_reviews":true,"require_code_owner_reviews":false,"required_approving_review_count":1}' \
        -f restrictions=null \
        -F required_linear_history=true \
        -F allow_force_pushes=false \
        -F allow_deletions=false \
        && echo "✅ Protection set for $branch" \
        || echo "❌ Failed to set protection for $branch"
    
    echo ""
}

# Main branch protection
echo "Configuring main branch..."
main_checks=(
    "test-unit-rust"
    "test-unit-go"
    "test-go-race"
    "lint"
    "build"
    "test-integration"
    "test-e2e"
    "coverage-rust"
    "coverage-go"
)
setup_branch_protection "main" "${main_checks[@]}"

# Develop branch protection (lighter)
echo "Configuring develop branch..."
develop_checks=(
    "test-unit-rust"
    "test-unit-go"
    "lint"
    "build"
)
setup_branch_protection "develop" "${develop_checks[@]}"

echo "=========================================="
echo "Setup Complete!"
echo "=========================================="
echo ""
echo "Next steps:"
echo "1. Review branch protection rules in GitHub web UI"
echo "2. Add CODECOV_TOKEN to repository secrets (optional)"
echo "3. Configure notification webhooks (optional)"
echo "4. Review .github/CI_BRANCH_PROTECTION.md for details"
echo ""
echo "Note: Release branch protection (release/**) should be"
echo "configured as needed through the GitHub web UI or by"
echo "modifying this script."
