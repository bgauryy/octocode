// Actual usage from Claude Code transcripts, and the cross-check that flags loaded context
// (MCP tools, skills) nobody calls.
import fs from "node:fs";
import path from "node:path";
import readline from "node:readline";
import { HOME, finding, projectSlug } from "./audit-common.mjs";

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

const bump = (map, key) => map.set(key, (map.get(key) ?? 0) + 1);
const sorted = (map) => Object.fromEntries([...map].toSorted((a, b) => b[1] - a[1]));

function countToolUse(part, state) {
  if (typeof part.name !== "string" || !part.name || typeof part.id !== "string" || !part.id) return 1;
  state.calls += 1;
  bump(state.tools, part.name);
  state.names.set(part.id, part.name);
  if (part.name === "Skill" && part.input?.skill) bump(state.skills, String(part.input.skill).replace(/^.*:/, ""));
  if (part.name === "Bash" && typeof part.input?.command === "string") bump(state.shell, shellKind(part.input.command));
  return 0;
}

function countToolResult(part, state) {
  if (typeof part.tool_use_id !== "string" || !part.tool_use_id || (part.is_error != null && typeof part.is_error !== "boolean")) return 1;
  if (part.is_error) bump(state.errors, state.names.get(part.tool_use_id) ?? "unknown");
  return 0;
}

function countPart(part, state) {
  if (!part || typeof part !== "object" || Array.isArray(part)) return 1;
  if (part.type === "tool_use") return countToolUse(part, state);
  if (part.type === "tool_result") return countToolResult(part, state);
  return 0;
}

function countLine(line, state) {
  let entry;
  try {
    entry = JSON.parse(line);
  } catch {
    return 1;
  }
  if (!entry || typeof entry !== "object" || Array.isArray(entry)) return 1;
  if (entry.message == null) return entry.type === "assistant" || entry.type === "user" ? 1 : 0;
  if (typeof entry.message === "string") return 0;
  if (typeof entry.message !== "object" || Array.isArray(entry.message)) return 1;
  const content = entry?.message?.content;
  // Claude progress and metadata records may omit content; text messages use a string.
  if (typeof content === "string") return 0;
  if (!Array.isArray(content)) return 1;
  return Number(content.reduce((invalid, part) => invalid + countPart(part, state), 0) > 0);
}

export async function collectUsage(workspace, days, allProjects) {
  const projectsDir = path.join(HOME, ".claude", "projects");
  let readFailures = 0;
  let dirs = [path.join(projectsDir, projectSlug(workspace))];
  if (allProjects) {
    try {
      dirs = fs.readdirSync(projectsDir, { withFileTypes: true }).filter(entry => entry.isDirectory()).map(entry => path.join(projectsDir, entry.name));
    } catch (error) {
      if (error.code !== "ENOENT") readFailures += 1;
      dirs = [];
    }
  }
  const since = Date.now() - days * 86_400_000;
  const tools = new Map();
  const skills = new Map();
  const errors = new Map();
  const shell = new Map();
  let sessions = 0;
  const state = { calls: 0, tools, skills, errors, shell, names: new Map() };
  let malformedLines = 0;
  for (const dir of dirs) {
    let files = [];
    try {
      files = fs.readdirSync(dir).filter((file) => file.endsWith(".jsonl")).map((file) => path.join(dir, file));
    } catch (error) {
      if (error.code !== "ENOENT") readFailures += 1;
      continue;
    }
    for (const file of files) {
      let modified;
      try {
        modified = fs.statSync(file).mtimeMs;
      } catch {
        readFailures += 1;
        continue;
      }
      if (modified < since) continue;
      sessions += 1;
      state.names = new Map();
      try {
        const lines = readline.createInterface({ input: fs.createReadStream(file), crlfDelay: Infinity });
        for await (const line of lines) malformedLines += countLine(line, state);
      } catch {
        readFailures += 1;
      }
    }
  }
  if (malformedLines || readFailures) finding("medium", "usage", "transcript coverage", `${malformedLines} malformed lines and ${readFailures} read failures may hide calls; unused-tool and error-rate judgments are suppressed.`);
  const servers = {};
  for (const [name, count] of tools) {
    const match = /^mcp__(.+?)__(.+)$/.exec(name);
    if (!match) continue;
    const server = (servers[match[1]] ??= { calls: 0, errors: 0, tools: {} });
    server.calls += count;
    server.errors += errors.get(name) ?? 0;
    server.tools[match[2]] = count;
  }
  return { days, sessions, calls: state.calls, malformedLines, readFailures, incomplete: Boolean(malformedLines || readFailures), tools: sorted(tools), skills: sorted(skills), errors: sorted(errors), servers, shell: sorted(shell) };
}

function assessServer(server, usage, trusted, unusedTools) {
  const prefix = `mcp__${server.name}__`;
  for (const tool of server.tools) {
    const count = usage.tools[`${prefix}${tool.name}`] ?? 0;
    tool.calls = count;
    tool.errors = usage.errors[`${prefix}${tool.name}`] ?? 0;
    if (count === 0 && trusted) unusedTools.push({ server: server.name, tool: tool.name, chars: tool.totalChars });
  }
  const used = server.tools.filter((tool) => tool.calls > 0).length;
  if (server.tools.length && trusted && used === 0) {
    finding("high", "usage", server.name, `No tool of this server was called in ${usage.sessions} sessions over ${usage.days} days; it still costs ~${server.totalTokens} tokens when loaded. Disable it or scope it to the projects that need it.`, { chars: server.totalChars });
  }
}

export function crossCheck(mcp, skills, usage) {
  const unusedTools = [];
  const trusted = usage.sessions > 0 && !usage.incomplete;
  // Transcripts are Claude Code's; other clients' servers cannot be judged by them.
  for (const server of mcp.filter((entry) => !entry.source.includes(".cursor"))) assessServer(server, usage, trusted, unusedTools);
  if (trusted) {
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
