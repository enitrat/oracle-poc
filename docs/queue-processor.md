# Queue Processor Documentation

## Overview

The Queue Processor is a durable, PostgreSQL-based queue system that ensures reliable fulfillment of randomness requests even if the oracle service restarts or crashes.

## Architecture

1. **Event Indexing**: When `RandomnessRequested` events are detected, they are enqueued in the `pending_requests` table instead of being processed immediately.

2. **Queue Processing**: A separate service polls the queue and processes pending requests using PostgreSQL's `FOR UPDATE SKIP LOCKED` for safe concurrent processing.

3. **Idempotency**: When `RandomnessFulfilled` events are detected, the corresponding requests are marked as completed in the queue to prevent duplicate processing.

## Database Schema

The `pending_requests` table tracks:

- `request_id`: Unique identifier for the randomness request
- `contract_address`: The VRF Oracle contract address
- `status`: Current state (pending, processing, fulfilled, failed)
- `created_at`: When the request was first seen
- `updated_at`: Last modification time
- `processing_started_at`: When processing began (for timeout detection)
- `retry_count`: Number of processing attempts
- `max_retries`: Maximum allowed retries (default: 5)
- `last_error`: Error message from the last failed attempt
- `network`: Network name (e.g., "anvil", "mainnet")

## Usage

### Running the Queue Processor

```bash
# Set the DATABASE_URL environment variable (required)
export DATABASE_URL="postgresql://user:pass@localhost/dbname"

# Run with migrations
cargo run -- queue-processor --migrate

# Run without migrations
cargo run -- queue-processor

# Custom poll interval (default: 5 seconds)
cargo run -- queue-processor --poll-interval 10
```

### Running Multiple Services

The easiest way to run all services:

```bash
# Set database URL
export DATABASE_URL="postgresql://user:pass@localhost/dbname"

# Run everything (indexer + GraphQL + queue processor)
cargo run
```

Or run services individually:

1. **Indexer**: Listens for events and enqueues requests

   ```bash
   cargo run -- indexer
   ```

2. **Queue Processor**: Processes the queue

   ```bash
   cargo run -- queue-processor
   ```

3. **GraphQL**: Query interface
   ```bash
   cargo run -- graphql
   ```

## Retry Logic

- Requests are retried up to 5 times by default
- Failed requests return to "pending" status if retries remain
- Requests stuck in "processing" for >5 minutes are automatically retried
- Permanently failed requests are marked as "failed"

## Monitoring

Check queue status:

```sql
-- Pending requests
SELECT COUNT(*) FROM zamaoracle_vrf_oracle.pending_requests WHERE status = 'pending';

-- Failed requests
SELECT * FROM zamaoracle_vrf_oracle.pending_requests WHERE status = 'failed';

-- Processing time stats
SELECT
    AVG(EXTRACT(EPOCH FROM (updated_at - created_at))) as avg_processing_seconds,
    MAX(EXTRACT(EPOCH FROM (updated_at - created_at))) as max_processing_seconds
FROM zamaoracle_vrf_oracle.pending_requests
WHERE status = 'fulfilled';
```

## Benefits

1. **Reliability**: Requests survive service restarts
2. **Scalability**: Multiple queue processors can run concurrently
3. **Observability**: Database provides audit trail and metrics
4. **Simplicity**: No additional infrastructure required beyond PostgreSQL
