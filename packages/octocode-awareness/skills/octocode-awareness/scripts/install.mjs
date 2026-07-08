#!/usr/bin/env node
process.removeAllListeners("warning");
process.on("warning", (warning) => {
  if (warning?.name === "ExperimentalWarning" && String(warning?.message).includes("SQLite")) return;
  console.error(warning?.stack ?? String(warning));
});

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptsDir = dirname(fileURLToPath(import.meta.url));
const skillRoot = dirname(scriptsDir);
const args = new Set(process.argv.slice(2));
const checkOnly = args.has("--check-only");
const skipDeps = args.has("--skip-deps");
const nodeBin = process.execPath;

function printHelp() {
  console.log(`Usage: node scripts/install.mjs [--check-only] [--skip-deps] [--help]

Check the octocode-awareness standalone runtime and print the host hook init flow.
This script never writes Codex/Cursor/Claude hook config; preview and install hooks
with scripts/awareness.mjs after explicit user approval.

Options:
  --check-only  Do not install missing npm dependencies.
  --skip-deps   Skip npm dependency installation.
  --help, -h    Show this help.

Examples:
  node scripts/install.mjs --check-only
  node scripts/install.mjs
  node scripts/awareness.mjs hooks install --host codex --project-dir . --dry-run --compact`);
}

if (args.has("--help") || args.has("-h")) {
  printHelp();
  process.exit(0);
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

function findNpm() {
  const candidates = [
    "npm",
    join(dirname(process.execPath), "npm"),
    "/opt/homebrew/bin/npm",
    "/usr/local/bin/npm",
  ];
  for (const candidate of candidates) {
    if (ok(candidate, ["--help"])) {
      return candidate;
    }
  }
  return null;
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

function schemaWorks() {
  return ok(nodeBin, [join(scriptsDir, "schema.mjs"), "list"]);
}

function installDependencies() {
  if (schemaWorks()) {
    return { installed: false, reason: "zod already resolvable" };
  }

  if (skipDeps || checkOnly) {
    return { installed: false, reason: "dependency install skipped" };
  }

  if (!existsSync(join(scriptsDir, "package.json"))) {
    fail("scripts/package.json is missing; cannot install local schema dependencies.");
  }
  const npm = findNpm();
  if (!npm) {
    fail("npm is required to install local Zod dependency for standalone use.");
  }

  const result = run(npm, ["install", "--omit=dev", "--no-audit", "--no-fund"], {
    cwd: scriptsDir,
  });
  if (result.status !== 0) {
    fail("npm install failed for local script dependencies.", { exitCode: result.status });
  }
  if (!schemaWorks()) {
    fail("schema.mjs still cannot run after installing dependencies.");
  }
  return { installed: true, reason: "installed scripts/package.json dependencies" };
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
const dependencyResult = installDependencies();
runSmokeChecks();

console.log(
  JSON.stringify(
    {
      ok: true,
      skillRoot,
      scriptsDir,
      node: nodeBin,
      dependencyResult,
      commands: {
        schema: `${nodeBin} scripts/schema.mjs list`,
        awareness: `${nodeBin} scripts/awareness.mjs workspace status`,
        init: `${nodeBin} scripts/awareness.mjs maintenance init --compact`,
        hooks_preview_codex: `${nodeBin} scripts/awareness.mjs hooks install --host codex --project-dir <repo> --dry-run --compact`,
        hooks_install_codex: `${nodeBin} scripts/awareness.mjs hooks install --host codex --project-dir <repo> --compact`,
        hooks_check_codex: `${nodeBin} scripts/awareness.mjs hooks check --host codex --project-dir <repo> --strict --compact`,
        hooks_preview_cursor: `${nodeBin} scripts/awareness.mjs hooks install --host cursor --project-dir <repo> --dry-run --compact`,
        hooks_install_cursor: `${nodeBin} scripts/awareness.mjs hooks install --host cursor --project-dir <repo> --compact`,
      },
      next_steps: [
        "Install the skill with npx octocode skill --add --path <octocode-awareness> --platform common.",
        "Run maintenance init, then workspace status in each repo.",
        "For Codex or Cursor, SKILL.md frontmatter is not enough: preview hooks with --dry-run, install after user approval, then run hooks check --strict.",
      ],
    },
    null,
    2,
  ),
);
