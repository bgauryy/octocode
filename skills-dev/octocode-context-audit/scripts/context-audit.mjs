#!/usr/bin/env node
// Context audit: what an agent harness loads into every session, what it
// costs, and what is never used. Zero dependencies.
//
//   node context-audit.mjs [--workspace DIR] [--days 30] [--all-projects]
//        [--no-probe] [--out DIR] [--json]
//
// Collects (1) instruction files (CLAUDE.md, AGENTS.md, rules, memory index),
// (2) skill descriptions, (3) every configured MCP server's instructions and
// tool schemas (stdio servers are probed live with initialize + tools/list),
// (4) actual tool/skill usage from Claude Code transcripts. Writes
// context-audit.json and context-audit.html; prints a compact summary.
import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import readline from "node:readline";

const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const option = (name, fallback) => {
  const index = args.indexOf(name);
  return index >= 0 && args[index + 1] ? args[index + 1] : fallback;
};
const HOME = os.homedir();
const WORKSPACE = path.resolve(option("--workspace", process.cwd()));
const DAYS = Number(option("--days", "30"));
const OUT = path.resolve(option("--out", path.join(WORKSPACE, ".octocode", "context-audit")));
const PROBE = !flag("--no-probe");

// Budgets are chars (≈ 4 chars per token). They mark review points, not errors.
const BUDGET = {
  instructionFile: 8_000,
  instructionTotal: 20_000,
  memoryIndex: 10_000,
  skillDescription: 1_024,
  skillDescriptionsTotal: 12_000,
  mcpInstructions: 4_000,
  mcpToolDescription: 1_000,
  mcpToolSchema: 8_000,
  mcpServerTotal: 60_000,
};
const tokens = (chars) => Math.round(chars / 4);
const SKIP_DIRS = new Set([
  "node_modules", ".git", "target", "dist", "out", "build", ".next", "vendor", "tmp", ".cache",
]);

const findings = [];
const finding = (severity, area, subject, message, evidence = {}) =>
  findings.push({ severity, area, subject, message, ...evidence });

const readText = (file) => {
  try {
    return fs.readFileSync(file, "utf8");
  } catch {
    return null;
  }
};
const readJson = (file) => {
  const text = readText(file);
  if (text == null) return null;
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
};

// ---------------------------------------------------------------- instructions
const foreignRepos = [];
const INSTRUCTION_NAMES = new Set([
  "AGENTS.md", "CLAUDE.md", "CLAUDE.local.md", "GEMINI.md", ".cursorrules", "copilot-instructions.md",
]);

function walkInstructions(root, depth = 0, found = []) {
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
      walkInstructions(full, depth + 1, found);
    } else if (INSTRUCTION_NAMES.has(entry.name) || (full.includes(`${path.sep}.cursor${path.sep}rules${path.sep}`) && entry.name.endsWith(".mdc"))) {
      found.push(full);
    }
  }
  return found;
}

function projectSlug(dir) {
  return dir.replace(/[^A-Za-z0-9]/g, "-");
}

function collectInstructions() {
  const files = [];
  const add = (file, scope, loaded) => {
    const text = readText(file);
    if (text == null) return;
    files.push({ path: file, scope, loaded, chars: text.length, tokens: tokens(text.length), lines: text.split("\n").length });
  };
  for (const file of walkInstructions(WORKSPACE)) {
    const relative = path.relative(WORKSPACE, file);
    const nested = relative.includes(path.sep) && !relative.startsWith(".github") && !relative.startsWith(".claude");
    add(file, nested ? "nested" : "workspace", nested ? "when working in that directory" : "every session");
  }
  add(path.join(HOME, ".claude", "CLAUDE.md"), "user", "every session");
  add(path.join(HOME, ".codex", "AGENTS.md"), "user", "every Codex session");
  const memory = path.join(HOME, ".claude", "projects", projectSlug(WORKSPACE), "memory", "MEMORY.md");
  add(memory, "memory", "every session");
  const always = files.filter((file) => file.loaded.startsWith("every"));
  const total = always.reduce((sum, file) => sum + file.chars, 0);
  for (const file of files) {
    const limit = file.scope === "memory" ? BUDGET.memoryIndex : BUDGET.instructionFile;
    if (file.chars > limit) {
      finding(file.chars > limit * 2.5 ? "high" : "medium", "instructions", path.relative(WORKSPACE, file.path) || file.path,
        `${file.chars} chars (~${file.tokens} tokens) loaded ${file.loaded}; budget ${limit}. Move reference detail to linked docs or skills.`,
        { chars: file.chars });
    }
  }
  if (total > BUDGET.instructionTotal) {
    finding("high", "instructions", "always-loaded total",
      `${total} chars (~${tokens(total)} tokens) of instructions load every session; budget ${BUDGET.instructionTotal}.`, { chars: total });
  }
  return { files, alwaysLoadedChars: total, alwaysLoadedTokens: tokens(total), nestedRepositoriesSkipped: foreignRepos.map((dir) => path.relative(WORKSPACE, dir)) };
}

