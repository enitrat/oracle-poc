#!/usr/bin/env node
import { readFileSync, writeFileSync } from "fs";
import { deployContract } from "./utils";

async function main() {
  const contractAddress = await deployContract();
  // Save to .env CONTRACT_ADDRESS
  const envContent = readFileSync(".env", "utf8").split("\n");
  const contractAddressIndex = envContent.findIndex((line) =>
    line.startsWith("CONTRACT_ADDRESS="),
  );

  if (contractAddressIndex !== -1) {
    envContent[contractAddressIndex] = `CONTRACT_ADDRESS=${contractAddress}`;
  } else {
    envContent.push(`CONTRACT_ADDRESS=${contractAddress}`);
  }
}

main();
