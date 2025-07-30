use crate::database::QueueDatabase;
use crate::oracle;
use crate::relayer::{Relayer, RelayerConfig};
use alloy::sol_types::SolCall;
use rindexer::PostgresClient;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time;
use tracing::{error, info, trace, warn};

pub struct QueueProcessor {
    queue_db: QueueDatabase,
    poll_interval: Duration,
    relayer: Option<Arc<Relayer>>,
    last_empty_log: Arc<Mutex<Option<Instant>>>,
}

const MAX_BATCH_SIZE: usize = 100;

impl QueueProcessor {
    pub fn new(postgres_client: Arc<PostgresClient>, poll_interval_millis: u64) -> Self {
        Self {
            queue_db: QueueDatabase::new(postgres_client),
            poll_interval: Duration::from_millis(poll_interval_millis),
            relayer: None,
            last_empty_log: Arc::new(Mutex::new(None)),
        }
    }

    /// Initialize the relayer
    pub async fn init_relayer(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Initializing relayer from environment variables...");

        match RelayerConfig::from_env() {
            Ok(config) => {
                info!(
                    "Loaded relayer config with {} accounts",
                    config.accounts.len()
                );
                let relayer = Arc::new(Relayer::new(config).await?);
                self.relayer = Some(relayer);
                Ok(())
            }
            Err(e) => {
                error!("Failed to load relayer config: {}", e);
                error!("Make sure RELAYER_PRIVATE_KEYS is set in your environment or .env file");
                error!("Example: RELAYER_PRIVATE_KEYS=0xkey1,0xkey2,0xkey3");
                Err(e)
            }
        }
    }

    /// Run database migrations
    pub async fn run_migrations(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.queue_db.run_migration().await
    }

    /// Start processing the queue
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Initialize relayer if not already done
        if self.relayer.is_none() {
            self.init_relayer().await?;
        }

        let relayer = self
            .relayer
            .as_ref()
            .ok_or("Failed to initialize relayer")?
            .clone();

        info!(
            "Starting queue processor with {} relayer accounts",
            relayer.get_addresses().len()
        );

        // Check if BEBE is configured
        let use_batching = relayer.accounts.iter().any(|a| a.bebe_address.is_some());
        if !use_batching {
            return Err("BEBE not configured. Batch processing requires BEBE to be deployed and configured.".into());
        }

