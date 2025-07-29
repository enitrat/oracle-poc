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
  USER_PRIVATE_KEY,
  anvilChain,
  publicClient,
} from "./utils";

const main = async () => {
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

  try {
    // Get fee
    const fee = await publicClient.readContract({
      address: contractAddress as `0x${string}`,
      abi: ABI,
      functionName: "fee",
    });

    console.log(`Fee: ${fee} wei (${Number(fee) / 1e18} ETH)`);

    // Step 1: User requests randomness
    console.log(
      `\n1️⃣  Requesting randomness from contract ${contractAddress}...`,
    );
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

    // We wait 5secs for the request to be processed
    await new Promise((resolve) => setTimeout(resolve, 1000));

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

    if (fulfilled && fetchedRandomness !== 0n) {
      console.log("\n✅ Test passed! VRF Oracle working correctly.");
    } else {
      console.log("\n❌ The oracle did not provide a value in time!");
    }
  } catch (error) {}
};
main();