// ---------------------------------------------------------------------- skills
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

function collectSkills() {
  const roots = [
    { dir: path.join(HOME, ".claude", "skills"), scope: "user" },
    { dir: path.join(WORKSPACE, ".claude", "skills"), scope: "workspace" },
    { dir: path.join(WORKSPACE, ".agents", "skills"), scope: "workspace-agents" },
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

// ------------------------------------------------------------------------- MCP
function configuredServers() {
  const servers = [];
  const push = (source, entries) => {
    for (const [name, config] of Object.entries(entries ?? {})) servers.push({ name, source, config });
  };
  const claude = readJson(path.join(HOME, ".claude.json"));
  push("~/.claude.json (user)", claude?.mcpServers);
  push("~/.claude.json (project)", claude?.projects?.[WORKSPACE]?.mcpServers);
  push(".mcp.json", readJson(path.join(WORKSPACE, ".mcp.json"))?.mcpServers);
  push("~/.cursor/mcp.json", readJson(path.join(HOME, ".cursor", "mcp.json"))?.mcpServers);
  return servers;
}

function probe(server, timeoutMs = 20_000) {
  const { command, args: serverArgs = [], env = {} } = server.config;
  if (!command) return Promise.resolve({ error: `not stdio (${server.config.type ?? server.config.url ?? "unknown"}); not probed` });
  return new Promise((resolve) => {
    const child = spawn(command, serverArgs, { env: { ...process.env, ...env }, stdio: ["pipe", "pipe", "pipe"], cwd: WORKSPACE });
    const pending = new Map();
    let buffer = "";
    let stderr = "";
    child.stderr.on("data", (chunk) => {
      stderr = (stderr + chunk).slice(-2000);
    });
    const failure = (reason) => {
      const detail = stderr.trim().split("\n").filter((line) => !/config warning/.test(line)).slice(-2).join(" ");
      return { error: detail ? `${reason}; stderr: ${detail}` : reason };
    };
    child.on("exit", (code) => done(failure(`exited with code ${code} before answering`)));
    let settled = false;
    const done = (result) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      child.kill();
      resolve(result);
    };
    const timer = setTimeout(() => done(failure(`no answer after ${timeoutMs} ms`)), timeoutMs);
    child.on("error", (error) => done({ error: error.message }));
    child.stdout.on("data", (chunk) => {
      buffer += chunk;
      let newline;
      while ((newline = buffer.indexOf("\n")) >= 0) {
        const line = buffer.slice(0, newline).trim();
        buffer = buffer.slice(newline + 1);
        if (!line) continue;
        try {
          const message = JSON.parse(line);
          pending.get(message.id)?.(message);
        } catch {}
      }
    });
    const request = (id, method, params) =>
      new Promise((answer) => {
        pending.set(id, answer);
        child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
      });
    (async () => {
      const init = await request(1, "initialize", { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "context-audit", version: "1" } });
      child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" })}\n`);
      const tools = [];
      let cursor;
      for (let page = 0; page < 20; page += 1) {
        const listed = await request(2 + page, "tools/list", cursor ? { cursor } : {});
        tools.push(...(listed.result?.tools ?? []));
        cursor = listed.result?.nextCursor;
        if (!cursor) break;
      }
      done({ instructions: init.result?.instructions ?? "", serverInfo: init.result?.serverInfo, tools });
    })().catch((error) => done({ error: String(error) }));
  });
}

