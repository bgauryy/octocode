import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { compare, growthByCycle, growthBySize, parseRows } from '../compare-runs.mjs';

const script = fileURLToPath(new URL('../compare-runs.mjs', import.meta.url));
const run = (args, input) => spawnSync(process.execPath, [script, ...args], { encoding: 'utf8', input });
const rounds = (variant, values) => values.map((cpuMs, i) => ({ variant, round: i + 1, cpuMs }));

test('a difference counts only when it exceeds both variants\' run-to-run spread', () => {
  const clear = compare([...rounds('old', [100, 104, 102]), ...rounds('new', [80, 83, 81])]);
  const cpu = clear.metrics.cpuMs;
  assert.equal(clear.base, 'old');
  assert.equal(cpu.old.median, 102);
  assert.equal(cpu.old.spread, 4);
  assert.equal(cpu.difference, cpu.new.median - cpu.old.median);
  assert.ok(Math.abs(cpu.difference) > Math.max(cpu.old.spread, cpu.new.spread));
  assert.equal(cpu.verdict, 'lower');
  const noisy = compare([...rounds('old', [100, 140, 120]), ...rounds('new', [95, 135, 110])]).metrics.cpuMs;
  assert.equal(noisy.exceedsSpread, false);
  assert.equal(noisy.verdict, 'within-spread');
  assert.equal(compare([...rounds('old', [1]), ...rounds('new', [2, 3])]).metrics.cpuMs.verdict, 'insufficient-rounds');
  assert.equal(compare([...rounds('a', [5, 5]), ...rounds('b', [9, 9])], 'b').metrics.cpuMs.verdict, 'lower', '--base picks the baseline');
  assert.throws(() => compare(rounds('old', [1, 2])), /exactly two variants/);
});

test('rows parse from CSV, JSON lines, JSON arrays and a driver result; warmup rows are skipped', () => {
  const csv = parseRows('variant,round,cpuMs,label\nold,1,10,"a, b"\nnew,1,8,x\n');
  assert.deepEqual(csv[0], { variant: 'old', round: 1, cpuMs: 10, label: 'a, b' });
  assert.equal(parseRows('{"variant":"old","cpuMs":1}\n{"variant":"new","cpuMs":2}\n').length, 2);
  assert.equal(parseRows('[{"variant":"old","cpuMs":1}]').length, 1);
  assert.equal(parseRows('{"ok":true,"rounds":[{"variant":"old","cpuMs":1}]}').length, 1);
  assert.throws(() => parseRows('variant,cpuMs\nold\n'), /fields/);
  const warmed = compare([{ variant: 'old', cpuMs: 999, warmup: true }, ...rounds('old', [10, 11]), ...rounds('new', [10, 11])]).metrics.cpuMs;
  assert.equal(warmed.old.n, 2);
});

test('size growth reports the log-log exponent; cycle growth tells a leak from a plateau', () => {
  const sizes = [2, 10, 40];
  const linear = growthBySize(sizes.flatMap((size) => [{ size, ms: 3 * size }, { size, ms: 3 * size + 0.1 }])).results[0];
  assert.equal(linear.nearestPower, 1);
  const quadratic = growthBySize(sizes.map((size) => ({ size, ms: size * size }))).results[0];
  assert.equal(quadratic.nearestPower, 2);
  assert.equal(growthBySize([{ size: 2, ms: 1 }, { size: 4, ms: 2 }]).results[0].nearestPower, null, 'two sizes cannot classify');
  assert.throws(() => growthBySize(sizes.map((size) => ({ size, ms: 0 }))), /above 0/);
  const cycles = Array.from({ length: 12 }, (_, i) => i + 1);
  const wobble = (i) => (i % 2 ? 0.3 : -0.3);
  const leak = growthByCycle(cycles.map((cycle) => ({ cycle, heapMiB: 50 + 2 * cycle + wobble(cycle) }))).results[0];
  assert.equal(leak.verdict, 'growing');
  assert.ok(leak.perCycle > 1.9 && leak.perCycle < 2.1);
  const cache = growthByCycle(cycles.map((cycle) => ({ cycle, heapMiB: 50 + 5 * Math.min(cycle, 6) + wobble(cycle) }))).results[0];
  assert.equal(cache.verdict, 'plateau');
  const flat = growthByCycle(cycles.map((cycle) => ({ cycle, heapMiB: 50 + wobble(cycle) }))).results[0];
  assert.equal(flat.verdict, 'flat');
  assert.equal(growthByCycle(cycles.slice(0, 5).map((cycle) => ({ cycle, heapMiB: cycle }))).results[0].verdict, 'insufficient-cycles');
});

test('the driver alternates rounds, measures child CPU and parses a printed counter; failures exit 1, bad input 2', (t) => {
  const busy = (n) => `"${process.execPath}" -e "let s=0;for(let i=0;i<${n};i++)s+=i;console.log('bytes/op', ${n})"`;
  const out = run(['--old', busy(1e6), '--new', busy(2e6), '--rounds', '2', '--warmup', '1', '--parse', 'bytes/op (\\d+)']);
  assert.equal(out.status, 0, out.stdout + out.stderr);
  const result = JSON.parse(out.stdout);
  assert.deepEqual(result.rounds.map((row) => `${row.variant}${row.warmup ? '*' : ''}`), ['old*', 'new*', 'old', 'new', 'old', 'new']);
  assert.ok(result.rounds.every((row) => row.cpuMs > 0 && row.wallMs > 0));
  assert.deepEqual(result.metrics.parsed.new.values, [2e6, 2e6]);
  assert.equal(result.metrics.parsed.verdict, 'higher');
  const failed = run(['--old', 'true', '--new', 'echo broken >&2; exit 3', '--rounds', '1']);
  assert.equal(failed.status, 1);
  const error = JSON.parse(failed.stdout);
  assert.equal(error.error.code, 'command-failed');
  assert.match(error.error.message, /exited 3: broken/);
  assert.equal(error.rounds.length, 2);
  assert.equal(run(['--old', 'true', '--rounds', '1']).status, 2);
  assert.equal(run(['--file', '-'], 'variant,cpuMs\nonly,1\n').status, 2);
  const dir = mkdtempSync(join(tmpdir(), 'compare-runs-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  writeFileSync(join(dir, 'cycles.csv'), `cycle,heapMiB\n${Array.from({ length: 8 }, (_, i) => `${i + 1},${10 + i}`).join('\n')}\n`);
  const growth = run(['--file', join(dir, 'cycles.csv'), '--growth', 'cycle']);
  assert.equal(growth.status, 0, growth.stdout);
  assert.equal(JSON.parse(growth.stdout).results[0].verdict, 'growing');
  assert.equal(run(['--help']).status, 0);
});
