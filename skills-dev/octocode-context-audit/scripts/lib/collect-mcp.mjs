// MCP cost: configured servers from Claude Code, project and Cursor configs; stdio servers
// are probed live (initialize + tools/list) and measured against budgets.
import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { BUDGET, HOME, finding, readText, tokens } from "./audit-common.mjs";

function responseResult(message, method) {
  if (message?.error) throw new Error(`${method}: JSON-RPC error ${Number.isInteger(message.error.code) ? message.error.code : "unknown"}`);
  if (!message?.result || typeof message.result !== "object" || Array.isArray(message.result)) throw new Error(`${method}: invalid result`);
  return message.result;
}

const isStdio = (config) => Boolean(config?.command);

function configFile(file) {
  const source = readText(file);
  if (source == null) {
    if (fs.existsSync(file)) finding("medium", "mcp", file, "Configuration exists but could not be read; server inventory incomplete.");
    return null;
  }
  try {
    const parsed = JSON.parse(source);
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) return parsed;
  } catch {
    // Report the malformed configuration below without including its contents.
  }
  finding("medium", "mcp", file, "Configuration is not a JSON object; server inventory incomplete.");
  return null;
}

function configuredServers(workspace) {
  const servers = [];
  const push = (source, entries) => {
    if (entries == null) return;
    if (typeof entries !== "object" || Array.isArray(entries)) {
      finding("medium", "mcp", source, "mcpServers is not an object; server inventory incomplete.");
      return;
    }
    for (const [name, config] of Object.entries(entries)) servers.push({ name, source, config });
  };
  const claude = configFile(path.join(HOME, ".claude.json"));
  push("<home>/.claude.json (user)", claude?.mcpServers);
  push("<home>/.claude.json (project)", claude?.projects?.[workspace]?.mcpServers);
  push(".mcp.json", configFile(path.join(workspace, ".mcp.json"))?.mcpServers);
  push("<home>/.cursor/mcp.json", configFile(path.join(HOME, ".cursor", "mcp.json"))?.mcpServers);
  return servers;
}

function probe(server, workspace, timeoutMs = 20_000) {
  const config = server.config;
  if (!config || typeof config !== "object" || Array.isArray(config)) return Promise.resolve({ error: "invalid server configuration" });
  const { command, args: serverArgs = [], env = {} } = config;
  if (!command) return Promise.resolve({ error: "not stdio; not probed" });
  if (typeof command !== "string" || !Array.isArray(serverArgs) || serverArgs.some(arg => typeof arg !== "string") || !env || typeof env !== "object" || Array.isArray(env)) return Promise.resolve({ error: "invalid stdio command, arguments, or environment" });
  return new Promise((resolve) => {
    let child;
    try {
      child = spawn(command, serverArgs, { env: { ...process.env, ...env }, stdio: ["pipe", "pipe", "pipe"], cwd: workspace });
    } catch {
      resolve({ error: "spawn failed (invalid process options)" });
      return;
    }
    const pending = new Map();
    let buffer = "";
    child.stderr.resume();
    child.on("exit", (code) => done({ error: `exited with code ${code} before answering` }));
    let settled = false;
    const done = (result) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      child.kill();
      resolve(result);
    };
    const timer = setTimeout(() => done({ error: `no answer after ${timeoutMs} ms` }), timeoutMs);
    child.on("error", (error) => done({ error: `spawn failed (${error.code ?? "unknown"})` }));
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
        } catch {
          // Ignore non-JSON log lines; the outstanding request still has a deadline.
        }
      }
    });
    const request = (id, method, params) =>
      new Promise((answer) => {
        pending.set(id, answer);
        child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
      });
    (async () => {
      const init = responseResult(await request(1, "initialize", { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "context-audit", version: "1" } }), "initialize");
      if (init.instructions != null && typeof init.instructions !== "string") throw new Error("initialize: invalid instructions");
      child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" })}\n`);
      const tools = [];
      let cursor;
      const seen = new Set();
      for (let page = 0; ; page += 1) {
        const listed = responseResult(await request(2 + page, "tools/list", cursor ? { cursor } : {}), "tools/list");
        if (!Array.isArray(listed.tools) || listed.tools.some(tool => !tool || typeof tool !== "object" || typeof tool.name !== "string" || (tool.description != null && typeof tool.description !== "string"))) throw new Error("tools/list: invalid tools");
        tools.push(...listed.tools);
        cursor = listed.nextCursor;
        if (cursor == null || cursor === "") break;
        if (typeof cursor !== "string" || seen.has(cursor)) throw new Error("tools/list: invalid or repeated cursor; measurement incomplete");
        seen.add(cursor);
      }
      done({ instructions: init.instructions ?? "", serverInfo: init.serverInfo, tools });
    })().catch((error) => done({ error: String(error) }));
  });
}

export async function collectMcp(workspace, probeServers) {
  const servers = [];
  for (const server of configuredServers(workspace)) {
    const probed = probeServers ? await probe(server, workspace) : { skipped: "probe disabled" };
    const measured = !probed.error && !probed.skipped;
    const tools = (probed.tools ?? []).map((tool) => {
      const description = tool.description ?? "";
      const schema = JSON.stringify(tool.inputSchema ?? {});
      const annotations = JSON.stringify(tool.annotations ?? {});
      const total = JSON.stringify(tool).length;
      return { name: tool.name, descriptionChars: description.length, schemaChars: schema.length, totalChars: total, annotations: annotations.length > 2 };
    });
    const instructionsChars = (probed.instructions ?? "").length;
    const total = instructionsChars + tools.reduce((sum, tool) => sum + tool.totalChars, 0);
    servers.push({ name: server.name, source: server.source, measurement: measured ? "measured" : probed.skipped ? "skipped" : "failed", error: probed.error, skipped: probed.skipped, serverInfo: probed.serverInfo, instructionsChars: measured ? instructionsChars : null, toolCount: measured ? tools.length : null, totalChars: measured ? total : null, totalTokens: measured ? tokens(total) : null, tools });
    if (probed.error) finding(isStdio(server.config) ? "high" : "low", "mcp", server.name, `Not measured: ${probed.error}.`);
    if (instructionsChars > BUDGET.mcpInstructions) finding("medium", "mcp", `${server.name} instructions`, `${instructionsChars} chars of server instructions every session; budget ${BUDGET.mcpInstructions}.`, { chars: instructionsChars });
    if (total > BUDGET.mcpServerTotal) finding("high", "mcp", server.name, `${tools.length} tools cost ${total} chars (~${tokens(total)} tokens) when loaded; budget ${BUDGET.mcpServerTotal}.`, { chars: total });
    for (const tool of tools) {
      if (tool.descriptionChars > BUDGET.mcpToolDescription) finding("low", "mcp", `${server.name}/${tool.name}`, `Description ${tool.descriptionChars} chars; budget ${BUDGET.mcpToolDescription}.`, { chars: tool.descriptionChars });
      if (tool.schemaChars > BUDGET.mcpToolSchema) finding("medium", "mcp", `${server.name}/${tool.name}`, `Input schema ${tool.schemaChars} chars; budget ${BUDGET.mcpToolSchema}.`, { chars: tool.schemaChars });
    }
  }
  return servers;
}