async function collectMcp() {
  const servers = [];
  for (const server of configuredServers()) {
    const probed = PROBE ? await probe(server) : { error: "probe disabled" };
    const tools = (probed.tools ?? []).map((tool) => {
      const description = tool.description ?? "";
      const schema = JSON.stringify(tool.inputSchema ?? {});
      const annotations = JSON.stringify(tool.annotations ?? {});
      const total = JSON.stringify(tool).length;
      return { name: tool.name, descriptionChars: description.length, schemaChars: schema.length, totalChars: total, annotations: annotations.length > 2 };
    });
    const instructionsChars = (probed.instructions ?? "").length;
    const total = instructionsChars + tools.reduce((sum, tool) => sum + tool.totalChars, 0);
    servers.push({ name: server.name, source: server.source, error: probed.error, serverInfo: probed.serverInfo, instructionsChars, toolCount: tools.length, totalChars: total, totalTokens: tokens(total), tools });
    if (probed.error) finding(server.config.command ? "high" : "low", "mcp", server.name, `Not measured: ${probed.error}.`);
    if (instructionsChars > BUDGET.mcpInstructions) finding("medium", "mcp", `${server.name} instructions`, `${instructionsChars} chars of server instructions every session; budget ${BUDGET.mcpInstructions}.`, { chars: instructionsChars });
    if (total > BUDGET.mcpServerTotal) finding("high", "mcp", server.name, `${tools.length} tools cost ${total} chars (~${tokens(total)} tokens) when loaded; budget ${BUDGET.mcpServerTotal}.`, { chars: total });
    for (const tool of tools) {
      if (tool.descriptionChars > BUDGET.mcpToolDescription) finding("low", "mcp", `${server.name}/${tool.name}`, `Description ${tool.descriptionChars} chars; budget ${BUDGET.mcpToolDescription}.`, { chars: tool.descriptionChars });
      if (tool.schemaChars > BUDGET.mcpToolSchema) finding("medium", "mcp", `${server.name}/${tool.name}`, `Input schema ${tool.schemaChars} chars; budget ${BUDGET.mcpToolSchema}.`, { chars: tool.schemaChars });
    }
  }
  return servers;
}

