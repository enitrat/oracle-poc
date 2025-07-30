use alloy::primitives::U256;
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RelayerConfig {
    pub accounts: Vec<AccountConfig>,
    pub scheduler: SchedulerType,
    pub pending_block_threshold: u64,
    pub bebe_address: Option<String>,
    pub batch_size: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AccountConfig {
    pub private_key: String,
    pub min_gas_wei: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerType {
    RoundRobin,
    Random,
}

impl Default for SchedulerType {
    fn default() -> Self {
        Self::RoundRobin
    }
}

impl RelayerConfig {
    /// Load configuration from environment variables
    /// Expected format:
    /// RELAYER_PRIVATE_KEYS=0xkey1,0xkey2,0xkey3
    /// RELAYER_MIN_GAS_WEI=50000000000000000
    /// RELAYER_SCHEDULER=round_robin
    /// RELAYER_PENDING_BLOCK_THRESHOLD=3
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Parse private keys - RELAYER_PRIVATE_KEYS is required
        let private_keys_str = env::var("RELAYER_PRIVATE_KEYS")
            .map_err(|_| "RELAYER_PRIVATE_KEYS environment variable is not set")?;

        let private_keys: Vec<String> = private_keys_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if private_keys.is_empty() {
            return Err("No private keys found in RELAYER_PRIVATE_KEYS".into());
        }

        // Parse min gas (use same value for all accounts)
        let min_gas_wei =
            env::var("RELAYER_MIN_GAS_WEI").unwrap_or_else(|_| "5000000000000000".to_string()); // 0.005 ETH default

        // Validate that min_gas_wei can be parsed as U256
        let _ = U256::from_str_radix(&min_gas_wei, 10)
            .map_err(|_| "Invalid RELAYER_MIN_GAS_WEI value")?;

        // Create account configs
        let accounts = private_keys
            .into_iter()
            .map(|private_key| AccountConfig {
                private_key,
                min_gas_wei: min_gas_wei.clone(),
            })
            .collect();

        // Parse scheduler type
        let scheduler_str =
            env::var("RELAYER_SCHEDULER").unwrap_or_else(|_| "round_robin".to_string());

        let scheduler = match scheduler_str.to_lowercase().as_str() {
            "round_robin" => SchedulerType::RoundRobin,
            "random" => SchedulerType::Random,
            _ => {
                return Err(format!(
                    "Invalid RELAYER_SCHEDULER value: {scheduler_str}. Must be one of: round_robin, random"
                )
                .into());
            }
        };

        // Parse pending block threshold
        let pending_block_threshold = env::var("RELAYER_PENDING_BLOCK_THRESHOLD")
            .unwrap_or_else(|_| "20".to_string())
            .parse::<u64>()
            .map_err(|_| "Invalid RELAYER_PENDING_BLOCK_THRESHOLD value")?;

        // Parse BEBE address
        let bebe_address = env::var("BEBE_ADDRESS").ok();

        // Parse batch size
        let batch_size = env::var("BATCH_SIZE")
            .unwrap_or_else(|_| "100".to_string())
            .parse::<usize>()
            .map_err(|_| "Invalid BATCH_SIZE value")?;

        Ok(Self {
            accounts,
            scheduler,
            pending_block_threshold,
            bebe_address,
            batch_size,
        })
    }
}
