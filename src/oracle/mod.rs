use crate::database::PendingRequest;
use alloy::sol_types::SolValue;
use alloy::{
    primitives::{Bytes, FixedBytes, U256},
    sol,
    sol_types::SolCall,
};
use rand::{rngs::OsRng, RngCore};
use tracing::trace;

// Define the contract interface using sol! macro
sol! {
    interface IVRFOracle {
        function fulfillRandomness(bytes32 requestId, uint256 randomness) external;
        function getRandomness(bytes32 requestId) external view returns (bool fulfilled, uint256 randomness);
    }
}

// Define the BEBE interface
sol! {
    interface IBEBE {
        function execute(bytes32 mode, bytes calldata executionData) external payable;
    }

    struct Call {
        address to; // Replaced as `address(this)` if `address(0)`. Renamed to `to` for Ithaca Porto.
        uint256 value; // Amount of native currency (i.e. Ether) to send.
        bytes data; // Calldata to send with the call.
    }
}

/// Generates a cryptographically secure random value
pub fn generate_random_value() -> U256 {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    U256::from_be_bytes(bytes)
}

/// Builds batch calls for multiple pending requests
/// Returns a vector of Call structs ready for ERC7821 batch execution
pub fn build_batch_calls(requests: &[PendingRequest]) -> Vec<Call> {
    requests
        .iter()
        .map(|request| {
            let random_value = generate_random_value();
            trace!(
                "Generated random value {} for request {}",
                random_value,
                hex::encode(request.request_id)
            );

            let call_data = IVRFOracle::fulfillRandomnessCall {
                requestId: request.request_id,
                randomness: random_value,
            };

            Call {
                to: request.contract_address,
                value: U256::ZERO,
                data: Bytes::from(call_data.abi_encode()),
            }
        })
        .collect()
}

/// Encodes batch calls for ERC7821 execution
/// The mode parameter should be the batch execution mode (typically 0x01000000...)
pub fn encode_batch_for_erc7821(calls: &[Call]) -> IBEBE::executeCall {
    let mode_bytes: [u8; 32] = [0x01; 1]
        .into_iter()
        .chain([0x00; 31])
        .collect::<Vec<u8>>()
        .try_into()
        .unwrap();
    let mode_fixed_bytes = FixedBytes::<32>::from_slice(&mode_bytes);
    let calldata = calls.abi_encode().into();

    IBEBE::executeCall {
        mode: mode_fixed_bytes,
        executionData: calldata,
    }
}

pub fn encode_get_randomness_call(request_id: FixedBytes<32>) -> IVRFOracle::getRandomnessCall {
    let result = IVRFOracle::getRandomnessCall {
        requestId: request_id,
    };
    result
}
