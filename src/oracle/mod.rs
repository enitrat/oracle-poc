use crate::provider::NonceManager;
use alloy::{
    primitives::{Address, FixedBytes, U256},
    rpc::types::TransactionRequest,
    sol,
    sol_types::SolCall,
};
use rand::{rngs::OsRng, RngCore};
use std::sync::Arc;
use tracing::{error, info, trace};

// Define the contract interface using sol! macro
sol! {
    interface IVRFOracle {
        function fulfillRandomness(bytes32 requestId, uint256 randomness) external;
    }
}

/// Generates a cryptographically secure random value
pub fn generate_random_value() -> U256 {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    U256::from_be_bytes(bytes)
}

/// Fulfills a randomness request by sending a transaction to the VRF Oracle contract
pub async fn fulfill_randomness_request_with_nonce(
    request_id: FixedBytes<32>,
    contract_address: Address,
    nonce_manager: Arc<NonceManager>,
    nonce: u64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Generate random value
    let random_value = generate_random_value();
    trace!(
        "Generated random value {} for request {} with nonce {}",
        random_value,
        hex::encode(request_id),
        nonce
    );

    // Prepare the fulfillRandomness call
    let call_data = IVRFOracle::fulfillRandomnessCall {
        requestId: request_id,
        randomness: random_value,
    };

    // Build transaction
    let tx = TransactionRequest::default()
        .to(contract_address)
        .input(call_data.abi_encode().into());

    // Send transaction with specific nonce
    let pending_tx = nonce_manager.send_transaction_with_nonce(tx, nonce).await?;

    // Wait for confirmation
    let receipt = pending_tx.get_receipt().await?;

    if !receipt.status() {
        error!(
            "Failed to fulfill randomness request {} in transaction {:?}",
            hex::encode(request_id),
            receipt.transaction_hash
        );
        return Err("Transaction failed".into());
    }

    Ok(())
}
