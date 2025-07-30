# Building a Resilient and Fast VRF Oracle in Rust

## 1. The Genesis: The Minimum Viable Product (MVP)

### 1.1. Core Logic & Architecture

In the initial commit (71fd3de), the VRF Oracle began as a simple event indexer built on the Rindexer framework. The MVP architecture was straightforward and linear:

1. **Event Listening**: Using Rindexer, the oracle monitored the blockchain for `RandomnessRequested` events emitted by the smart contract
2. **Event Processing**: When a request was detected, the event handler would:
   - Extract the request ID and requester information
   - Store the event data in both PostgreSQL and CSV formats for persistence
3. **Basic Infrastructure**: The initial setup included:
   - A simple Rust binary using `tokio` for async operations
   - Integration with `alloy` (v1.0.4) for Ethereum interactions
   - Basic CLI argument parsing for GraphQL and indexer configuration

### 1.2. Key Code Snippets (MVP)

The event handler demonstrated the basic linear flow:

```rust
async fn randomness_requested_handler(
    manifest_path: &PathBuf,
    registry: &mut EventCallbackRegistry,
) {
    let handler = RandomnessRequestedEvent::handler(|results, context| async move {
        if results.is_empty() {
            return Ok(());
        }

        // Store events in database and CSV
        for result in results.iter() {
            // Extract event data and persist to storage
            let data = vec![
                EthereumSqlTypeWrapper::Address(result.tx_information.address),
                EthereumSqlTypeWrapper::Bytes(result.event_data.requestId.into()),
                // ... more fields
            ];
            postgres_bulk_data.push(data);
        }

        // Bulk insert to database
        context.database.bulk_insert(
            "zamaoracle_vrf_oracle.randomness_requested",
            &rows,
            &postgres_bulk_data,
        ).await?;

        Ok(())
    });
}
```

### 1.3. Inherent Limitations

The MVP, while functional as an event indexer, had several critical limitations:

- **No Fulfillment Logic**: The oracle could listen for requests but had no mechanism to actually generate and submit random numbers
- **No Transaction Management**: Missing any capability to send transactions back to the blockchain
- **Single-threaded Processing**: Events were processed sequentially without concurrency
- **No Error Recovery**: If the indexer crashed, there was no mechanism to recover and process missed events
- **No Gas Management**: No consideration for transaction costs or gas price optimization

## 2. Iteration 1: Hardening with Custom Nonce Management

### 2.1. The Problem: Race Conditions and Dropped Transactions

The second major evolution (commit 411f690 - "relayers") introduced the critical oracle fulfillment logic. However, this brought a new challenge: when multiple randomness requests arrived simultaneously, the oracle needed to send multiple transactions quickly. Relying on the Ethereum node's `eth_getTransactionCount` for nonce management proved unreliable:

- Multiple concurrent requests would receive the same nonce from the node
- This led to transaction conflicts and dropped transactions
- The oracle would fail to fulfill requests reliably under load

### 2.2. The Solution: A Stateful, In-Memory Nonce Manager

The solution introduced a sophisticated `NonceManager` that maintained nonce state locally:

```rust
pub struct NonceManager {
    nonce_mutex: Arc<Mutex<u64>>,
    pub account_address: Address,
    provider: Arc<dyn Provider<Ethereum> + Send + Sync>,
}

impl NonceManager {
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
}
```

This atomic approach ensured that:

- Each transaction received a unique nonce
- The nonce was only incremented after successful submission
- Concurrent requests could be processed without conflicts

### 2.3. Evidence from Git History

```diff
commit 411f69072ce78b68fa89d1de2e92244bcd32aed5
Author: enitrat <msaug@protonmail.com>
Date:   Tue Jul 29 23:37:08 2025 +0200

    relayers

+ src/provider.rs                           | 105 +++++++
+ src/relayer/account.rs                    | 199 ++++++++++++++
+ src/relayer/config.rs                     | 123 +++++++++
+ src/relayer/mod.rs                        |  24 ++
+ src/relayer/scheduler.rs                  | 208 ++++++++++++++
```

## 3. Iteration 2: Building a Resilient Relayer

### 3.1. The Problem: Monolithic Design and Gas Volatility

As the oracle evolved, the transaction submission logic became increasingly complex. The initial implementation had several issues:

- Transaction submission logic was intertwined with business logic
- No sophisticated gas price management for volatile network conditions
- Single account bottleneck - one stuck transaction would block all subsequent requests
- No retry mechanism for failed transactions

### 3.2. The Solution: A Separate Module for Transaction Submission

The relayer module introduced a clean separation of concerns with sophisticated features:

```rust
pub struct Relayer {
    accounts: Vec<Arc<RelayerAccount>>,
    scheduler_type: SchedulerType,
    pending_block_threshold: u64,
    round_robin_index: AtomicUsize,
    rpc_url: String,
}

impl Relayer {
    /// Get the next available account and its nonce
    pub async fn next_available(
        &self,
    ) -> Result<(Arc<NonceManager>, u64), Box<dyn std::error::Error + Send + Sync>> {
        // Try each account to find one that's available
        for account in &self.accounts {
            // Check if account has sufficient gas
            // Check if account has pending transactions
            // Return first available account
        }
    }
}
```

### 3.3. Key Features of the Relayer

- **Multi-Account Support**: Multiple funded accounts to parallelize transaction submission
- **Gas Estimation**: Dynamic gas price calculation based on network conditions
- **Transaction Retries**: Automatic retry with exponential backoff for failed transactions
- **Account Health Monitoring**: Skip accounts with insufficient gas or too many pending transactions
- **Flexible Scheduling**: Support for round-robin or random account selection

### 3.4. Code Archaeology

The relayer module's creation shows a clear architectural improvement:

```diff
+mod account;      // Individual account management
+mod config;       // Configuration for multi-account setup
+mod metrics;      // Prometheus metrics for monitoring
+mod scheduler;    // Account selection strategies
```

## 4. Iteration 3: The Leap to Account Abstraction

### 4.1. The Motivation: Moving Beyond EOAs

The final major evolution (commit e5b83de - "use batched requests for randomness with EIP7702") represents a paradigm shift. Traditional Externally Owned Accounts (EOAs) have limitations:

- One transaction per account per block (due to nonce constraints)
- No ability to batch operations efficiently
- Complex multi-account management for high throughput

EIP-7702 Account Abstraction offered solutions:

- Batch multiple oracle fulfillments in a single transaction
- Reduced gas costs through operation bundling
- Simplified account management

### 4.2. The Implementation: From Raw Transactions to UserOperations

The implementation introduced the BEBE (Basic EOA Batch Executor) contract:

```solidity
contract BasicEOABatchExecutor is ERC7821 {
    /// @dev Validates the signature with ERC1271 return.
    function isValidSignature(bytes32 hash, bytes calldata signature)
        public
        view
        virtual
        returns (bytes4 result)
    {
        bool success = ECDSA.recoverCalldata(hash, signature) == address(this);
        // Return standard ERC1271 magic value on success
    }
}
```

And updated the TypeScript client to use batched operations:

```typescript
// Prepare batch calls
const requestCalls = Array.from({ length: batchSize }, () => ({
  to: contractAddress as `0x${string}`,
  abi: ABI,
  functionName: "requestRandomness",
  value: fee,
}));

// Send batch transaction using EIP-7702
const requestTx = await walletClient.execute({
  address: userAccount.address,
  calls: requestCalls,
});
```

### 4.3. Code Snippets: Building a UserOperation

The queue processor was updated to handle batch fulfillments:

```rust
// Build batch calls
let calls = oracle::build_batch_calls(&requests);

// Send batch transaction
match account.send_batch(&calls).await {
    Ok(tx_hash) => {
        info!("Batch transaction sent: {}", tx_hash);

        // Record metrics for batch fulfillment
        crate::relayer::metrics::record_batch_fulfillment(batch_size);
    }
    Err(e) => {
        error!("Failed to send batch transaction: {:?}", e);
    }
}
```

### 4.4. Architectural Impact

This change fundamentally altered the oracle's throughput capabilities:

- Instead of 1 fulfillment per transaction, the oracle could now process 100+ fulfillments per transaction
- Gas costs were dramatically reduced through batching
- The system became more resilient to individual transaction failures

## 5. Conclusion: A Mature and Resilient System

The VRF Oracle's evolution from a simple event listener to a sophisticated, high-throughput system demonstrates several key architectural patterns:

1. **Progressive Enhancement**: Starting with a minimal viable product and iteratively adding features based on real-world requirements

2. **Separation of Concerns**: Moving from monolithic design to modular architecture with distinct responsibilities (indexing, queue processing, relaying, batching)

3. **Resilience Through Redundancy**: Multi-account support, retry mechanisms, and persistent queue storage ensure the oracle can recover from failures

4. **Performance Through Innovation**: Adopting cutting-edge standards like EIP-7702 to overcome fundamental blockchain limitations

The final system achieves:

- **High Throughput**: 100+ requests fulfilled per transaction through batching
- **Reliability**: Persistent queue ensures no request is lost
- **Cost Efficiency**: Dramatically reduced gas costs through operation bundling
- **Operational Excellence**: Comprehensive metrics, monitoring, and error recovery

This journey illustrates how a production-grade oracle must evolve beyond simple smart contract interactions to handle the complexities of real-world blockchain infrastructure.
