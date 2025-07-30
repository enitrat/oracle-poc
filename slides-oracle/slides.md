---
theme: seriph
background: https://images.unsplash.com/photo-1620121692029-d088224ddc74?ixlib=rb-4.0.3&ixid=M3wxMjA3fDB8MHxwaG90by1wYWdlfHx8fGVufDB8fHx8fA%3D%3D&auto=format&fit=crop&w=2832&q=80
title: Building a Resilient VRF Oracle
info: |
  ## Building a Resilient and Fast VRF Oracle in Rust
  A chronological journey through the evolution of ZamaOracle, from a simple event indexer to a high-performance, batch-processing system.
class: text-center
drawings:
  persist: false
transition: slide-left
mdc: true
---

# Building a Resilient VRF Oracle in Rust

A chronological journey from a simple indexer to a sophisticated, high-throughput system (as much as you can achieve in a day and a half...).

<div class="pt-12">
  <span @click="$slidev.nav.next" class="px-2 py-1 rounded cursor-pointer" hover:bg="white hover:bg-opacity-10">
    Go To Presentation <carbon:arrow-right class="inline"/>
  </span>
</div>

<div class="abs-br m-6 flex gap-2">
  <a href="https://github.com/zama-ai/zama-oracle" target="_blank" alt="GitHub" title="Open in GitHub"
    class="text-xl slidev-icon-btn opacity-50 !border-none !hover:text-white">
    <carbon-logo-github />
  </a>
</div>

<!--
This presentation chronicles the real-world evolution of the ZamaOracle system, showing how architectural decisions evolved from a simple MVP to a sophisticated, production-ready system capable of handling high throughput with modern Ethereum features.
-->

---
transition: fade-out
---

# What is a VRF Oracle?

A **Verifiable Random Function (VRF) Oracle** provides cryptographically secure and verifiable randomness to smart contracts.

<div class="grid grid-cols-2 gap-8 mt-8">

<div>

### The Challenge

- Blockchains are deterministic.
- `block.timestamp` and `block.hash` are not secure sources of randomness.
- On-chain games, NFTs, and protocols need unpredictable outcomes.

</div>

<div>

### The Solution

An **Oracle**: a trusted off-chain system that listens for on-chain requests, computes a value (in this case, randomness), and securely delivers it back on-chain.

</div>

</div>

<div class="mt-8 text-center text-gray-400">
How to build such a system from the ground up.
</div>

<!--
VRF Oracles are critical infrastructure for blockchain applications that need secure randomness. The challenge is building a system that's both cryptographically secure and operationally reliable, scalable, and fast.
-->

---
transition: slide-up
---

# The Evolution Timeline

Four major iterations that transformed a simple indexer into a something that deserves the name "oracle".

*The real complexity: C++ toolchain on Mac...*

<div class="timeline-container mt-8">

```mermaid {theme: 'neutral', scale: 0.5}
timeline
    title ZamaOracle Evolution
    section MVP
        Phase 1 : Simple Event Indexer
                  : `Rindexer` + PostgreSQL
                  : **Problem**: Listens but doesn't act.
    section Resilience
        Phase 2 : Durable Queue
                  : Added `queue_processor`
                  : **Problem**: Fails on restart, no retries.
        Phase 3 : Multi-Account Relayer
                  : `relayer` module with scheduling
                  : **Problem**: Stuck nonces, single point of failure.
    section Performance
        Phase 4 : Account Abstraction
                  : EIP-7702 + ERC-7821 Batching
                  : **Problem**: High gas costs, low throughput.
```

</div>

<!--
This timeline shows the four major evolutionary phases of the oracle system, each solving critical production challenges that emerged as the system scaled.
-->

---
layout: section
background: https://images.unsplash.com/photo-1551288049-bebda4e38f71?ixlib=rb-4.0.3&auto=format&fit=crop&w=2340&q=80
---

# Phase 1: The Genesis
## The Minimum Viable Product: An Event Indexer

<div class="text-center mt-16">
  <div class="text-6xl mb-4">🌱</div>
  <div class="text-xl opacity-80">Just listening.</div>
</div>

<!--
Every great system starts with an MVP. The ZamaOracle began as a simple event indexer - the foundation that would eventually support a production-grade oracle system.
-->

---
layout: two-cols
---

