use super::{
    account::RelayerAccount,
    config::{RelayerConfig, SchedulerType},
    metrics, SkipReason,
};
use crate::provider::NonceManager;
use alloy::primitives::{Address, U256};
use rand::Rng;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tracing::{info, instrument, span, trace, warn, Level};

/// Main relayer struct that manages multiple accounts
pub struct Relayer {
    accounts: Vec<Arc<RelayerAccount>>,
    scheduler_type: SchedulerType,
    pending_block_threshold: u64,
    round_robin_index: AtomicUsize,
    rpc_url: String,
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

        // Initialize accounts
        let mut accounts = Vec::new();
        for (idx, account_config) in config.accounts.iter().enumerate() {
            info!("Initializing relayer account {}", idx);

            let min_gas_balance = U256::from_str_radix(&account_config.min_gas_wei, 10)?;
            let account = Arc::new(
                RelayerAccount::new(&account_config.private_key, &rpc_url, min_gas_balance).await?,
            );

            info!(
                "Initialized account {} with address {}",
                idx, account.address
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
        })
    }

    /// Get the next available account and its nonce
    #[instrument(skip(self))]
    pub async fn next_available(
        &self,
    ) -> Result<(Arc<NonceManager>, u64), Box<dyn std::error::Error + Send + Sync>> {
        let mut attempts = 0;
        let max_attempts = self.accounts.len() * 2; // Try each account at most twice

        while attempts < max_attempts {
            attempts += 1;

            // Select next account based on scheduler
            let account = match self.scheduler_type {
                SchedulerType::RoundRobin => self.select_round_robin().await,
                SchedulerType::Random => self.select_random().await,
            };

            // Check if account is available
            match account.is_available(self.pending_block_threshold).await {
                Ok(true) => {
                    // Get next nonce
                    let nonce = account.nonce_manager.get_next_nonce().await;

                    // Mark transaction as sent
                    account.mark_transaction_sent().await;

                    // Emit tracing span for selection
                    span!(
                        Level::INFO,
                        "relayer.select",
                        address = %account.address,
                        nonce = %nonce
                    )
                    .in_scope(|| {
                        trace!("Selected account {} with nonce {}", account.address, nonce);
                    });

                    // Record selection in Prometheus metrics
                    metrics::record_selection(&account.address.to_string());

                    return Ok((account.nonce_manager.clone(), nonce));
                }
                Ok(false) => {
                    // Account not available, record skip
                    let reason = self.determine_skip_reason(&account).await?;

                    // Emit tracing span for skip
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

                    // Record skip in Prometheus metrics
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

        Err("No available relayer accounts after maximum attempts".into())
    }

    /// Mark that a nonce was used (transaction confirmed or failed)
    pub async fn invalidate_nonce(&self, address: Address, success: bool) {
        // Find the account
        for account in &self.accounts {
            if account.address == address {
                if success {
                    account.mark_transaction_confirmed().await;
                } else {
                    account.mark_transaction_failed().await;
                }
                return;
            }
        }

        warn!(
            "Attempted to invalidate nonce for unknown address: {}",
            address
        );
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
        // Check balance
        use alloy::providers::Provider;
        let provider = alloy::providers::ProviderBuilder::new()
            .wallet(account.wallet.clone())
            .on_http(self.rpc_url.parse()?);

        let balance = provider.get_balance(account.address).await?;
        if balance < account.min_gas_balance {
            return Ok(SkipReason::InsufficientGas);
        }

        // TODO: Check for pending transactions more accurately
        Ok(SkipReason::PendingTransaction)
    }

    /// Get addresses of all managed accounts
    pub fn get_addresses(&self) -> Vec<Address> {
        self.accounts.iter().map(|a| a.address).collect()
    }
}
