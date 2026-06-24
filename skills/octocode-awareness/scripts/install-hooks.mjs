#!/usr/bin/env node
// Merge the octocode-awareness file-claim hooks into a project's
// .claude/settings.json so enforcement is session-wide (active even when the
// skill is not loaded). Idempotent and non-destructive: it never touches hooks
// other than its own. ALWAYS run only after the user has approved it.
//
// Usage:
//   node scripts/install-hooks.mjs [--project-dir <path>]   install/merge
//   node scripts/install-hooks.mjs --check                  report status only
//   node scripts/install-hooks.mjs --dry-run                show result, don't write
//   node scripts/install-hooks.mjs --remove                 remove our hooks
import { readFileSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join, resolve, relative, isAbsolute, sep } from "node:path";
import { fileURLToPath } from "node:url";

const args = process.argv.slice(2);
const flag = (f) => args.includes(f);
const opt = (name, def) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : def;
};

const projectDir = resolve(opt("--project-dir", process.cwd()));
const settingsPath = join(projectDir, ".claude", "settings.json");
// Resolve hook scripts from THIS installer's location so the command works
// wherever the skill lives, not just a hardcoded repo path.
const hookDirAbs = join(dirname(fileURLToPath(import.meta.url)), "hooks");
const MATCHER = "Write|Edit|MultiEdit|NotebookEdit";

function hookCommand(name) {
  const abs = join(hookDirAbs, name);
  const rel = relative(projectDir, abs);
  // Inside the project → portable, shareable ${CLAUDE_PROJECT_DIR}-relative path.
  // Outside (e.g. user-scope install) → absolute path that actually resolves.
  if (rel && !rel.startsWith("..") && !isAbsolute(rel)) {
    return "${CLAUDE_PROJECT_DIR}/" + rel.split(sep).join("/");
  }
  return abs;
}

const COMMANDS = {
  PreToolUse: hookCommand("pre-edit.sh"),
  PostToolUse: hookCommand("post-edit.sh"),
};

function fail(message, extra = {}) {
  console.error(JSON.stringify({ ok: false, error: message, ...extra }, null, 2));
  process.exit(1);
}

function load() {
  if (!existsSync(settingsPath)) return {};
  try {
    return JSON.parse(readFileSync(settingsPath, "utf8"));
  } catch (error) {
    fail(`cannot parse ${settingsPath}: ${error.message}`);
  }
}

function entry(command) {
  return { matcher: MATCHER, hooks: [{ type: "command", command, timeout: 20 }] };
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
  settingsPath,
  PreToolUse: hasCommand(settings.hooks?.PreToolUse, COMMANDS.PreToolUse),
  PostToolUse: hasCommand(settings.hooks?.PostToolUse, COMMANDS.PostToolUse),
};

if (check) {
  console.log(JSON.stringify({ ok: true, action: "check", installed: status }, null, 2));
  process.exit(0);
}

let changed = false;
settings.hooks ||= {};

if (remove) {
  for (const event of Object.keys(COMMANDS)) {
    const result = removeCommand(settings.hooks[event], COMMANDS[event]);
    if (result.removed) {
      changed = true;
      if (result.groups.length > 0) settings.hooks[event] = result.groups;
      else delete settings.hooks[event];
    }
  }
  if (Object.keys(settings.hooks).length === 0) delete settings.hooks;
} else {
  for (const event of Object.keys(COMMANDS)) {
    if (!hasCommand(settings.hooks[event], COMMANDS[event])) {
      (settings.hooks[event] ||= []).push(entry(COMMANDS[event]));
      changed = true;
    }
  }
}

if (dryRun) {
  console.log(
    JSON.stringify(
      { ok: true, action: "dry-run", changed, settingsPath, resultingSettings: settings },
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
      changed,
      settingsPath,
      note: changed ? "settings.json updated" : "already up to date — no change",
    },
    null,
    2,
  ),
);
