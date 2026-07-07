#!/usr/bin/env node
// Merge the octocode-awareness lifecycle hooks into a host's project/user
// hook config so awareness is session-wide (active even when the skill is not
// loaded).
// Idempotent and non-destructive: it never touches hooks other than its own.
// ALWAYS run only after the user has approved it.
//
// Usage:
//   node scripts/install-hooks.mjs [--host claude|codex] [--project-dir <path>]   install/merge
//   node scripts/install-hooks.mjs [--host claude|codex] --global                 install user-scope hooks
//   node scripts/install-hooks.mjs --check                  report status only
//   node scripts/install-hooks.mjs --dry-run                show result, don't write
//   node scripts/install-hooks.mjs --remove                 remove our hooks
import { readFileSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve, relative, isAbsolute, sep } from "node:path";
import { fileURLToPath } from "node:url";

const args = process.argv.slice(2);
const flag = (f) => args.includes(f);
const opt = (name, def) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : def;
};

function printHelp() {
  console.log(`Usage: node scripts/install-hooks.mjs [options]

Install, check, dry-run, or remove octocode-awareness lifecycle hooks.

Targets:
  --host claude         Write Claude Code hooks to .claude/settings.json (default).
  --host codex         Write Codex hooks to .codex/hooks.json.
  --claude             Alias for --host claude.
  --codex              Alias for --host codex.

Options:
  --project-dir <path>  Target a project hook file under <path> (default: cwd).
  --global              Target the user hook file with absolute hook paths.
  --check               Report whether the hooks are installed.
  --dry-run             Print the resulting settings without writing.
  --remove              Remove only octocode-awareness hooks.
  --help, -h            Show this help.
`);
}

function fail(message, extra = {}) {
  console.error(JSON.stringify({ ok: false, error: message, ...extra }, null, 2));
  process.exit(1);
}

if (flag("--help") || flag("-h")) {
  printHelp();
  process.exit(0);
}
if (flag("--global") && args.includes("--project-dir")) {
  fail("use either --global or --project-dir, not both");
}

const requestedHost = flag("--codex")
  ? "codex"
  : flag("--claude")
    ? "claude"
    : opt("--host", "claude");
const host = String(requestedHost).toLowerCase();
if (host !== "claude" && host !== "codex") {
  fail("invalid --host; expected claude or codex", { host: requestedHost });
}

const globalMode = flag("--global");
const projectDir = resolve(opt("--project-dir", process.cwd()));
const settingsPath = globalMode
  ? join(homedir(), host === "codex" ? ".codex" : ".claude", host === "codex" ? "hooks.json" : "settings.json")
  : join(projectDir, host === "codex" ? ".codex" : ".claude", host === "codex" ? "hooks.json" : "settings.json");
// Resolve hook scripts from THIS installer's location so the command works
// wherever the skill lives, not just a hardcoded repo path.
const hookDirAbs = join(dirname(fileURLToPath(import.meta.url)), "hooks");
const WRITE_MATCHER = "Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch";

function hookCommand(name) {
  const abs = join(hookDirAbs, name);
  // Codex config files do not provide Claude's project/skill placeholders.
  // Absolute commands work from user and project hook scopes, even when Codex
  // starts in a subdirectory.
  if (host === "codex") return abs;
  if (globalMode) return abs;
  const rel = relative(projectDir, abs);
  // Inside the project → portable, shareable ${CLAUDE_PROJECT_DIR}-relative path.
  // Outside (e.g. user-scope install) → absolute path that actually resolves.
  if (rel && !rel.startsWith("..") && !isAbsolute(rel)) {
    return "${CLAUDE_PROJECT_DIR}/" + rel.split(sep).join("/");
  }
  return abs;
}

const CLAUDE_HOOKS = [
  { event: "PreToolUse", matcher: WRITE_MATCHER, command: hookCommand("pre-edit.sh") },
  { event: "PreToolUse", matcher: WRITE_MATCHER, command: hookCommand("harness-guard.sh") },
  { event: "PostToolUse", matcher: WRITE_MATCHER, command: hookCommand("post-edit.sh") },
  { event: "Stop", command: hookCommand("stop-verify.sh") },
  { event: "SubagentStop", command: hookCommand("stop-verify.sh") },
  { event: "SessionEnd", command: hookCommand("session-end.sh") },
  { event: "UserPromptSubmit", command: hookCommand("notify-deliver.sh") },
];

