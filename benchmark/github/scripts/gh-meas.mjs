#!/usr/bin/env node
// gh-meas.mjs — Run a gh command, log character counts and elapsed_ms to $LOG.
//
// Drop-in replacement for gh-meas.sh. Timing and character-counting run in-process.
//
// Ruler: same as mcp-meas.mjs.
//   in_chars  = Unicode codepoints of argv TAIL (no "gh " prefix)
//   out_chars = Unicode codepoints of stdout + stderr produced by the command
//
// Usage: node gh-meas.mjs <gh args...>   (or via gh-meas.sh wrapper)
// Env:   LOG (jsonl path, required), RUN (run dir, required)
import { spawnSync } from 'child_process';
import { appendFileSync, readFileSync, existsSync } from 'fs';
import { join } from 'path';

const LOG = process.env.LOG;
const RUN = process.env.RUN;
if (!LOG) { console.error('gh-meas: $LOG required'); process.exit(2); }
if (!RUN) { console.error('gh-meas: $RUN required'); process.exit(2); }

const cps = (s) => [...String(s ?? '')].length;

const SENTINEL = join(RUN, '.current-q');
if (!existsSync(SENTINEL)) {
  console.error(`gh-meas: ${SENTINEL} missing — run scripts/set-q.sh <n> first`);
  process.exit(2);
}
const raw = readFileSync(SENTINEL, 'utf8').trim();
const Q = parseInt(raw, 10);
if (!Number.isFinite(Q) || !/^\d+$/.test(raw)) {
  console.error(`gh-meas: invalid Q in sentinel: ${raw}`);
  process.exit(2);
}

const ghArgs = process.argv.slice(2);
const argsStr = ghArgs.join(' ');
const cmd = `gh ${argsStr}`;

// in_chars: payload ruler — argv tail only, no "gh " prefix.
const inChars = cps(argsStr);

// Run gh. stdin is inherited (gh may need it for auth prompts).
// stdout + stderr are captured for character-counting and forwarding.
const t0 = Date.now();
const result = spawnSync('gh', ghArgs, {
  encoding: 'buffer',
  stdio: ['inherit', 'pipe', 'pipe'],
  maxBuffer: 50 * 1024 * 1024,
});
const elapsed = Date.now() - t0;

const stdout = result.stdout ? result.stdout.toString('utf8') : '';
const stderr = result.stderr ? result.stderr.toString('utf8') : '';

// out_chars: combined stdout + stderr, matching the benchmark ruler.
const outChars = cps(stdout + stderr);
const exit = result.status ?? 1;

appendFileSync(LOG, JSON.stringify({
  ts:          new Date().toISOString(),
  q:           Q,
  agent:       'gh',
  cmd,
  in_chars:    inChars,
  out_chars:   outChars,
  elapsed_ms:  elapsed,
  exit,
}) + '\n');

// Forward output to calling process.
if (stdout) process.stdout.write(stdout);
if (stderr) process.stderr.write(stderr);
process.exit(exit);
