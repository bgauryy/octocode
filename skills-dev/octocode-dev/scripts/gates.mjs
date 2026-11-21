#!/usr/bin/env node
// The release gate set as one command: `node skills-dev/octocode-dev/scripts/dev.mjs gates`.
//
// A small dependency graph instead of a hand-written chain of commands:
//   1. build        the host binary every later gate reads
//   2. side by side lint, contract check, config tests, docs, skills, the Rust suite
//                   (cargo work queues on the machine-wide build slots by itself)
//   3. core         the sibling core's tests and typecheck, alone: they time out
//                   when they share the machine with the Rust suite
// Every gate runs to its end whatever the others do, except that a failed build
// skips the rest (they would only repeat its failure). Output goes to one log per
// gate; the summary table states each gate's exit, time and the line that counts.
// Exit 0 only when every gate passed.
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { ROOT } from './lib/workspaces.mjs';

const DEV = path.join(path.dirname(fileURLToPath(import.meta.url)), 'dev.mjs');
const CORE = path.resolve(ROOT, '..', 'octocode-mcp-host', 'packages', 'octocode-core');

const dev = (...args) => ['node', [DEV, ...args]];
const yarn = (...args) => ['yarn', args];
const last = pattern => log => log.match(pattern)?.at(-1)?.trim();

/**
 * The gates by stage. A stage starts when the one before it has ended; the gates
 * of a stage run side by side. `fact` picks the line of a gate's log worth
 * reading in the summary; `cwd` defaults to the repository root.
 */
export const STAGES = [
  [{ name: 'build:dev', run: dev('build:dev'), fact: last(/^build: .*$/gm), stops: true }],
  [
    { name: 'check', run: dev('check'), fact: last(/^rust-lint: .*$/gm) },
    { name: 'check:tool-contract', run: yarn('workspace', '@octocodeai/config', 'check:tool-contract') },
    { name: 'config tests', run: yarn('workspace', '@octocodeai/config', 'test'), fact: last(/Tests\s+.*$/gm) },
    { name: 'docs:verify', run: dev('docs:verify') },
    { name: 'skills:check', run: dev('skills:check') },
    {
      name: 'test:rust',
      run: yarn('test:rust'),
      fact: log => [last(/^=== binaries: .*$/gm)(log), last(/Summary \[.*$/gm)(log)].filter(Boolean).join(' | ') || undefined,
    },
  ],
  [
    { name: 'core tests', run: yarn('vitest', 'run'), cwd: CORE, fact: last(/Tests\s+.*$/gm), needs: CORE },
    { name: 'core typecheck', run: yarn('typecheck'), cwd: CORE, needs: CORE },
  ],
];

/** Run one gate into its log; resolves with its row for the summary. */
function runGate(gate, logDir) {
  const log = path.join(logDir, `${gate.name.replace(/[^a-z0-9]+/gi, '-')}.log`);
  if (gate.needs && !fs.existsSync(gate.needs)) {
    return Promise.resolve({ gate, state: 'skipped', note: `${path.relative(ROOT, gate.needs)} is not checked out`, log });
  }
  const started = Date.now();
  const out = fs.openSync(log, 'w');
  const [command, args] = gate.run;
  return new Promise(resolve => {
    const child = spawn(command, args, { cwd: gate.cwd ?? ROOT, stdio: ['ignore', out, out] });
    const done = (code, note) => {
      fs.closeSync(out);
      const seconds = Math.round((Date.now() - started) / 1000);
      const text = fs.readFileSync(log, 'utf8');
      resolve({ gate, state: code === 0 ? 'pass' : 'FAIL', seconds, note: note ?? gate.fact?.(text), log });
    };
    child.on('error', error => done(1, `did not start: ${error.message}`));
    child.on('close', code => done(code ?? 1));
  });
}

function summary(rows) {
  const width = Math.max(...rows.map(({ gate }) => gate.name.length));
  return rows
    .map(({ gate, state, seconds, note }) => {
      const time = seconds === undefined ? '' : `${seconds}s`;
      return `${state.padEnd(7)} ${gate.name.padEnd(width)}  ${time.padStart(5)}  ${note ?? ''}`.trimEnd();
    })
    .join('\n');
}

async function main() {
  const logDir = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-gates-'));
  console.log(`gates: logs in ${logDir}`);
  const rows = [];
  let stopped;
  for (const stage of STAGES) {
    if (stopped) {
      rows.push(...stage.map(gate => ({ gate, state: 'skipped', note: `${stopped} failed` })));
      continue;
    }
    console.log(`gates: ${stage.map(gate => gate.name).join(', ')}`);
    const ended = await Promise.all(stage.map(gate => runGate(gate, logDir)));
    rows.push(...ended);
    stopped = ended.find(({ gate, state }) => gate.stops && state === 'FAIL')?.gate.name;
  }
  console.log(`\n${summary(rows)}`);
  const failed = rows.filter(({ state }) => state === 'FAIL');
  for (const { gate, log } of failed) console.log(`\n${gate.name}: ${log}`);
  process.exit(failed.length === 0 ? 0 : 1);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