# MVP Architecture

The initial version is a one-way street: from the chain to the database.

**Core Components:**
- **`VRFOracle.sol`**: A smart contract that emits a `RandomnessRequested` event.
- **`Rindexer` Library**: An off-chain service that listens for these events.
- **PostgreSQL**: A database to store the event data.

---
layout: two-cols
---

**The Workflow:**
1. User calls `requestRandomness()` on the smart contract.
2. The contract emits a `RandomnessRequested` event.
3. The `Rindexer` service catches the event.
4. The event handler saves the request details into a PostgreSQL table.

::right::

```mermaid {scale: 0.3}
flowchart LR
    subgraph "On-Chain"
        A["`VRFOracle.sol`"]
    end

    subgraph "Off-Chain"
        B["`Event Indexer (Rindexer)`"]
        C["`Database (PostgreSQL)`"]
    end

    User -- "requests randomness" --> A
    A -- "emits \`RandomnessRequested\`" --> B
    B -- "stores event data" --> C
```

<div class="mt-8">

**Limitations of the MVP:**
- ❌ **No Fulfillment**: The oracle only logs requests; it never fulfills them.
- ❌ **No Action**: It's a passive listener, not an active participant.
- ✅ **Foundation**: It establishes a link between the on-chain and off-chain worlds.

</div>

---
layout: section
background: https://images.unsplash.com/photo-1518314916381-77a37c2a49ae?ixlib=rb-4.0.3&auto=format&fit=crop&w=2340&q=80
---

# Phase 2: Building Resilience
## From Fragile Script to Durable System

<div class="text-center mt-16">
  <div class="text-6xl mb-4">🛡️</div>
  <div class="text-xl opacity-80">Making it robust.</div>
</div>

---
layout: two-cols-header
---

## The Problem: A Brittle System

A naive fulfillment implementation (`event -> fulfill transaction`) is doomed to fail.

<div class="grid grid-cols-2 gap-4 mt-8">
<div>

### No State Persistence
If the oracle service crashes or restarts, any requests that were being processed are lost forever.

```mermaid {scale: 0.45}
graph TD
    A[Event Received] --> B{Process Request}
    B --> C{Send Tx}
    C --> D[Crash! 💥]
    E[Request Lost]
```

</div>

<div>

### The Nonce Nightmare
If a transaction gets stuck (e.g., low gas), all subsequent transactions from that account will fail until the stuck one is mined or replaced. The entire oracle grinds to a halt.

```mermaid {scale: 0.45}
sequenceDiagram
    participant O as Oracle
    participant N as Node
    O->>N: Send Tx (nonce 5, low gas)
    O->>N: Send Tx (nonce 6)
    N-->>O: Reject (nonce gap)
    O->>N: Send Tx (nonce 7)
    N-->>O: Reject (nonce gap)
```

</div>
</div>

---

# Solution 1: The Durable Queue

We introduce a **persistent queue** using PostgreSQL. This decouples event indexing from request processing, forming the backbone of our oracle's reliability.

**New Component: `QueueProcessor`**
- A dedicated service that polls the `pending_requests` table in the database.

<div class="grid grid-cols-2 gap-8 mt-4">
<div>

```mermaid {scale: 0.45}
flowchart LR
    B["`Event Indexer`"] -- "enqueues" --> C["\`pending_requests\` table"]
    D["`Queue Processor`"] -- "dequeues" --> C
```

**Benefits:**
- **Reliability**: Requests survive service restarts.
- **Scalability**: Multiple queue processors can run in parallel.
- **Retry Logic**: Failed requests can be re-queued and retried.
</div>


<div>

**The New Workflow:**
```mermaid
flowchart TD
    A[Event Received] --> B[Indexer enqueues request in DB]
    subgraph " "
        direction LR
        C(Queue Processor 1) -- "dequeues job" --> D[Database]
        E(Queue Processor 2) -- "dequeues job" --> D
    end
    B --> D
    C --> F[Fulfill Request]
    E --> G[Fulfill Request]
```

</div>
</div>

---

# Solution 2: The Multi-Account Relayer

To solve the "stuck nonce" problem and increase throughput, we introduce a `Relayer` module that manages a pool of EOA (Externally Owned Account) wallets.

