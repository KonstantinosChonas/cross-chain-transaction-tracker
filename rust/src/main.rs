use anyhow::anyhow;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use tokio_stream::StreamExt;

use ethers::prelude::*;
use ethers::providers::{Provider, Ws};
use ethers::types::{Address, Filter};

use solana_client::{
    pubsub_client::PubsubClient,
    rpc_client::RpcClient,
    rpc_config::RpcTransactionConfig,
    rpc_request::{RpcLogsConfig, RpcLogsFilter},
};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature};
use solana_transaction_status::UiTransactionEncoding;

use tracing::{error, info, warn};
use tracing_subscriber::{fmt, EnvFilter};
mod config;

#[derive(Serialize, Debug)]
struct Token {
    address: String,
    symbol: String,
    decimals: u8,
}

#[derive(Serialize, Debug, Serialize, Debug)]
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

    if !cfg.eth_rpc_url.starts_with("ws") {
        error!("This implementation requires a WebSocket RPC URL (e.g. wss://...)");
        std::process::exit(1);
    }
    let provider_url = cfg.eth_rpc_url.clone();

    let watched_addresses: Vec<Address> = cfg
        .watched_addresses
        .iter()
        .map(|s| s.parse().unwrap())
        .collect();

    let processed_txs: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let last_block: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));

    loop {
        info!("Connecting to provider at {}", provider_url);
        let ws = match Ws::connect(provider_url.clone()).await {
            Ok(ws) => ws,
            Err(e) => {
                error!("Failed to connect WebSocket: {:?}. Retrying in 10s.", e);
                sleep(Duration::from_secs(10)).await;
                continue;
            }
        };
        let provider = Arc::new(Provider::new(ws));
        info!("Successfully connected to provider.");

        let native_tracker = track_native_transfers(
            Arc::clone(&provider),
            watched_addresses.clone(),
            cfg.network.clone(),
            Arc::clone(&processed_txs),
            Arc::clone(&last_block),
        );

        if watched_addresses.is_empty() {
            warn!("No watched addresses for ERC-20 transfers. Tracking native transfers only.");
            if let Err(e) = native_tracker.await {
                warn!("Native transfer tracker failed: {}.", e);
            }
        } else {
            let erc20_tracker = track_erc20_transfers(
                Arc::clone(&provider),
                watched_addresses.clone(),
                cfg.network.clone(),
                Arc::clone(&processed_txs),
                Arc::clone(&last_block),
            );

            tokio::select! {
                res = erc20_tracker => {
                    if let Err(e) = res {
                        warn!("ERC-20 tracker failed: {}.", e);
                    }
                },
                res = native_tracker => {
                    if let Err(e) = res {
                        warn!("Native transfer tracker failed: {}.", e);
                    }
                },
            }
        }

        warn!("A tracker task has finished. Restarting trackers after 5s delay.");
        sleep(Duration::from_secs(5)).await;
    }
}

