#!/usr/bin/env node
// Logging wrapper for benchmark solver agents.
// usage: node run-step.mjs <agentOutDir> <stepId> -- <command> [args...]
// Saves raw output to <agentOutDir>/raw/<stepId>.txt, appends a metrics line
// to <agentOutDir>/commands.ndjson, and echoes output so the agent can read it.
import { spawnSync } from 'node:child_process';
import { mkdirSync, appendFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

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
const start = Date.now();
const res = spawnSync(cmd[0], cmd.slice(1), {
  encoding: 'utf8',
  maxBuffer: 64 * 1024 * 1024,
});
const ms = Date.now() - start;
const stdout = res.stdout ?? '';
const stderr = res.stderr ?? '';
writeFileSync(
  join(outDir, 'raw', `${stepId}.txt`),
  stdout + (stderr ? `\n--- STDERR ---\n${stderr}` : '')
);
appendFileSync(
  join(outDir, 'commands.ndjson'),
  JSON.stringify({
    id: stepId,
    cmd: cmd.join(' '),
    exit: res.status ?? -1,
    ms,
    bytes: Buffer.byteLength(stdout) + Buffer.byteLength(stderr),
  }) + '\n'
);
process.stdout.write(stdout);
if (stderr) process.stderr.write(stderr);
process.exit(res.status ?? 1);
