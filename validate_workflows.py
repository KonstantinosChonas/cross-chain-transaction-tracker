#!/usr/bin/env python3
"""Validate GitHub Actions workflow YAML files"""

import yaml
import sys
from pathlib import Path


def validate_workflows():
    workflows_dir = Path(".github/workflows")
    all_valid = True

    if not workflows_dir.exists():
        print(f"❌ Directory not found: {workflows_dir}")
        return False

    yaml_files = list(workflows_dir.glob("*.yml"))
    if not yaml_files:
        print(f"❌ No .yml files found in {workflows_dir}")
        return False

    print(f"Found {len(yaml_files)} workflow file(s)\n")

    for yaml_file in sorted(yaml_files):
        print(f"=== Checking: {yaml_file} ===")
        try:
            with open(yaml_file, "r") as f:
                content = yaml.safe_load(f)

            # Basic validation
            if not isinstance(content, dict):
                print(f"❌ Invalid structure: root should be a dict")
                all_valid = False
                continue

            # Check required GitHub Actions fields
            if "name" not in content:
                print(f"⚠️  Warning: 'name' field missing")

            if "on" not in content:
                print(f"❌ Error: 'on' field (triggers) missing")
                all_valid = False
                continue

            if "jobs" not in content:
                print(f"❌ Error: 'jobs' field missing")
                all_valid = False
                continue

            job_count = len(content["jobs"])
            print(f"✅ Valid YAML syntax ({job_count} job(s) defined)")

        except yaml.YAMLError as e:
            print(f"❌ YAML Error: {e}")
            all_valid = False
        except Exception as e:
            print(f"❌ Unexpected error: {e}")
            all_valid = False

        print()

    return all_valid


if __name__ == "__main__":
    print("GitHub Actions Workflow Validator\n")
    print("=" * 50)

    if validate_workflows():
        print("=" * 50)
        print("✅ All workflows are valid!")
        sys.exit(0)
    else:
        print("=" * 50)
        print("❌ Some workflows have errors")
        sys.exit(1)
