import { createHash } from "node:crypto";
import { access, mkdir, readFile, rm } from "node:fs/promises";
import { spawn } from "node:child_process";
import { homedir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const MODULE_DIR = path.dirname(fileURLToPath(import.meta.url));
const PROJECT_DIR = path.resolve(MODULE_DIR, "..");
const CONFIG_PATH = path.join(PROJECT_DIR, "deployments/local.magicblock.json");
const ARTIFACT_PATH = path.join(PROJECT_DIR, "target/deploy/permutation_state_receipt.so");
const STORAGE_PATH = path.join(PROJECT_DIR, "magicblock-test-storage");
const LEDGER_PATH = path.resolve(PROJECT_DIR, "../../work/magicblock-local/ledger");
const SESSION_ROOT = path.resolve(PROJECT_DIR, "../../work/devnet");
const SESSION_PATH = path.join(SESSION_ROOT, "sessions");

const config = JSON.parse(await readFile(CONFIG_PATH, "utf8"));
if (config.cluster !== "localnet") throw new Error("The local stack launcher only accepts a localnet config");

async function rpcIsReachable(endpoint) {
  try {
    const response = await fetch(endpoint, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "getHealth", params: [] }),
      signal: AbortSignal.timeout(800),
    });
    return response.ok;
  } catch {
    return false;
  }
}

async function executableDirectory(binary, extraDirectories = []) {
  const directories = [
    ...(process.env.PATH || "").split(path.delimiter),
    ...extraDirectories,
  ].filter(Boolean);
  for (const directory of [...new Set(directories)]) {
    try {
      await access(path.join(directory, binary));
      return directory;
    } catch {
      // Continue searching without mutating any generated world data.
    }
  }
  return null;
}

if (await rpcIsReachable(config.baseHttp) || await rpcIsReachable(config.erHttp)) {
  throw new Error("A local MagicBlock validator is already running; stop it before requesting a reset");
}
await access(ARTIFACT_PATH);
const artifact = await readFile(ARTIFACT_PATH);
const artifactHash = createHash("sha256").update(artifact).digest("hex");
if (artifactHash !== config.programArtifactSha256) {
  throw new Error(`SBF artifact hash mismatch: expected ${config.programArtifactSha256}, received ${artifactHash}`);
}

const mbStackDirectory = await executableDirectory("mb-stack");
if (!mbStackDirectory) {
  throw new Error("mb-stack was not found. Install @magicblock-labs/ephemeral-validator and ensure it is on PATH");
}
const solanaDirectory = await executableDirectory("solana-test-validator", [
  path.join(homedir(), ".local/share/solana/install/active_release/bin"),
  path.join(homedir(), ".local/share/agave/install/active_release/bin"),
]);
if (!solanaDirectory) {
  throw new Error("solana-test-validator was not found. Install the pinned Solana/Agave CLI before resetting the local world");
}
const childPath = [...new Set([mbStackDirectory, solanaDirectory, process.env.PATH].filter(Boolean))]
  .join(path.delimiter);

// This directory is generated exclusively by the standalone ER process. A
// base-layer reset must never reuse it, or old delegated accounts can outlive
// the validator they came from. Keep the deletion target exact and local.
if (path.dirname(STORAGE_PATH) !== PROJECT_DIR || path.basename(STORAGE_PATH) !== "magicblock-test-storage") {
  throw new Error("Refusing to reset an unexpected MagicBlock storage path");
}
if (path.dirname(SESSION_PATH) !== SESSION_ROOT || path.basename(SESSION_PATH) !== "sessions") {
  throw new Error("Refusing to reset an unexpected gateway session path");
}
await rm(STORAGE_PATH, { recursive: true, force: true });
// These receipts index accounts in the dedicated ledger. Keeping them across
// an intentional validator reset would show events from a world that no longer
// exists, so reset only this exact generated sessions directory with it.
await rm(SESSION_PATH, { recursive: true, force: true });
await mkdir(LEDGER_PATH, { recursive: true });

const child = spawn("mb-stack", [
  "--bpf-program",
  config.programId,
  ARTIFACT_PATH,
  "--reset",
  "--ledger",
  LEDGER_PATH,
], {
  cwd: PROJECT_DIR,
  stdio: "inherit",
  env: { ...process.env, PATH: childPath },
});

child.once("error", (error) => {
  if (error.code === "ENOENT") {
    console.error("mb-stack was not found. Install @magicblock-labs/ephemeral-validator and ensure it is on PATH.");
  } else {
    console.error(error.message);
  }
  process.exitCode = 1;
});

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.once(signal, () => child.kill(signal));
}

child.once("exit", (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  else process.exitCode = code ?? 1;
});
