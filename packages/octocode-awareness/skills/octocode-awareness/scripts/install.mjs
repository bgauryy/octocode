#!/usr/bin/env node
process.removeAllListeners("warning");
process.on("warning", (warning) => {
  if (warning?.name === "ExperimentalWarning" && String(warning?.message).includes("SQLite")) return;
  console.error(warning?.stack ?? String(warning));
});

import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptsDir = dirname(fileURLToPath(import.meta.url));
const skillRoot = dirname(scriptsDir);
const args = new Set(process.argv.slice(2));
const nodeBin = process.execPath;

function printHelp() {
  console.log(`Usage: node scripts/install.mjs [--help]

Check the octocode-awareness standalone runtime and print the host hook init flow.
The package bundles every runtime dependency. This script never installs packages
or writes Codex/Cursor/Claude hook config; preview and install hooks with
scripts/awareness.mjs after explicit user approval.

Options:
  --help, -h    Show this help.

Examples:
  node scripts/install.mjs
  node scripts/awareness.mjs hooks install --host codex --project-dir . --dry-run --compact
  node scripts/awareness.mjs hooks check --host claude --project-dir . --strict --compact`);
}

if (args.has("--help") || args.has("-h")) {
  printHelp();
  process.exit(0);
}

if (args.size > 0) {
  fail(`unknown option(s): ${[...args].join(", ")}`);
}

function run(command, commandArgs, options = {}) {
  const result = spawnSync(command, commandArgs, {
    cwd: options.cwd || scriptsDir,
    encoding: "utf8",
    stdio: options.capture ? "pipe" : "inherit",
  });
  return result;
}

function ok(command, commandArgs, options = {}) {
  return run(command, commandArgs, { ...options, capture: true }).status === 0;
}

function fail(message, details = {}) {
  console.error(JSON.stringify({ ok: false, error: message, ...details }, null, 2));
  process.exit(1);
}

function ensureRuntime() {
  if (!ok(nodeBin, ["-e", ""])) {
    fail("Node.js runtime is not executable for scripts/schema.mjs.", { node: nodeBin });
  }
  const sqliteProbe = [
    "process.removeAllListeners('warning');",
    "process.on('warning', (w) => { if (w?.name === 'ExperimentalWarning' && String(w?.message).includes('SQLite')) return; console.error(w?.stack ?? String(w)); });",
    "await import('node:sqlite');",
  ].join("\n");
  if (!ok(nodeBin, ["--input-type=module", "-e", sqliteProbe])) {
    fail("Node >=22 with node:sqlite is required.");
  }
}

function runSmokeChecks() {
  const schema = run(nodeBin, [join(scriptsDir, "schema.mjs"), "example", "tell_memory"], {
    cwd: scriptsDir,
    capture: true,
  });
  if (schema.status !== 0) {
    fail("schema example smoke check failed.", { stderr: schema.stderr });
  }

  const validate = spawnSync(
    nodeBin,
    [join(scriptsDir, "schema.mjs"), "validate", "tell_memory", "-"],
    {
      cwd: scriptsDir,
      input: schema.stdout,
      encoding: "utf8",
      stdio: ["pipe", "pipe", "pipe"],
    },
  );
  if (validate.status !== 0) {
    fail("schema validation smoke check failed.", {
      stdout: validate.stdout,
      stderr: validate.stderr,
    });
  }

  const awareness = run(nodeBin, [join(scriptsDir, "awareness.mjs"), "maintenance", "self-test"], {
    cwd: skillRoot,
    capture: true,
  });
  if (awareness.status !== 0) {
    fail("awareness.mjs maintenance self-test failed.", {
      stdout: awareness.stdout,
      stderr: awareness.stderr,
    });
  }
}

ensureRuntime();
runSmokeChecks();

console.log(
  JSON.stringify(
    {
      ok: true,
      skillRoot,
      scriptsDir,
      node: nodeBin,
      runtime: { dependencies: "bundled", writes: false },
      commands: {
        schema: `${nodeBin} scripts/schema.mjs list`,
        awareness: `${nodeBin} scripts/awareness.mjs workspace status`,
        init: `${nodeBin} scripts/awareness.mjs maintenance init --compact`,
        hooks_preview_claude: `${nodeBin} scripts/awareness.mjs hooks install --host claude --project-dir <repo> --dry-run --compact`,
        hooks_install_claude: `${nodeBin} scripts/awareness.mjs hooks install --host claude --project-dir <repo> --compact`,
        hooks_check_claude: `${nodeBin} scripts/awareness.mjs hooks check --host claude --project-dir <repo> --strict --compact`,
        hooks_preview_codex: `${nodeBin} scripts/awareness.mjs hooks install --host codex --project-dir <repo> --dry-run --compact`,
        hooks_install_codex: `${nodeBin} scripts/awareness.mjs hooks install --host codex --project-dir <repo> --compact`,
        hooks_check_codex: `${nodeBin} scripts/awareness.mjs hooks check --host codex --project-dir <repo> --strict --compact`,
        hooks_preview_cursor: `${nodeBin} scripts/awareness.mjs hooks install --host cursor --project-dir <repo> --dry-run --compact`,
        hooks_install_cursor: `${nodeBin} scripts/awareness.mjs hooks install --host cursor --project-dir <repo> --compact`,
        hooks_check_cursor: `${nodeBin} scripts/awareness.mjs hooks check --host cursor --project-dir <repo> --strict --compact`,
        pi_bridge: "import { wirePiAwarenessHooks } from '@octocodeai/octocode-awareness'; wirePiAwarenessHooks(pi, { skillRoot })",
      },
      next_steps: [
        "This package bundles the skill under dist/skills/octocode-awareness; install that local/bundled path with npx octocode skill --add --path <awareness-package>/dist/skills/octocode-awareness --platform common.",
        "Use npx octocode for skill install/update/lint and research/search operations; do not fetch octocode-awareness by registry name.",
        "Run maintenance init, then workspace status in each repo.",
        "For Claude, Codex, and Cursor project hooks: preview with --dry-run, install after user approval, then run hooks check --strict for that host.",
        "For Pi: do not run shell hook install; call wirePiAwarenessHooks(pi, { skillRoot }) or use @octocodeai/pi-extension, then smoke tool_call/tool_result and agent_end behavior.",
      ],
    },
    null,
    2,
  ),
);
