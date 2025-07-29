# Oracle

A Verifiable Random Function (VRF) Oracle implementation using Rust and rindexer for Ethereum-compatible chains.

## Overview

This oracle listens for randomness requests on-chain and fulfills them with cryptographically secure random values. The current implementation uses a basic RNG, with plans to implement a proper VRF (BLS 12-381 or secp256k1-based Schnorr) for production use.

## Prerequisites

- Rust 1.87+
- Node.js 20+
- Anvil (local Ethereum node)
- Environment variables configured (see `.env.example`)

## Setup

1. Clone the repository
2. Copy `.env.example` to `.env` and configure:
   ```bash
   cp .env.example .env
   ```
3. Install dependencies:
   ```bash
   npm install
   cargo build
   ```

## Running the Oracle

1. Start Anvil in a separate terminal:
   ```bash
   anvil
   ```

2. Deploy the VRFOracle contract:
   ```bash
   bun run deploy
   ```

3. Start the oracle indexer:
   ```bash
   cargo run -- --indexer
   ```

4. Request randomness (in another terminal):
   ```bash
   bun run request-randomness
   ```

## Architecture

- `src/oracle/`: Oracle logic for generating random values and fulfilling requests
- `src/rindexer_lib/`: Event indexing and handling logic
- `contracts/`: Solidity smart contracts
- `script/`: Deployment and testing scripts

## Security Considerations

### Current Implementation (PoC)
- Uses basic RNG (not suitable for production)
- Empty proofs (VRF implementation pending)
- Basic gas estimation

### Production Requirements
- [ ] Implement proper VRF with proof generation
- [ ] Secure key storage (HSM/KMS)
- [ ] Chain ID verification
- [ ] Double-spend protection
- [ ] Gas optimization with proper estimation

## Environment Variables

See `.env.example` for required configuration:
- `ORACLE_PRIVATE_KEY`: Private key for the oracle (keep secure!)
- `CONTRACT_ADDRESS`: Deployed VRFOracle contract address
- `RPC_URL`: Ethereum RPC endpoint (defaults to local Anvil)
- `USER_PRIVATE_KEY`: Test user private key
- `DEPLOYER_PRIVATE_KEY`: Contract deployer private key

## Testing

```bash
# Run integration test
npm test

# Run Rust tests
cargo test
```

## License

MIT
