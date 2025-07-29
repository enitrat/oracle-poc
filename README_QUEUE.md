# ZamaOracle Queue System

## Default Behavior

When you run `cargo run` without any arguments, the oracle will automatically start:

- **Indexer**: Listens for RandomnessRequested events and enqueues them
- **GraphQL Server**: Provides query interface on port 3001
- **Queue Processor**: Processes pending requests from the database queue

## Prerequisites

Set the DATABASE_URL environment variable:

```bash
export DATABASE_URL="postgresql://user:password@localhost/dbname"
```

## Running Modes

### 1. Default Mode (Recommended)

Runs all services together:

```bash
cargo run
```

### 2. Individual Services

Run only the indexer:

```bash
cargo run -- indexer
```

Run only GraphQL:

```bash
cargo run -- graphql
```

Run only the queue processor:

```bash
cargo run -- queue-processor
```

### 3. Custom Configurations

Run with custom GraphQL port:

```bash
cargo run -- --port 8080
```

Run indexer with GraphQL:

```bash
cargo run -- indexer --graphql
```

Run all services explicitly:

```bash
cargo run -- run
```

## Queue Processor Behavior

- **Automatic Start**: The queue processor starts automatically in default mode if DATABASE_URL is set
- **Graceful Fallback**: If DATABASE_URL is not set, the indexer and GraphQL still run, but a warning is displayed
- **Background Processing**: Queue processor runs in a background task, polling every 5 seconds
- **Auto-Migration**: Database migrations run automatically on startup
- **Retry Logic**: Failed requests are retried up to 5 times
- **Idempotency**: Fulfilled requests are marked complete to prevent duplicate processing

## Monitoring

Check the logs to see all services running:

```
[INFO] Starting ZamaOracle - Indexer: true, GraphQL: true, Queue: true, Port: Some(3001)
[INFO] Starting Queue Processor in background
[INFO] Successfully ran pending_requests migration
```

If DATABASE_URL is not set:

```
[WARN] DATABASE_URL not set, queue processor will not start. Set DATABASE_URL to enable queue processing.
```
