#!/usr/bin/env node
// call-tool.mjs — Minimal MCP client that routes ONE tool call through mcp-meas.mjs
// so every call gets logged to $LOG with real in_chars/out_chars/elapsed_ms.
//
// Usage:
//   node call-tool.mjs <tool_name> '<queries_json>'
//
// Env: LOG, RUN (inherited from shell; same as mcp-meas.mjs requires).
// The question id is read by the proxy from $RUN/.current-q — set it via
// scripts/set-q.sh before calling this script.
//
// Stdout: the MCP tool result text (for the agent to read)
// The proxy writes the metrics line to $LOG automatically.

import { spawn } from 'child_process';
import { createInterface } from 'readline';
import { existsSync } from 'fs';
import { fileURLToPath } from 'url';
import { dirname, join, isAbsolute, resolve } from 'path';

const __dir = dirname(fileURLToPath(import.meta.url));
// Server resolution (first match wins):
//   1. $OCTOCODE_MCP_SERVER (absolute or relative-to-cwd path or command in PATH)
//   2. Conventional monorepo path: ../../../../../packages/octocode-mcp/dist/index.js
//   3. `octocode-mcp` on PATH (npm-global install)
let SERVER_CMD;
let SERVER_ARGS = [];
if (process.env.OCTOCODE_MCP_SERVER) {
  const v = process.env.OCTOCODE_MCP_SERVER;
  if (v.endsWith('.js') || v.endsWith('.mjs')) {
    SERVER_CMD = 'node';
    SERVER_ARGS = [isAbsolute(v) ? v : resolve(process.cwd(), v)];
  } else {
    SERVER_CMD = v;
  }
} else {
  const monorepo = join(__dir, '../../../../../packages/octocode-mcp/dist/index.js');
  if (existsSync(monorepo)) {
    SERVER_CMD = 'node';
    SERVER_ARGS = [monorepo];
  } else {
    SERVER_CMD = 'octocode-mcp';
  }
}

const toolName = process.argv[2];
const queriesRaw = process.argv[3];

if (!toolName || !queriesRaw) {
  console.error('Usage: call-tool.mjs <tool_name> \'<queries_json>\'');
  process.exit(1);
}

let queries;
try {
  queries = JSON.parse(queriesRaw);
  if (!Array.isArray(queries)) queries = [queries];
} catch (e) {
  console.error('Invalid queries JSON:', e.message);
  process.exit(1);
}

// Auto-fill required research fields if absent
queries = queries.map((q, i) => ({
  id: q.id ?? `q${i}`,
  mainResearchGoal: q.mainResearchGoal ?? 'benchmark research',
  researchGoal: q.researchGoal ?? 'benchmark research',
  reasoning: q.reasoning ?? 'benchmark',
  ...q,
}));

// Spawn mcp-meas.mjs (which spawns the real server)
const proxy = spawn('node', [
  join(__dir, 'mcp-meas.mjs'),
  SERVER_CMD,
  ...SERVER_ARGS,
], {
  stdio: ['pipe', 'pipe', 'inherit'],
  env: { ...process.env },
});

let msgId = 0;
const pending = new Map();
const send = (obj) => {
  const line = JSON.stringify(obj);
  proxy.stdin.write(line + '\n');
};

const rl = createInterface({ input: proxy.stdout });
rl.on('line', (line) => {
  let msg;
  try { msg = JSON.parse(line); } catch { return; }

  // Handle initialize result
  if (msg.id === 1 && msg.result?.capabilities) {
    // Initialized — now call the tool
    const id = ++msgId + 10;
    pending.set(id, true);
    send({
      jsonrpc: '2.0',
      id,
      method: 'tools/call',
      params: {
        name: toolName,
        arguments: { queries },
      },
    });
    return;
  }

  // Handle tool result
  if (msg.id !== undefined && msg.id > 10 && pending.has(msg.id)) {
    pending.delete(msg.id);
    if (msg.error) {
      console.error('Tool error:', JSON.stringify(msg.error));
      proxy.stdin.end();
      process.exit(1);
    }
    // Print result content
    const content = msg.result?.content ?? [];
    for (const c of content) {
      if (c.type === 'text') process.stdout.write(c.text + '\n');
    }
    proxy.stdin.end();
  }
});

proxy.on('close', () => process.exit(0));

// Step 1: initialize
send({
  jsonrpc: '2.0',
  id: 1,
  method: 'initialize',
  params: {
    protocolVersion: '2024-11-05',
    capabilities: {},
    clientInfo: { name: 'benchmark-client', version: '1.0' },
  },
});
// Step 2: send initialized notification
setTimeout(() => {
  send({ jsonrpc: '2.0', method: 'notifications/initialized', params: {} });
}, 100);
