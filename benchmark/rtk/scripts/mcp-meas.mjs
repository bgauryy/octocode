#!/usr/bin/env node
// mcp-meas.mjs — Transparent MCP stdio proxy.
//
// Logs every `tools/call` (request + response, paired by JSON-RPC `id`) to $LOG
// as one JSONL row with the same schema gh-meas.sh produces.
//
// === Ruler ===
// Same definition for both agents:
//   in_chars  = Unicode codepoints of the meaningful PAYLOAD the agent sent
//               (octocode: stringified `params.arguments`; gh: argv tail)
//   out_chars = Unicode codepoints of the meaningful RESULT the agent received
//               (octocode: concatenated `result.content[].text`;
//                gh: stdout+stderr)
// JSON-RPC envelope bytes (`jsonrpc`, `id`, `method`, etc.) are excluded so
// neither agent gets penalised for transport overhead.
//
// === MCP init context ===
// `initialize` and `tools/list` responses are ALSO logged (with q=0 and
// cmd="_initialize" / "_tools/list"). These represent the one-time cost the
// agent pays to load the server's instructions + tool schemas into context.
// gh has no equivalent (CLI doesn't push schemas into the LLM context), so
// counting these is what makes the comparison honest. They're attributed to
// q=0 regardless of the sentinel value so the metering is correct even if
// the operator reordered set-q.sh and the MCP handshake.
//
// === Per-question routing ===
// The current question id is read from a sentinel file at every `tools/call`
// (NOT from $Q at startup). This lets one long-lived MCP session serve all N
// questions in QUESTIONS.md while each call is attributed correctly.
//
// Sentinel file: $RUN/.current-q  (one ASCII integer, newline optional)
// Update it between questions with: bash scripts/set-q.sh <n>
//
// === Usage in agent's MCP config ===
//   {
//     "command": "node",
//     "args": ["benchmark/github/scripts/mcp-meas.mjs", "octocode-mcp"],
//     "env": { "RUN": "<run dir>", "LOG": "<run>/log.jsonl" }
//   }
//
// === Env ===
//   LOG  required — JSONL log path
//   RUN  required — run dir (used to locate .current-q sentinel)

import { spawn } from 'child_process';
import { createInterface } from 'readline';
import { appendFileSync, readFileSync, existsSync } from 'fs';
import { join } from 'path';

const LOG = process.env.LOG;
const RUN = process.env.RUN;
if (!LOG) { console.error('mcp-meas: $LOG required'); process.exit(2); }
if (!RUN) { console.error('mcp-meas: $RUN required'); process.exit(2); }
if (!process.argv[2]) { console.error('Usage: mcp-meas.mjs <server-cmd> [args...]'); process.exit(2); }

const cps = (s) => [...String(s ?? '')].length;

const SENTINEL = join(RUN, '.current-q');
const readQ = () => {
  if (!existsSync(SENTINEL)) return 0;
  const raw = readFileSync(SENTINEL, 'utf8').trim();
  const n = Number.parseInt(raw, 10);
  return Number.isFinite(n) && n > 0 ? n : 0;
};

// Payload extraction. Anything that's not the meaningful agent-visible content
// is excluded so the gh/octocode rulers match.
const reqPayloadChars = (msg) => cps(JSON.stringify(msg?.params?.arguments ?? {}));
const resPayloadChars = (msg) => {
  if (msg?.error) return cps(JSON.stringify(msg.error));
  const content = msg?.result?.content;
  if (!Array.isArray(content)) return cps(JSON.stringify(msg?.result ?? {}));
  return cps(content.map(c => c?.text ?? '').join(''));
};

const child = spawn(process.argv[2], process.argv.slice(3), {
  stdio: ['pipe', 'pipe', 'inherit'],
  env: process.env,
});

// Per JSON-RPC `id`, remember the inbound request so we can pair it with the
// matching response. `kind` decides how the row gets tagged:
//   - "call"  → q sampled from sentinel, cmd = tool name
//   - "init"  → q forced to 0, cmd = "_initialize" or "_tools/list"
const pending = new Map();

// For init responses we count the FULL result (capabilities + instructions for
// initialize, full tools array for tools/list) since the agent loads it all
// into its system context. There's no `result.content[].text` shape here.
const initOutChars = (msg) => {
  if (msg?.error) return cps(JSON.stringify(msg.error));
  return cps(JSON.stringify(msg?.result ?? {}));
};

createInterface({ input: process.stdin }).on('line', (line) => {
  try {
    const m = JSON.parse(line);
    if (m.id !== undefined) {
      if (m.method === 'tools/call') {
        pending.set(m.id, {
          kind: 'call',
          name: m.params?.name ?? '?',
          in_chars: reqPayloadChars(m),
          t0: Date.now(),
          q: readQ(),
        });
      } else if (m.method === 'initialize' || m.method === 'tools/list') {
        pending.set(m.id, {
          kind: 'init',
          name: '_' + m.method,                     // _initialize / _tools/list
          in_chars: reqPayloadChars(m),
          t0: Date.now(),
          q: 0,                                     // session-level, not per-Q
        });
      }
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
      const outChars = p.kind === 'init' ? initOutChars(m) : resPayloadChars(m);
      appendFileSync(LOG, JSON.stringify({
        ts: new Date().toISOString(),
        q: p.q,
        agent: 'octocode',
        cmd: p.name,
        in_chars: p.in_chars,
        out_chars: outChars,
        elapsed_ms: Date.now() - p.t0,
        exit: m.error ? 1 : 0,
      }) + '\n');
    }
  } catch {}
});

child.on('exit', (code) => process.exit(code ?? 0));
process.on('SIGINT', () => child.kill('SIGINT'));
process.on('SIGTERM', () => child.kill('SIGTERM'));
