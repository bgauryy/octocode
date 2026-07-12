#!/usr/bin/env node
// Logging wrapper for benchmark solver agents.
// usage: node run-step.mjs <agentOutDir> <stepId> -- <command> [args...]
// Saves raw output to <agentOutDir>/raw/<stepId>.txt, appends a metrics line
// to <agentOutDir>/commands.ndjson, and echoes output so the agent can read it.
import { spawnSync } from 'node:child_process';
import {
  mkdirSync,
  appendFileSync,
  writeFileSync,
  existsSync,
} from 'node:fs';
import { join } from 'node:path';
import { estimateTokens, tokenizerMethod } from './token-estimate.mjs';

const argv = process.argv.slice(2);
const sep = argv.indexOf('--');
const outDir = argv[0];
const stepId = argv[1];
const cmd = sep >= 0 ? argv.slice(sep + 1) : [];
if (!outDir || !stepId || sep !== 2 || cmd.length === 0) {
  console.error('usage: node run-step.mjs <agentOutDir> <stepId> -- <command> [args...]');
  process.exit(2);
}
mkdirSync(join(outDir, 'raw'), { recursive: true });
// Never overwrite prior evidence: a retried step id gets a .2/.3 suffix so
// the failed attempt's output stays auditable.
let rawName = `${stepId}.txt`;
if (existsSync(join(outDir, 'raw', rawName))) {
  let attempt = 2;
  while (existsSync(join(outDir, 'raw', `${stepId}.${attempt}.txt`)))
    attempt += 1;
  rawName = `${stepId}.${attempt}.txt`;
}
const start = Date.now();
const res = spawnSync(cmd[0], cmd.slice(1), {
  encoding: 'utf8',
  maxBuffer: 64 * 1024 * 1024,
});
const ms = Date.now() - start;
const stdout = res.stdout ?? '';
const stderr = res.stderr ?? '';
writeFileSync(
  join(outDir, 'raw', rawName),
  stdout + (stderr ? `\n--- STDERR ---\n${stderr}` : '')
);
// tokens is the primary cost metric (what actually enters the solver's
// context window); bytes is kept for backward-compat/audit reconciliation.
appendFileSync(
  join(outDir, 'commands.ndjson'),
  JSON.stringify({
    id: stepId,
    cmd: cmd.join(' '),
    exit: res.status ?? -1,
    ms,
    bytes: Buffer.byteLength(stdout) + Buffer.byteLength(stderr),
    tokens: estimateTokens(stdout) + estimateTokens(stderr),
    tokenizer: tokenizerMethod(),
  }) + '\n'
);
process.stdout.write(stdout);
if (stderr) process.stderr.write(stderr);
process.exit(res.status ?? 1);
