import assert from 'node:assert/strict';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import {
  NATIVE,
  crateDirs,
  fuzzInvocation,
  installCommand,
  inventory,
  llvmLinesInvocations,
  loadManifest,
  miriInvocations,
  parseVersion,
  pinSpec,
} from './rust-quality.mjs';

const manifest = loadManifest();

test('a version is the first x.y.z in a tool banner', () => {
  assert.equal(parseVersion('cargo-nextest 0.9.148 (abc 2026-10-01)'), '0.9.148');
  assert.equal(parseVersion('Version: 1.14.0'), '1.14.0');
  assert.equal(parseVersion('cargo-xwin-xwin 0.23.0'), '0.23.0');
  assert.equal(parseVersion('no version here'), null);
});

test('inventory compares each pinned tool with the installed one and never installs', () => {
  const tools = [
    { name: 'a', version: '1.0.0', versionCommand: ['a', '--version'] },
    { name: 'b', version: '2.0.0', versionCommand: ['b', '--version'] },
    { name: 'c', version: '3.0.0', versionCommand: ['c', '--version'] },
    { name: 'd', version: '4.0.0', versionCommand: ['d', '--version'], optional: true },
  ];
  const banners = { a: 'a 1.0.0', b: 'b 2.1.0' };
  const calls = [];
  const run = (command, args) => {
    calls.push([command, ...args]);
    return command in banners ? { status: 0, stdout: banners[command] } : { status: null, stdout: '' };
  };
  const rows = inventory({ tools, toolchains: {} }, run);
  assert.deepEqual(rows.map(row => [row.name, row.state, row.have]), [
    ['a', 'ok', '1.0.0'],
    ['b', 'mismatch', '2.1.0'],
    ['c', 'missing', null],
    ['d', 'missing', null],
  ]);
  assert.equal(rows[3].optional, true);
  assert.equal(rows[1].install, 'cargo install --locked b@2.0.0');
  assert.ok(calls.every(([, ...args]) => args.join(' ') === '--version'), 'only version probes run');
});

test('every pinned tool installs at its exact version', () => {
  for (const tool of manifest.tools) {
    assert.match(tool.version, /^\d+\.\d+\.\d+$/, tool.name);
    assert.equal(installCommand(tool), `cargo install --locked ${tool.name}@${tool.version}`);
  }
  assert.match(manifest.toolchains.nightly.channel, /^nightly-\d{4}-\d{2}-\d{2}$/, 'a dated nightly');
});

test('pins name each tool at its manifest version for CI installs', () => {
  const pins = { tools: [{ name: 'a', version: '1.0.0' }, { name: 'b', version: '2.0.0' }] };
  assert.equal(pinSpec(pins, ['b', 'a']), 'b@2.0.0,a@1.0.0');
  assert.throws(() => pinSpec(pins, ['c']), /c is not pinned/);
  assert.throws(() => pinSpec(pins, []), /at least one/);
});

test('CI installs Rust dev tools only through the dev-tools.json pins', () => {
  const workflows = path.join(NATIVE, '..', '..', '.github', 'workflows');
  for (const file of readdirSync(workflows).filter(name => name.endsWith('.yml'))) {
    const text = readFileSync(path.join(workflows, file), 'utf8');
    for (const [, value] of text.matchAll(/^\s*tool:\s*(.+)$/gm)) {
      assert.match(value, /^\$\{\{ steps\.\w+\.outputs\.\w+ \}\}$/, `${file}: install-action tool "${value}" bypasses rust-quality.mjs pins`);
    }
    for (const [line] of text.matchAll(/cargo (?:b?install)\b.*$/gm)) {
      assert.match(line, /\$\{\{ steps\.\w+\.outputs\.\w+ \}\}/, `${file}: "${line}" bypasses rust-quality.mjs pins`);
    }
  }
});

