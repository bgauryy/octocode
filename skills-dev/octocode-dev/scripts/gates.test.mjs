import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { STAGES } from './gates.mjs';

const DEV = fileURLToPath(new URL('./dev.mjs', import.meta.url));
const gates = STAGES.flat();

test('the build runs alone and first, and a failed build stops the rest', () => {
  assert.equal(STAGES[0].length, 1);
  assert.equal(STAGES[0][0].name, 'build:dev');
  assert.equal(STAGES[0][0].stops, true);
  assert.deepEqual(gates.filter(gate => gate.stops), [STAGES[0][0]]);
});

test('the core gates run in the last stage, apart from the Rust suite', () => {
  const stageOf = name => STAGES.findIndex(stage => stage.some(gate => gate.name === name));
  const core = gates.filter(gate => gate.needs);
  assert.ok(core.length > 0);
  for (const gate of core) {
    assert.equal(stageOf(gate.name), STAGES.length - 1, gate.name);
    assert.ok(stageOf(gate.name) > stageOf('test:rust'), gate.name);
  }
});

test('every gate has its own name, so each writes its own log', () => {
  const names = gates.map(gate => gate.name.replace(/[^a-z0-9]+/gi, '-'));
  assert.equal(new Set(names).size, names.length);
});

test('every dev task a gate runs is a task the runner lists', () => {
  const { stdout } = spawnSync('node', [DEV, '--help'], { encoding: 'utf8' });
  const tasks = new Set([...stdout.matchAll(/^ {2}(\S+) {2}/gm)].map(match => match[1]));
  assert.ok(tasks.has('gates'), 'the runner lists the gate set itself');
  for (const { name, run: [command, args] } of gates) {
    if (command === 'node' && args[0] === DEV) assert.ok(tasks.has(args[1]), `${name}: ${args[1]}`);
  }
});

test('contract regeneration is one locked run that typechecks core before anything else', async () => {
  const { STEPS } = await import('./contracts-regen.mjs');
  assert.equal(STEPS[0][0], 'core typecheck');
  assert.deepEqual(STEPS.map(([what]) => what).slice(1), ['core build', 'install pinned dependencies', 'generate the tool contract', 'tool-flow route table']);
  const { stdout } = spawnSync('node', [DEV, '--help'], { encoding: 'utf8' });
  assert.match(stdout, /contracts:regen\s+Regenerate the tool contract, one run at a time/);
});