async fn track_erc20_transfers(
    provider: Arc<Provider<Ws>>,
    watched_addresses: Vec<Address>,
    network: String,
    processed_txs: Arc<Mutex<HashSet<String>>>,
    last_block: Arc<Mutex<Option<u64>>>,
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
                    value: U256::from_big_endian(&log.data).to_string(),
                    event_type: "erc20_transfer".into(),
                    slot: None,
                    token: None,
                };

                if let Ok(event_json) = serde_json::to_string(&event) {
                    println!("{}", event_json);
                    processed_txs.lock().await.insert(event_id);

                    if let Some(bn) = block_number {
                        let mut last = last_block.lock().await;
                        let current_bn = bn.as_u64();
                        if last.is_none() || current_bn > last.unwrap() {
                            *last = Some(current_bn);
                            info!("Updated last processed block to: {}", current_bn);
                        }
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
                            if let Ok(event_json) = serde_json::to_string(&event) {
                                println!("{}", event_json);
                                processed_txs.lock().await.insert(event_id);
                            }
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
) -> anyhow::Result<()> {
    let pubsub_client = PubsubClient::new(ws_url).await?;
    let rpc_url = ws_url.replace("ws:", "http:").replace("wss:", "https:");
    let rpc_client = RpcClient::new(rpc_url);

    info!("Subscribing to Solana accounts for native and token transfers");

    for address in watched_addresses {
        let pubkey = *address;
        let network = network.to_string();
        let rpc_client = rpc_client.clone();
        let (mut subscription, _) = pubsub_client
            .logs_subscribe(
                RpcLogsFilter::Mentions(vec![pubkey.to_string()]),
                RpcLogsConfig {
                    commitment: Some(CommitmentConfig::confirmed()),
                },
            )
            .await?;

        tokio::spawn(async move {
            info!("Listening for logs mentioning {}", pubkey);
            while let Some(log_info) = subscription.next().await {
                let signature = log_info.value.signature;
                if let Err(e) =
                    process_solana_transaction(&rpc_client, &network, signature, &pubkey).await
                {
                    warn!(
                        "Failed to process solana tx {}: {:?}",
                        log_info.value.signature, e
                    );
                }
            }
            info!("Subscription ended for {}", pubkey);
        });
    }
    Ok(())
}

async fn process_solana_transaction(
    rpc_client: &RpcClient,
    network: &str,
    signature: String,
    watched_address: &Pubkey,
) -> anyhow::Result<()> {
    let sig = Signature::from_str(&signature)?;
    let tx = rpc_client
        .get_transaction_with_config(
            &sig,
            RpcTransactionConfig {
                encoding: Some(UiTransactionEncoding::JsonParsed),
                commitment: Some(CommitmentConfig::confirmed()),
                max_supported_transaction_version: Some(0),
            },
        )
        .await?;

    if let Some(tx_with_meta) = tx.transaction {
        let slot = tx_with_meta.slot;
        let block_time = tx_with_meta.block_time.unwrap_or(0);
        let timestamp = chrono::DateTime::from_timestamp(block_time, 0)
            .unwrap()
            .to_rfc3339();

        if let Some(transaction) = tx_with_meta.transaction.decode() {
            // --- Check for Native SOL transfers ---
            for instruction in &transaction.message.instructions {
                if let solana_transaction_status::option_serializer::OptionSerializer::Some(
                    solana_transaction_status::UiParsedInstruction::Parsed(
                        solana_transaction_status::UiParsedInstructionEnum::System(
                            parsed_instruction,
                        ),
                    ),
                ) = &instruction.parsed
                {
                    if parsed_instruction.instruction_type == "transfer" {
                        if let Ok(info) = serde_json::from_value::<
                            solana_transaction_status::parse_system::Transfer,
                        >(parsed_instruction.info.clone())
                        {
                            if &Pubkey::from_str(&info.source)? == watched_address
                                || &Pubkey::from_str(&info.destination)? == watched_address
                            {
                                let event = Event {
                                    event_id: format!("sol:{}", signature),
                                    chain: "solana".into(),
                                    network: network.to_string(),
                                    tx_hash: signature.clone(),
                                    timestamp: timestamp.clone(),
                                    from: info.source,
                                    to: info.destination,
                                    value: info.lamports.to_string(),
                                    event_type: "transfer".into(),
                                    slot: Some(slot),
                                    token: None,
                                };
                                if let Ok(event_json) = serde_json::to_string(&event) {
                                    println!("{}", event_json);
                                }
                            }
                        }
                    }
                }
            }
        }

        // --- Check for SPL Token transfers ---
        if let Some(meta) = tx_with_meta.meta {
            if let Some(inner_instructions) = meta.inner_instructions {
                for inner_instruction in inner_instructions {
                    for instruction in inner_instruction.instructions {
                        if let solana_transaction_status::option_serializer::OptionSerializer::Some(
                            solana_transaction_status::UiParsedInstruction::Parsed(
                                solana_transaction_status::UiParsedInstructionEnum::SplToken(
                                    parsed_instruction,
                                ),
                            ),
                        ) = instruction.parsed
                        {
                            if parsed_instruction.instruction_type == "transfer" {
                                if let Ok(info) = serde_json::from_value::<
                                    solana_transaction_status::parse_token::Transfer,
                                >(
                                    parsed_instruction.info
                                ) {
                                    if &Pubkey::from_str(&info.source)? == watched_address
                                        || &Pubkey::from_str(&info.destination)? == watched_address
                                    {
                                        let event = Event {
                                            event_id: format!("sol:{}", signature),
                                            chain: "solana".into(),
                                            network: network.to_string(),
                                            tx_hash: signature.clone(),
                                            timestamp: timestamp.clone(),
                                            from: info.source.clone(),
                                            to: info.destination.clone(),
                                            value: info.amount.clone(),
                                            event_type: "spl_transfer".into(),
                                            slot: Some(slot),
                                            token: Some(Token {
                                                address: "".to_string(), // Mint address requires another RPC call
                                                symbol: "".to_string(),
                                                decimals: info.decimals.unwrap_or(0),
                                            }),
                                        };
                                        if let Ok(event_json) = serde_json::to_string(&event) {
                                            println!("{}", event_json);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
