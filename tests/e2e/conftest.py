import pytest
import subprocess
import logging
import time

logger = logging.getLogger("e2e.conftest")


@pytest.fixture(scope="session", autouse=True)
def ensure_clean_state():
    """Ensure clean state before and after test session"""
    logger.info("Setting up E2E test session...")

    # Before tests: restart key services to clear in-memory state and reconnect to fresh DB
    logger.info("Restarting services to ensure clean state...")
    try:
        # Clear DB first
        subprocess.run(
            [
                "docker",
                "exec",
                "infra-postgres-1",
                "psql",
                "-U",
                "testuser",
                "-d",
                "testdb",
                "-c",
                "TRUNCATE TABLE events;",
            ],
            capture_output=True,
            text=True,
            timeout=10,
        )
        logger.info("✓ Database cleared")

        # Restart API and Rust to reconnect with fresh state
        subprocess.run(
            ["docker", "restart", "infra-api-1", "infra-rust-1"],
            capture_output=True,
            timeout=30,
            check=True,
        )
        logger.info("✓ Services restarted")
    except Exception as e:
        logger.warning(f"Failed to reset state: {e}")

    # Wait for services to be ready
    time.sleep(10)

    yield

    # After tests: no explicit cleanup needed as test-stop.sh handles it
    logger.info("E2E test session completed")


@pytest.fixture(scope="function")
def unique_test_id():
    """Generate unique test ID for idempotent test runs"""
    import uuid

    test_id = str(uuid.uuid4())[:8]
    logger.info(f"Test ID: {test_id}")
    return test_id


@pytest.fixture(scope="function")
def docker_logs_before_test():
    """Capture docker logs timestamp before test starts"""
    # This helps isolate logs from current test run
    result = subprocess.run(
        ["docker", "logs", "infra-rust-1", "--tail", "1"],
        capture_output=True,
        text=True,
        timeout=5,
    )
    return time.time()
