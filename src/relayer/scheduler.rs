use super::{
    account::RelayerAccount,
    config::{RelayerConfig, SchedulerType},
    metrics, SkipReason,
};
use alloy::primitives::{Address, U256};
use rand::Rng;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, span, trace, warn, Level};

/// Main relayer struct that manages multiple accounts
pub struct Relayer {
    pub accounts: Vec<Arc<RelayerAccount>>,
    scheduler_type: SchedulerType,
    pending_block_threshold: u64,
    round_robin_index: AtomicUsize,
    rpc_url: String,
    pub batch_size: usize,
    // Track accounts currently in use for batch processing
    accounts_in_use: Arc<Mutex<HashSet<Address>>>,
}

impl Relayer {
    /// Create a new relayer from configuration
    pub async fn new(
        config: RelayerConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Initialize metrics
        metrics::init_metrics();
        let rpc_url =
            std::env::var("RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8545".to_string());

        // Parse BEBE address if provided
        let bebe_address = if let Some(bebe_str) = &config.bebe_address {
            Some(
                bebe_str
                    .parse::<Address>()
                    .map_err(|_| "Invalid BEBE_ADDRESS format")?,
            )
        } else {
            None
        };

        // Initialize accounts
        let mut accounts = Vec::new();
        for (idx, account_config) in config.accounts.iter().enumerate() {
            info!("Initializing relayer account {}", idx);

            let min_gas_balance = U256::from_str_radix(&account_config.min_gas_wei, 10)?;
            let account = Arc::new(
                RelayerAccount::new(
                    &account_config.private_key,
                    &rpc_url,
                    min_gas_balance,
                    bebe_address,
                )
                .await?,
            );

            info!(
                "Initialized account {} with address {}{}",
                idx,
                account.address,
                if bebe_address.is_some() {
                    " (BEBE enabled)"
                } else {
                    ""
                }
            );

            accounts.push(account);
        }

        if accounts.is_empty() {
            return Err("No relayer accounts configured".into());
        }

        info!(
            "Relayer initialized with {} accounts using {} scheduler",
            accounts.len(),
            match config.scheduler {
                SchedulerType::RoundRobin => "round-robin",
                SchedulerType::Random => "random",
            }
        );

        Ok(Self {
            accounts,
            scheduler_type: config.scheduler,
            pending_block_threshold: config.pending_block_threshold,
            round_robin_index: AtomicUsize::new(0),
            rpc_url,
            batch_size: config.batch_size,
            accounts_in_use: Arc::new(Mutex::new(HashSet::new())),
        })
    }

    /// Round-robin selection
    async fn select_round_robin(&self) -> Arc<RelayerAccount> {
        let index = self.round_robin_index.fetch_add(1, Ordering::Relaxed) % self.accounts.len();
        self.accounts[index].clone()
    }

    /// Random selection
    async fn select_random(&self) -> Arc<RelayerAccount> {
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..self.accounts.len());
        self.accounts[index].clone()
    }

    /// Determine why an account was skipped
    async fn determine_skip_reason(
        &self,
        account: &RelayerAccount,
    ) -> Result<SkipReason, Box<dyn std::error::Error + Send + Sync>> {
        // The account's is_available method already checks balance
        // If we're here, it's likely due to pending transactions or recent failure
        Ok(SkipReason::PendingTransaction)
    }

    /// Get addresses of all managed accounts
    pub fn get_addresses(&self) -> Vec<Address> {
        self.accounts.iter().map(|a| a.address).collect()
    }

    /// Get next available account for batch sending
    pub async fn next_available_batch(
        &self,
    ) -> Result<Arc<RelayerAccount>, Box<dyn std::error::Error + Send + Sync>> {
        let mut attempts = 0;
        let max_attempts = self.accounts.len() * 3; // More attempts since we check for in-use

        while attempts < max_attempts {
            attempts += 1;

            // Select next account based on scheduler
            let account = match self.scheduler_type {
                SchedulerType::RoundRobin => self.select_round_robin().await,
                SchedulerType::Random => self.select_random().await,
            };

            // Check if account is already in use
            {
                let in_use = self.accounts_in_use.lock().await;
                if in_use.contains(&account.address) {
                    trace!("Account {} is already in use, skipping", account.address);
                    continue;
                }
            }

            // Check if account is available
            match account.is_available(self.pending_block_threshold).await {
                Ok(true) => {
                    // Check if account has BEBE configured
                    if account.bebe_address.is_none() {
                        warn!(
                            "Account {} selected but BEBE not configured",
                            account.address
                        );
                        continue;
                    }

                    // Mark account as in use
                    {
                        let mut in_use = self.accounts_in_use.lock().await;
                        in_use.insert(account.address);
                    }

                    span!(
                        Level::INFO,
                        "relayer.select_batch",
                        address = %account.address
                    )
                    .in_scope(|| {
                        trace!("Selected account {} for batch", account.address);
                    });

                    metrics::record_selection(&account.address.to_string());
                    return Ok(account);
                }
                Ok(false) => {
                    let reason = self.determine_skip_reason(&account).await?;
                    span!(
                        Level::WARN,
                        "relayer.skip",
                        address = %account.address,
                        reason = %reason
                    )
                    .in_scope(|| {
                        warn!(
                            "Skipping account {} (reason: {:?})",
                            account.address, reason
                        );
                    });
                    metrics::record_skip(&account.address.to_string(), &reason.to_string());
                }
                Err(e) => {
                    warn!(
                        "Error checking account {} availability: {}",
                        account.address, e
                    );
                    metrics::record_skip(
                        &account.address.to_string(),
                        &SkipReason::RecentFailure.to_string(),
                    );
                }
            }
        }

        Err("No available relayer accounts with BEBE configured".into())
    }

    /// Release an account after batch processing
    pub async fn release_account(&self, address: Address) {
        let mut in_use = self.accounts_in_use.lock().await;
        in_use.remove(&address);
        trace!("Released account {} from batch processing", address);
    }
}