**New Component: `Relayer`**
- Manages a list of private keys.
- Implements scheduling strategies (`RoundRobin`) to pick an account for each transaction.
- Monitors account health:
  - Is the gas balance sufficient?
  - Are there too many pending transactions?

---

<div class="grid grid-cols-2 gap-8 mt-4">

<div>

**How it Works:**
1. The `QueueProcessor` needs to send a transaction.
2. It asks the `Relayer` for an `next_available()` account.
3. The `Scheduler` picks an account (e.g., round-robin).
4. It checks if the account is healthy (gas, pending txs).
5. If yes, it returns the account. If no, it tries the next one.

</div>

<div>

```mermaid {scale: 0.55}
graph TD
    subgraph Relayer
        direction LR
        A[Queue Processor] --> B{Scheduler}
        B --> C[Account 1]
        B --> D[Account 2]
        B --> E[Account 3]
    end

    C --> F{Healthy?}
    D --> G{Healthy?}
    E --> H{Healthy?}
    F -- Yes --> I[Use for Tx]
    G -- No --> J[Skip]
    H -- Yes --> K[Use for Tx]
```
**This helps mitigate the single-point-of-failure from a stuck nonce... but what if we run out of accounts?**

</div>

</div>

---
layout: full
---

# Architecture After Phase 2

With a durable queue and a multi-account relayer, we're in the right path... but can we do better?

```mermaid
flowchart LR
    subgraph "On-Chain"
        A["`VRFOracle.sol`"]
    end

    subgraph "Off-Chain Services"
        B["`Event Indexer (Rindexer)`"]
        C["`Queue Database (PostgreSQL)`"]
        D["`Queue Processor`"]
        E["`Multi-Account Relayer`"]
    end

    User -- "requestRandomness()" --> A
    A -- "emits \`RandomnessRequested\`" --> B
    B -- "enqueues request" --> C
    D -- "dequeues request" --> C
    D -- "gets relayer account" --> E
    E -- "sends \`fulfillRandomness\` tx" --> A
```

