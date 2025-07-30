import {
  createPublicClient,
  createWalletClient,
  http,
  parseEther,
  parseAbiItem,
  decodeEventLog,
  defineChain,
  encodeFunctionData,
} from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { erc7821Actions } from "viem/experimental";

// import { vrfOracleAbi } from "../src/generated";
import { readFileSync } from "fs";
import {
  ABI,
  ANVIL_URL,
  USER_PRIVATE_KEY,
  anvilChain,
  publicClient,
} from "./utils";

// ==================== Types ====================
interface RequestResult {
  requestId: string;
  fulfilled: boolean;
  timestamp: number;
  batchId: number;
}

interface BatchResult {
  batchId: number;
  requestIds: string[];
  fulfilled: number;
  total: number;
  successRate: number;
  timestamp: number;
}

// ==================== Configuration ====================
const BATCH_SIZE = 25;
const BATCH_INTERVAL_MS = 1000;
const RESULT_CLEANUP_INTERVAL_MS = 120000; // 2 minutes

// ==================== State Management ====================
const results: RequestResult[] = [];
const batchResults: BatchResult[] = [];
let currentBatchId = 0;
let isRunning = false;

// ==================== Client Setup ====================
function createClients() {
  const userAccount = privateKeyToAccount(USER_PRIVATE_KEY);

  const walletClient = createWalletClient({
    account: userAccount,
    chain: anvilChain,
    transport: http(ANVIL_URL),
  }).extend(erc7821Actions());

  return { walletClient, userAccount };
}

// ==================== Contract Interactions ====================
async function getOracleFee(contractAddress: string): Promise<bigint> {
  return await publicClient.readContract({
    address: contractAddress as `0x${string}`,
    abi: ABI,
    functionName: "fee",
  });
}

async function checkRandomnessResult(
  contractAddress: string,
  requestId: string,
): Promise<[boolean, bigint]> {
  const result = await publicClient.readContract({
    address: contractAddress as `0x${string}`,
    abi: ABI,
    functionName: "getRandomness",
    args: [requestId],
  });
  return result as [boolean, bigint];
}

// ==================== Event Processing ====================
function extractRandomnessRequestedEvents(receipt: any): string[] {
  const requestIds: string[] = [];

  for (const log of receipt.logs) {
    try {
      const decoded = decodeEventLog({
        abi: [
          parseAbiItem(
            "event RandomnessRequested(bytes32 indexed requestId, address indexed requester, uint256 paid)",
          ),
        ],
        data: log.data,
        topics: log.topics as [`0x${string}`, ...`0x${string}`[]],
      });

      if (decoded.eventName === "RandomnessRequested") {
        requestIds.push(decoded.args.requestId as string);
      }
    } catch {
      // Skip logs that don't match our event
    }
  }

  return requestIds;
}

// ==================== Batch Processing ====================
async function sendBatchRequest(
  contractAddress: string,
  batchSize: number = BATCH_SIZE,
  batchId: number,
): Promise<BatchResult> {
  const { walletClient, userAccount } = createClients();
  const fee = await getOracleFee(contractAddress);

  // Prepare batch calls
  const requestCalls = Array.from({ length: batchSize }, () => ({
    to: contractAddress as `0x${string}`,
    abi: ABI,
    functionName: "requestRandomness",
    value: fee,
  }));
  console.log(`✅ Batch #${batchId}: Sending ${batchSize} requests`);

  const timestamp = Date.now();

  try {
    // Send batch transaction
    const requestTx = await walletClient.execute({
      address: userAccount.address,
      calls: requestCalls,
    });

    // Wait for receipt
    const requestReceipt = await publicClient.waitForTransactionReceipt({
      hash: requestTx,
    });

    if (requestReceipt.status !== "success") {
      throw new Error("Batch transaction failed");
    }

    // Extract all request IDs from events
    const requestIds = extractRandomnessRequestedEvents(requestReceipt);

    // Wait for oracle processing
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Check fulfillment for each request
    const fulfillmentResults = await Promise.all(
      requestIds.map(async (requestId) => {
        const [fulfilled, randomness] = await checkRandomnessResult(
          contractAddress,
          requestId,
        );
        return {
          requestId,
          fulfilled: fulfilled && randomness !== 0n,
          timestamp,
          batchId: batchId,
        };
      }),
    );

    // Store individual results
    results.push(...fulfillmentResults);

    // Calculate batch statistics
    const fulfilledCount = fulfillmentResults.filter((r) => r.fulfilled).length;
    const batchResult: BatchResult = {
      batchId: batchId,
      requestIds,
      fulfilled: fulfilledCount,
      total: requestIds.length,
      successRate: (fulfilledCount / requestIds.length) * 100,
      timestamp,
    };

    batchResults.push(batchResult);

    console.log(
      `📊 Batch #${batchId}: ${fulfilledCount}/${requestIds.length} fulfilled (${batchResult.successRate.toFixed(1)}%)`,
    );

    return batchResult;
  } catch (error) {
    console.error(`❌ Batch #${batchId} failed:`, error);

    const batchResult: BatchResult = {
      batchId: batchId,
      requestIds: [],
      fulfilled: 0,
      total: batchSize,
      successRate: 0,
      timestamp,
    };

    batchResults.push(batchResult);
    return batchResult;
  }
}

