# Chaos Testing and Failure Injection

This document describes how to run chaos tests that simulate network failures, RPC disconnects, broker restarts, and service restarts, and how the system should recover.

## Scenarios

- RPC disconnects (Ethereum Anvil / Solana validator): stop and start the chain nodes while listeners run; expect resume without duplicates.
- Message bus downtime (Redis): stop Redis during publish; Rust retries with exponential backoff; upon Redis restart, events are delivered once.
- Service restart (Go API): restart API mid-ingestion; Postgres-backed persistence prevents duplicates; API resumes serving data.

## Running automated chaos tests

The E2E pytest suite includes chaos tests:

```bash
# from repo root
E2E=true ./scripts/e2e.sh  # brings up infra
# inside the tests/e2e folder, run the chaos tests only
cd tests/e2e
pytest -k chaos -v
```

Key tests:

- `test_rpc_disconnects_eth_resume_no_duplicates`
- `test_message_bus_downtime_redis_retry_and_delivery`
- `test_api_restart_mid_ingestion_persistence_and_resume`

## Ad-hoc chaos harness

A simple random chaos script is available:

```bash
./scripts/chaos.sh 10  # run 10 random stop/start cycles
```

It randomly stops/starts one of: `anvil`, `solana`, `redis`, `api`, `rust`, and asserts the API health recovers.

## Expected behavior and acceptance

After simulated outages, the system should show either:

- All transactions are eventually delivered and de-duplicated; or
- If delivery fails (e.g., prolonged Redis outage past retry window, or API down during Pub/Sub message), the missing event is logged in Rust as a publish failure (`Failed to publish event ... after retries`). Such events require manual replay.

### Notes and caveats

- The current transport is Redis Pub/Sub (ephemeral). If the API is down exactly when messages are published, those messages are not buffered by Redis. With the new Postgres persistence in the API, duplicates from re-processing are prevented via `event_id` primary key, but missed Pub/Sub messages may still need manual replay.
- Rust publisher now uses exponential backoff when publishing to Redis, mitigating short broker outages without data loss.
- Ethereum tracking uses HTTP polling in tests; after Anvil restart, the poller resumes from the last seen block and processes new blocks without duplication.

## Manual replay guidance (if required)

If an event was logged as a publish failure:

- Identify the transaction hash from Rust logs.
- Re-run a small replay job or script that fetches the normalized event by tx hash and republishes it to Redis. (A simple replay utility can be added later.)
