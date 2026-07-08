#!/usr/bin/env node
// Compatibility entrypoint. Hook install/check/remove behavior lives in the
// package CLI so skills do not own operational script logic.
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const awarenessCli = join(scriptDir, "awareness.mjs");
const args = process.argv.slice(2);

if (args.includes("--help") || args.includes("-h")) {
  console.log(`usage: install-hooks [--host claude|codex|cursor] [--project-dir <path>] [--dry-run]

Compatibility wrapper for:
  octocode-awareness hooks install [options]

Run octocode-awareness hooks install --help for the full contract.`);
  process.exit(0);
}

const result = spawnSync(process.execPath, [awarenessCli, "hooks", "install", ...args], {
  stdio: "inherit",
});

process.exit(result.status ?? 1);
