// How hints name this package's CLI: `octocode-graph` only when that name on PATH runs this package, else the
// no-install npx form. Hermetic: each case builds its own PATH directories.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, mkdirSync, symlinkSync, writeFileSync } from 'node:fs';
import { delimiter, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { tempDir } from './test-tmp.mjs';
import { graphCommand, NPX, packageBin, shellArg } from '../src/runtime/invocation.ts';

const bin = packageBin();
const pathOf = (...dirs) => ({ PATH: dirs.join(delimiter) });

test('the package bin is this package: bin/octocode-graph.mjs beside package.json', () => {
  assert.equal(relative(fileURLToPath(new URL('..', import.meta.url)), bin), join('bin', 'octocode-graph.mjs'));
});

test('no octocode-graph on PATH: hints name the npx form', () => {
  const empty = tempDir('graph-path-empty-');
  assert.equal(graphCommand(pathOf(empty)), NPX);
  assert.equal(NPX, 'npx -y @octocodeai/octocode-graph');
});

test('octocode-graph on PATH linking to this package: hints name the bare command', () => {
  const dir = tempDir('graph-path-link-');
  symlinkSync(bin, join(dir, 'octocode-graph'));
  assert.equal(graphCommand(pathOf(tempDir('graph-path-none-'), dir)), 'octocode-graph');
});

test('a package-manager shim on PATH that runs this package counts; one that runs another copy does not', () => {
  const dir = tempDir('graph-path-shim-');
  const shim = join(dir, 'octocode-graph');
  // npm's and pnpm's POSIX shims name the CLI relative to their own directory.
  writeFileSync(shim, `#!/bin/sh\nbasedir=$(dirname "$0")\nexec node "$basedir/${relative(dir, bin)}" "$@"\n`);
  chmodSync(shim, 0o755);
  assert.equal(graphCommand(pathOf(dir)), 'octocode-graph');
  const other = tempDir('graph-path-other-');
  writeFileSync(join(other, 'octocode-graph'), '#!/bin/sh\nexec node "$basedir/../lib/node_modules/@octocodeai/octocode-graph/bin/octocode-graph.mjs" "$@"\n', { mode: 0o755 });
  assert.equal(graphCommand(pathOf(other, dir)), NPX, 'the first octocode-graph on PATH is the one a shell runs');
});

test('a node_modules/.bin a package runner put on PATH is not the shell PATH: npx form', () => {
  const runner = join(tempDir('graph-path-runner-'), 'node_modules', '.bin');
  mkdirSync(runner, { recursive: true });
  symlinkSync(bin, join(runner, 'octocode-graph'));
  assert.equal(graphCommand(pathOf(runner)), NPX);
});

test('shell arguments are quoted only when a shell would split or expand them', () => {
  assert.equal(shellArg('/repo/packages/a-b_c.d@1'), '/repo/packages/a-b_c.d@1');
  assert.equal(shellArg("/my repo/it's"), process.platform === 'win32' ? '"/my repo/it\'s"' : "'/my repo/it'\\''s'");
});

test('the query not-found hint names an invocation that runs here', () => {
  const project = tempDir('graph-hint-project-');
  const query = fileURLToPath(new URL('../src/runtime/commands/query.ts', import.meta.url));
  const env = { ...process.env, ...pathOf(tempDir('graph-hint-path-')), OCTOCODE_HOME: tempDir('graph-hint-home-') };
  const r = spawnSync(process.execPath, [query, project, 'search a'], { encoding: 'utf8', cwd: project, env });
  assert.equal(r.status, 2, r.stderr);
  assert.match(r.stderr, new RegExp(`build one with npx -y @octocodeai/octocode-graph build ${project.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}`));
});
