import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

test('a normal development build never cleans shared artifacts, even for an oversized cache', () => {
  const url = new URL('./dev.mjs', import.meta.url);
  const source = fs.readFileSync(url, 'utf8')
    .replace(/^#!.*\n/, '')
    .replace(/^import .*;\n/gm, '')
    .replaceAll('import.meta.url', JSON.stringify(url.href));
  const commands = [];
  vm.runInNewContext(source, {
    path, fileURLToPath, ROOT: '/repository', SKILL_COLLECTIONS: [],
    skillTestCommand: () => ['node', ['skill-tests']],
    cargoIsRunning: () => false, exceedsSize: () => true,
    NATIVE_DEBUG_LIMIT_BYTES: 1, nativeDebugDir: () => '/repository/target/debug',
    spawnSync: (command, args) => { commands.push([command, [...args]]); return { status: 0 }; },
    console: { log() {}, error() {} },
    process: { argv: ['node', 'dev.mjs', 'build:dev'], exit: code => { throw new Error(`exit ${code}`); } },
  });
  assert.equal(commands.length, 1, 'build planning must never add an implicit cleanup step');
  assert.match(commands[0][1][0], /workspace-health\.mjs$/);
  assert.ok(commands[0][1].includes('build'));
});


test('Rust test selections never clean shared build artifacts', () => {
  const url = new URL('../../../packages/octocode-native/scripts/rust-tests.cjs', import.meta.url);
  const source = fs.readFileSync(url, 'utf8').replace(/^#!.*\n/, '');
  const commands = [];
  const module = { exports: {} };
  vm.runInNewContext(`${source}\nmain();`, {
    module, __dirname: path.dirname(fileURLToPath(url)),
    require: name => {
      if (name === 'node:path') return path;
      if (name === 'node:child_process') return {
        spawnSync: (command, args) => { commands.push([command, [...args]]); return { status: 0 }; },
      };
      throw new Error(`unexpected dependency ${name}`);
    },
    console: { log() {}, error() {} },
    process: { argv: ['node', 'rust-tests.cjs'], exit: code => { throw new Error(`exit ${code}`); } },
  });
  assert.equal(commands.length, module.exports.SELECTIONS.length);
  assert.ok(commands.every(([command, args]) => command === 'cargo' && args[0] === 'nextest'));
});
