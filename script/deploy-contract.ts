#!/usr/bin/env node
import { readFileSync, writeFileSync } from "fs";
import { authorizeDelegation, deployBebe, deployContract } from "./utils";

async function deployOracle() {
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
  writeFileSync(".env", envContent.join("\n"));
}

async function setupEoaDelegation() {
  const bebeAddress = await deployBebe();
  if (!bebeAddress) {
    throw new Error("Failed to deploy Bebe");
  }

  // Save BEBE address to .env
  const envContent = readFileSync(".env", "utf8").split("\n");
  const bebeAddressIndex = envContent.findIndex((line) =>
    line.startsWith("BEBE_ADDRESS="),
  );

  if (bebeAddressIndex !== -1) {
    envContent[bebeAddressIndex] = `BEBE_ADDRESS=${bebeAddress}`;
  } else {
    envContent.push(`BEBE_ADDRESS=${bebeAddress}`);
  }
  writeFileSync(".env", envContent.join("\n"));

  await authorizeDelegation(bebeAddress);
}

async function main() {
  await deployOracle();
  await setupEoaDelegation();
}

main();
