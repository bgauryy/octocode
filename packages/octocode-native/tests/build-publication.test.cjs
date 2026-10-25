'use strict';

const assert = require('node:assert/strict');
const { test } = require('node:test');
const { mkdtempSync, readFileSync, readdirSync, statSync, writeFileSync, rmSync } = require('node:fs');
const { join } = require('node:path');
const { tmpdir } = require('node:os');

const json = path => JSON.parse(readFileSync(path, 'utf8'));

test('every native build script routes through build-native.cjs', () => {
  const scripts = json(join(__dirname, '..', 'package.json')).scripts;
  const { PLATFORMS } = require('../bin/platform.cjs');
  const driver = 'node scripts/build-native.cjs';
  assert.equal(scripts.build, `${driver} --release`);
  assert.equal(scripts['build:dev'], driver);
  assert.equal(scripts['build:all'], `yarn clean:binaries && ${driver} --release --all`);
  assert.equal(scripts['build:target'], `${driver} --release --target`);
  assert.equal(scripts['check:target'], `${driver} --check --target`);
  for (const platform of Object.keys(PLATFORMS)) assert.equal(scripts[`build:${platform}`], undefined, platform);
  for (const [name, command] of Object.entries(scripts)) {
    if (name.startsWith('build')) assert.doesNotMatch(command, /cargo build|--only/, name);
  }
});

test('test:rust is nextest runs, and bacon test jobs use the same selections', () => {
  const { testRustSelections } = require('../scripts/test-rust-selections.cjs');
  const selections = testRustSelections();
  assert.ok(selections.length > 0);
  for (const selection of selections) assert.equal(selection[0], '--locked');
  const bacon = readFileSync(join(__dirname, '..', 'bacon.toml'), 'utf8').replace(/,\s+/g, ', ');
  for (const selection of selections) {
    assert.ok(bacon.includes(selection.map(arg => JSON.stringify(arg)).join(', ')), selection.join(' '));
  }
  assert.throws(() => testRustSelections({ scripts: { 'test:rust': 'cargo test -p x' } }), /not "cargo nextest run/);
});

test('cargo dev-test is the first test:rust selection under libtest', () => {
  const { testRustSelections } = require('../scripts/test-rust-selections.cjs');
  const config = readFileSync(join(__dirname, '..', '.cargo', 'config.toml'), 'utf8');
  const alias = /^dev-test = "test (.+)"$/m.exec(config);
  assert.ok(alias, 'the dev-test alias is defined');
  assert.deepEqual(alias[1].split(/\s+/), testRustSelections()[0]);
});

test('host build selects the CLI crate in one locked cargo invocation', () => {
  const { PLATFORMS } = require('../bin/platform.cjs');
  const { GLIBC_FLOOR, hostsCommand } = require('../scripts/build-native.cjs');
  for (const platform of [null, ...Object.keys(PLATFORMS)]) {
    for (const mode of [{ release: true }, { check: true }]) {
      const { command, args } = hostsCommand(platform, mode, '/tmp/hosts');
      const line = args.join(' ');
      assert.match(command, /^cargo(-zigbuild)?$/);
      assert.match(line, /-p octocode-cli --bins/);
      assert.match(line, /--locked/);
      assert.match(line, /--target-dir \/tmp\/hosts/);
      assert.doesNotMatch(line, /--all-features/);
      assert.equal(line.includes('--release'), Boolean(mode.release), line);
      if (platform) assert.match(line, new RegExp(`--target ${PLATFORMS[platform].triple}`));
      // A check never links: every cross tool runs its `check` subcommand.
      if (mode.check) assert.ok([args[0], args[1]].includes('check'), `${command} ${line}`);
      // zigbuild links gnu targets against Node's glibc floor, not zig's default.
      if (command === 'cargo-zigbuild' && PLATFORMS[platform].libc === 'gnu') {
        assert.match(line, new RegExp(`--target ${PLATFORMS[platform].triple}\\.${GLIBC_FLOOR.replace('.', '\\.')} `));
      }
    }
  }
});

test('host and platform builds never share a target dir', () => {
  const { PLATFORMS } = require('../bin/platform.cjs');
  const { targetDir } = require('../scripts/build-native.cjs');
  const dirs = [targetDir(null), ...Object.keys(PLATFORMS).map(platform => targetDir(platform))];
  assert.equal(new Set(dirs).size, dirs.length);
});

test('stageFile replaces the destination inode instead of overwriting it', t => {
  const { stageFile } = require('../scripts/stage-binary.cjs');
  const root = mkdtempSync(join(tmpdir(), 'octocode-stage-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  writeFileSync(join(root, 'source'), 'new');
  writeFileSync(join(root, 'destination'), 'old');
  const before = statSync(join(root, 'destination')).ino;
  stageFile(join(root, 'source'), join(root, 'destination'), { executable: true });
  assert.equal(readFileSync(join(root, 'destination'), 'utf8'), 'new');
  assert.notEqual(statSync(join(root, 'destination')).ino, before);
  assert.deepEqual(readdirSync(root).sort(), ['destination', 'source']);
});


test('stageFile keeps unchanged bytes on their existing inode', t => {
  const { stageFile } = require('../scripts/stage-binary.cjs');
  const root = mkdtempSync(join(tmpdir(), 'octocode-stage-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const source = join(root, 'source');
  const destination = join(root, 'destination');
  const original = Buffer.alloc(128 * 1024 + 3, 65);
  writeFileSync(source, original);
  stageFile(source, destination, { executable: true });
  const first = statSync(destination);
  stageFile(source, destination, { executable: true });
  assert.equal(statSync(destination).ino, first.ino);
  assert.equal(statSync(destination).mtimeMs, first.mtimeMs);
  const changed = Buffer.from(original);
  changed[64 * 1024 + 1] = 66;
  writeFileSync(source, changed);
  stageFile(source, destination, { executable: true });
  assert.notEqual(statSync(destination).ino, first.ino);
  assert.deepEqual(readFileSync(destination), changed);
  assert.deepEqual(readdirSync(root).sort(), ['destination', 'source']);
});

test('runForwardingSignals passes exit codes through and maps signal deaths to 128+N', async () => {
  const { runForwardingSignals } = require('../bin/launch-native.cjs');
  assert.equal(await runForwardingSignals(process.execPath, ['-e', 'process.exit(7)']), 7);
  assert.equal(
    await runForwardingSignals(process.execPath, ['-e', "process.kill(process.pid, 'SIGTERM')"]),
    128 + require('node:os').constants.signals.SIGTERM
  );
  await assert.rejects(runForwardingSignals(join(tmpdir(), 'octocode-no-such-binary'), []));
});

test('takeSignals owns the forwarded signals until restore', () => {
  const { takeSignals } = require('../bin/launch-native.cjs');
  const previous = () => {};
  process.on('SIGHUP', previous);
  const handler = () => {};
  const restore = takeSignals(handler);
  assert.deepEqual(process.listeners('SIGHUP'), [handler]);
  restore();
  restore();
  assert.deepEqual(process.listeners('SIGHUP'), [previous]);
  process.removeListener('SIGHUP', previous);
});

test('each platform package ships exactly the staged binary', () => {
  const { PLATFORMS, executableName } = require('../bin/platform.cjs');
  for (const [platform, { os }] of Object.entries(PLATFORMS)) {
    const files = json(join(__dirname, '..', 'npm', platform, 'package.json')).files;
    assert.deepEqual([...files].sort(), [executableName('octocode', os), 'README.md'].sort(), platform);
  }
});