test('a fuzz run selects the pinned nightly, its own build directory and a time bound', () => {
  const { command, args } = fuzzInvocation(manifest, { target: 'utf8_offsets', seconds: 30, targetDir: '/tmp/fz' });
  assert.equal(command, 'cargo');
  assert.deepEqual(args.slice(0, 7), [
    `+${manifest.toolchains.nightly.channel}`, 'fuzz', 'run', '--fuzz-dir', 'fuzz', '--target-dir', '/tmp/fz',
  ]);
  assert.deepEqual(args.slice(7, 10), ['utf8_offsets', 'fuzz/corpus/utf8_offsets', 'fuzz/seeds/utf8_offsets']);
  assert.ok(args.includes('-max_total_time=30'));
  assert.ok(args.indexOf('--') < args.indexOf('-max_total_time=30'));
  assert.throws(() => fuzzInvocation(manifest, { target: 'nope', seconds: 1 }), /unknown fuzz target/);
});

test('every fuzz target is a bin of the fuzz workspace with a committed seed corpus', () => {
  const cargo = readFileSync(path.join(NATIVE, manifest.fuzz.dir, 'Cargo.toml'), 'utf8');
  for (const target of manifest.fuzz.targets) {
    assert.match(cargo, new RegExp(`name = "${target}"`), target);
    const seeds = path.join(NATIVE, manifest.fuzz.dir, 'seeds', target);
    assert.ok(existsSync(seeds) && readdirSync(seeds).length > 0, `${target} seeds`);
  }
  assert.match(readFileSync(path.join(NATIVE, 'Cargo.toml'), 'utf8'), /exclude = \["fuzz"\]/);
});

test('miri runs one bounded nightly invocation per selection', () => {
  const runs = miriInvocations(manifest, {});
  assert.equal(runs.length, manifest.miri.runs.length);
  const [io, engine] = runs;
  assert.deepEqual(io.args.slice(0, 7), [`+${manifest.toolchains.nightly.channel}`, 'miri', 'test', '--locked', '-p', 'octocode-io', '--lib']);
  assert.ok(io.args.includes('--exact'));
  assert.equal(io.env.MIRIFLAGS, '-Zmiri-many-seeds=0..8');
  assert.deepEqual(engine.args.slice(-7), [
    'text::utf8_offsets::tests', '--skip', 'proptests::', '--skip', 'a_minified_single_line_stays_linear',
    '--skip', 'checkpointed_columns_match_a_naive_scan_everywhere',
  ]);
  assert.deepEqual(engine.env, {});
});

test('every selected miri test still exists in its crate', () => {
  const dirs = crateDirs();
  for (const run of manifest.miri.runs) {
    const dir = dirs.get(run.package);
    assert.ok(dir, run.package);
    const source = sources(path.join(dir, 'src')).join('\n');
    for (const name of [...(run.exact ?? []), ...(run.skip ?? [])]) {
      // `module::` names a test module; anything else a test function.
      const pattern = name.endsWith('::') ? `mod ${name.slice(0, -2).split('::').at(-1)};` : `fn ${name.split('::').at(-1)}\\(`;
      assert.match(source, new RegExp(pattern), `${run.package}: ${name}`);
    }
  }
});

test('llvm-lines can record build timings for the same selection first', () => {
  const [timings, lines] = llvmLinesInvocations(manifest, { pkg: 'octocode-io', timings: true });
  assert.deepEqual(timings.args, ['build', '--locked', '-p', 'octocode-io', '--lib', '--all-features', '--timings']);
  assert.deepEqual(lines.args, ['llvm-lines', '--locked', '-p', 'octocode-io', '--lib', '--all-features']);
  assert.equal(timings.env.CARGO_TARGET_DIR, lines.env.CARGO_TARGET_DIR);
  assert.equal(llvmLinesInvocations(manifest, {}).length, 1);
});

function sources(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) return sources(full);
    return entry.name.endsWith('.rs') ? [readFileSync(full, 'utf8')] : [];
  });
}
