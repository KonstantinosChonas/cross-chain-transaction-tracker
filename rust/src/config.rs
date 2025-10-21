use anyhow::{Context, Result};
use dotenvy::dotenv;

#[derive(Debug, Clone)]
pub struct Config {
    pub eth_rpc_url: String,
    pub sol_rpc_url: String,
    pub redis_url: String,
    pub watched_addresses_eth: Vec<String>,
    pub watched_addresses_sol: Vec<String>,
    pub eth_network: String,
    pub sol_network: String,
    pub poll_interval_secs: u64,
    pub log_level: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenv().ok();

        let eth_rpc_url = std::env::var("ETH_RPC_URL").context("ETH_RPC_URL must be set")?;
        let sol_rpc_url = std::env::var("SOL_RPC_URL").context("SOL_RPC_URL must be set")?;
        let redis_url = std::env::var("REDIS_URL").context("REDIS_URL must be set")?;
        let watched_addresses_eth = std::env::var("WATCHED_ADDRESSES_ETH")
            .map(|s| {
                if s.is_empty() {
                    Vec::new()
                } else {
                    s.split(',').map(|s| s.trim().to_string()).collect()
                }
            })
            .unwrap_or_default();

        let watched_addresses_sol = std::env::var("WATCHED_ADDRESSES_SOL")
            .map(|s| {
                if s.is_empty() {
                    Vec::new()
                } else {
                    s.split(',').map(|s| s.trim().to_string()).collect()
                }
            })
            .unwrap_or_default();

        let eth_network = std::env::var("ETH_NETWORK").context("ETH_NETWORK must be set")?;
        let sol_network = std::env::var("SOL_NETWORK").context("SOL_NETWORK must be set")?;

        let poll_interval_secs = std::env::var("POLL_INTERVAL_SECS")
            .unwrap_or_else(|_| "10".to_string())
            .parse::<u64>()
            .context("POLL_INTERVAL_SECS must be a number")?;

        let log_level = std::env::var("LOG_LEVEL").ok();

        Ok(Config {
            eth_rpc_url,
            sol_rpc_url,
            redis_url,
            watched_addresses_eth,
            watched_addresses_sol,
            eth_network,
            sol_network,
            poll_interval_secs,
            log_level,
        })
    }
}
