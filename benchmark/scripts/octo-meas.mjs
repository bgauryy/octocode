#!/usr/bin/env node
// octo-meas.mjs — Run one Octocode CLI tool call, log character counts and elapsed_ms to $LOG.
//
// Drop-in equivalent of gh-meas.mjs for the octocode CLI agent.
// Timing and character-counting run in-process.
//
// Ruler: same as gh-meas.mjs.
//   in_chars  = Unicode codepoints of the queries JSON string (the meaningful input payload)
//   out_chars = Unicode codepoints of stdout produced by `octocode tools`
//
// Usage: node octo-meas.mjs <tool-name> '<queries-json>'
// Env:   LOG (jsonl path, required), RUN (run dir, required)

import { spawnSync } from 'child_process';
import { appendFileSync, readFileSync, existsSync } from 'fs';
import { join } from 'path';

const LOG = process.env.LOG;
const RUN = process.env.RUN;
if (!LOG) { console.error('octo-meas: $LOG required'); process.exit(2); }
if (!RUN) { console.error('octo-meas: $RUN required'); process.exit(2); }

const cps = (s) => [...String(s ?? '')].length;

const SENTINEL = join(RUN, '.current-q');
if (!existsSync(SENTINEL)) {
  console.error(`octo-meas: ${SENTINEL} missing — run benchmark/scripts/set-q.sh <n> first`);
  process.exit(2);
}
const raw = readFileSync(SENTINEL, 'utf8').trim();
const Q = parseInt(raw, 10);
if (!Number.isFinite(Q) || !/^\d+$/.test(raw)) {
  console.error(`octo-meas: invalid Q in sentinel: ${raw}`);
  process.exit(2);
}

const toolName = process.argv[2];
const queriesJson = process.argv[3];

if (!toolName || !queriesJson) {
  console.error("Usage: octo-meas.mjs <tool-name> '<queries-json>'");
  process.exit(2);
}

// in_chars: payload ruler — queries JSON string only (the meaningful input payload)
const inChars = cps(queriesJson);

// Run `node <octocode-cli.js> tools <tool-name> --queries '<queries-json>'`.
// Use local CLI binary. ALLOWED_PATHS lets local tools access the benchmark clone.
// stdout is captured as the tool result; stderr forwarded but not counted.
//
// OCTOCODE_CLI_BIN must be set to the local build path:
//   /Users/guybary/Documents/octocode-mcp/packages/octocode-cli/out/octocode-cli.js
// Falling back to a globally installed `octocode` is not allowed — it may be a
// different version and will produce results that cannot be compared to the local build.
const LOCAL_BUILD = '/Users/guybary/Documents/octocode-mcp/packages/octocode-cli/out/octocode-cli.js';
const CLI_BIN = process.env.OCTOCODE_CLI_BIN || LOCAL_BUILD;
if (!process.env.OCTOCODE_CLI_BIN) {
  console.error(`octo-meas: OCTOCODE_CLI_BIN not set — defaulting to local build: ${LOCAL_BUILD}`);
  console.error('  Set OCTOCODE_CLI_BIN explicitly to silence this warning.');
}
const CLI_ARGS = ['tools', toolName, '--queries', queriesJson];
const CLI_CMD = CLI_BIN.endsWith('.js') ? 'node' : CLI_BIN;
const CLI_ARGV = CLI_BIN.endsWith('.js') ? [CLI_BIN, ...CLI_ARGS] : CLI_ARGS;

const t0 = Date.now();
const result = spawnSync(CLI_CMD, CLI_ARGV, {
  encoding: 'buffer',
  stdio: ['inherit', 'pipe', 'pipe'],
  maxBuffer: 50 * 1024 * 1024,
  env: { ...process.env },
});
const elapsed = Date.now() - t0;

const stdout = result.stdout ? result.stdout.toString('utf8') : '';
const stderr = result.stderr ? result.stderr.toString('utf8') : '';

// out_chars: stdout only — the tool result text returned to the agent.
// stderr is diagnostic/logging noise, not tool payload, so excluded from the ruler.
const outChars = cps(stdout);
const exit = result.status ?? 1;

appendFileSync(LOG, JSON.stringify({
  ts:         new Date().toISOString(),
  q:          Q,
  agent:      'octocode',
  cmd:        toolName,
  in_chars:   inChars,
  out_chars:  outChars,
  elapsed_ms: elapsed,
  exit,
}) + '\n');

// Forward output to calling process.
if (stdout) process.stdout.write(stdout);
if (stderr) process.stderr.write(stderr);
process.exit(exit);
