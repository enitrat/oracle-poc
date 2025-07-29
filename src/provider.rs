use alloy::{
    network::{Ethereum, EthereumWallet},
    primitives::Address,
    providers::{PendingTransactionBuilder, Provider, ProviderBuilder},
    rpc::types::TransactionRequest,
    signers::local::PrivateKeySigner,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

/// Manages nonces sequentially for concurrent transactions
pub struct NonceManager {
    nonce_mutex: Arc<Mutex<u64>>,
    pub account_address: Address,
    provider: Arc<dyn Provider<Ethereum> + Send + Sync>,
}

impl NonceManager {
    pub async fn new(
        rpc_url: &str,
        private_key: &str,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Setup signer
        let signer: PrivateKeySigner = private_key
            .parse()
            .map_err(|e| format!("Failed to parse private key: {e}"))?;
        let account_address = signer.address();
        let wallet = EthereumWallet::from(signer);

        // Create a persistent provider
        let provider: Arc<dyn Provider<Ethereum> + Send + Sync> = Arc::new(
            ProviderBuilder::new()
                .wallet(wallet)
                .on_http(rpc_url.parse()?),
        );

        // Get the current nonce
        let current_nonce = provider.get_transaction_count(account_address).await?;
        info!(
            "Initialized nonce manager with starting nonce: {}",
            current_nonce
        );

        Ok(Self {
            nonce_mutex: Arc::new(Mutex::new(current_nonce)),
            account_address,
            provider,
        })
    }

    /// Get the next nonce atomically (deprecated - use send_transaction_atomic instead)
    pub async fn get_next_nonce(&self) -> u64 {
        let mut nonce = self.nonce_mutex.lock().await;
        let current = *nonce;
        *nonce += 1;
        current
    }

    /// Send a transaction with a specific nonce (deprecated - use send_transaction_atomic instead)
    pub async fn send_transaction_with_nonce(
        &self,
        tx: TransactionRequest,
        nonce: u64,
    ) -> Result<PendingTransactionBuilder<Ethereum>, Box<dyn std::error::Error + Send + Sync>> {
        // Set the nonce on the transaction
        let tx_with_nonce = tx.nonce(nonce);

        // Send the transaction using the persistent provider
        Ok(self.provider.send_transaction(tx_with_nonce).await?)
    }

    /// Atomically get next nonce and send transaction
    pub async fn send_transaction_atomic(
        &self,
        tx: TransactionRequest,
    ) -> Result<(u64, PendingTransactionBuilder<Ethereum>), Box<dyn std::error::Error + Send + Sync>>
    {
        let mut nonce_guard = self.nonce_mutex.lock().await;
        let nonce = *nonce_guard;

        // Set the nonce on the transaction
        let tx_with_nonce = tx.nonce(nonce);

        // Send the transaction while still holding the lock
        let pending_tx = self.provider.send_transaction(tx_with_nonce).await?;

        // Only increment nonce after successful send
        *nonce_guard += 1;

        Ok((nonce, pending_tx))
    }

    /// Reset nonce (useful for error recovery)
    pub async fn reset_nonce(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let current_nonce = self
            .provider
            .get_transaction_count(self.account_address)
            .await?;
        let mut nonce = self.nonce_mutex.lock().await;
        *nonce = current_nonce;
        info!("Reset nonce to: {}", current_nonce);
        Ok(())
    }
}
