use alloy::primitives::U256;
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RelayerConfig {
    pub accounts: Vec<AccountConfig>,
    pub scheduler: SchedulerType,
    pub pending_block_threshold: u64,
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

        Ok(Self {
            accounts,
            scheduler,
            pending_block_threshold,
        })
    }

    /// Create a sample .env.example file content
    pub fn example_env() -> String {
        r#"# Relayer Configuration

# Comma-separated list of private keys (without 0x prefix is also ok)
RELAYER_PRIVATE_KEYS=0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80,0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d,0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a

# Minimum gas balance required for each account (in Wei)
# Default: 0.005 ETH = 5000000000000000 Wei
RELAYER_MIN_GAS_WEI=5000000000000000

# Scheduler type: round_robin or random
# Default: round_robin
RELAYER_SCHEDULER=round_robin

# Maximum number of pending transactions before skipping an account
# Default: 3
RELAYER_PENDING_BLOCK_THRESHOLD=3

# Other required environment variables
ORACLE_PRIVATE_KEY=0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
RPC_URL=http://127.0.0.1:8545
DATABASE_URL=postgresql://user:password@localhost/zamaoracle
"#.to_string()
    }
}
