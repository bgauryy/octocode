#!/usr/bin/env node
process.removeAllListeners("warning");
process.on("warning", (warning) => {
  if (warning?.name === "ExperimentalWarning" && String(warning?.message).includes("SQLite")) return;
  console.error(warning?.stack ?? String(warning));
});

import { spawnSync } from "node:child_process";
import { existsSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptsDir = dirname(fileURLToPath(import.meta.url));
const skillRoot = dirname(scriptsDir);
const bundledSkillsDir = dirname(skillRoot);
const args = new Set(process.argv.slice(2));
const compact = args.delete("--compact");
const nodeBin = process.execPath;
const quote = (value) => JSON.stringify(value);
const awarenessCommand = `${quote(nodeBin)} ${quote(join(scriptsDir, "awareness.mjs"))}`;
const schemaCommand = `${quote(nodeBin)} ${quote(join(scriptsDir, "schema.mjs"))}`;

// Discovered at runtime — a sibling of this skill's own folder — so this list
// can never silently drift from whatever build.mjs actually bundled here.
const REQUIRED_BUNDLED_SKILLS = new Set(["octocode-awareness"]);
function discoverBundledSkills(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && existsSync(join(dir, entry.name, "SKILL.md")))
    .map((entry) => ({
      name: entry.name,
      path: join(dir, entry.name),
      required: REQUIRED_BUNDLED_SKILLS.has(entry.name),
    }))
    .sort((a, b) => (a.required === b.required ? a.name.localeCompare(b.name) : a.required ? -1 : 1));
}
const bundledSkills = discoverBundledSkills(bundledSkillsDir);

function printHelp() {
  console.log(`Usage: node scripts/install.mjs [--compact] [--help]

Check the octocode-awareness standalone runtime and print the host hook init flow.
The package bundles every runtime dependency. This script never installs packages
or writes Codex/Cursor/Claude hook config; preview and install hooks with
scripts/awareness.mjs after explicit user approval.

Options:
  --compact     Print one bounded agent receipt.
  --help, -h    Show this help.

Examples:
  node scripts/install.mjs
  node scripts/awareness.mjs hooks install --host codex --project-dir . --dry-run --compact
  node scripts/awareness.mjs hooks check --host codex --project-dir . --strict --compact`);
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
    fail("Node >=22.13.0 with unflagged node:sqlite is required.");
  }
}

function runSmokeChecks() {
  const schema = run(nodeBin, [join(scriptsDir, "schema.mjs"), "example", "memory_record"], {
    cwd: scriptsDir,
    capture: true,
  });
  if (schema.status !== 0) {
    fail("schema example smoke check failed.", { stderr: schema.stderr });
  }

  const validate = spawnSync(
    nodeBin,
    [join(scriptsDir, "schema.mjs"), "validate", "memory_record", "-"],
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

if (compact) {
  console.log(JSON.stringify({
    ok: true,
    required_skills: bundledSkills.filter((skill) => skill.required).map((skill) => skill.name),
    optional_skill_count: bundledSkills.filter((skill) => !skill.required).length,
    next: "Run maintenance init once, then attend --compact.",
  }));
  process.exit(0);
}

console.log(
  JSON.stringify(
    {
      ok: true,
      skillRoot,
      scriptsDir,
      node: nodeBin,
      runtime: { dependencies: "bundled", writes: false },
      bundled_skills: bundledSkills,
      commands: {
        schema: `${schemaCommand} list`,
        awareness: `${awarenessCommand} workspace status --workspace "$PWD" --compact`,
        init: `${awarenessCommand} maintenance init --compact`,
        attend: `${awarenessCommand} attend --workspace "$PWD" --agent-id "$OCTOCODE_AGENT_ID" --compact`,
        hooks_preview_codex: `${awarenessCommand} hooks install --host codex --project-dir "$PWD" --dry-run --compact`,
        hooks_install_codex: `${awarenessCommand} hooks install --host codex --project-dir "$PWD" --compact`,
        hooks_check_codex: `${awarenessCommand} hooks check --host codex --project-dir "$PWD" --strict --compact`,
        hooks_preview_cursor: `${awarenessCommand} hooks install --host cursor --project-dir "$PWD" --dry-run --compact`,
        hooks_install_cursor: `${awarenessCommand} hooks install --host cursor --project-dir "$PWD" --compact`,
        hooks_check_cursor: `${awarenessCommand} hooks check --host cursor --project-dir "$PWD" --strict --compact`,
        pi_bridge: "import { wirePiAwarenessHooks } from '@octocodeai/octocode-awareness'; wirePiAwarenessHooks(pi, { skillRoot })",
      },
      next_steps: [
        `This package bundles ${bundledSkills.length} skill(s) under bundled_skills above (only octocode-awareness is required; the rest are optional); install any optional skill with npx octocode skill --add --path <bundled_skills[i].path> --platform common.`,
        "Use npx octocode for skill install/update/lint and research/search operations; do not fetch bundled skills by registry name — always install from the bundled path above.",
        "Export one stable OCTOCODE_AGENT_ID for the CLI and host hooks.",
        "Run maintenance init once for the store, then workspace status and attend from each repo.",
        "When Claude skill frontmatter is active, use it as the hook surface and do not also install duplicate project settings; hooks check inspects settings files only.",
        "For Codex and Cursor project hooks: preview with --dry-run, install after user approval, then run hooks check --strict for that host.",
        "For Pi: do not run shell hook install; call wirePiAwarenessHooks(pi, { skillRoot }) or use @octocodeai/pi-extension, then smoke tool_call/tool_result and agent_end behavior.",
      ],
    },
    null,
    2,
  ),
);