**State of the System:**
- ✅ **Reliable**: Requests are not lost (so long as they're indexed!)
- ✅ **Resilient**: Recovers from crashes (the indexer can index blocks uncovered in crash, and the DB holds the requests fulfilled and pending)
- ⚠️ **Scalability Bottleneck**: Throughput is limited by `1 transaction = 1 fulfillment`.
- ⚠️ **High Gas Costs**: Each fulfillment requires a separate transaction, which is expensive.

---
layout: section
background: https://images.unsplash.com/photo-1639755243942-d363554a7fabe?ixlib=rb-4.0.3&auto=format&fit=crop&w=2340&q=80
---

# Phase 3: The Performance Leap
## Batching with Account Abstraction

<div class="text-center mt-16">
  <div class="text-6xl mb-4">🚀</div>
  <div class="text-xl opacity-80">Doing more with less.</div>
</div>

---
layout: two-cols-header
---

## The Problem: Gas Fees and Throughput Limits

The resilient architecture works, but it's slow and expensive at scale.

<div class="grid grid-cols-2 gap-8 mt-8">

<div>

### The Cost Barrier
Every single randomness request requires a separate `fulfillRandomness` transaction. On Ethereum mainnet, this could cost several dollars per request, making the service unviable for many applications.

- **10 Requests = 10 Transactions = 10x Gas Cost**

</div>

<div>

### The Speed Limit
A blockchain can only process a certain number of transactions per second (TPS). Our oracle's throughput is directly limited by this, as each fulfillment consumes one of these valuable transaction slots.

</div>

</div>

<div class="text-center mt-16 p-4 bg-red-500/10 rounded">
This model doesn't scale for high-demand use cases like on-chain gaming.
</div>

---

# The Solution: EIP-7702 + ERC-7821

We leverage modern Ethereum features to batch multiple fulfillments into a single transaction.

- **EIP-7702 | SetCode Transactions **: A new transaction type that allows an Externally Owned Account (EOA) to temporarily act like a smart contract for a single transaction. It lets us add "code" to a normal wallet address.
- **ERC-7821 (`Basic EOA Batch Executor (BEBE)`)**: A standard for a minimal contract that can execute a batch of calls (`execute(calls[])`) on behalf of an EOA, using the authorization from EIP-7702.

---

<div class="grid grid-cols-2 gap-8 mt-8">

<div>

**The Old Way (1 Tx per fulfillment):**
```mermaid
sequenceDiagram
    participant R as Relayer
    participant V as VRFOracle
    R->>V: fulfill(req1)
    R->>V: fulfill(req2)
    R->>V: fulfill(req3)
```
**Total: 3 transactions**

</div>

<div>

**The New Way (1 Tx for MANY fulfillments):**
```mermaid
sequenceDiagram
    participant R as Relayer
    participant B as BEBE
    participant V as VRFOracle
    R->>B: execute([fulfill(req1), fulfill(req2), fulfill(req3)])
    B->>V: fulfill(req1)
    B->>V: fulfill(req2)
    B->>V: fulfill(req3)
```
**Total: 1 transaction**

</div>

</div>

---
layout: two-cols
---

# The "Smart Batching" Strategy

<div class="text-sm">

The `QueueProcessor` is upgraded to be batch-aware.

1.  **Check for Work**: The processor wakes up and checks the number of pending requests in the queue.
2.  **Get a Relayer**: It secures an available relayer account.
3.  **Dequeue in Bulk**: It dequeues up to `BATCH_SIZE` (e.g., 100) requests from the database at once.
4.  **Build the Batch**: The `oracle` module generates a random value for each request and creates an array of `fulfillRandomness` calls.
5.  **Encode & Send**: This array is encoded and passed to the relayer's `send_batch` function, which executes it via the `BEBE` contract in a single EIP-7702 transaction.
6.  **Update Status**: Upon success, all requests in the batch are marked as `fulfilled` in the database. If the transaction fails, they are all marked for retry.

</div>

::right::

```mermaid {scale: 0.5}
flowchart TD
    A[Queue has 150 pending requests] --> B{Get Available Relayer};
    B --> C[Dequeue 100 requests];
    C --> D[Build array of 100 `fulfill` calls];
    D --> E[Send 1 transaction via BEBE];
    E --> F[100 requests fulfilled on-chain];
    F --> G[Mark 100 requests as 'fulfilled' in DB];
    G --> H[Loop back for remaining 50];
```

<div class="mt-8 text-center text-lg">
This results in a **~90% reduction in gas fees** per request** (pay intrinsic gas once -> only pay for SSTOREs)
</div>

---
layout: center
class: text-center
---

# Final Architecture

The complete, high-performance, and resilient Oracle system.

```mermaid
flowchart LR
    subgraph "On-Chain"
        A["`VRFOracle.sol`"]
        BEBE["`BEBE (ERC-7821)`"]
    end

    subgraph "Off-Chain Services"
        B["`Event Indexer`"]
        C["`DB Queue`"]
        D["`Queue Processor`"]
        E["`Multi-Account Relayer`"]
        F["`Oracle Module`"]
    end

    User --> A
    A -- "emits Event" --> B
    B -- "enqueues" --> C
    D -- "dequeues BATCH" --> C
    D -- "builds batch" --> F
    F -- "gets relayer" --> E
    E -- "sends single TX via" --> BEBE
    BEBE -- "executes batch on" --> A
```

---
layout: section
---

# Conclusion & Key Takeaways

We iteratively built this oracle system by solving concrete problems at each stage, in the order of importance.

1.  **Start Simple**: Begin with a basic, linear process to validate the core idea and get it running.
2.  **Build for Resilience**: Introduce durable queues and redundancy (multi-account relayers) to handle real-world failures like crashes and network congestion.
3.  **Optimize for Performance**: Once the system is reliable, focus on efficiency. Leverage modern protocol features (like EIP-7702) to drastically reduce costs and increase scale.

---
layout: two-cols
---

# What's Next?

- Automated Relayer System (funding, monitoring, etc)
- Reorg management - current system is not resilient to reorgs. We could simply add a re-indexing hook once reorgs are detected - which should be proposed by any serious indexing infrastructure.
- Properly add `onlyPublisher` checks on the oracle's `fulfillRandomness` entrypoint.
  - Naive way: Check inclusion in a whitelist - 🙅but all relayers must be whitelisted!
  - ECRecover a signed message, which always comes from the same EOA.
- Test coverage 🤠
---
layout: center
class: text-center
---

# Thank You ✨

**Q&A**

<br>
<br>
