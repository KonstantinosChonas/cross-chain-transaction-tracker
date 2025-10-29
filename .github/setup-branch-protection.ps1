# Branch Protection Setup Script (PowerShell)
# This script helps configure branch protection rules via GitHub CLI

$ErrorActionPreference = "Stop"

$REPO_OWNER = "KonstantinosChonas"
$REPO_NAME = "cross-chain-transaction-tracker"

Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "Branch Protection Setup" -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host ""

# Check if gh CLI is installed
try {
    $null = gh --version
    Write-Host "✅ GitHub CLI is installed" -ForegroundColor Green
}
catch {
    Write-Host "❌ GitHub CLI (gh) is not installed." -ForegroundColor Red
    Write-Host "Please install it from: https://cli.github.com/" -ForegroundColor Yellow
    exit 1
}

# Check authentication
try {
    $null = gh auth status 2>&1
    Write-Host "✅ Authenticated with GitHub CLI" -ForegroundColor Green
}
catch {
    Write-Host "❌ Not authenticated with GitHub CLI." -ForegroundColor Red
    Write-Host "Please run: gh auth login" -ForegroundColor Yellow
    exit 1
}

Write-Host ""

# Function to set up branch protection
function Set-BranchProtection {
    param(
        [string]$Branch,
        [string[]]$StatusChecks
    )
    
    Write-Host "Setting up protection for branch: $Branch" -ForegroundColor Yellow
    
    # Create the protection configuration
    $body = @{
        required_status_checks        = @{
            strict   = $true
            contexts = $StatusChecks
        }
        enforce_admins                = $true
        required_pull_request_reviews = @{
            dismiss_stale_reviews           = $true
            require_code_owner_reviews      = $false
            required_approving_review_count = 1
        }
        restrictions                  = $null
        required_linear_history       = $true
        allow_force_pushes            = $false
        allow_deletions               = $false
    } | ConvertTo-Json -Depth 10
    
    # Save to temp file for gh api input
    $tempFile = [System.IO.Path]::GetTempFileName()
    try {
        $body | Out-File -FilePath $tempFile -Encoding UTF8
        
        gh api `
            --method PUT `
            -H "Accept: application/vnd.github+json" `
            "/repos/$REPO_OWNER/$REPO_NAME/branches/$Branch/protection" `
            --input $tempFile
        
        Write-Host "✅ Protection set for $Branch" -ForegroundColor Green
    }
    catch {
        Write-Host "❌ Failed to set protection for $Branch" -ForegroundColor Red
        Write-Host $_.Exception.Message -ForegroundColor Red
    }
    finally {
        # Clean up temp file
        if (Test-Path $tempFile) {
            Remove-Item $tempFile -Force
        }
    }
    
    Write-Host ""
}

# Main branch protection
Write-Host "Configuring main branch..." -ForegroundColor Cyan
$mainChecks = @(
    "test-unit-rust",
    "test-unit-go",
    "test-go-race",
    "lint",
    "build",
    "test-integration",
    "test-e2e",
    "coverage-rust",
    "coverage-go"
)
Set-BranchProtection -Branch "main" -StatusChecks $mainChecks

# Develop branch protection (lighter)
Write-Host "Configuring develop branch..." -ForegroundColor Cyan
$developChecks = @(
    "test-unit-rust",
    "test-unit-go",
    "lint",
    "build"
)
Set-BranchProtection -Branch "develop" -StatusChecks $developChecks

Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "Setup Complete!" -ForegroundColor Green
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Next steps:" -ForegroundColor Yellow
Write-Host "1. Review branch protection rules in GitHub web UI"
Write-Host "2. Add CODECOV_TOKEN to repository secrets (optional)"
Write-Host "3. Configure notification webhooks (optional)"
Write-Host "4. Review .github/CI_BRANCH_PROTECTION.md for details"
Write-Host ""
Write-Host "Note: Release branch protection (release/**) should be" -ForegroundColor Gray
Write-Host "configured as needed through the GitHub web UI or by" -ForegroundColor Gray
Write-Host "modifying this script." -ForegroundColor Gray
