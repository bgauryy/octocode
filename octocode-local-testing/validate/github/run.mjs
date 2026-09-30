#!/usr/bin/env node
// Usage: node run.mjs <taskId> <side:gh|oc> <step> '<shell command>'
// Runs the command through /bin/sh, measures wall ms, stdout chars (what an agent reads),
// stderr chars, exit code. Appends one JSON line to runs.jsonl and saves stdout under out/.
import { spawnSync } from 'node:child_process';
import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const [taskId, side, step, cmd] = process.argv.slice(2);
if (!cmd) { console.error('usage: run.mjs task side step cmd'); process.exit(2); }
mkdirSync(join(here, 'out'), { recursive: true });
const t0 = process.hrtime.bigint();
const r = spawnSync('/bin/sh', ['-c', cmd], { encoding: 'utf8', maxBuffer: 512 * 1024 * 1024, cwd: '/tmp' });
const ms = Number(process.hrtime.bigint() - t0) / 1e6;
const stdout = r.stdout ?? '';
const stderr = r.stderr ?? '';
const file = `out/${taskId}.${side}.${step}.txt`;
writeFileSync(join(here, file), stdout + (stderr ? `\n--- STDERR ---\n${stderr}` : ''));
const rec = {
  taskId, side, step, cmd, exit: r.status, ms: Math.round(ms),
  stdoutChars: [...stdout].length, stderrChars: [...stderr].length,
  // agent reads stdout+stderr (a shell tool returns both); report both, count both
  readChars: [...stdout].length + [...stderr].length,
  out: file, at: new Date().toISOString(),
};
appendFileSync(join(here, 'runs.jsonl'), JSON.stringify(rec) + '\n');
console.log(JSON.stringify({ exit: rec.exit, ms: rec.ms, stdoutChars: rec.stdoutChars, stderrChars: rec.stderrChars }));
const show = process.env.SHOW ? Number(process.env.SHOW) : 3000;
console.log(stdout.slice(0, show));
if (stderr) console.log('--- STDERR ---\n' + stderr.slice(0, 1500));
