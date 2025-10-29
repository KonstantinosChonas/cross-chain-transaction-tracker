#!/usr/bin/env python3
"""Fix workflow files - replace docker compose.yml with docker-compose.yml"""

import os
from pathlib import Path

workflows_dir = Path(".github/workflows")

for yml_file in workflows_dir.glob("*.yml"):
    print(f"Processing {yml_file}...")

    with open(yml_file, "r", encoding="utf-8") as f:
        content = f.read()

    original = content

    # Fix the typo: "docker compose.yml" → "docker-compose.yml"
    content = content.replace("docker compose.yml", "docker-compose.yml")

    if content != original:
        with open(yml_file, "w", encoding="utf-8") as f:
            f.write(content)
        print(f"  ✅ Fixed {yml_file}")
    else:
        print(f"  ⏭️  No changes needed")

print("\n✅ All workflow files processed!")
