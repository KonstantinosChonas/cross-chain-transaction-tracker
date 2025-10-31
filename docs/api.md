# API Contract and Event Schema

## High-level integration decision

Phase A suggests: **Rust emits normalized JSON** to stdout or publisher. For Phase A scaffolding we keep JSON over local HTTP POST (Rust -> Go) or file/stdout for testing. For Phase B onward choose Redis pub/sub, NATS, or protobuf/gRPC.

---

## REST Endpoints (Go API)

### Health

`GET /health`
Response: `200 OK` body: `OK`

### Get wallet transactions

`GET /wallet/{address}/transactions`
Query params: `chain` (optional), `limit` (optional, default 50), `offset` (optional)
Response: JSON array of normalized events (see schema)

Example:

```
GET /wallet/0xabc.../transactions?chain=ethereum&token=USDC&limit=25
```

### Get recent transactions

`GET /transactions`
Query params: `chain`, `token`, `from`, `to`, `min_value`, `start_time`, `end_time`, `limit`, `offset`

### SSE / WebSocket for live events

`GET /events/subscribe` (SSE recommended for simplicity)

- SSE messages contain normalized JSON events

---

## Normalized event schema (JSON)

Fields (all fields present where applicable):

````json
{
  "event_id": "string", // generated id (chain+tx_hash)
  "chain": "ethereum", // e.g. "ethereum", "solana"
  "network": "sepolia", // e.g. "mainnet", "sepolia", "devnet"
  "tx_hash": "0x..", // transaction hash (or signature for solana)
  "block_number": 123456, // integer, or null for pending
  "slot": null, // solana slot if applicable
  "timestamp": "2025-10-14T12:34:56Z",
  "from": "0x..",
  "to": "0x..",
  "value": "1000000000000000000", // in wei/lamports or token smallest unit
  "value_decimal": "1.0", // human friendly decimal string (optional)
  "token": {
    // if ERC-20 or SPL token, otherwise null
    "address": "0x..",
    "symbol": "USDT",
    "decimals": 18
  },
  "event_type": "transfer", // transfer, mint, burn, swap, etc
  "raw_payload": {}, // original JSON/logs as captured
  "meta": {
    // optional metadata
    "watchlist_match": true
  }
}

Example response:

```json
[
  {
    "event_id": "eth:0x...",
    "chain": "ethereum",
    "network": "sepolia",
    "tx_hash": "0x...",
    "timestamp": "2025-10-14T12:34:56Z",
    "from": "0x...",
    "to": "0x...",
    "value": "1000000000000000000",
    "event_type": "transfer",
    "token": null
  }
]
````

```

```
