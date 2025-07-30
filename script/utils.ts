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
import { vrfOracleAbi } from "../out/generated";
import { delegationAbi } from "../out/generated";
import { readFileSync } from "fs";

export const anvilChain = /*#__PURE__*/ defineChain({
  id: 31_337,
  name: "anvilChain",
  nativeCurrency: {
    decimals: 18,
    name: "Ether",
    symbol: "ETH",
  },
  rpcUrls: {
    default: { http: ["http://127.0.0.1:8545"] },
  },
});

export const getContractBytecode = (path: string) => {
  const artifact = JSON.parse(readFileSync(path, "utf8"));
  return artifact.bytecode.object as `0x${string}`;
};

// Configuration
export const ANVIL_URL = "http://127.0.0.1:8545";
export const ABI = vrfOracleAbi;
export const ORACLE_ADDRESS = process.env.ORACLE_ADDRESS as `0x${string}`;
export const ORACLE_PRIVATE_KEY = process.env
  .ORACLE_PRIVATE_KEY as `0x${string}`;
export const USER_PRIVATE_KEY = process.env.USER_PRIVATE_KEY as `0x${string}`;

// Setup clients
export const publicClient = createPublicClient({
  chain: anvilChain,
  transport: http(ANVIL_URL),
});
export const deployerClient = createWalletClient({
  account: privateKeyToAccount(
    process.env.DEPLOYER_PRIVATE_KEY as `0x${string}`,
  ),
  chain: anvilChain,
  transport: http(anvilChain.rpcUrls.default.http[0]),
});

export const deployContract = async () => {
  // Deploy contract
  const FEE = parseEther("0.001");
  const contractBytecode = getContractBytecode("out/oracle.sol/VRFOracle.json");
  const deployTx = await deployerClient.deployContract({
    abi: vrfOracleAbi,
    bytecode: contractBytecode,
    args: [ORACLE_ADDRESS, FEE],
  });

  const receipt = await publicClient.waitForTransactionReceipt({
    hash: deployTx,
  });
  const contractAddress = receipt.contractAddress;

  console.log("🧪 Running VRF Oracle Integration Test\n");
  console.log(`Contract: ${contractAddress}\n`);
  return contractAddress;
};

export const deployBebe = async () => {
  const contractBytecode = getContractBytecode(
    "out/bebe.sol/BasicEOABatchExecutor.json",
  );
  const deployTx = await deployerClient.deployContract({
    abi: delegationAbi,
    bytecode: contractBytecode,
  });
  const receipt = await publicClient.waitForTransactionReceipt({
    hash: deployTx,
  });
  console.log("🧪 Deploying Bebe\n");
  console.log(`Contract: ${receipt.contractAddress}\n`);
  return receipt.contractAddress;
};

export const authorizeDelegation = async (contractAddress: `0x${string}`) => {
  const USER_PRIVATE_KEY = process.env.USER_PRIVATE_KEY as `0x${string}`;
  const eoa = privateKeyToAccount(USER_PRIVATE_KEY);
  const eoaClient = createWalletClient({
    account: eoa,
    chain: anvilChain,
    transport: http(anvilChain.rpcUrls.default.http[0]),
  });

  const authorization = await eoaClient.signAuthorization({
    contractAddress,
  });

  const hash = await deployerClient.sendTransaction({
    authorizationList: [authorization],
    to: contractAddress,
  });

  const receipt = await publicClient.waitForTransactionReceipt({
    hash,
  });
  if (receipt.status === "success") {
    console.log("🧪 Authorized Delegation of EOA to Bebe\n");
    console.log(`Receipt: ${receipt.transactionHash}\n`);
  } else {
    throw new Error("Failed to authorize delegation");
  }
};
