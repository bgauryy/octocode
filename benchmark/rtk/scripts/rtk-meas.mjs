#!/usr/bin/env node
// rtk-meas.mjs — Run an rtk command, log character counts and elapsed_ms to $LOG.
//
// Mirrors the same ruler as gh-meas.mjs:
//   in_chars  = Unicode codepoints of argv TAIL (no "rtk " prefix)
//   out_chars = Unicode codepoints of stdout + stderr produced by the command
//
// Usage: node rtk-meas.mjs <rtk args...>   (or via rtk-meas.sh wrapper)
// Env:   LOG (jsonl path, required), RUN (run dir, required)
import { spawnSync } from 'child_process';
import { appendFileSync, readFileSync, existsSync } from 'fs';
import { join } from 'path';

const LOG = process.env.LOG;
const RUN = process.env.RUN;
if (!LOG) { console.error('rtk-meas: $LOG required'); process.exit(2); }
if (!RUN) { console.error('rtk-meas: $RUN required'); process.exit(2); }

const cps = (s) => [...String(s ?? '')].length;

const SENTINEL = join(RUN, '.current-q');
if (!existsSync(SENTINEL)) {
  console.error(`rtk-meas: ${SENTINEL} missing — run scripts/set-q.sh <n> first`);
  process.exit(2);
}
const raw = readFileSync(SENTINEL, 'utf8').trim();
const Q = parseInt(raw, 10);
if (!Number.isFinite(Q) || !/^\d+$/.test(raw)) {
  console.error(`rtk-meas: invalid Q in sentinel: ${raw}`);
  process.exit(2);
}

const rtkArgs = process.argv.slice(2);
const argsStr = rtkArgs.join(' ');
const cmd = `rtk ${argsStr}`;

// in_chars: payload ruler — argv tail only, no "rtk " prefix.
const inChars = cps(argsStr);

// Run rtk. stdin is inherited (rtk may need tty for some commands).
// stdout + stderr are captured for character-counting and forwarding.
const t0 = Date.now();
const result = spawnSync('rtk', rtkArgs, {
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
  agent:       'rtk',
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
