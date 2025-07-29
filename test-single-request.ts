import { createWalletClient, createPublicClient, http, parseEther } from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { anvil } from "viem/chains";
import { readFileSync } from "fs";
import { resolve } from "path";

// Load contract ABI
const contractAbi = JSON.parse(
  readFileSync(resolve(__dirname, "out/oracle.sol/VRFOracle.json"), "utf-8"),
).abi;

// Setup
const account = privateKeyToAccount(
  process.env.USER_PRIVATE_KEY as `0x${string}`,
);
const contractAddress =
  "0xe7f1725e7734ce288f8367e1bb143e90bb3f0512" as `0x${string}`; // Use the CONTRACT_ADDRESS from .env

const walletClient = createWalletClient({
  account,
  chain: anvil,
  transport: http(process.env.RPC_URL || "http://127.0.0.1:8545"),
});

const publicClient = createPublicClient({
  chain: anvil,
  transport: http(process.env.RPC_URL || "http://127.0.0.1:8545"),
});

async function requestRandomness() {
  console.log("🎲 Requesting single randomness...");
  console.log(`Contract: ${contractAddress}`);

  // Request randomness
  const hash = await walletClient.writeContract({
    address: contractAddress,
    abi: contractAbi,
    functionName: "requestRandomness",
    value: parseEther("0.001"), // 0.001 ETH fee
  });

  console.log(`✅ Transaction sent: ${hash}`);

  // Wait for confirmation
  const receipt = await publicClient.waitForTransactionReceipt({ hash });
  console.log(`✅ Transaction confirmed in block ${receipt.blockNumber}`);

  // Get request ID from logs
  const requestId = receipt.logs[0].topics[1];
  console.log(`📋 Request ID: ${requestId}`);

  // Wait a bit for oracle to process
  console.log("⏳ Waiting 5 seconds for oracle to process...");
  await new Promise((resolve) => setTimeout(resolve, 5000));

  // Check if fulfilled
  const [fulfilled, randomValue] = await publicClient.readContract({
    address: contractAddress,
    abi: contractAbi,
    functionName: "getRandomness",
    args: [requestId],
  });

  console.log(`✅ Fulfilled: ${fulfilled}`);
  console.log(`🎲 Random value: ${randomValue}`);

  if (fulfilled) {
    console.log("✨ Success! Oracle fulfilled the request.");
  } else {
    console.log("❌ Oracle has not fulfilled the request yet.");
  }
}

requestRandomness().catch(console.error);
