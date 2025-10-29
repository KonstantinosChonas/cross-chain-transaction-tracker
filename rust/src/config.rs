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
    #[allow(dead_code)]
    pub poll_interval_secs: u64,
    #[allow(dead_code)]
    pub log_level: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        // Prefer existing environment variables set by the process. Only
        // load a .env file if a required variable is missing. This avoids
        // dotenv overriding values tests set via std::env::set_var.
        fn get_required(name: &str) -> Result<String> {
            if let Ok(v) = std::env::var(name) {
                return Ok(v);
            }
            // try loading from .env once
            dotenv().ok();
            std::env::var(name).context(format!("{} must be set", name))
        }

        let eth_rpc_url = get_required("ETH_RPC_URL")?;
        let sol_rpc_url = get_required("SOL_RPC_URL")?;
        let redis_url = get_required("REDIS_URL")?;

        // For optional comma-separated lists, prefer existing env then try .env
        let watched_addresses_eth = match std::env::var("WATCHED_ADDRESSES_ETH") {
            Ok(s) => {
                if s.is_empty() {
                    Vec::new()
                } else {
                    s.split(',').map(|s| s.trim().to_string()).collect()
                }
            }
            Err(_) => {
                dotenv().ok();
                std::env::var("WATCHED_ADDRESSES_ETH")
                    .map(|s| {
                        if s.is_empty() {
                            Vec::new()
                        } else {
                            s.split(',').map(|s| s.trim().to_string()).collect()
                        }
                    })
                    .unwrap_or_default()
            }
        };

        let watched_addresses_sol = match std::env::var("WATCHED_ADDRESSES_SOL") {
            Ok(s) => {
                if s.is_empty() {
                    Vec::new()
                } else {
                    s.split(',').map(|s| s.trim().to_string()).collect()
                }
            }
            Err(_) => {
                dotenv().ok();
                std::env::var("WATCHED_ADDRESSES_SOL")
                    .map(|s| {
                        if s.is_empty() {
                            Vec::new()
                        } else {
                            s.split(',').map(|s| s.trim().to_string()).collect()
                        }
                    })
                    .unwrap_or_default()
            }
        };

        let eth_network = get_required("ETH_NETWORK")?;
        let sol_network = get_required("SOL_NETWORK")?;

        // POLL_INTERVAL_SECS: if present use it (and parse), otherwise try .env
        let poll_interval_secs = match std::env::var("POLL_INTERVAL_SECS") {
            Ok(s) => s
                .parse::<u64>()
                .context("POLL_INTERVAL_SECS must be a number")?,
            Err(_) => {
                dotenv().ok();
                match std::env::var("POLL_INTERVAL_SECS") {
                    Ok(s2) => s2
                        .parse::<u64>()
                        .context("POLL_INTERVAL_SECS must be a number")?,
                    Err(_) => 10u64,
                }
            }
        };

        let _log_level = std::env::var("LOG_LEVEL").ok();

        Ok(Config {
            eth_rpc_url,
            sol_rpc_url,
            redis_url,
            watched_addresses_eth,
            watched_addresses_sol,
            eth_network,
            sol_network,
            poll_interval_secs,
            _log_level,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cleanup_env() {
        // Remove all environment variables that might affect the test
        std::env::remove_var("ETH_RPC_URL");
        std::env::remove_var("SOL_RPC_URL");
        std::env::remove_var("REDIS_URL");
        std::env::remove_var("WATCHED_ADDRESSES_ETH");
        std::env::remove_var("WATCHED_ADDRESSES_SOL");
        std::env::remove_var("ETH_NETWORK");
        std::env::remove_var("SOL_NETWORK");
        std::env::remove_var("POLL_INTERVAL_SECS");
        std::env::remove_var("LOG_LEVEL");
    }

    #[test]
    fn test_config_from_env_success_and_overrides() {
        // Ensure we start with a clean environment
        cleanup_env();

        // Set required environment variables
        std::env::set_var("ETH_RPC_URL", "wss://example.eth");
        std::env::set_var("SOL_RPC_URL", "wss://example.sol");
        std::env::set_var("REDIS_URL", "redis://localhost");
        std::env::set_var(
            "WATCHED_ADDRESSES_ETH",
            "0x0000000000000000000000000000000000000001,0x0000000000000000000000000000000000000002",
        );
        std::env::set_var("WATCHED_ADDRESSES_SOL", "Addr1,Addr2");
        std::env::set_var("ETH_NETWORK", "mainnet");
        std::env::set_var("SOL_NETWORK", "mainnet");
        std::env::set_var("POLL_INTERVAL_SECS", "42");

        // Verify that POLL_INTERVAL_SECS is set correctly
        assert_eq!(
            std::env::var("POLL_INTERVAL_SECS").unwrap(),
            "42",
            "POLL_INTERVAL_SECS not set correctly"
        );

        // Load config
        let cfg = Config::from_env().expect("config should load");

        // Verify all values
        assert_eq!(cfg.eth_rpc_url, "wss://example.eth");
        assert_eq!(cfg.sol_rpc_url, "wss://example.sol");
        assert_eq!(cfg.redis_url, "redis://localhost");
        assert_eq!(cfg.watched_addresses_eth.len(), 2);
        assert_eq!(cfg.watched_addresses_sol.len(), 2);
        assert_eq!(cfg.poll_interval_secs, 42);

        // Clean up after test
        cleanup_env();
    }

    #[test]
    fn test_config_from_env_invalid_poll_interval() {
        cleanup_env();
        std::env::set_var("ETH_RPC_URL", "wss://example.eth");
        std::env::set_var("SOL_RPC_URL", "wss://example.sol");
        std::env::set_var("REDIS_URL", "redis://localhost");
        std::env::set_var("ETH_NETWORK", "mainnet");
        std::env::set_var("SOL_NETWORK", "mainnet");
        std::env::set_var("POLL_INTERVAL_SECS", "not-a-number");

        let res = Config::from_env();
        assert!(res.is_err());
    }
}