const CODEX_HOOKS = [
  { event: "PreToolUse", matcher: WRITE_MATCHER, command: hookCommand("pre-edit.sh") },
  { event: "PreToolUse", matcher: WRITE_MATCHER, command: hookCommand("harness-guard.sh") },
  { event: "PostToolUse", matcher: WRITE_MATCHER, command: hookCommand("post-edit.sh") },
  { event: "Stop", command: hookCommand("stop-verify.sh") },
  { event: "SubagentStop", command: hookCommand("stop-verify.sh") },
  // Codex does not currently expose SessionEnd. PreCompact gives the closest
  // durable handoff checkpoint before context is rewritten.
  { event: "PreCompact", command: hookCommand("session-end.sh") },
  { event: "UserPromptSubmit", command: hookCommand("notify-deliver.sh") },
];

const HOOKS = host === "codex" ? CODEX_HOOKS : CLAUDE_HOOKS;

function load() {
  if (!existsSync(settingsPath)) return {};
  try {
    return JSON.parse(readFileSync(settingsPath, "utf8"));
  } catch (error) {
    fail(`cannot parse ${settingsPath}: ${error.message}`);
  }
}

function entry(spec) {
  return {
    ...(spec.matcher ? { matcher: spec.matcher } : {}),
    hooks: [{ type: "command", command: spec.command, timeout: 20 }],
  };
}

function hasCommand(groups, command) {
  return (groups || []).some((g) => (g.hooks || []).some((h) => h.command === command));
}

function removeCommand(groups, command) {
  let removed = false;
  const out = [];
  for (const group of groups || []) {
    const hooks = (group.hooks || []).filter((h) => {
      if (h.command === command) {
        removed = true;
        return false;
      }
      return true;
    });
    if (hooks.length > 0) out.push({ ...group, hooks });
  }
  return { groups: out, removed };
}

const settings = load();
const check = flag("--check");
const dryRun = flag("--dry-run");
const remove = flag("--remove");

const status = {
  host,
  settingsPath,
  hooks: Object.fromEntries(
    HOOKS.map((spec) => [`${spec.event}:${spec.command.split(/[\\/]/).pop()}`, hasCommand(settings.hooks?.[spec.event], spec.command)]),
  ),
};

if (check) {
  console.log(JSON.stringify({ ok: true, action: "check", installed: status }, null, 2));
  process.exit(0);
}

let changed = false;
settings.hooks ||= {};

if (remove) {
  for (const spec of HOOKS) {
    const result = removeCommand(settings.hooks[spec.event], spec.command);
    if (result.removed) {
      changed = true;
      if (result.groups.length > 0) settings.hooks[spec.event] = result.groups;
      else delete settings.hooks[spec.event];
    }
  }
  if (Object.keys(settings.hooks).length === 0) delete settings.hooks;
} else {
  for (const spec of HOOKS) {
    if (!hasCommand(settings.hooks[spec.event], spec.command)) {
      (settings.hooks[spec.event] ||= []).push(entry(spec));
      changed = true;
    }
  }
}

if (dryRun) {
  console.log(
    JSON.stringify(
      { ok: true, action: "dry-run", host, changed, settingsPath, resultingSettings: settings },
      null,
      2,
    ),
  );
  process.exit(0);
}

if (changed) {
  mkdirSync(dirname(settingsPath), { recursive: true });
  writeFileSync(settingsPath, JSON.stringify(settings, null, 2) + "\n");
}

console.log(
  JSON.stringify(
    {
      ok: true,
      action: remove ? "remove" : "install",
      host,
      changed,
      settingsPath,
      note: changed ? `${settingsPath.split(/[\\/]/).pop()} updated` : "already up to date — no change",
    },
    null,
    2,
  ),
);
