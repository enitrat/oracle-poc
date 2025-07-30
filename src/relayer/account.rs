use crate::oracle::Call;
use alloy::primitives::Bytes;
use alloy::{
    network::{Ethereum, EthereumWallet},
    primitives::{Address, U256},
    providers::{Provider, ProviderBuilder},
    rpc::types::TransactionRequest,
    signers::local::PrivateKeySigner,
    sol,
    sol_types::SolCall,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

// Define the BEBE interface
sol! {
    interface IBEBE {
        function execute(bytes32 mode, bytes calldata executionData) external payable;
    }
}

/// Represents a single relayer account with its own provider
pub struct RelayerAccount {
    pub address: Address,
    pub min_gas_balance: U256,
    pub bebe_address: Option<Address>,

    // Alloy provider with automatic nonce management
    provider: Arc<dyn Provider<Ethereum> + Send + Sync>,

    // Track account state
    state: Arc<Mutex<AccountState>>,
}

#[derive(Debug, Clone)]
struct AccountState {
    last_balance_check: Option<Instant>,
    cached_balance: U256,
    pending_tx_count: usize,
    last_failure: Option<Instant>,
    total_transactions: u64,
    total_failures: u64,
}

impl RelayerAccount {
    pub async fn new(
        private_key: &str,
        rpc_url: &str,
        min_gas_balance: U256,
        bebe_address: Option<Address>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Parse private key and create wallet
        let signer: PrivateKeySigner = private_key
            .parse()
            .map_err(|e| format!("Failed to parse private key: {e}"))?;
        let address = signer.address();
        let wallet = EthereumWallet::from(signer);

        // Create provider with automatic nonce management
        let provider: Arc<dyn Provider<Ethereum> + Send + Sync> = Arc::new(
            ProviderBuilder::new()
                .with_cached_nonce_management()
                .wallet(wallet)
                .connect_http(rpc_url.parse()?),
        );

        // Initialize state
        let state = Arc::new(Mutex::new(AccountState {
            last_balance_check: None,
            cached_balance: U256::ZERO,
            pending_tx_count: 0,
            last_failure: None,
            total_transactions: 0,
            total_failures: 0,
        }));

        let account = Self {
            address,
            min_gas_balance,
            bebe_address,
            provider: provider.clone(),
            state,
        };

        // Check initial balance
        account.update_balance().await?;

        Ok(account)
    }

    /// Check if this account is available for use
    pub async fn is_available(
        &self,
        pending_block_threshold: u64,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let mut state = self.state.lock().await;

        // Check if recently failed (cooldown period of 30 seconds)
        if let Some(last_failure) = state.last_failure {
            if last_failure.elapsed() < Duration::from_secs(30) {
                debug!("Account {} is in failure cooldown", self.address);
                return Ok(false);
            } else {
                // Clear the failure flag after cooldown
                state.last_failure = None;
            }
        }

        // Check pending transactions
        if state.pending_tx_count >= pending_block_threshold as usize {
            debug!(
                "Account {} has too many pending transactions: {}",
                self.address, state.pending_tx_count
            );
            return Ok(false);
        }

        // Check balance (with caching to avoid too many RPC calls)
        let should_check_balance = state
            .last_balance_check
            .map(|t| t.elapsed() > Duration::from_secs(60))
            .unwrap_or(true);

        if should_check_balance {
            drop(state); // Release lock before RPC call
            self.update_balance().await?;
            state = self.state.lock().await;
        }

        if state.cached_balance < self.min_gas_balance {
            warn!(
                "Account {} has insufficient balance: {} < {}",
                self.address, state.cached_balance, self.min_gas_balance
            );
            return Ok(false);
        }

        Ok(true)
    }

    /// Update the cached balance
    async fn update_balance(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let balance = self.provider.get_balance(self.address).await?;

        let mut state = self.state.lock().await;
        state.cached_balance = balance;
        state.last_balance_check = Some(Instant::now());

        info!(
            "Updated balance for {}: {} ETH",
            self.address,
            format_ether(balance)
        );

        Ok(())
    }

    /// Mark that a transaction is being sent
    pub async fn mark_transaction_sent(&self) {
        let mut state = self.state.lock().await;
        state.pending_tx_count += 1;
        state.total_transactions += 1;
        debug!(
            "Account {} now has {} pending transactions",
            self.address, state.pending_tx_count
        );
    }

    /// Mark that a transaction was confirmed
    pub async fn mark_transaction_confirmed(&self) {
        let mut state = self.state.lock().await;
        if state.pending_tx_count > 0 {
            state.pending_tx_count -= 1;
        }
        debug!(
            "Account {} now has {} pending transactions",
            self.address, state.pending_tx_count
        );
    }

    /// Mark that a transaction failed
    pub async fn mark_transaction_failed(&self) {
        let mut state = self.state.lock().await;
        if state.pending_tx_count > 0 {
            state.pending_tx_count -= 1;
        }
        state.last_failure = Some(Instant::now());
        state.total_failures += 1;
        warn!(
            "Account {} marked as failed, entering cooldown",
            self.address
        );
    }

    /// Get account metrics
    pub async fn get_metrics(&self) -> (u64, u64) {
        let state = self.state.lock().await;
        (state.total_transactions, state.total_failures)
    }

    pub async fn send_call(
        &self,
        to: Address,
        data: Bytes,
    ) -> Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        let tx = TransactionRequest::default().to(to).input(data.into());

        let call_result = self.provider.call(tx).await?;

        Ok(call_result)
    }

    /// Send a batch of calls through BEBE (ERC7821)
    pub async fn send_batch(
        &self,
        calls: &[Call],
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // Encode the batch for ERC7821
        let batch_data = crate::oracle::encode_batch_for_erc7821(calls);

        // Build transaction to send to the account's own address (EIP-7702 delegation)
        let tx = TransactionRequest::default()
            .to(self.address)
            .input(batch_data.abi_encode().into());

        // Mark transaction as being sent
        self.mark_transaction_sent().await;

        // Send transaction - Alloy handles nonce automatically
        let pending_tx = match self.provider.send_transaction(tx).await {
            Ok(tx) => tx,
            Err(e) => {
                self.mark_transaction_failed().await;
                error!("Failed to send batch transaction: {:?}", e);
                return Err(format!("Failed to send transaction: {e}").into());
            }
        };

        let tx_hash = pending_tx.tx_hash().to_string();

        info!(
            "Sent batch transaction {} with {} calls from account {}",
            tx_hash,
            calls.len(),
            self.address
        );

        // Wait for confirmation
        match pending_tx.get_receipt().await {
            Ok(receipt) => {
                if receipt.status() {
                    self.mark_transaction_confirmed().await;
                    Ok(tx_hash)
                } else {
                    self.mark_transaction_failed().await;
                    Err("Batch transaction failed".into())
                }
            }
            Err(e) => {
                self.mark_transaction_failed().await;
                error!("Failed to get receipt: {:?}", e);
                Err(format!("Failed to get receipt: {e}").into())
            }
        }
    }
}

/// Format Wei as ETH for logging
fn format_ether(wei: U256) -> String {
    let eth = wei / U256::from(10).pow(U256::from(18));
    let remainder = wei % U256::from(10).pow(U256::from(18));
    let decimal = remainder / U256::from(10).pow(U256::from(14)); // 4 decimal places
    format!("{eth}.{decimal:04}")
}
