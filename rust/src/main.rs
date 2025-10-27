use anyhow::anyhow;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use tokio_stream::StreamExt;

use ethers::prelude::*;
use ethers::providers::{Middleware, Provider, Ws};
use solana_client::{rpc_client::RpcClient, rpc_config::RpcTransactionConfig};

use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature};
use solana_transaction_status::UiTransactionEncoding;

use tracing::{error, info, warn};
use tracing_subscriber::{fmt, EnvFilter};
mod config;
mod retry;
mod solana_parser;

async fn publish_event_to_redis(redis_client: &redis::Client, event: &Event) -> anyhow::Result<()> {
    let mut con = redis_client.get_multiplexed_async_connection().await?;
    let payload = serde_json::to_string(event)?;
    con.publish::<_, _, ()>("cross_chain_events", payload)
        .await?;
    info!("Published event to Redis: {}", event.event_id);
    Ok(())
}

#[derive(Deserialize)]
struct SystemTransfer {
    source: String,
    destination: String,
    lamports: u64,
}

#[derive(Deserialize)]
struct TokenTransfer {
    source: String,
    destination: String,
    amount: String,
    decimals: Option<u8>,
}

#[derive(Serialize, Debug)]
struct Token {
    address: String,
    symbol: String,
    decimals: u8,
}

#[derive(Serialize, Debug)]
struct Event {
    event_id: String,
    chain: String,
    network: String,
    tx_hash: String,
    timestamp: String,
    from: String,
    to: String,
    value: String,
    event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    slot: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<Token>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    fmt().with_env_filter(EnvFilter::from_default_env()).init();

    // Load config
    let cfg = match config::Config::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Config error: {:?}", e);
            std::process::exit(1);
        }
    };

    let redis_client = redis::Client::open(cfg.redis_url.clone())?;

    let processed_txs: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let last_eth_block: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));
    let last_sol_slot: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));

    let eth_tracker = {
        let cfg = cfg.clone();
        let processed_txs = Arc::clone(&processed_txs);
        let last_eth_block = Arc::clone(&last_eth_block);
        let redis_client = redis_client.clone();
        tokio::spawn(async move {
            if !cfg.eth_rpc_url.starts_with("ws") {
                error!("ETH tracking requires a WebSocket RPC URL (e.g. wss://...)");
                return;
            }
            loop {
                info!("Connecting to ETH provider at {}", cfg.eth_rpc_url);
                let ws = match Ws::connect(cfg.eth_rpc_url.clone()).await {
                    Ok(ws) => ws,
                    Err(e) => {
                        error!("Failed to connect ETH WebSocket: {:?}. Retrying in 10s.", e);
                        sleep(Duration::from_secs(10)).await;
                        continue;
                    }
                };
                let provider = Arc::new(Provider::new(ws));
                info!("Successfully connected to ETH provider.");

                let watched_addresses: Vec<Address> = cfg
                    .watched_addresses_eth
                    .iter()
                    .map(|s| s.parse().expect("Invalid ETH address"))
                    .collect();

                let native_tracker = track_native_transfers(
                    Arc::clone(&provider),
                    watched_addresses.clone(),
                    cfg.eth_network.clone(),
                    Arc::clone(&processed_txs),
                    Arc::clone(&last_eth_block),
                    redis_client.clone(),
                );

                if watched_addresses.is_empty() {
                    warn!("No watched ETH addresses for ERC-20 transfers. Tracking native transfers only.");
                    if let Err(e) = native_tracker.await {
                        warn!("Native ETH transfer tracker failed: {}.", e);
                    }
                } else {
                    let erc20_tracker = track_erc20_transfers(
                        Arc::clone(&provider),
                        watched_addresses.clone(),
                        cfg.eth_network.clone(),
                        Arc::clone(&processed_txs),
                        Arc::clone(&last_eth_block),
                        redis_client.clone(),
                    );

                    tokio::select! {
                        res = erc20_tracker => {
                            if let Err(e) = res {
                                warn!("ERC-20 tracker failed: {}.", e);
                            }
                        },
                        res = native_tracker => {
                            if let Err(e) = res {
                                warn!("Native ETH transfer tracker failed: {}.", e);
                            }
                        },
                    }
                }
                warn!("An ETH tracker task has finished. Restarting trackers after 5s delay.");
                sleep(Duration::from_secs(5)).await;
            }
        })
    };

    let sol_tracker = {
        let cfg = cfg.clone();
        let redis_client = redis_client.clone();
        tokio::spawn(async move {
            track_solana_transfers(
                &cfg.sol_rpc_url,
                &cfg.sol_network,
                &cfg.watched_addresses_sol,
                Arc::clone(&processed_txs),
                Arc::clone(&last_sol_slot),
                redis_client,
            )
            .await
        })
    };

    tokio::try_join!(eth_tracker, sol_tracker)?;

    Ok(())
}

