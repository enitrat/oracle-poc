use crate::database::QueueDatabase;
use crate::oracle;
use crate::relayer::{Relayer, RelayerConfig};
use rindexer::PostgresClient;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore};
use tokio::time;
use tracing::{error, info, trace, warn};

pub struct QueueProcessor {
    queue_db: QueueDatabase,
    poll_interval: Duration,
    max_concurrent_requests: usize,
    relayer: Option<Arc<Relayer>>,
    last_empty_log: Arc<Mutex<Option<Instant>>>,
}

impl QueueProcessor {
    pub fn new(postgres_client: Arc<PostgresClient>, poll_interval_millis: u64) -> Self {
        Self {
            queue_db: QueueDatabase::new(postgres_client),
            poll_interval: Duration::from_millis(poll_interval_millis),
            max_concurrent_requests: 10, // Process up to 10 requests in parallel
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
            "Starting queue processor with poll interval: {:?}, max concurrent: {}",
            self.poll_interval, self.max_concurrent_requests
        );
        info!(
            "Relayer managing {} accounts",
            relayer.get_addresses().len()
        );

        // Use a semaphore to limit concurrent requests
        let semaphore = Arc::new(Semaphore::new(self.max_concurrent_requests));

        loop {
            // Check how many slots are available
            let available_permits = semaphore.available_permits();

            if available_permits > 0 {
                // Dequeue multiple requests at once
                let requests = self.dequeue_multiple_requests(available_permits).await?;

                if requests.is_empty() {
                    // No requests to process, wait before polling again
                    time::sleep(self.poll_interval).await;
                } else {
                    // Process requests in parallel
                    let queue_db = Arc::new(self.queue_db.clone());
                    let mut tasks = Vec::new();

                    for request in requests {
                        let permit = semaphore.clone().acquire_owned().await?;
                        let queue_db_clone = queue_db.clone();
                        let relayer_clone = relayer.clone();

                        let task = tokio::spawn(async move {
                            let result = Self::process_single_request_with_relayer(
                                request,
                                queue_db_clone,
                                relayer_clone,
                            )
                            .await;
                            drop(permit); // Release the permit when done
                            result
                        });

                        tasks.push(task);
                    }

                    // Don't wait for all tasks to complete - let them run in background
                    // This allows us to immediately check for more work
                }
            } else {
                // All slots are busy, wait a bit before checking again
                time::sleep(Duration::from_millis(10)).await;
            }

            // Log queue status periodically
            match self.queue_db.get_pending_count().await {
                Ok(pending_count) => {
                    if pending_count > 0 {
                        trace!("Pending requests in queue: {}", pending_count);
                    } else {
                        // Log every 10 seconds that queue is empty
                        let now = Instant::now();
                        let mut last_log = self.last_empty_log.lock().await;

                        if last_log.is_none()
                            || now.duration_since(last_log.unwrap()) > Duration::from_secs(10)
                        {
                            info!("Queue is empty, waiting for new requests...");
                            *last_log = Some(now);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to get pending count: {}", e);
                }
            }
        }
    }

    /// Dequeue multiple requests at once
    async fn dequeue_multiple_requests(
        &self,
        limit: usize,
    ) -> Result<Vec<crate::database::PendingRequest>, Box<dyn std::error::Error + Send + Sync>>
    {
        let mut requests = Vec::new();

        // Try to dequeue up to 'limit' requests
        for _ in 0..limit {
            match self.queue_db.dequeue_request().await? {
                Some(req) => requests.push(req),
                None => break, // No more requests
            }
        }

        if !requests.is_empty() {
            trace!(
                "Dequeued {} requests for parallel processing",
                requests.len()
            );
        }

        Ok(requests)
    }

    /// Process a single request with relayer
    async fn process_single_request_with_relayer(
        request: crate::database::PendingRequest,
        queue_db: Arc<QueueDatabase>,
        relayer: Arc<Relayer>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        trace!(
            "Processing request {} (attempt {})",
            hex::encode(request.request_id),
            request.retry_count
        );

        // Get the next available account and nonce
        let (nonce_manager, nonce) = match relayer.next_available().await {
            Ok(result) => result,
            Err(e) => {
                error!("Failed to get available relayer account: {:?}", e);
                // Mark request as failed but with retry
                queue_db
                    .mark_failed(request.request_id, &format!("No available relayer: {e:?}"))
                    .await?;
                return Ok(());
            }
        };

        // Get the account address for tracking
        let account_address = nonce_manager.account_address;

        // Attempt to fulfill the randomness request
        match oracle::fulfill_randomness_request_with_nonce(
            request.request_id,
            request.contract_address,
            nonce_manager,
            nonce,
        )
        .await
        {
            Ok(_) => {
                // Mark as fulfilled
                queue_db.mark_fulfilled(request.request_id).await?;
                // Notify relayer of success
                relayer.invalidate_nonce(account_address, true).await;
            }
            Err(e) => {
                let error_msg = format!("Failed to fulfill request: {e:?}");
                warn!(
                    "Failed to fulfill randomness request {} (attempt {}) using account {}: {:?}",
                    hex::encode(request.request_id),
                    request.retry_count,
                    account_address,
                    e
                );

                // Notify relayer of failure
                relayer.invalidate_nonce(account_address, false).await;

                // Mark as failed (will retry if under max retries)
                queue_db.mark_failed(request.request_id, &error_msg).await?;
            }
        }

        Ok(())
    }
}

/// Create a PostgreSQL client using rindexer
pub async fn create_postgres_client(
) -> Result<Arc<PostgresClient>, Box<dyn std::error::Error + Send + Sync>> {
    // Rindexer manages the database connection internally based on environment variables
    let client = PostgresClient::new().await?;
    Ok(Arc::new(client))
}
