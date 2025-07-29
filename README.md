# ZamaOracle

A high-performance Verifiable Random Function (VRF) Oracle implementation using Rust and Rindexer for Ethereum-compatible chains.

## Overview

ZamaOracle provides cryptographically secure random values to smart contracts through a scalable, fault-tolerant architecture. It features parallel request processing, multi-account transaction relaying, and durable queue management via PostgreSQL.

For a deep dive into the system's design, see [docs/architecture.md](docs/architecture.md).

## Quick Start

### 1. Prerequisites

- Rust (latest stable version)
- Foundry (for compiling smart contracts)
- Bun (for running scripts)
- Docker (for running PostgreSQL)

### 2. Setup

```bash
# Clone the repository
git clone <repository_url>
cd zamaoracle

# Copy and configure environment variables
cp .env.example .env

# Install Node.js dependencies
npm install

# Build the Rust project
cargo build

# Compile smart contracts
forge build
```

### 3. Run Services

```bash
# 1. Start a local blockchain in a separate terminal
anvil

# 2. Start a PostgreSQL database using Docker
docker run -d --name zamaoracle-db \
  -e POSTGRES_USER=user \
  -e POSTGRES_PASSWORD=password \
  -e POSTGRES_DB=zamaoracle \
  -p 5432:5432 \
  postgres:15

# 3. Deploy the contract. This script also updates .env with the new address.
bun run script/deploy-contract.ts

# 4. Run all oracle services (indexer, queue processor, metrics)
cargo run -- run
```

### 4. Test

```bash
# In another terminal, simulate continuous load:
bun run script/request-randomness.ts
```

## Dashboard

A real-time terminal dashboard is available for monitoring the oracle:

```bash
cargo run --bin dashboard
```

The dashboard displays:

- Queue metrics (pending, fulfilled, failed requests)
- Average latency and performance gauges
- Relayer account skip reasons
- Real-time sparkline charts

See [dashboard.md](dashboard.md) for details.

## Architecture

- `contracts/`: Solidity smart contracts for the on-chain VRF oracle.
- `src/oracle/`: Core Rust logic for generating random values and fulfilling requests.
- `src/rindexer_lib/`: Event indexing and handling logic using the Rindexer framework.
- `src/database/`: PostgreSQL-based durable queue implementation.
- `src/queue_processor/`: Parallel request processor with semaphore-based concurrency control.
- `src/relayer/`: Multi-account relayer system for high-throughput, nonce-safe transaction submission.
- `script/`: TypeScript scripts for deployment and load testing.

## Environment Variables

The `.env` file is required for operation. See `.env.example` for a template.

- `ORACLE_ADDRESS`: Public address of the oracle account.
- `ORACLE_PRIVATE_KEY`: Private key for the oracle account.
- `DEPLOYER_PRIVATE_KEY`: Private key for the account that deploys the contract.
- `USER_PRIVATE_KEY`: Private key for a test user account.
- `CONTRACT_ADDRESS`: Deployed `VRFOracle` contract address (auto-populated by `deploy-contract.ts`).
- `RPC_URL`: Ethereum RPC endpoint (defaults to local Anvil).
- `DATABASE_URL`: PostgreSQL connection string.

### Relayer Configuration

- `RELAYER_PRIVATE_KEYS`: Comma-separated list of private keys for multi-account relaying
- `RELAYER_MIN_GAS_WEI`: Minimum gas balance required for each account (default: 0.005 ETH)
- `RELAYER_SCHEDULER`: Scheduler type: `round_robin` or `random` (default: `round_robin`)
- `RELAYER_PENDING_BLOCK_THRESHOLD`: Max pending transactions before skipping an account (default: 3)

## Testing

```bash
# Run integration test
npm test

# Run Rust tests
cargo test
```

## License

MIT
