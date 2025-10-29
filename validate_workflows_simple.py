#!/usr/bin/env python3
"""Simple workflow validation without external dependencies."""

import json
from pathlib import Path


def check_workflow_file(filepath):
    """Basic validation of workflow file."""
    print(f"=== Checking: {filepath} ===")

    try:
        with open(filepath, "r") as f:
            content = f.read()

        # Basic checks
        errors = []

        # Check for required fields
        if "name:" not in content:
            errors.append("Missing 'name:' field")

        if "on:" not in content:
            errors.append("Missing 'on:' trigger field")

        if "jobs:" not in content:
            errors.append("Missing 'jobs:' field")

        # Check for common syntax issues
        lines = content.split("\n")
        for i, line in enumerate(lines, 1):
            # Check for tabs (GitHub Actions requires spaces)
            if "\t" in line:
                errors.append(f"Line {i}: Contains tab character (use spaces)")

            # Check for basic YAML issues
            if line.strip().startswith("-") and ":" in line:
                if line.count(":") > 1 and not any(q in line for q in ['"', "'"]):
                    # Might be unquoted value with colon
                    pass

        if errors:
            print("❌ Issues found:")
            for error in errors:
                print(f"   - {error}")
            return False
        else:
            print("✅ Basic validation passed")
            print(f"   - Has name, triggers, and jobs")
            print(f"   - {len(lines)} lines")
            return True

    except Exception as e:
        print(f"❌ Error reading file: {e}")
        return False

    print()


def main():
    workflows_dir = Path(".github/workflows")

    if not workflows_dir.exists():
        print(f"❌ Directory not found: {workflows_dir}")
        return False

    yaml_files = list(workflows_dir.glob("*.yml"))

    if not yaml_files:
        print(f"❌ No .yml files found in {workflows_dir}")
        return False

    print(f"Found {len(yaml_files)} workflow files\n")

    all_valid = True
    for yaml_file in yaml_files:
        if not check_workflow_file(yaml_file):
            all_valid = False
        print()

    if all_valid:
        print("=" * 50)
        print("✅ All workflows passed basic validation!")
        print("=" * 50)
        print("\nNext steps:")
        print("1. Commit and push to GitHub")
        print("2. GitHub will do full validation when workflows run")
        print("3. Check Actions tab for any runtime issues")
    else:
        print("=" * 50)
        print("❌ Some workflows have issues")
        print("=" * 50)

    return all_valid


if __name__ == "__main__":
    import sys

    sys.exit(0 if main() else 1)
