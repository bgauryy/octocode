#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptsDir = dirname(fileURLToPath(import.meta.url));
const skillRoot = dirname(scriptsDir);
const args = new Set(process.argv.slice(2));
const checkOnly = args.has("--check-only");
const skipDeps = args.has("--skip-deps");

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
    if (ok(candidate, ["--version"])) {
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
  if (!ok("node", ["--version"])) {
    fail("Node.js is required for scripts/schema.mjs.");
  }
  if (!ok("python3", ["--version"])) {
    fail("python3 is required for scripts/awareness.py.");
  }
  if (!ok("python3", ["-c", "import sqlite3"])) {
    fail("Python sqlite3 module is required for scripts/awareness.py.");
  }
}

function schemaWorks() {
  return ok("node", [join(scriptsDir, "schema.mjs"), "list"]);
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
  const schema = run("node", [join(scriptsDir, "schema.mjs"), "example", "tell_memory"], {
    cwd: scriptsDir,
    capture: true,
  });
  if (schema.status !== 0) {
    fail("schema example smoke check failed.", { stderr: schema.stderr });
  }

  const validate = spawnSync(
    "node",
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

  const awareness = run("python3", [join(scriptsDir, "awareness.py"), "self-test"], {
    cwd: skillRoot,
    capture: true,
  });
  if (awareness.status !== 0) {
    fail("awareness.py self-test failed.", {
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
      dependencyResult,
      commands: {
        schema: "node scripts/schema.mjs list",
        awareness: "python3 scripts/awareness.py status",
      },
    },
    null,
    2,
  ),
);
