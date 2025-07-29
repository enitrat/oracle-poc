use alloy::primitives::{Address, FixedBytes};
use rindexer::PostgresClient;
use std::sync::Arc;
use tracing::{error, info, trace};

#[derive(Debug, Clone)]
pub struct PendingRequest {
    pub request_id: FixedBytes<32>,
    pub contract_address: Address,
    pub status: String,
    pub retry_count: i32,
    pub network: String,
}

#[derive(Clone)]
pub struct QueueDatabase {
    client: Arc<PostgresClient>,
}

impl QueueDatabase {
    pub const fn new(client: Arc<PostgresClient>) -> Self {
        Self { client }
    }

    /// Enqueue a new randomness request
    pub async fn enqueue_request(
        &self,
        request_id: FixedBytes<32>,
        contract_address: Address,
        network: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let query = r#"
            INSERT INTO zamaoracle_vrf_oracle.pending_requests
            (request_id, contract_address, network, status)
            VALUES ($1, $2, $3, 'pending')
            ON CONFLICT (request_id) DO NOTHING
        "#;

        self.client
            .execute(
                query,
                &[
                    &request_id.as_slice(),
                    &contract_address.to_string(),
                    &network,
                ],
            )
            .await?;

        trace!(
            "Enqueued request {} for contract {}",
            hex::encode(request_id),
            contract_address
        );

        Ok(())
    }

    /// Dequeue a pending request for processing
    pub async fn dequeue_request(
        &self,
    ) -> Result<Option<PendingRequest>, Box<dyn std::error::Error + Send + Sync>> {
        let query = r#"
            UPDATE zamaoracle_vrf_oracle.pending_requests
            SET status = 'processing',
                processing_started_at = NOW(),
                retry_count = retry_count + 1
            WHERE request_id = (
                SELECT request_id
                FROM zamaoracle_vrf_oracle.pending_requests
                WHERE (status = 'pending'
                    OR (status = 'processing'
                        AND processing_started_at < NOW() - INTERVAL '5 minutes'))
                    AND retry_count < max_retries
                ORDER BY created_at
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            RETURNING request_id, contract_address, status, retry_count, network
        "#;

        let rows = self.client.query(query, &[]).await?;

        if let Some(row) = rows.first() {
            let request_id_bytes: &[u8] = row.get(0);
            let request_id = FixedBytes::<32>::try_from(request_id_bytes)
                .map_err(|_| "Invalid request_id bytes")?;

            let contract_address_str: String = row.get(1);
            let contract_address = contract_address_str
                .parse::<Address>()
                .map_err(|_| "Invalid contract address")?;

            Ok(Some(PendingRequest {
                request_id,
                contract_address,
                status: row.get(2),
                retry_count: row.get(3),
                network: row.get(4),
            }))
        } else {
            Ok(None)
        }
    }

    /// Mark a request as fulfilled
    pub async fn mark_fulfilled(
        &self,
        request_id: FixedBytes<32>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let query = r#"
            UPDATE zamaoracle_vrf_oracle.pending_requests
            SET status = 'fulfilled',
                updated_at = NOW()
            WHERE request_id = $1
        "#;

        self.client
            .execute(query, &[&request_id.as_slice()])
            .await?;

        trace!("Marked request {} as fulfilled", hex::encode(request_id));

        Ok(())
    }

    /// Mark a request as failed with error message
    pub async fn mark_failed(
        &self,
        request_id: FixedBytes<32>,
        error_message: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let query = r#"
            UPDATE zamaoracle_vrf_oracle.pending_requests
            SET status = CASE
                    WHEN retry_count >= max_retries THEN 'failed'
                    ELSE 'pending'
                END,
                last_error = $2,
                processing_started_at = NULL,
                updated_at = NOW()
            WHERE request_id = $1
        "#;

        self.client
            .execute(query, &[&request_id.as_slice(), &error_message])
            .await?;

        error!(
            "Marked request {} as failed: {}",
            hex::encode(request_id),
            error_message
        );

        Ok(())
    }

    /// Get pending request count
    pub async fn get_pending_count(&self) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let query = r#"
            SELECT COUNT(*)
            FROM zamaoracle_vrf_oracle.pending_requests
            WHERE status IN ('pending', 'processing')
        "#;

        let row = self.client.query_one(query, &[]).await?;
        Ok(row.get(0))
    }

    /// Run the migration to create the pending_requests table
    pub async fn run_migration(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let migration = include_str!("../../migrations/001_create_pending_requests.sql");
        self.client.batch_execute(migration).await?;
        info!("Successfully ran pending_requests migration");
        Ok(())
    }
}
