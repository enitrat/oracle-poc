import {
  createPublicClient,
  createWalletClient,
  http,
  parseEther,
  parseAbiItem,
  decodeEventLog,
  defineChain,
} from "viem";
import { privateKeyToAccount } from "viem/accounts";
// import { vrfOracleAbi } from "../src/generated";
import { readFileSync } from "fs";
import {
  ABI,
  ANVIL_URL,
  USER_PRIVATE_KEY,
  anvilChain,
  publicClient,
} from "./utils";

// Track request results
interface RequestResult {
  requestId: string;
  fulfilled: boolean;
  timestamp: number;
  batchId: number;
}

const results: RequestResult[] = [];
let currentBatchId = 0;

const main = async (
  requestIndex: number,
  nonce: number,
  batchId: number,
): Promise<RequestResult> => {
  const contractAddress = process.env.CONTRACT_ADDRESS;
  if (!contractAddress) {
    throw new Error("No contract address set in environment.");
  }

  // A user requests randomness
  const userClient = createWalletClient({
    account: privateKeyToAccount(USER_PRIVATE_KEY),
    chain: anvilChain,
    transport: http(ANVIL_URL),
  });

  const timestamp = Date.now();

  try {
    // Get fee
    const fee = await publicClient.readContract({
      address: contractAddress as `0x${string}`,
      abi: ABI,
      functionName: "fee",
    });

    // Request randomness
    const requestTx = await userClient.writeContract({
      address: contractAddress as `0x${string}`,
      abi: ABI,
      functionName: "requestRandomness",
      value: fee,
      nonce: nonce,
    });

    const requestReceipt = await publicClient.waitForTransactionReceipt({
      hash: requestTx,
    });

    // Find the RandomnessRequested event
    const requestEvent = requestReceipt.logs.find((log) => {
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
        return decoded.eventName === "RandomnessRequested";
      } catch {
        return false;
      }
    });

    if (!requestEvent) {
      throw new Error("RandomnessRequested event not found");
    }

    const decodedRequest = decodeEventLog({
      abi: [
        parseAbiItem(
          "event RandomnessRequested(bytes32 indexed requestId, address indexed requester, uint256 paid)",
        ),
      ],
      data: requestEvent.data,
      topics: requestEvent.topics as [`0x${string}`, ...`0x${string}`[]],
    });

    const requestId = decodedRequest.args.requestId;

    // Wait 1 second for processing
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Check result
    const result = await publicClient.readContract({
      address: contractAddress as `0x${string}`,
      abi: ABI,
      functionName: "getRandomness",
      args: [requestId],
    });

    const [fulfilled, fetchedRandomness] = result as [boolean, bigint];

    return {
      requestId: requestId as string,
      fulfilled: fulfilled && fetchedRandomness !== 0n,
      timestamp,
      batchId,
    };
  } catch (error) {
    console.error(`Error in request #${requestIndex + 1}:`, error);
    return {
      requestId: "error",
      fulfilled: false,
      timestamp,
      batchId,
    };
  }
};

// Process a batch asynchronously
const processBatch = async (
  batchId: number,
  batchSize: number,
  startingNonce: number,
) => {
  const batchPromises: Promise<RequestResult>[] = [];

  for (let i = 0; i < batchSize; i++) {
    const promise = main(i, startingNonce + i, batchId);
    batchPromises.push(promise);
  }

  // Wait for batch to complete
  const batchResults = await Promise.allSettled(batchPromises);

  // Process results
  const processedResults: RequestResult[] = [];
  batchResults.forEach((result) => {
    if (result.status === "fulfilled") {
      results.push(result.value);
      processedResults.push(result.value);
    } else {
      const errorResult = {
        requestId: "error",
        fulfilled: false,
        timestamp: Date.now(),
        batchId: batchId,
      };
      results.push(errorResult);
      processedResults.push(errorResult);
    }
  });

  // Report analytics for this batch
  const fulfilled = processedResults.filter((r) => r.fulfilled).length;
  const total = processedResults.length;
  const successRate =
    total > 0 ? ((fulfilled / total) * 100).toFixed(1) : "0.0";

  console.log(
    `📊 Batch #${batchId}: ${fulfilled}/${total} requests fulfilled (${successRate}%)`,
  );
};

const runContinuousRequests = async (): Promise<void> => {
  console.log("🚀 Starting continuous randomness requests...\n");

  // Get the current nonce for the user account
  const userAccount = privateKeyToAccount(USER_PRIVATE_KEY);
  let currentNonce = await publicClient.getTransactionCount({
    address: userAccount.address,
  });

  console.log(`📋 Starting nonce: ${currentNonce}`);

  // Fire batches every second
  setInterval(() => {
    // Random batch size between 5 and 30
    const batchSize = Math.floor(Math.random() * 55) + 5;
    currentBatchId++;

    console.log(
      `\n🚀 Firing batch #${currentBatchId} with ${batchSize} requests`,
    );

    // Process batch asynchronously (don't await)
    processBatch(currentBatchId, batchSize, currentNonce);

    // Update nonce for next batch
    currentNonce += batchSize;

    // Clean up old results (keep last 2 minutes)
    const cutoff = Date.now() - 120000;
    const oldLength = results.length;
    results.splice(
      0,
      results.findIndex((r) => r.timestamp > cutoff),
    );
    if (results.length < oldLength) {
      console.log(`  Cleaned up ${oldLength - results.length} old results`);
    }
  }, 1000);
};

// Start the continuous requests
runContinuousRequests().catch(console.error);