// ----------------------------------------------------------------------- usage
/** First meaningful command of a shell call, grouped by what a tool could replace. */
function shellKind(command) {
  const first = command
    .split(/&&|\|\||;|\|/)
    .map((part) => part.trim().replace(/^(cd\s+\S+|export\s+\S+|[A-Z_]+=\S+\s+)*/, "").trim())
    .find((part) => part && !/^(cd|export|set|source|true|echo)\b/.test(part)) ?? "";
  if (/octocode\.c?js\b|(^|[\s;&|(])(npx\s+)?octocode(-mcp)?(?=\s|$)/.test(command)) return "octocode CLI";
  const word = first.split(/\s+/)[0]?.replace(/^.*\//, "") ?? "";
  if (["grep", "rg", "ag", "ack"].includes(word)) return "text search (grep/rg)";
  if (["find", "ls", "tree", "fd", "du", "wc"].includes(word)) return "file discovery (find/ls)";
  if (["cat", "head", "tail", "sed", "awk", "less", "nl"].includes(word)) return "file read (cat/sed/head)";
  if (["cargo", "yarn", "npm", "npx", "pnpm", "node", "tsc", "vitest", "python3", "python"].includes(word)) return `build/run (${word})`;
  if (word === "git" || word === "gh") return word;
  return word ? `other (${word})` : "other";
}

async function collectUsage() {
  const projectsDir = path.join(HOME, ".claude", "projects");
  const dirs = flag("--all-projects")
    ? fs.readdirSync(projectsDir).map((dir) => path.join(projectsDir, dir))
    : [path.join(projectsDir, projectSlug(WORKSPACE))];
  const since = Date.now() - DAYS * 86_400_000;
  const tools = new Map();
  const skills = new Map();
  const errors = new Map();
  const shell = new Map();
  let sessions = 0;
  let calls = 0;
  const bump = (map, key, by = 1) => map.set(key, (map.get(key) ?? 0) + by);
  for (const dir of dirs) {
    let files = [];
    try {
      files = fs.readdirSync(dir).filter((file) => file.endsWith(".jsonl")).map((file) => path.join(dir, file));
    } catch {
      continue;
    }
    for (const file of files) {
      if (fs.statSync(file).mtimeMs < since) continue;
      sessions += 1;
      const names = new Map();
      const lines = readline.createInterface({ input: fs.createReadStream(file), crlfDelay: Infinity });
      for await (const line of lines) {
        if (!line.includes('"tool_use"') && !line.includes('"tool_result"')) continue;
        let entry;
        try {
          entry = JSON.parse(line);
        } catch {
          continue;
        }
        for (const part of entry.message?.content ?? []) {
          if (part?.type === "tool_use") {
            calls += 1;
            bump(tools, part.name);
            names.set(part.id, part.name);
            if (part.name === "Skill" && part.input?.skill) bump(skills, String(part.input.skill).replace(/^.*:/, ""));
            if (part.name === "Bash" && typeof part.input?.command === "string") bump(shell, shellKind(part.input.command));
          } else if (part?.type === "tool_result" && part.is_error) {
            bump(errors, names.get(part.tool_use_id) ?? "unknown");
          }
        }
      }
    }
  }
  const sorted = (map) => Object.fromEntries([...map].sort((a, b) => b[1] - a[1]));
  const servers = {};
  for (const [name, count] of tools) {
    const match = /^mcp__(.+?)__(.+)$/.exec(name);
    if (!match) continue;
    const server = (servers[match[1]] ??= { calls: 0, errors: 0, tools: {} });
    server.calls += count;
    server.errors += errors.get(name) ?? 0;
    server.tools[match[2]] = count;
  }
  return { days: DAYS, sessions, calls, tools: sorted(tools), skills: sorted(skills), errors: sorted(errors), servers, shell: sorted(shell) };
}

function crossCheck(mcp, skills, usage) {
  const unusedTools = [];
  // Transcripts are Claude Code's; other clients' servers cannot be judged by them.
  for (const server of mcp.filter((entry) => !entry.source.includes(".cursor"))) {
    const prefix = `mcp__${server.name}__`;
    for (const tool of server.tools) {
      const count = usage.tools[`${prefix}${tool.name}`] ?? 0;
      tool.calls = count;
      tool.errors = usage.errors[`${prefix}${tool.name}`] ?? 0;
      if (count === 0) unusedTools.push({ server: server.name, tool: tool.name, chars: tool.totalChars });
    }
    const used = server.tools.filter((tool) => tool.calls > 0).length;
    if (server.tools.length && usage.sessions && used === 0) {
      finding("high", "usage", server.name, `No tool of this server was called in ${usage.sessions} sessions over ${usage.days} days; it still costs ~${server.totalTokens} tokens when loaded. Disable it or scope it to the projects that need it.`, { chars: server.totalChars });
    }
  }
  if (usage.sessions) {
    for (const tool of unusedTools) {
      finding("low", "usage", `${tool.server}/${tool.tool}`, `Never called in ${usage.sessions} sessions (${tool.chars} chars of schema). Candidate to merge, gate, or drop.`, { chars: tool.chars });
    }
    // Transcripts are Claude Code's: judge only skills Claude loads.
    const claudeLoaded = skills.skills.filter((skill) => skill.scope === "user" || skill.scope === "workspace");
    for (const skill of new Map(claudeLoaded.map((s) => [s.name, s])).values()) {
      skill.calls = usage.skills[skill.name] ?? 0;
      if (!skill.calls) finding("low", "usage", `skill ${skill.name}`, `Never invoked in ${usage.sessions} sessions; its ${skill.descriptionChars}-char description still loads every session.`, { chars: skill.descriptionChars });
    }
    const shell = Object.entries(usage.shell ?? {});
    const raw = shell.filter(([kind]) => /^(text search|file discovery|file read)/.test(kind)).reduce((sum, [, count]) => sum + count, 0);
    const mcpCalls = Object.values(usage.servers).reduce((sum, server) => sum + server.calls, 0);
    const cli = usage.shell?.["octocode CLI"] ?? 0;
    if (raw > (mcpCalls + cli) && raw >= 20) {
      finding("medium", "usage", "raw shell reads", `${raw} grep/find/cat-style shell calls vs ${mcpCalls} MCP + ${cli} octocode CLI calls. Agents bypass the research tools; check routing instructions and tool availability.`);
    }
    for (const [tool, count] of Object.entries(usage.errors)) {
      const calls = usage.tools[tool] ?? 0;
      if (calls >= 5 && count / calls >= 0.2) finding("medium", "usage", tool, `${count}/${calls} calls errored (${Math.round((100 * count) / calls)}%); inspect the schema or description.`);
    }
  }
  return unusedTools;
}

// ---------------------------------------------------------------------- report
const escape = (value) => String(value).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

function html(report) {
  const rank = { high: 0, medium: 1, low: 2 };
  const rows = [...report.findings].sort((a, b) => rank[a.severity] - rank[b.severity] || (b.chars ?? 0) - (a.chars ?? 0));
  const table = (headers, body) => `<table><thead><tr>${headers.map((h) => `<th>${h}</th>`).join("")}</tr></thead><tbody>${body.join("")}</tbody></table>`;
  const bar = (value, max) => `<span class="bar" style="width:${Math.max(2, Math.round((100 * value) / Math.max(1, max)))}%"></span>`;
  const maxTool = Math.max(1, ...report.mcp.flatMap((server) => server.tools.map((tool) => tool.totalChars)));
  const maxFile = Math.max(1, ...report.instructions.files.map((file) => file.chars));
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Context Audit</title>
<style>
:root{--bg:#fbfaf7;--fg:#1d1c1a;--muted:#6b675f;--line:#e4e0d8;--accent:#b4532a;--high:#b42318;--medium:#b54708;--low:#475467;--barc:#d9a58a}
@media (prefers-color-scheme:dark){:root:not([data-theme="light"]){--bg:#171614;--fg:#ecebe8;--muted:#a19d95;--line:#34312c;--accent:#e08a5f;--high:#f97066;--medium:#fdb022;--low:#98a2b3;--barc:#8a5a43;color-scheme:dark}}
:root[data-theme="dark"]{--bg:#171614;--fg:#ecebe8;--muted:#a19d95;--line:#34312c;--accent:#e08a5f;--high:#f97066;--medium:#fdb022;--low:#98a2b3;--barc:#8a5a43;color-scheme:dark}
body{background:var(--bg);color:var(--fg);font:14px/1.5 system-ui,sans-serif;margin:0;padding-block:24px;padding-inline:16px;max-width:1100px;margin-inline:auto}
h1{font-size:22px;margin:0 0 4px}h2{font-size:16px;margin:28px 0 8px;border-bottom:1px solid var(--line);padding-bottom:4px}
.muted{color:var(--muted)}.kpis{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:10px;margin:16px 0}
.kpi{border:1px solid var(--line);border-radius:8px;padding:10px}.kpi b{display:block;font-size:20px}
.wrap{overflow-x:auto}table{border-collapse:collapse;width:100%;font-size:13px}th,td{text-align:left;padding:5px 8px;border-bottom:1px solid var(--line);vertical-align:top}
th{color:var(--muted);font-weight:600}td.num{text-align:right;font-variant-numeric:tabular-nums;white-space:nowrap}
.sev{font-weight:700;text-transform:uppercase;font-size:11px}.high{color:var(--high)}.medium{color:var(--medium)}.low{color:var(--low)}
.bar{display:inline-block;height:8px;background:var(--barc);border-radius:2px;vertical-align:middle}td.barcell{width:30%}
code{font-size:12px;word-break:break-all}
</style></head><body>
<h1>Context audit</h1><div class="muted">${escape(report.workspace)} · ${escape(report.generatedAt)} · usage window ${report.usage.days} days, ${report.usage.sessions} sessions, ${report.usage.calls} tool calls</div>
<div class="kpis">
<div class="kpi"><span class="muted">Always-loaded instructions</span><b>~${report.instructions.alwaysLoadedTokens.toLocaleString()} tok</b></div>
<div class="kpi"><span class="muted">Skill descriptions (${report.skills.uniqueCount})</span><b>~${report.skills.descriptionTokens.toLocaleString()} tok</b></div>
<div class="kpi"><span class="muted">MCP servers (${report.mcp.length})</span><b>~${report.mcp.reduce((s, x) => s + x.totalTokens, 0).toLocaleString()} tok</b></div>
<div class="kpi"><span class="muted">Unused MCP tools</span><b>${report.unusedTools.length}</b></div>
<div class="kpi"><span class="muted">Findings</span><b>${report.findings.length}</b></div>
</div>
<h2>Findings</h2><div class="wrap">${table(["Severity", "Area", "Subject", "Finding"], rows.map((f) => `<tr><td class="sev ${f.severity}">${f.severity}</td><td>${escape(f.area)}</td><td><code>${escape(f.subject)}</code></td><td>${escape(f.message)}</td></tr>`))}</div>
<h2>MCP tools</h2><div class="wrap">${table(["Server/tool", "Total chars", "", "Desc", "Schema", "Calls", "Errors"], report.mcp.flatMap((server) => [
  `<tr><td colspan="7"><b>${escape(server.name)}</b> <span class="muted">${escape(server.source)} · ${server.error ? escape(server.error) : `${server.toolCount} tools · instructions ${server.instructionsChars} chars · total ~${server.totalTokens} tok`}</span></td></tr>`,
  ...[...server.tools].sort((a, b) => b.totalChars - a.totalChars).map((tool) => `<tr><td><code>${escape(tool.name)}</code></td><td class="num">${tool.totalChars.toLocaleString()}</td><td class="barcell">${bar(tool.totalChars, maxTool)}</td><td class="num">${tool.descriptionChars}</td><td class="num">${tool.schemaChars.toLocaleString()}</td><td class="num">${tool.calls ?? "–"}</td><td class="num">${tool.errors ?? "–"}</td></tr>`),
]))}</div>
<h2>Instruction files</h2><div class="wrap">${table(["File", "Loaded", "Chars", ""], [...report.instructions.files].sort((a, b) => b.chars - a.chars).map((file) => `<tr><td><code>${escape(path.relative(report.workspace, file.path) || file.path)}</code></td><td>${escape(file.loaded)}</td><td class="num">${file.chars.toLocaleString()}</td><td class="barcell">${bar(file.chars, maxFile)}</td></tr>`))}</div>
<h2>Skills</h2><div class="wrap">${table(["Skill", "Scope", "Description chars", "Invocations"], [...report.skills.skills].sort((a, b) => b.descriptionChars - a.descriptionChars).map((skill) => `<tr><td><code>${escape(skill.name)}</code></td><td>${escape(skill.scope)}</td><td class="num">${skill.descriptionChars}</td><td class="num">${skill.calls ?? "–"}</td></tr>`))}</div>
<h2>MCP usage by server (transcripts)</h2><div class="wrap">${table(["Server", "Calls", "Errors", "Tools called"], Object.entries(report.usage.servers).sort((a, b) => b[1].calls - a[1].calls).map(([name, server]) => `<tr><td><code>${escape(name)}</code></td><td class="num">${server.calls}</td><td class="num">${server.errors}</td><td>${Object.entries(server.tools).sort((a, b) => b[1] - a[1]).map(([tool, count]) => `${escape(tool)} ${count}`).join(" · ")}</td></tr>`))}</div>
<h2>Shell calls by kind</h2><div class="wrap">${table(["Kind", "Calls"], Object.entries(report.usage.shell ?? {}).slice(0, 20).map(([kind, count]) => `<tr><td>${escape(kind)}</td><td class="num">${count}</td></tr>`))}</div>
<h2>Most used tools</h2><div class="wrap">${table(["Tool", "Calls", "Errors"], Object.entries(report.usage.tools).slice(0, 30).map(([name, count]) => `<tr><td><code>${escape(name)}</code></td><td class="num">${count}</td><td class="num">${report.usage.errors[name] ?? 0}</td></tr>`))}</div>
<p class="muted">Tokens ≈ chars ÷ 4. Budgets are review points, not errors. Usage comes from Claude Code transcripts only.</p>
</body></html>`;
}

const instructions = collectInstructions();
const skills = collectSkills();
const mcp = await collectMcp();
const usage = await collectUsage();
const unusedTools = crossCheck(mcp, skills, usage);
const report = { workspace: WORKSPACE, generatedAt: new Date().toISOString(), budgets: BUDGET, instructions, skills, mcp, usage, unusedTools, findings };
fs.mkdirSync(OUT, { recursive: true });
fs.writeFileSync(path.join(OUT, "context-audit.json"), JSON.stringify(report, null, 2));
fs.writeFileSync(path.join(OUT, "context-audit.html"), html(report));
if (flag("--json")) {
  process.stdout.write(`${JSON.stringify(report)}\n`);
} else {
  const count = (severity) => findings.filter((f) => f.severity === severity).length;
  console.log(`context-audit: ${findings.length} findings (high ${count("high")}, medium ${count("medium")}, low ${count("low")})`);
  console.log(`  instructions always loaded ~${instructions.alwaysLoadedTokens} tok · skills ${skills.uniqueCount} (~${skills.descriptionTokens} tok) · MCP ${mcp.map((s) => `${s.name} ${s.toolCount} tools ~${s.totalTokens} tok`).join(", ") || "none"}`);
  console.log(`  usage: ${usage.sessions} sessions, ${usage.calls} tool calls in ${usage.days} days · unused MCP tools ${unusedTools.length}`);
  for (const f of findings.filter((f) => f.severity !== "low").slice(0, 15)) console.log(`  [${f.severity}] ${f.area} ${f.subject}: ${f.message}`);
  console.log(`  report: ${path.join(OUT, "context-audit.html")}`);
}