// ==================== Result Management ====================
function cleanupOldResults(cutoffTime: number) {
  const oldResultCount = results.length;
  const oldBatchCount = batchResults.length;

  // Clean individual results
  const resultIndex = results.findIndex((r) => r.timestamp > cutoffTime);
  if (resultIndex > 0) {
    results.splice(0, resultIndex);
  }

  // Clean batch results
  const batchIndex = batchResults.findIndex((r) => r.timestamp > cutoffTime);
  if (batchIndex > 0) {
    batchResults.splice(0, batchIndex);
  }

  const removedResults = oldResultCount - results.length;
  const removedBatches = oldBatchCount - batchResults.length;

  if (removedResults > 0 || removedBatches > 0) {
    console.log(
      `🧹 Cleaned up ${removedResults} results and ${removedBatches} batch records`,
    );
  }
}

// ==================== Analytics ====================
function printOverallStatistics() {
  if (batchResults.length === 0) return;

  const totalRequests = batchResults.reduce(
    (sum, batch) => sum + batch.total,
    0,
  );
  const totalFulfilled = batchResults.reduce(
    (sum, batch) => sum + batch.fulfilled,
    0,
  );
  const overallSuccessRate = (totalFulfilled / totalRequests) * 100;

  console.log(`\n📈 Overall Statistics:`);
  console.log(`   Total batches: ${batchResults.length}`);
  console.log(`   Total requests: ${totalRequests}`);
  console.log(`   Total fulfilled: ${totalFulfilled}`);
  console.log(`   Overall success rate: ${overallSuccessRate.toFixed(1)}%\n`);
}

// ==================== Main Loop ====================
async function runContinuousRequests(contractAddress: string): Promise<void> {
  console.log("🚀 Starting continuous randomness requests...\n");
  console.log(`📋 Configuration:`);
  console.log(`   Batch size: ${BATCH_SIZE} requests`);
  console.log(`   Batch interval: ${BATCH_INTERVAL_MS}ms`);
  console.log(`   Contract: ${contractAddress}\n`);

  isRunning = true;

  // Main processing loop
  const intervalId = setInterval(async () => {
    if (!isRunning) {
      clearInterval(intervalId);
      return;
    }

    try {
      const randomizedBatchSize = Math.floor(Math.random() * BATCH_SIZE) + 1;
      currentBatchId++;
      sendBatchRequest(contractAddress, randomizedBatchSize, currentBatchId);

      // Periodic cleanup
      const cutoff = Date.now() - RESULT_CLEANUP_INTERVAL_MS;
      cleanupOldResults(cutoff);

      // Print statistics every 10 batches
      if (currentBatchId % 10 === 0) {
        printOverallStatistics();
      }
    } catch (error) {
      console.error("❌ Error in batch processing:", error);
    }
  }, BATCH_INTERVAL_MS);

  // Graceful shutdown handler
  process.on("SIGINT", () => {
    console.log("\n\n🛑 Shutting down...");
    isRunning = false;
    printOverallStatistics();
    process.exit(0);
  });
}

// ==================== Single Batch Mode ====================
async function runSingleBatch(
  contractAddress: string,
  batchSize: number = BATCH_SIZE,
): Promise<void> {
  console.log(`🚀 Sending single batch of ${batchSize} requests...\n`);

  currentBatchId++;
  const result = await sendBatchRequest(
    contractAddress,
    batchSize,
    currentBatchId,
  );

  console.log(`\n✅ Batch completed:`);
  console.log(`   Request IDs: ${result.requestIds.length}`);
  console.log(`   Fulfilled: ${result.fulfilled}/${result.total}`);
  console.log(`   Success rate: ${result.successRate.toFixed(1)}%`);
}

// ==================== Main Entry Point ====================
async function main() {
  const contractAddress = process.env.CONTRACT_ADDRESS;
  if (!contractAddress) {
    throw new Error("No contract address set in environment.");
  }

  // Check command line arguments
  const args = process.argv.slice(2);
  const mode = args[0] || "continuous";

  switch (mode) {
    case "continuous":
      await runContinuousRequests(contractAddress);
      break;
    case "single":
      const batchSize = parseInt(args[1]) || BATCH_SIZE;
      await runSingleBatch(contractAddress, batchSize);
      break;
    default:
      console.error(`Unknown mode: ${mode}. Use 'single' or 'continuous'`);
      process.exit(1);
  }
}

// Run the script
main().catch(console.error);
