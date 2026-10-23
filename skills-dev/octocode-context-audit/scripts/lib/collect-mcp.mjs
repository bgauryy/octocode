// MCP cost: configured servers from Claude Code, project and Cursor configs; stdio servers
// are probed live (initialize + tools/list) and measured against budgets.
import { spawn } from "node:child_process";
import path from "node:path";
import { BUDGET, HOME, finding, readJson, tokens } from "./audit-common.mjs";

function configuredServers(workspace) {
  const servers = [];
  const push = (source, entries) => {
    for (const [name, config] of Object.entries(entries ?? {})) servers.push({ name, source, config });
  };
  const claude = readJson(path.join(HOME, ".claude.json"));
  push("<home>/.claude.json (user)", claude?.mcpServers);
  push("<home>/.claude.json (project)", claude?.projects?.[workspace]?.mcpServers);
  push(".mcp.json", readJson(path.join(workspace, ".mcp.json"))?.mcpServers);
  push("<home>/.cursor/mcp.json", readJson(path.join(HOME, ".cursor", "mcp.json"))?.mcpServers);
  return servers;
}

function probe(server, workspace, timeoutMs = 20_000) {
  const { command, args: serverArgs = [], env = {} } = server.config;
  if (!command) return Promise.resolve({ error: `not stdio (${server.config.type ?? server.config.url ?? "unknown"}); not probed` });
  return new Promise((resolve) => {
    const child = spawn(command, serverArgs, { env: { ...process.env, ...env }, stdio: ["pipe", "pipe", "pipe"], cwd: workspace });
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
