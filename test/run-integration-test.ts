#!/usr/bin/env node
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
import { vrfOracleAbi } from "../src/generated";
import { readFileSync } from "fs";
import {
  ABI,
  ANVIL_URL,
  ORACLE_ADDRESS,
  ORACLE_PRIVATE_KEY,
  USER_PRIVATE_KEY,
  anvilChain,
  deployContract,
  deployerClient,
  getContractBytecode,
  publicClient,
} from "../script/utils";

/**
 * VRF Oracle Integration Test Runner
 *
 * Prerequisites:
 * 1. Start Anvil: `anvil`
 * 2. Deploy contract: `forge create contracts/oracle.sol:VRFOracle --constructor-args <oracle_address> <fee> --private-key <key>`
 * 3. Run this test: `npx tsx contracts/test/run-integration-test.ts <contract_address>`
 */

async function main() {
  const contractAddress = await deployContract();
  const oracleClient = createWalletClient({
    account: privateKeyToAccount(ORACLE_PRIVATE_KEY),
    chain: anvilChain,
    transport: http(ANVIL_URL),
  });

  const userClient = createWalletClient({
    account: privateKeyToAccount(USER_PRIVATE_KEY),
    chain: anvilChain,
    transport: http(ANVIL_URL),
  });

  try {
    // Get fee
    const fee = await publicClient.readContract({
      address: contractAddress as `0x${string}`,
      abi: ABI,
      functionName: "fee",
    });

    console.log(`Fee: ${fee} wei (${Number(fee) / 1e18} ETH)`);

    // Step 1: User requests randomness
    console.log("\n1️⃣  Requesting randomness...");
    const requestTx = await userClient.writeContract({
      address: contractAddress as `0x${string}`,
      abi: ABI,
      functionName: "requestRandomness",
      value: fee,
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
          topics: log.topics as [string, ...string[]],
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
      topics: requestEvent.topics as [string, ...string[]],
    });

    const requestId = decodedRequest.args.requestId;
    console.log(`✅ Request ID: ${requestId}`);

    // Step 2: Oracle fulfills request
    console.log("\n2️⃣  Oracle fulfilling request...");
    const randomValue = BigInt(Math.floor(Math.random() * 1000000));

    const fulfillTx = await oracleClient.writeContract({
      address: contractAddress as `0x${string}`,
      abi: ABI,
      functionName: "fulfillRandomness",
      args: [requestId, randomValue],
    });

    await publicClient.waitForTransactionReceipt({ hash: fulfillTx });
    console.log(`✅ Fulfilled with random value: ${randomValue}`);

    // Step 3: Verify result
    console.log("\n3️⃣  Fetching result...");
    const result = await publicClient.readContract({
      address: contractAddress as `0x${string}`,
      abi: ABI,
      functionName: "getRandomness",
      args: [requestId],
    });

    const [fulfilled, fetchedRandomness] = result as [boolean, bigint];
    console.log(`✅ Fulfilled: ${fulfilled}`);
    console.log(`✅ Random value: ${fetchedRandomness}`);

    if (fulfilled && fetchedRandomness === randomValue) {
      console.log("\n✅ Test passed! VRF Oracle working correctly.");
    } else {
      console.log("\n❌ Test failed!");
    }
  } catch (error) {
    console.error("\n❌ Error:", error);
    process.exit(1);
  }
}

main();
