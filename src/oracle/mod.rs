use alloy::{
    network::EthereumWallet,
    primitives::{Address, Bytes, FixedBytes, U256},
    providers::{
        fillers::{CachedNonceManager, ChainIdFiller, GasFiller, NonceFiller},
        Provider, ProviderBuilder,
    },
    rpc::types::TransactionRequest,
    signers::local::PrivateKeySigner,
    sol,
    sol_types::SolCall,
};
use rand::{thread_rng, RngCore};
use std::env;
use tracing::{error, info};

// Define the contract interface using sol! macro
sol! {
    interface IVRFOracle {
        function fulfillRandomness(bytes32 requestId, uint256 randomness, bytes memory proof) external;
    }
}

/// Generates a cryptographically secure random value
pub fn generate_random_value() -> U256 {
    let mut rng = thread_rng();
    let mut bytes = [0u8; 32];
    rng.fill_bytes(&mut bytes);
    U256::from_be_bytes(bytes)
}

/// Fulfills a randomness request by sending a transaction to the VRF Oracle contract
pub async fn fulfill_randomness_request(
    request_id: FixedBytes<32>,
    contract_address: Address,
) -> Result<(), Box<dyn std::error::Error>> {
    // Get environment variables
    let oracle_private_key =
        env::var("ORACLE_PRIVATE_KEY").expect("ORACLE_PRIVATE_KEY must be set");
    let rpc_url = env::var("RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8545".to_string());

    // Generate random value
    let random_value = generate_random_value();
    info!(
        "Generated random value {} for request {}",
        random_value,
        hex::encode(request_id)
    );

    // Setup signer
    let signer: PrivateKeySigner = oracle_private_key
        .parse()
        .map_err(|e| format!("Failed to parse private key: {e}"))?;
    let wallet = EthereumWallet::from(signer);

    // Setup provider
    let provider = ProviderBuilder::new()
        .filler(NonceFiller::<CachedNonceManager>::default())
        .filler(GasFiller)
        .filler(ChainIdFiller::default())
        .wallet(wallet)
        .connect(&rpc_url)
        .await?;

    // Prepare the fulfillRandomness call
    let call_data = IVRFOracle::fulfillRandomnessCall {
        requestId: request_id,
        randomness: random_value,
        proof: Bytes::new(), // TODO: Empty proof for now
    };

    // Build transaction
    let tx = TransactionRequest::default()
        .to(contract_address)
        .input(call_data.abi_encode().into());

    // Send transaction
    let pending_tx = provider.send_transaction(tx).await?;
    info!(
        "Sent fulfillRandomness transaction: {:?}",
        pending_tx.tx_hash()
    );

    // Wait for confirmation
    let receipt = pending_tx.get_receipt().await?;

    if receipt.status() {
        info!(
            "Successfully fulfilled randomness request {} in transaction {:?}",
            hex::encode(request_id),
            receipt.transaction_hash
        );
    } else {
        error!(
            "Failed to fulfill randomness request {} in transaction {:?}",
            hex::encode(request_id),
            receipt.transaction_hash
        );
        return Err("Transaction failed".into());
    }

    Ok(())
}
