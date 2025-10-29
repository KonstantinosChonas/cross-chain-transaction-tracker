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
use ethers::providers::{Http, Middleware, Provider, Ws};
use solana_client::{rpc_client::RpcClient, rpc_config::RpcTransactionConfig};

use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature};
use solana_transaction_status::UiTransactionEncoding;

use tracing::{error, info, warn};
use tracing_subscriber::{fmt, EnvFilter};
mod config;
mod retry;
mod solana_parser;

// Include the golden test module
mod tests;

async fn publish_event_to_redis(redis_client: &redis::Client, event: &Event) -> anyhow::Result<()> {
    use retry::retry_with_backoff;
    let payload = serde_json::to_string(event)?;
    // Retry publish with exponential backoff to survive short redis outages
    let attempts = 8usize;
    let base = Duration::from_millis(500);
    let factor = 2.0;
    let event_id = event.event_id.clone();
    let res: anyhow::Result<()> = retry_with_backoff(attempts, base, factor, || {
        let client = redis_client.clone();
        let payload = payload.clone();
        async move {
            match client.get_multiplexed_async_connection().await {
                Ok(mut con) => match con.publish::<_, _, ()>("cross_chain_events", payload).await {
                    Ok(_) => Ok(()),
                    Err(e) => Err(anyhow!(e)),
                },
                Err(e) => Err(anyhow!(e)),
            }
        }
    })
    .await;

    match res {
        Ok(_) => {
            info!("Published event to Redis: {}", event_id);
            Ok(())
        }
        Err(e) => {
            error!(
                "Failed to publish event {} to Redis after retries: {:?}",
                event_id, e
            );
            Err(e)
        }
    }
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
            // Support both WebSocket (for production) and HTTP (for Anvil testing)
            let use_websocket = cfg.eth_rpc_url.starts_with("ws");

            if use_websocket {
                loop {
                    info!(
                        "Connecting to ETH WebSocket provider at {}",
                        cfg.eth_rpc_url
                    );
                    let ws = match Ws::connect(cfg.eth_rpc_url.clone()).await {
                        Ok(ws) => ws,
                        Err(e) => {
                            error!("Failed to connect ETH WebSocket: {:?}. Retrying in 10s.", e);
                            sleep(Duration::from_secs(10)).await;
                            continue;
                        }
                    };
                    let provider = Arc::new(Provider::new(ws));
                    info!("Successfully connected to ETH WebSocket provider.");

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
                    warn!("An ETH WebSocket tracker task has finished. Restarting trackers after 5s delay.");
                    sleep(Duration::from_secs(5)).await;
                }
            } else {
                // HTTP polling mode for Anvil testing
                info!("Using HTTP polling mode for ETH at {}", cfg.eth_rpc_url);
                poll_eth_blocks(
                    cfg.eth_rpc_url.clone(),
                    cfg.watched_addresses_eth.clone(),
                    cfg.eth_network.clone(),
                    Arc::clone(&processed_txs),
                    Arc::clone(&last_eth_block),
                    redis_client.clone(),
                )
                .await;
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

async fn poll_eth_blocks(
    rpc_url: String,
    watched_addresses_str: Vec<String>,
    network: String,
    processed_txs: Arc<Mutex<HashSet<String>>>,
    last_block: Arc<Mutex<Option<u64>>>,
    redis_client: redis::Client,
) {
    use ethers::providers::Http;

    info!("Starting ETH HTTP polling mode");
    let watched_addresses: Vec<Address> = watched_addresses_str
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();

    let provider = match Provider::<Http>::try_from(rpc_url.clone()) {
        Ok(p) => Arc::new(p),
        Err(e) => {
            error!("Failed to create HTTP provider: {:?}", e);
            return;
        }
    };

    loop {
        match provider.get_block_number().await {
            Ok(current_block) => {
                let current = current_block.as_u64();
                let start = {
                    let mut last = last_block.lock().await;
                    match *last {
                        Some(prev) => {
                            if current < prev {
                                // Chain likely restarted (e.g., Anvil reset). Reset window with a small lookback
                                // to ensure we pick up immediate post-restart transactions.
                                let lookback = 10u64;
                                let new_start = current.saturating_sub(lookback);
                                *last = Some(new_start);
                                info!(
                                    "ETH poller detected block regression (prev={}, current={}); resetting start to {}",
                                    prev, current, new_start
                                );
                                new_start
                            } else {
                                // No regression if current == prev; just continue next loop
                                prev
                            }
                        }
                        None => {
                            // Initial state: start from block 0 if chain has any blocks
                            if current > 0 {
                                0
                            } else {
                                current
                            }
                        }
                    }
                };

                // Process blocks even when current == start (to catch block 1 on fresh chains)
                if current >= start {
                    let range_start = if current == start { start } else { start + 1 };
                    if range_start <= current {
                        info!("Polling blocks {} to {}", range_start, current);
                        for block_num in range_start..=current {
                            if let Err(e) = process_eth_block(
                                &provider,
                                block_num,
                                &watched_addresses,
                                &network,
                                &processed_txs,
                                &redis_client,
                            )
                            .await
                            {
                                warn!("Error processing block {}: {:?}", block_num, e);
                            }
                        }
                    }
                    let mut last = last_block.lock().await;
                    *last = Some(current);
                }
            }
            Err(e) => {
                error!("Failed to get block number: {:?}", e);
            }
        }
        sleep(Duration::from_secs(2)).await;
    }
}

async fn process_eth_block(
    provider: &Provider<Http>,
    block_num: u64,
    watched_addresses: &[Address],
    network: &str,
    processed_txs: &Arc<Mutex<HashSet<String>>>,
    redis_client: &redis::Client,
) -> anyhow::Result<()> {
    use ethers::types::BlockNumber;

    let block = match provider
        .get_block_with_txs(BlockNumber::Number(block_num.into()))
        .await?
    {
        Some(b) => b,
        None => return Ok(()),
    };

    for tx in block.transactions {
        // Check native transfers
        // If watched_addresses is empty, track ALL transactions (useful for testing)
        let track_all = watched_addresses.is_empty();
        let from_watched = track_all || watched_addresses.contains(&tx.from);
        let to_watched = track_all
            || tx
                .to
                .map(|to| watched_addresses.contains(&to))
                .unwrap_or(false);

        if from_watched || to_watched {
            let event_id = format!("eth:{:?}", tx.hash);
            if processed_txs.lock().await.insert(event_id.clone()) {
                let event = Event {
                    event_id: event_id.clone(),
                    chain: "ethereum".into(),
                    network: network.to_string(),
                    tx_hash: format!("{:?}", tx.hash),
                    timestamp: block.timestamp.to_string(),
                    from: format!("{:?}", tx.from),
                    to: format!("{:?}", tx.to.unwrap_or_default()),
                    value: tx.value.to_string(),
                    event_type: "transfer".into(),
                    slot: None,
                    token: None,
                };
                publish_event_to_redis(redis_client, &event).await?;
            }
        }

        // Check for ERC20 Transfer logs in transaction receipt
        // Always check receipts (either for specific addresses or all if list is empty)
        if let Ok(Some(receipt)) = provider.get_transaction_receipt(tx.hash).await {
            for log in receipt.logs {
                if log.topics.len() == 3
                    && log.topics[0]
                        == ethers::core::utils::keccak256("Transfer(address,address,uint256)")
                            .into()
                {
                    let from = Address::from(log.topics[1]);
                    let to = Address::from(log.topics[2]);

                    // Track all ERC20 transfers if watched_addresses is empty
                    let track_all = watched_addresses.is_empty();
                    if track_all
                        || watched_addresses.contains(&from)
                        || watched_addresses.contains(&to)
                    {
                        let event_id =
                            format!("eth:{:?}:log{}", tx.hash, log.log_index.unwrap_or_default());
                        if processed_txs.lock().await.insert(event_id.clone()) {
                            let event = Event {
                                event_id: event_id.clone(),
                                chain: "ethereum".into(),
                                network: network.to_string(),
                                tx_hash: format!("{:?}", tx.hash),
                                timestamp: block.timestamp.to_string(),
                                from: format!("{:?}", from),
                                to: format!("{:?}", to),
                                value: U256::from_big_endian(&log.data.0).to_string(),
                                event_type: "erc20_transfer".into(),
                                slot: None,
                                token: Some(Token {
                                    address: format!("{:?}", log.address),
                                    symbol: "".into(),
                                    decimals: 18,
                                }),
                            };
                            publish_event_to_redis(redis_client, &event).await?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
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

    // Support both WebSocket and HTTP URLs
    let use_websocket = ws_url.starts_with("ws");

    if !use_websocket {
        info!("Using HTTP polling mode for Solana at {}", ws_url);
        // For HTTP mode, convert URL and use polling
        let rpc_url = ws_url.to_string();
        poll_solana_transfers(
            &rpc_url,
            network,
            watched_addresses_str,
            processed_txs,
            last_slot,
            redis_client,
        )
        .await;
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

async fn poll_solana_transfers(
    rpc_url: &str,
    network: &str,
    watched_addresses_str: &[String],
    processed_txs: Arc<Mutex<HashSet<String>>>,
    last_slot: Arc<Mutex<Option<u64>>>,
    redis_client: redis::Client,
) {
    info!("Starting Solana HTTP polling mode");
    let rpc_client = Arc::new(RpcClient::new(rpc_url.to_string()));
    let watched_addresses: Vec<Pubkey> = watched_addresses_str
        .iter()
        .filter_map(|s| Pubkey::from_str(s).ok())
        .collect();

    for address in watched_addresses {
        let pubkey = address;
        let network = network.to_string();
        let rpc_client = rpc_client.clone();
        let processed_txs = Arc::clone(&processed_txs);
        let last_slot = Arc::clone(&last_slot);
        let redis_client = redis_client.clone();

        tokio::spawn(async move {
            info!("Starting poll loop for Solana address {}", pubkey);
            loop {
                let signatures_res = tokio::task::spawn_blocking({
                    let rpc_client = rpc_client.clone();
                    let pubkey = pubkey;
                    move || rpc_client.get_signatures_for_address(&pubkey)
                })
                .await;

                match signatures_res {
                    Ok(Ok(signatures)) => {
                        for sig_info in signatures.iter() {
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

    // Keep the main task alive
    loop {
        sleep(Duration::from_secs(60)).await;
    }
}
