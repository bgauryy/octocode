import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

/** The commands `dev.mjs <task> [extra...]` runs, as [command, args, options], with spawnSync stubbed. */
function plan(task, ...extra) {
  const url = new URL('./dev.mjs', import.meta.url);
  const source = fs.readFileSync(url, 'utf8')
    .replace(/^#!.*\n/, '')
    .replace(/^import .*;\n/gm, '')
    .replaceAll('import.meta.url', JSON.stringify(url.href));
  const commands = [];
  vm.runInNewContext(source, {
    path, fileURLToPath, ROOT: '/repository', SKILL_COLLECTIONS: [],
    skillTestCommand: () => ['node', ['skill-tests']],
    spawnSync: (command, args, options) => { commands.push([command, [...args], options]); return { status: 0 }; },
    console: { log() {}, error() {} },
    process: { argv: ['node', 'dev.mjs', task, ...extra], exit: code => { throw new Error(`exit ${code}`); } },
  });
  return commands;
}

const NATIVE = '/repository/packages/octocode-native';
const BUILD_SLOT = `${NATIVE}/scripts/build-slot.cjs`;

test('a normal development build never cleans shared artifacts, even for an oversized cache', () => {
  const commands = plan('build:dev');
  assert.equal(commands.length, 1, 'build planning must never add an implicit cleanup step');
  assert.match(commands[0][1][0], /workspace-health\.mjs$/);
  assert.ok(commands[0][1].includes('build'));
});

test('test:dev runs the test runner in dev mode from the native package, with the crate and filter last', () => {
  const [[command, args, options], ...rest] = plan('test:dev', '--crate', 'tool-local', '--', 'local_search::');
  assert.deepEqual(rest, []);
  assert.equal(command, 'node');
  // rust-tests.cjs takes the build slot itself, so the plan adds no slot wrapper.
  assert.deepEqual(args, [`${NATIVE}/scripts/rust-tests.cjs`, 'dev', '--crate', 'tool-local', '--', 'local_search::']);
  assert.equal(options.cwd, NATIVE);
});

test('the Rust quality tasks that compile run in a build slot', () => {
  for (const task of ['fuzz:smoke', 'fuzz', 'miri', 'llvm-lines']) {
    const [[command, args]] = plan(task);
    assert.equal(command, 'node', task);
    assert.deepEqual(args.slice(0, 3), [BUILD_SLOT, '--', 'node'], task);
    assert.match(args[3], /rust-quality\.mjs$/, task);
  }
});

test('Rust test selections never clean shared build artifacts', async () => {
  const { RUNS, UNIT, buildArgs, runArgs } = await import('../../../packages/octocode-native/scripts/rust-tests.cjs');
  const steps = [buildArgs(UNIT), ...RUNS.map(runArgs)];
  assert.ok(steps.every(args => !args.includes('clean')));
  // Cargo's JSON messages name the units each run used: target-hygiene.cjs keeps them through prune:rust.
  assert.equal(buildArgs(UNIT)[buildArgs(UNIT).indexOf('--message-format') + 1], 'json-render-diagnostics');
  for (const run of RUNS) {
    const args = runArgs(run);
    assert.equal(args[args.indexOf('--cargo-message-format') + 1], 'json-render-diagnostics', run.name);
  }
});