async fn track_erc20_transfers(
    provider: Arc<Provider<Ws>>,
    watched_addresses: Vec<Address>,
    network: String,
    processed_txs: Arc<Mutex<HashSet<String>>>,
    last_block: Arc<Mutex<Option<u64>>>,
    redis_client: redis::Client,
) -> anyhow::Result<()> {
    let filter = Filter::new().event("Transfer(address,address,uint256)");
    let mut stream = provider.subscribe_logs(&filter).await?;
    info!("Subscribed to all ERC-20 Transfer logs");

    while let Some(log) = stream.next().await {
        if log.topics.len() == 3 {
            let from = Address::from(log.topics[1]);
            let to = Address::from(log.topics[2]);

            if watched_addresses.contains(&from) || watched_addresses.contains(&to) {
                let tx_hash = log.transaction_hash.unwrap_or_default();
                let event_id = format!("eth:{:?}", tx_hash);

                if processed_txs.lock().await.contains(&event_id) {
                    info!("Duplicate event skipped: {}", event_id);
                    continue;
                }

                let block_number = log.block_number;
                let timestamp = match block_number {
                    Some(bn) => match provider.get_block(bn).await {
                        Ok(Some(block)) => block.timestamp.to_string(),
                        _ => {
                            warn!("Could not get block for log in tx {:?}", tx_hash);
                            "".to_string()
                        }
                    },
                    None => "".to_string(),
                };

                let event = Event {
                    event_id: event_id.clone(),
                    chain: "ethereum".into(),
                    network: network.clone(),
                    tx_hash: format!("{:?}", tx_hash),
                    timestamp,
                    from: format!("{:?}", from),
                    to: format!("{:?}", to),
                    value: U256::from_big_endian(&log.data.0).to_string(),
                    event_type: "erc20_transfer".into(),
                    slot: None,
                    token: None,
                };

                if let Err(e) = publish_event_to_redis(&redis_client, &event).await {
                    error!("Failed to publish event to Redis: {:?}", e);
                }
                processed_txs.lock().await.insert(event_id);

                if let Some(bn) = block_number {
                    let mut last = last_block.lock().await;
                    let current_bn = bn.as_u64();
                    if last.is_none() || current_bn > last.unwrap() {
                        *last = Some(current_bn);
                        info!("Updated last processed ETH block to: {}", current_bn);
                    }
                }
            }
        }
    }
    warn!("ERC-20 log stream ended.");
    Err(anyhow!("ERC-20 log stream ended"))
}

async fn track_native_transfers(
    provider: Arc<Provider<Ws>>,
    watched_addresses: Vec<Address>,
    network: String,
    processed_txs: Arc<Mutex<HashSet<String>>>,
    last_block: Arc<Mutex<Option<u64>>>,
    redis_client: redis::Client,
) -> anyhow::Result<()> {
    let mut stream = provider.subscribe_blocks().await?;
    info!("Subscribed to new blocks for native transfers");

    while let Some(block_sub) = stream.next().await {
        if let Some(block_hash) = block_sub.hash {
            match provider.get_block_with_txs(block_hash).await {
                Ok(Some(block)) => {
                    let block_number = block.number.unwrap_or_default();
                    for tx in block.transactions {
                        let from_watched =
                            tx.from != Address::zero() && watched_addresses.contains(&tx.from);
                        let to_watched =
                            tx.to.is_some() && watched_addresses.contains(&tx.to.unwrap());

                        if from_watched || to_watched {
                            let event_id = format!("eth:{:?}", tx.hash);

                            if processed_txs.lock().await.contains(&event_id) {
                                info!("Duplicate event skipped: {}", event_id);
                                continue;
                            }

                            let event = Event {
                                event_id: event_id.clone(),
                                chain: "ethereum".into(),
                                network: network.clone(),
                                tx_hash: format!("{:?}", tx.hash),
                                timestamp: block.timestamp.to_string(),
                                from: format!("{:?}", tx.from),
                                to: format!("{:?}", tx.to.unwrap_or_default()),
                                value: tx.value.to_string(),
                                event_type: "transfer".into(),
                                slot: None,
                                token: None,
                            };
                            if let Err(e) = publish_event_to_redis(&redis_client, &event).await {
                                error!("Failed to publish event to Redis: {:?}", e);
                            }
                            processed_txs.lock().await.insert(event_id);
                        }
                    }
                    let mut last = last_block.lock().await;
                    let current_bn = block_number.as_u64();
                    if last.is_none() || current_bn > last.unwrap() {
                        *last = Some(current_bn);
                        info!("Updated last processed block to: {}", current_bn);
                    }
                }
                Ok(None) => {
                    warn!(
                        "Block {:?} not found after receiving it from subscription.",
                        block_hash
                    );
                }
                Err(e) => {
                    error!("Error getting block with transactions: {:?}", e);
                }
            }
        }
    }
    warn!("Native transfer block stream ended.");
    Err(anyhow!("Native transfer block stream ended"))
}