        loop {
            // Sleep for the poll interval
            time::sleep(self.poll_interval).await;

            // 1. Check if there's an available relayer
            let available_account = match relayer.try_get_available_batch().await {
                Some(account) => account,
                None => {
                    trace!("No available relayer accounts, waiting...");
                    continue;
                }
            };

            let account_address = available_account.address;
            info!("Found available relayer account: {}", account_address);

            // 2. Get all pending requests from the queue
            let pending_count = match self.queue_db.get_pending_count().await {
                Ok(count) => count,
                Err(e) => {
                    error!("Failed to get pending count: {}", e);
                    relayer.release_account(account_address).await;
                    continue;
                }
            };

            if pending_count == 0 {
                // Log empty queue periodically
                let now = Instant::now();
                let mut last_log = self.last_empty_log.lock().await;
                if last_log.is_none()
                    || now.duration_since(last_log.unwrap()) > Duration::from_secs(10)
                {
                    info!("Queue is empty, waiting for new requests...");
                    *last_log = Some(now);
                }
                relayer.release_account(account_address).await;
                continue;
            }

            // Dequeue ALL pending requests (up to a reasonable limit to avoid memory issues)
            let max_batch_size = 100; // Reasonable limit for a single multicall
            let requests_to_dequeue = std::cmp::min(pending_count as usize, max_batch_size);

            let requests = match self.queue_db.dequeue_requests(requests_to_dequeue).await {
                Ok(reqs) => reqs,
                Err(e) => {
                    error!("Failed to dequeue requests: {:?}", e);
                    relayer.release_account(account_address).await;
                    continue;
                }
            };

            if requests.is_empty() {
                relayer.release_account(account_address).await;
                continue;
            }

            info!(
                "Processing {} requests with relayer {}",
                requests.len(),
                account_address
            );

            // 3. Process all requests in a single multicall
            let queue_db = self.queue_db.clone();
            let result = Self::process_all_requests(requests, queue_db, available_account).await;

            // Always release the account after processing
            relayer.release_account(account_address).await;

            if let Err(e) = result {
                error!("Failed to process requests: {:?}", e);
            }
        }
    }

    /// Process all requests in a single multicall
    async fn process_all_requests(
        requests: Vec<crate::database::PendingRequest>,
        queue_db: QueueDatabase,
        account: Arc<crate::relayer::RelayerAccount>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if requests.is_empty() {
            return Ok(());
        }

        let batch_size = requests.len();
        let request_ids: Vec<_> = requests.iter().map(|r| r.request_id).collect();

        info!("Processing batch of {} requests", batch_size);

        // Build batch calls
        let calls = oracle::build_batch_calls(&requests);

        // Send batch transaction
        match account.send_batch(&calls).await {
            Ok(tx_hash) => {
                info!("Batch transaction sent: {}", tx_hash);

                // Record metrics for batch fulfillment
                crate::relayer::metrics::record_batch_fulfillment(batch_size);

                // Wait a bit for transaction to be mined
                time::sleep(Duration::from_secs(2)).await;

                // Check which requests were actually fulfilled and only mark those as completed
                // let mut fulfilled_requests = Vec::new();
                // let mut unfulfilled_requests = Vec::new();

                // for request in requests.iter() {
                //     let encoded_call = oracle::encode_get_randomness_call(request.request_id);
                //     match account.send_call(request.contract_address, encoded_call.abi_encode().into()).await {
                //         Ok(call_result) => {
                //             let call_res_array = call_result.as_ref();
                //             match oracle::IVRFOracle::getRandomnessCall::abi_decode_returns(call_res_array) {
                //                 Ok(decoded_result) => {
                //                     if decoded_result.fulfilled {
                //                         fulfilled_requests.push(request.request_id);
                //                     } else {
                //                         crate::relayer::metrics::record_batch_unfulfilled(1);
                //                         unfulfilled_requests.push(request.request_id);
                //                     }
                //                 }
                //                 Err(e) => {
                //                     error!("Failed to decode call result for request {}: {:?}", hex::encode(request.request_id), e);
                //                     unfulfilled_requests.push(request.request_id);
                //                 }
                //             }
                //         }
                //         Err(e) => {
                //             error!("Failed to send call for request {}: {:?}", hex::encode(request.request_id), e);
                //             unfulfilled_requests.push(request.request_id);
                //         }
                //     }
                // }

                // Mark only the fulfilled requests as completed
                for request_id in request_ids.iter() {
                    queue_db.mark_fulfilled(*request_id).await?;
                }

                // // Put unfulfilled requests back in the queue for retry
                // for request_id in unfulfilled_requests.iter() {
                //     queue_db.requeue_request(*request_id).await?;
                // }

                // info!(
                //     "Batch processing complete: {} succeeded, {} failed/retrying. Used account {}",
                //     fulfilled_requests.len(), unfulfilled_requests.len(), account.address
                // );
                Ok(())
            }
            Err(e) => {
                let error_msg = format!("Failed to fulfill batch: {e:?}");
                warn!(
                    "Failed to fulfill batch of {} requests: {:?}",
                    batch_size, e
                );

                // Mark all requests as failed (will retry if under max retries)
                queue_db.mark_batch_failed(&request_ids, &error_msg).await?;
                Ok(())
            }
        }
    }
}

/// Create a PostgreSQL client using rindexer
pub async fn create_postgres_client(
) -> Result<Arc<PostgresClient>, Box<dyn std::error::Error + Send + Sync>> {
    // Rindexer manages the database connection internally based on environment variables
    let client = PostgresClient::new().await?;
    Ok(Arc::new(client))
}
