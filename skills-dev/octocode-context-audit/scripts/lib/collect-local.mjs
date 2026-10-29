// Always-loaded local context: instruction files (workspace, user, memory index) and
// installed skill descriptions, each checked against its budget.
import fs from "node:fs";
import path from "node:path";
import { BUDGET, HOME, finding, projectSlug, readText, tokens } from "./audit-common.mjs";

const SKIP_DIRS = new Set([
  "node_modules", ".git", "target", "dist", "out", "build", ".next", "vendor", "tmp", ".cache",
]);

const INSTRUCTION_NAMES = new Set([
  "AGENTS.md", "CLAUDE.md", "CLAUDE.local.md", "GEMINI.md", ".cursorrules", "copilot-instructions.md",
]);

function walkInstructions(root, foreignRepos, depth = 0, found = []) {
  if (depth > 8) return found;
  let entries = [];
  try {
    entries = fs.readdirSync(root, { withFileTypes: true });
  } catch {
    return found;
  }
  for (const entry of entries) {
    const full = path.join(root, entry.name);
    if (entry.isDirectory()) {
      if (SKIP_DIRS.has(entry.name) || (entry.name.startsWith(".") && ![".github", ".cursor", ".claude"].includes(entry.name))) continue;
      // Agent worktrees and nested repositories are other projects: their
      // instructions load only for sessions started there.
      if (entry.name === "worktrees" && path.basename(root) === ".claude") continue;
      if (fs.existsSync(path.join(full, ".git"))) {
        foreignRepos.push(full);
        continue;
      }
      walkInstructions(full, foreignRepos, depth + 1, found);
    } else if (INSTRUCTION_NAMES.has(entry.name) || (full.includes(`${path.sep}.cursor${path.sep}rules${path.sep}`) && entry.name.endsWith(".mdc"))) {
      found.push(full);
    }
  }
  return found;
}

export function collectInstructions(workspace) {
  const files = [];
  const foreignRepos = [];
  const add = (file, scope, loaded) => {
    const text = readText(file);
    if (text == null) return;
    files.push({ path: file, scope, loaded, chars: text.length, tokens: tokens(text.length), lines: text.split("\n").length });
  };
  for (const file of walkInstructions(workspace, foreignRepos)) {
    const relative = path.relative(workspace, file);
    const nested = relative.includes(path.sep) && !relative.startsWith(".github") && !relative.startsWith(".claude");
    add(file, nested ? "nested" : "workspace", nested ? "when working in that directory" : "every session");
  }
  add(path.join(HOME, ".claude", "CLAUDE.md"), "user", "every session");
  add(path.join(HOME, ".codex", "AGENTS.md"), "user", "every Codex session");
  const memory = path.join(HOME, ".claude", "projects", projectSlug(workspace), "memory", "MEMORY.md");
  add(memory, "memory", "every session");
  const always = files.filter((file) => file.loaded.startsWith("every"));
  const total = always.reduce((sum, file) => sum + file.chars, 0);
  for (const file of files) {
    const limit = file.scope === "memory" ? BUDGET.memoryIndex : BUDGET.instructionFile;
    if (file.chars > limit) {
      finding(file.chars > limit * 2.5 ? "high" : "medium", "instructions", path.relative(workspace, file.path) || file.path,
        `${file.chars} chars (~${file.tokens} tokens) loaded ${file.loaded}; budget ${limit}. Move reference detail to linked docs or skills.`,
        { chars: file.chars });
    }
  }
  if (total > BUDGET.instructionTotal) {
    finding("high", "instructions", "always-loaded total",
      `${total} chars (~${tokens(total)} tokens) of instructions load every session; budget ${BUDGET.instructionTotal}.`, { chars: total });
  }
  return { files, alwaysLoadedChars: total, alwaysLoadedTokens: tokens(total), nestedRepositoriesSkipped: foreignRepos.map((dir) => path.relative(workspace, dir)) };
}

function frontmatter(text) {
  const match = /^---\n([\s\S]*?)\n---/.exec(text);
  if (!match) return {};
  const fields = {};
  let key = null;
  for (const line of match[1].split("\n")) {
    const pair = /^([A-Za-z_-]+):\s*(.*)$/.exec(line);
    if (pair) {
      key = pair[1];
      fields[key] = pair[2].replace(/^["']|["']$/g, "");
    } else if (key && /^\s+/.test(line)) {
      fields[key] += ` ${line.trim()}`;
    }
  }
  return fields;
}

export function collectSkills(workspace) {
  const roots = [
    { dir: path.join(HOME, ".claude", "skills"), scope: "user" },
    { dir: path.join(workspace, ".claude", "skills"), scope: "workspace" },
    { dir: path.join(workspace, ".agents", "skills"), scope: "workspace-agents" },
  ];
  const skills = [];
  for (const { dir, scope } of roots) {
    let entries = [];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      const full = path.join(dir, entry.name);
      let target = full;
      try {
        target = fs.realpathSync(full);
      } catch {
        finding("medium", "skills", `${scope}/${entry.name}`, "Broken skill link: the target no longer exists. Remove the link.", {
          target: (() => { try { return fs.readlinkSync(full); } catch { return null; } })(),
        });
        continue;
      }
      const text = readText(path.join(target, "SKILL.md"));
      if (text == null) continue;
      const meta = frontmatter(text);
      const description = meta.description ?? "";
      skills.push({ name: meta.name || entry.name, scope, path: target, descriptionChars: description.length, bodyChars: text.length });
      if (description.length > BUDGET.skillDescription) {
        finding("medium", "skills", meta.name || entry.name,
          `Description ${description.length} chars; every session pays it. Budget ${BUDGET.skillDescription}.`, { chars: description.length });
      }
    }
  }
  const byName = new Map();
  for (const skill of skills) byName.set(skill.name, [...(byName.get(skill.name) ?? []), skill]);
  for (const [name, copies] of byName) {
    const targets = new Set(copies.map((copy) => copy.path));
    // Same-source listings are deduplicated by name at load; only divergent
    // copies can put two different skills behind one name.
    if (targets.size > 1) {
      finding("medium", "skills", name,
        `Installed ${copies.length}× from different sources (${[...targets].join(", ")}); agents may load divergent copies.`);
    }
  }
  const unique = [...byName.values()].map((copies) => copies[0]);
  const total = unique.reduce((sum, skill) => sum + skill.descriptionChars, 0);
  if (total > BUDGET.skillDescriptionsTotal) {
    finding("medium", "skills", "descriptions total", `${unique.length} skills, ${total} description chars every session; budget ${BUDGET.skillDescriptionsTotal}.`, { chars: total });
  }
  return { skills, uniqueCount: unique.length, descriptionChars: total, descriptionTokens: tokens(total) };
}
