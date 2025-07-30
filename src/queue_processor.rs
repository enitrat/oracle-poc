use crate::database::QueueDatabase;
use crate::oracle;
use crate::oracle::IVRFOracle::getRandomnessCall;
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
    batch_timeout: Duration,
    relayer: Option<Arc<Relayer>>,
    last_empty_log: Arc<Mutex<Option<Instant>>>,
    last_batch_time: Arc<Mutex<Instant>>,
}

impl QueueProcessor {
    pub fn new(postgres_client: Arc<PostgresClient>, poll_interval_millis: u64) -> Self {
        Self {
            queue_db: QueueDatabase::new(postgres_client),
            poll_interval: Duration::from_millis(poll_interval_millis),
            batch_timeout: Duration::from_millis(1000), // Process partial batches after 1s
            relayer: None,
            last_empty_log: Arc::new(Mutex::new(None)),
            last_batch_time: Arc::new(Mutex::new(Instant::now())),
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
            "Starting queue processor with batch size: {}, batch timeout: {:?}",
            relayer.batch_size, self.batch_timeout
        );
        info!(
            "Relayer managing {} accounts with BEBE batch processing",
            relayer.get_addresses().len()
        );

        // Check if BEBE is configured
        let use_batching = relayer.accounts.iter().any(|a| a.bebe_address.is_some());
        if !use_batching {
            return Err("BEBE not configured. Batch processing requires BEBE to be deployed and configured.".into());
        }

        loop {
            // Check pending count first
            let pending_count = match self.queue_db.get_pending_count().await {
                Ok(count) => count,
                Err(e) => {
                    error!("Failed to get pending count: {}", e);
                    0
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

                // Wait before polling again
                time::sleep(self.poll_interval).await;
                continue;
            }

            // Check if we should process immediately or wait
            let last_batch_elapsed = {
                let last_time = self.last_batch_time.lock().await;
                last_time.elapsed()
            };

            let should_process = pending_count >= relayer.batch_size as i64
                || (pending_count > 0 && last_batch_elapsed >= self.batch_timeout);

            if should_process {
                let reason = if pending_count >= relayer.batch_size as i64 {
                    format!(
                        "queue has {} requests (>= batch size {})",
                        pending_count, relayer.batch_size
                    )
                } else {
                    format!(
                        "timeout elapsed ({:?} >= {:?})",
                        last_batch_elapsed, self.batch_timeout
                    )
                };

                trace!("Processing batch: {}", reason);
                // Calculate how many batches we can process based on available relayers
                let available_relayers = relayer.accounts.len();
                let batches_to_process = std::cmp::min(
                    (pending_count as usize).div_ceil(relayer.batch_size),
                    available_relayers,
                );

                trace!(
                    "Processing up to {} batches with {} available relayers (queue has {} pending)",
                    batches_to_process,
                    available_relayers,
                    pending_count
                );

                // Update last batch time
                {
                    let mut last_time = self.last_batch_time.lock().await;
                    *last_time = Instant::now();
                }

                // Spawn multiple batch processors based on available relayers
                for _ in 0..batches_to_process {
                    // Dequeue up to batch_size requests
                    let requests = match self.queue_db.dequeue_requests(relayer.batch_size).await {
                        Ok(reqs) => reqs,
                        Err(e) => {
                            error!("Failed to dequeue requests: {:?}", e);
                            break;
                        }
                    };

                    if requests.is_empty() {
                        break; // No more requests
                    }

                    let batch_size = requests.len();
                    trace!("Spawning processor for batch of {} requests", batch_size);

                    // Process batch in background
                    let queue_db = Arc::new(self.queue_db.clone());
                    let relayer_clone = relayer.clone();

                    tokio::spawn(async move {
                        if let Err(e) =
                            Self::process_batch_requests(requests, queue_db, relayer_clone).await
                        {
                            error!("Failed to process batch: {:?}", e);
                        }
                    });
                }
            } else {
                // Wait a bit before checking again
                time::sleep(Duration::from_millis(50)).await;
            }
        }
    }

    /// Process a batch of requests
    async fn process_batch_requests(
        requests: Vec<crate::database::PendingRequest>,
        queue_db: Arc<QueueDatabase>,
        relayer: Arc<Relayer>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if requests.is_empty() {
            return Ok(());
        }

        let batch_size = requests.len();
        let request_ids: Vec<_> = requests.iter().map(|r| r.request_id).collect();

        info!("Processing batch of {} requests", batch_size);

        // Wait for the next available account with BEBE configured
        // This will block until a relayer is available, providing natural backpressure
        let account = loop {
            match relayer.next_available_batch().await {
                Ok(acc) => break acc,
                Err(e) => {
                    warn!(
                        "No available relayer account for batch: {:?}. Waiting...",
                        e
                    );
                    // Wait a bit before retrying
                    time::sleep(Duration::from_millis(500)).await;
                }
            }
        };

        let account_address = account.address;

        // Ensure we release the account when done
        let result = async {
            // Build batch calls
            let calls = oracle::build_batch_calls(&requests);

            // Send batch transaction
            match account.send_batch(&calls).await {
                Ok(tx_hash) => {
                    // Record metrics for batch fulfillment
                    crate::relayer::metrics::record_batch_fulfillment(batch_size);

                    // Check which requests were actually fulfilled and only mark those as completed
                    let mut fulfilled_requests = Vec::new();
                    let mut unfulfilled_requests = Vec::new();

                    for request in requests.iter() {
                        let encoded_call = oracle::encode_get_randomness_call(request.request_id);
                        match account.send_call(request.contract_address, encoded_call.abi_encode().into()).await {
                            Ok(call_result) => {
                                let call_res_array = call_result.as_ref();
                                match oracle::IVRFOracle::getRandomnessCall::abi_decode_returns(call_res_array) {
                                    Ok(decoded_result) => {
                                        if decoded_result.fulfilled {
                                            fulfilled_requests.push(request.request_id);
                                        } else {
                                            crate::relayer::metrics::record_batch_unfulfilled(1);
                                            unfulfilled_requests.push(request.request_id);
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to decode call result for request {}: {:?}", hex::encode(request.request_id), e);
                                        unfulfilled_requests.push(request.request_id);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to send call for request {}: {:?}", hex::encode(request.request_id), e);
                                unfulfilled_requests.push(request.request_id);
                            }
                        }
                    }

                    // Mark only the fulfilled requests as completed
                    for request_id in fulfilled_requests.iter() {
                        queue_db.mark_fulfilled(*request_id).await?;
                    }

                    // Put unfulfilled requests back in the queue for retry
                    for request_id in unfulfilled_requests.iter() {
                        queue_db.requeue_request(*request_id).await?;
                    }

                    info!(
                        "Batch processing complete: {} succeeded, {} failed/retrying. Used account {}",
                        fulfilled_requests.len(), unfulfilled_requests.len(), account_address
                    );
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
        }.await;

        // Always release the account
        relayer.release_account(account_address).await;

        result
    }
}

/// Create a PostgreSQL client using rindexer
pub async fn create_postgres_client(
) -> Result<Arc<PostgresClient>, Box<dyn std::error::Error + Send + Sync>> {
    // Rindexer manages the database connection internally based on environment variables
    let client = PostgresClient::new().await?;
    Ok(Arc::new(client))
}
