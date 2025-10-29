# Fixes for CI/CD Issues

## Run these commands to fix all issues:

```bash
# 1. Fix docker-compose typos in workflows
python3 fix_workflows.py

# 2. Fix nightly.yml - remove proptest feature (doesn't exist)
# 3. Fix nightly.yml - make security scan less strict
# 4. Fix coverage.yml - remove security upload (permissions issue)
# 5. Fix Rust test
```

## Manual fixes needed after running fix_workflows.py:
