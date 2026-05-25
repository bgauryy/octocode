#!/usr/bin/env node
// mcp-meas.mjs — Transparent MCP stdio proxy. Logs every tools/call as
// {q, agent: "octocode", cmd, in_chars, out_chars, elapsed_ms, exit} to $LOG.
//
// MCP stdio transport is newline-delimited JSON-RPC. Each line is one
// complete message. The proxy spawns the real server, forwards every line
// in both directions, and records request/response pairs by JSON-RPC `id`.
// Non-tool-call traffic (initialize, tools/list, notifications) is forwarded
// without logging, so the log contains only tool usage.
//
// Usage in agent's MCP config:
//   {
//     "command": "node",
//     "args": ["docs/dev/benchmark/github/scripts/mcp-meas.mjs", "octocode-mcp"],
//     "env": { "LOG": "<run>/log.jsonl", "Q": "1" }
//   }
//
// Env:
//   LOG  required — JSONL log path (same as gh-meas.sh)
//   Q    required — current question number (operator updates between questions)

import { spawn } from 'child_process';
import { createInterface } from 'readline';
import { appendFileSync } from 'fs';

const LOG = process.env.LOG;
const Q = process.env.Q ?? '0';
if (!LOG) { console.error('mcp-meas: $LOG required'); process.exit(2); }
if (!process.argv[2]) { console.error('Usage: mcp-meas.mjs <server-cmd> [args...]'); process.exit(2); }

const child = spawn(process.argv[2], process.argv.slice(3), {
  stdio: ['pipe', 'pipe', 'inherit'],
  env: process.env,
});

const pending = new Map(); // id -> { name, in_chars, t0 }
const cps = (s) => [...s].length;

createInterface({ input: process.stdin }).on('line', (line) => {
  try {
    const m = JSON.parse(line);
    if (m.method === 'tools/call' && m.id !== undefined) {
      pending.set(m.id, { name: m.params?.name ?? '?', in_chars: cps(line), t0: Date.now() });
    }
  } catch {}
  child.stdin.write(line + '\n');
}).on('close', () => child.stdin.end());

createInterface({ input: child.stdout }).on('line', (line) => {
  process.stdout.write(line + '\n');
  try {
    const m = JSON.parse(line);
    if (m.id !== undefined && pending.has(m.id)) {
      const p = pending.get(m.id);
      pending.delete(m.id);
      appendFileSync(LOG, JSON.stringify({
        ts: new Date().toISOString(),
        q: +Q,
        agent: 'octocode',
        cmd: p.name,
        in_chars: p.in_chars,
        out_chars: cps(line),
        elapsed_ms: Date.now() - p.t0,
        exit: m.error ? 1 : 0,
      }) + '\n');
    }
  } catch {}
});

child.on('exit', (code) => process.exit(code ?? 0));
process.on('SIGINT', () => child.kill('SIGINT'));
process.on('SIGTERM', () => child.kill('SIGTERM'));