async fn subscribe_to_solana_transfers(
    ws_url: &str,
    network: &str,
    watched_addresses: &[Pubkey],
    processed_txs: Arc<Mutex<HashSet<String>>>,
    last_slot: Arc<Mutex<Option<u64>>>,
    redis_client: redis::Client,
) -> anyhow::Result<()> {
    // The solana `PubsubClient` / logs_subscribe API surface has changed across
    // versions. To avoid depending on the websocket pubsub API and the
    // unresolved types, poll the RPC for recent signatures for each watched
    // address and process any new transactions.
    let rpc_url = ws_url.replace("ws:", "http:").replace("wss:", "https:");
    let rpc_client = Arc::new(RpcClient::new(rpc_url));

    info!("Polling Solana RPC for transfers (no websocket pubsub used)");

    for address in watched_addresses {
        let pubkey = *address;
        let network = network.to_string();
        let rpc_client = rpc_client.clone();
        let processed_txs = Arc::clone(&processed_txs);
        let last_slot = Arc::clone(&last_slot);
        let redis_client = redis_client.clone();

        tokio::spawn(async move {
            info!("Starting poll loop for {}", pubkey);
            loop {
                // Use the synchronous RpcClient method inside a blocking task
                // so we don't block the async runtime's reactor.
                let signatures_res = tokio::task::spawn_blocking({
                    let rpc_client = rpc_client.clone();
                    let pubkey = pubkey.clone();
                    move || rpc_client.get_signatures_for_address(&pubkey)
                })
                .await;

                match signatures_res {
                    Ok(Ok(signatures)) => {
                        for sig_info in signatures.iter() {
                            // ConfirmedSignatureInfo.signature is a String
                            let signature = sig_info.signature.clone();
                            if let Err(e) = process_solana_transaction(
                                &rpc_client,
                                &network,
                                signature,
                                &pubkey,
                                Arc::clone(&processed_txs),
                                Arc::clone(&last_slot),
                                &redis_client,
                            )
                            .await
                            {
                                warn!(
                                    "Failed to process solana tx {}: {:?}",
                                    sig_info.signature, e
                                );
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        warn!("Error fetching signatures for {}: {:?}", pubkey, e);
                    }
                    Err(e) => {
                        warn!(
                            "Task panicked while fetching signatures for {}: {:?}",
                            pubkey, e
                        );
                    }
                }
                sleep(Duration::from_secs(5)).await;
            }
        });
    }
    Ok(())
}

async fn process_solana_transaction(
    rpc_client: &RpcClient,
    network: &str,
    signature: String,
    watched_address: &Pubkey,
    processed_txs: Arc<Mutex<HashSet<String>>>,
    last_slot: Arc<Mutex<Option<u64>>>,
    redis_client: &redis::Client,
) -> anyhow::Result<()> {
    let event_id = format!("sol:{}", signature);
    if processed_txs.lock().await.contains(&event_id) {
        info!("Duplicate event skipped: {}", event_id);
        return Ok(());
    }

    let sig = Signature::from_str(&signature)?;
    let tx_with_meta = rpc_client.get_transaction_with_config(
        &sig,
        RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::JsonParsed),
            commitment: Some(CommitmentConfig::confirmed()),
            max_supported_transaction_version: Some(0),
        },
    )?;

    let slot = tx_with_meta.slot;
    let block_time = tx_with_meta.block_time.unwrap_or(0);
    let timestamp = chrono::DateTime::from_timestamp(block_time, 0)
        .unwrap()
        .to_rfc3339();

    // Decode the transaction if possible. Different solana crate versions
    // expose parsed or compiled forms; to be robust across versions we only
    // check whether the watched address appears among the transaction's
    // account keys. This is a simpler, reliable signal that the transaction
    // touched the watched address (covers native and token transfers).
    if let Some(decoded_tx) = tx_with_meta.transaction.transaction.decode() {
        let account_keys = decoded_tx.message.static_account_keys();
        if account_keys.iter().any(|k| k == watched_address) {
            let event = Event {
                event_id: event_id.clone(),
                chain: "solana".into(),
                network: network.to_string(),
                tx_hash: signature.clone(),
                timestamp: timestamp.clone(),
                from: "".into(),
                to: "".into(),
                value: "".into(),
                event_type: "solana_tx".into(),
                slot: Some(slot),
                token: None,
            };
            if let Err(e) = publish_event_to_redis(redis_client, &event).await {
                error!("Failed to publish event to Redis: {:?}", e);
            }
            processed_txs.lock().await.insert(event_id.clone());
        }
    }

    let mut last = last_slot.lock().await;
    let current_slot = tx_with_meta.slot;
    if last.is_none() || current_slot > last.unwrap() {
        *last = Some(current_slot);
        info!("Updated last processed SOL slot to: {}", current_slot);
    }

    Ok(())
}

