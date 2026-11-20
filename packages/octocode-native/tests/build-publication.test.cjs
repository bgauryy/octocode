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

test('every package script that compiles Rust holds a build slot', () => {
  const scripts = json(join(__dirname, '..', 'package.json')).scripts;
  const compiles = /\bcargo (?:build|check|clippy|hack|llvm-cov|mutants|nextest|test)\b/;
  const slotted = Object.entries(scripts).filter(([, command]) => compiles.test(command));
  assert.ok(slotted.length > 0);
  for (const [name, command] of slotted) assert.match(command, /^node scripts\/build-slot\.cjs -- cargo /, name);
});

test('test:rust and coverage:rust run the selections, and bacon test jobs repeat them', () => {
  const { SELECTIONS } = require('../scripts/rust-tests.cjs');
  const scripts = json(join(__dirname, '..', 'package.json')).scripts;
  assert.equal(scripts['test:rust'], 'node scripts/rust-tests.cjs');
  assert.equal(scripts['coverage:rust'], 'node scripts/rust-tests.cjs --coverage');
  assert.ok(SELECTIONS.length > 0);
  for (const selection of SELECTIONS) assert.equal(selection[0], '--locked');
  const bacon = readFileSync(join(__dirname, '..', 'bacon.toml'), 'utf8').replace(/,\s+/g, ', ');
  for (const selection of SELECTIONS) {
    assert.ok(bacon.includes(selection.map(arg => JSON.stringify(arg)).join(', ')), selection.join(' '));
  }
});

test('libtest skips exactly what nextest runs, and every run reuses one selection', () => {
  const { SELECTIONS, PROCESS_TESTS, PROCESS_FILTER, UNIT, RUNS, buildArgs, runArgs, unitTestArgs } = require('../scripts/rust-tests.cjs');
  // libtest's --skip matches a literal piece anywhere in a name, as an unanchored nextest
  // regex does only while the piece holds no regex syntax.
  for (const name of PROCESS_TESTS) assert.doesNotMatch(name, /[\\^$.*+?()[\]{}|/]/, name);
  assert.equal(PROCESS_FILTER, `test(/${PROCESS_TESTS.join('|')}/)`);
  assert.deepEqual(unitTestArgs(UNIT), PROCESS_TESTS.flatMap(name => ['--skip', name]));
  assert.ok(SELECTIONS.includes(UNIT.selection));
  assert.deepEqual(buildArgs(UNIT).slice(-UNIT.selection.length), UNIT.selection);
  for (const run of RUNS) {
    const args = runArgs(run);
    assert.equal(args[args.indexOf('-E') + 1], PROCESS_FILTER, run.name);
    assert.ok(SELECTIONS.includes(run.selection), run.name);
  }
});

test('the process-heavy group names only tests the nextest run holds', () => {
  const { PROCESS_TESTS } = require('../scripts/rust-tests.cjs');
  const nextest = readFileSync(join(__dirname, '..', '.config', 'nextest.toml'), 'utf8');
  const group = /^filter = '(.+)'\ntest-group = "process-heavy"$/m.exec(nextest);
  assert.ok(group, 'nextest.toml assigns the process-heavy group');
  const pieces = [...group[1].matchAll(/test\(\/(.+?)\/\)/g)].flatMap(([, regex]) => regex.split('|').map(piece => piece.replace(/^\^/, '')));
  assert.ok(pieces.length > 0);
  for (const piece of pieces) assert.ok(PROCESS_TESTS.includes(piece), piece);
});

test('cargo dev-test is the first Rust test selection under libtest', () => {
  const { SELECTIONS } = require('../scripts/rust-tests.cjs');
  const config = readFileSync(join(__dirname, '..', '.cargo', 'config.toml'), 'utf8');
  const alias = /^dev-test = "test (.+) --config=target\.'cfg\(all\(\)\)'\.runner=\\"scripts\/test-watchdog\.cjs\\""$/m.exec(config);
  assert.ok(alias, 'the dev-test alias is defined, with its stuck-test runner');
  assert.deepEqual(alias[1].split(/\s+/), SELECTIONS[0]);
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

test('only a staged release --all build drops its platform build dir; target/ itself never', () => {
  const { buildDirToDrop, targetDir } = require('../scripts/build-native.cjs');
  const all = { all: true, release: true, check: false };
  assert.equal(buildDirToDrop('linux-x64-musl', true, all), targetDir('linux-x64-musl'));
  assert.equal(buildDirToDrop('darwin-arm64', false, all), null);
  for (const options of [{ ...all, all: false }, { ...all, release: false }, { ...all, release: false, check: true }]) {
    assert.equal(buildDirToDrop('linux-x64-musl', true, options), null, JSON.stringify(options));
  }
});

test('release builds remap build paths and pin the macOS floor; dev and check builds add nothing', () => {
  const os = require('os');
  const { PLATFORMS } = require('../bin/platform.cjs');
  const { MACOS_FLOOR, hostsCommand } = require('../scripts/build-native.cjs');
  for (const [platform, target] of Object.entries(PLATFORMS)) {
    const { args, env } = hostsCommand(platform, { release: true }, '/tmp/hosts');
    const config = args[args.indexOf('--config') + 1];
    assert.ok(config.startsWith(`target.${target.triple}.rustflags=`), config);
    const flags = JSON.parse(config.slice(config.indexOf('=') + 1));
    assert.ok(flags.some(flag => flag.startsWith(`--remap-path-prefix=${os.homedir()}=`)), platform);
    assert.equal(env.MACOSX_DEPLOYMENT_TARGET, target.os === 'darwin' ? MACOS_FLOOR : undefined, platform);
    assert.equal(env.CFLAGS?.includes(`-ffile-prefix-map=${os.homedir()}=`) ?? false, target.os !== 'win32', platform);
    for (const mode of [{}, { check: true }]) {
      const plain = hostsCommand(platform, mode, '/tmp/hosts');
      assert.ok(!plain.args.includes('--config'), platform);
      assert.deepEqual(plain.env, {}, platform);
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

test('verifyBinaryRuns requires the staged binary to report the package version', { skip: process.platform === 'win32' }, t => {
  const { verifyBinaryRuns } = require('../scripts/stage-binary.cjs');
  const root = mkdtempSync(join(tmpdir(), 'octocode-smoke-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const bin = join(root, 'octocode');
  writeFileSync(bin, '#!/bin/sh\necho "octocode 1.2.3"\n', { mode: 0o755 });
  verifyBinaryRuns(bin, '1.2.3');
  assert.throws(() => verifyBinaryRuns(bin, '1.3.0'), /reports 'octocode 1\.2\.3', expected 'octocode 1\.3\.0'/);
});

test('verifyBinaryRuns accepts the contract fingerprint suffix and rejects other suffixes', { skip: process.platform === 'win32' }, t => {
  const { verifyBinaryRuns } = require('../scripts/stage-binary.cjs');
  const root = mkdtempSync(join(tmpdir(), 'octocode-smoke-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const reporting = (name, output) => {
    const bin = join(root, name);
    writeFileSync(bin, `#!/bin/sh\necho "${output}"\n`, { mode: 0o755 });
    return bin;
  };
  verifyBinaryRuns(reporting('fingerprint', 'octocode 1.2.3+01a553b7'), '1.2.3');
  assert.throws(() => verifyBinaryRuns(reporting('wrong', 'octocode 1.3.0+01a553b7'), '1.2.3'), /expected 'octocode 1\.2\.3'/);
  assert.throws(() => verifyBinaryRuns(reporting('suffix', 'octocode 1.2.3+nothex'), '1.2.3'), /expected 'octocode 1\.2\.3'/);
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