async fn track_solana_transfers(
    ws_url: &str,
    network: &str,
    watched_addresses_str: &[String],
    processed_txs: Arc<Mutex<HashSet<String>>>,
    last_slot: Arc<Mutex<Option<u64>>>,
    redis_client: redis::Client,
) {
    if watched_addresses_str.is_empty() {
        info!("No Solana addresses to watch.");
        return;
    }
    if !ws_url.starts_with("ws") {
        error!("SOL tracking requires a WebSocket URL (e.g. wss://...)");
        return;
    }
    let watched_addresses: Vec<Pubkey> = watched_addresses_str
        .iter()
        .map(|s| Pubkey::from_str(s).expect("Invalid Solana address"))
        .collect();

    loop {
        match subscribe_to_solana_transfers(
            ws_url,
            network,
            &watched_addresses,
            Arc::clone(&processed_txs),
            Arc::clone(&last_slot),
            redis_client.clone(),
        )
        .await
        {
            Ok(_) => info!("Solana subscription stream ended. This should not happen."),
            Err(e) => error!("Solana subscription failed: {:?}. Reconnecting...", e),
        }
        sleep(Duration::from_secs(5)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethers::types::{Address, Bytes, Log, H256, U256, U64};
    use std::str::FromStr;

    #[test]
    fn test_eth_event_id_generation() {
        let tx_hash =
            H256::from_str("0x123456789012345678901234567890123456789012345678901234567890abcd")
                .unwrap();
        let expected_id = "eth:0x123456789012345678901234567890123456789012345678901234567890abcd";
        let event_id = format!("eth:{:?}", tx_hash);
        assert_eq!(event_id, expected_id);
    }

    #[test]
    fn test_sol_event_id_generation() {
        let signature = "5wLkiRHwfgxj8PvAkcsHXEbGYAKQWy6Phu6JX49tBwwBKpPVpRHKPUNFqUbvFPmpXSxmRqGNgHErkBDu2XCfBJVb";
        let expected_id = "sol:5wLkiRHwfgxj8PvAkcsHXEbGYAKQWy6Phu6JX49tBwwBKpPVpRHKPUNFqUbvFPmpXSxmRqGNgHErkBDu2XCfBJVb";
        let event_id = format!("sol:{}", signature);
        assert_eq!(event_id, expected_id);
    }

    #[test]
    fn test_parse_erc20_log() {
        // construct a fake ERC-20 Transfer log: topics[1] = from, topics[2] = to
        let from_addr = Address::from_str("0x0000000000000000000000000000000000000001").unwrap();
        let to_addr = Address::from_str("0x0000000000000000000000000000000000000002").unwrap();

        // topics are H256 where last 20 bytes are the address
        let mut t1 = [0u8; 32];
        t1[12..].copy_from_slice(from_addr.as_bytes());
        let mut t2 = [0u8; 32];
        t2[12..].copy_from_slice(to_addr.as_bytes());

        let topics = vec![H256::zero(), H256::from(t1), H256::from(t2)];

        // data: 32 byte big-endian amount (e.g. 42)
        let mut amount_bytes = vec![0u8; 32];
        amount_bytes[31] = 42u8;

        let log = Log {
            address: Address::zero(),
            topics,
            data: Bytes::from(amount_bytes.clone()),
            block_number: Some(U64::from(123u64)),
            transaction_hash: Some(H256::from_slice(&[1u8; 32])),
            block_hash: None,
            transaction_index: None,
            log_index: None,
            transaction_log_index: None,
            log_type: None,
            removed: Some(false),
        };

        // parse like in track_erc20_transfers
        let from = Address::from(log.topics[1]);
        let to = Address::from(log.topics[2]);
        assert_eq!(from, from_addr);
        assert_eq!(to, to_addr);

        let value = U256::from_big_endian(&log.data.0);
        assert_eq!(value.as_u64(), 42);
    }

    #[test]
    fn test_processed_txs_deduplication_logic() {
        let mut set = std::collections::HashSet::new();
        let id = "eth:0xdeadbeef".to_string();
        assert!(!set.contains(&id));
        set.insert(id.clone());
        assert!(set.contains(&id));
    }
}
