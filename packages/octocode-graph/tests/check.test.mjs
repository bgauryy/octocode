// `octocode-graph check`: a rules file over a saved graph, a baseline of known violations, text, JSON and SARIF 2.1.0
// output and the exit code CI reads (V3); `check --since <rev>` refreshes the graph over its base and reports only the
// violations the change introduced (V4). The engine and git are fakes: no build reaches a real repository, and every
// git call is read-only and recorded.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { delimiter, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { fakeCatalog, tempDir } from './test-tmp.mjs';

const commands = fileURLToPath(new URL('../src/runtime/commands/', import.meta.url));
const OLD = new Date(Date.UTC(2025, 0, 1));

const imp = (source, target, line) => ({ source, target, line, lines: [line], confidence: 'high', resolution: ['static-import'], test: false });
const fs = (files) => ({
  id: 'pkg:node:fs',
  count: files.length,
  files,
  kinds: Object.fromEntries(files.map((f) => [f, ['external-builtin', 'static-import']])),
});

// A project whose files exist on disk (written before the graph), and its saved result.
function project() {
  const base = tempDir('graph-check-');
  const root = join(base, 'project');
  const out = join(base, 'result');
  const ids = ['src/index.ts', 'src/browser/view.ts', 'src/browser/legacy.ts', 'src/shared/io.ts'];
  for (const id of ids) {
    mkdirSync(dirname(join(root, id)), { recursive: true });
    writeFileSync(join(root, id), '// fixture\n');
    utimesSync(join(root, id), OLD, OLD);
  }
  mkdirSync(out, { recursive: true });
  const save = (imports, fsImporters) =>
    writeFileSync(
      join(out, 'data.json'),
      JSON.stringify({ section: root, files: ids.map((id) => ({ id, label: id, chunk: 'app', language: 'typescript' })), imports, symbols: [], calls: [], externals: [fs(fsImporters)] })
    );
  mkdirSync(join(root, '.octocode'), { recursive: true });
  const rules = (value) => writeFileSync(join(root, '.octocode', 'graph-rules.json'), JSON.stringify(value));
  rules({
    forbidden: [
      { name: 'no-fs-in-browser', severity: 'error', comment: 'Browser code cannot load Node modules', from: { path: '^src/browser/' }, to: { path: '^pkg:node:fs$', reachable: true } },
      { name: 'index-no-io', severity: 'warn', from: { path: '^src/index\\.ts$' }, to: { path: '^src/shared/io\\.ts$' } },
    ],
  });
  return { root, out, save, rules };
}

const check = (args, env = {}) => spawnSync(process.execPath, [join(commands, 'check.ts'), ...args], { encoding: 'utf8', env: { ...process.env, ...env } });

test('V3: a baseline keeps known violations from failing; SARIF reports the new error with its chain as codeFlows', () => {
  const p = project();
  p.save([imp('src/index.ts', 'src/browser/legacy.ts', 1)], ['src/browser/legacy.ts']);
  const written = check([p.out, '--write-baseline']);
  assert.equal(written.status, 0, written.stderr);
  const baseline = JSON.parse(readFileSync(join(p.root, '.octocode', 'graph-rules.baseline.json'), 'utf8'));
  assert.deepEqual(baseline.violations, [{ rule: 'no-fs-in-browser', from: 'src/browser/legacy.ts', to: 'pkg:node:fs' }]);
  const known = check([p.out]);
  assert.equal(known.status, 0, known.stdout + known.stderr);
  assert.match(known.stdout, /1 known/);

  // One new import chain: view → io → node:fs, plus a warn-only direct import.
  p.save(
    [imp('src/index.ts', 'src/browser/legacy.ts', 1), imp('src/index.ts', 'src/browser/view.ts', 2), imp('src/index.ts', 'src/shared/io.ts', 3), imp('src/browser/view.ts', 'src/shared/io.ts', 4)],
    ['src/browser/legacy.ts', 'src/shared/io.ts']
  );
  const sarif = check([p.out, '--format', 'sarif']);
  assert.equal(sarif.status, 1, sarif.stderr);
  const log = JSON.parse(sarif.stdout);
  assert.equal(log.version, '2.1.0');
  assert.match(log.$schema, /sarif-2\.1\.0/);
  const [run] = log.runs;
  assert.equal(run.tool.driver.name, 'octocode-graph');
  assert.deepEqual(run.tool.driver.rules.map((r) => r.id).toSorted(), ['index-no-io', 'no-fs-in-browser']);
  const errors = run.results.filter((r) => r.level === 'error');
  assert.equal(errors.length, 1, 'the baseline violation is absent');
  const [error] = errors;
  assert.equal(error.ruleId, 'no-fs-in-browser');
  assert.equal(run.tool.driver.rules[error.ruleIndex].id, 'no-fs-in-browser');
  assert.equal(error.locations[0].physicalLocation.artifactLocation.uri, 'src/browser/view.ts');
  const flow = error.codeFlows[0].threadFlows[0].locations.map((l) => [l.location.physicalLocation.artifactLocation.uri, l.location.physicalLocation.region.startLine]);
  assert.deepEqual(flow, [['src/browser/view.ts', 4], ['src/shared/io.ts', 1]]);
  assert.match(error.codeFlows[0].threadFlows[0].locations.at(-1).location.message.text, /pkg:node:fs/);
  assert.deepEqual(run.results.filter((r) => r.level === 'warning').map((r) => r.ruleId), ['index-no-io']);

  const json = JSON.parse(check([p.out, '--format', 'json']).stdout);
  assert.equal(json.failed, true);
  assert.deepEqual(json.counts, { error: 1, warn: 1, info: 0, known: 1, fixed: 0 });
  assert.deepEqual(json.violations.filter((v) => v.known).map((v) => v.from), ['src/browser/legacy.ts']);
  const text = check([p.out]);
  assert.equal(text.status, 1);
  assert.match(text.stdout, /error\s+no-fs-in-browser\s+src\/browser\/view\.ts → pkg:node:fs/);
  // A package import's line is cited only when the build recorded it.
  assert.match(text.stdout, /src\/browser\/view\.ts:4 \[static-import → eager\] → src\/shared\/io\.ts \[external-builtin,static-import → eager\] → pkg:node:fs/);
});

test('check refuses a rules file that names nothing in the graph, a missing rules file and bad arguments (exit 2)', () => {
  const p = project();
  p.save([], []);
  const missing = check([p.out, '--rules', join(p.root, 'nope.json')]);
  assert.equal(missing.status, 2);
  assert.match(missing.stderr, /no rules file/);
  p.rules({ forbidden: [{ name: 'typo', from: { path: '^src/browsr/' }, to: { path: 'fs', reachable: true } }] });
  const typo = check([p.out]);
  assert.equal(typo.status, 2);
  assert.match(typo.stderr, /from\.path \^src\/browsr\/ matches no file or package/);
  assert.equal(check([p.out, '--format', 'xml']).status, 2);
  assert.equal(check([p.out, '--since']).status, 2);
});

// The engine fake of refresh.test.mjs, cut to what a build and a refresh read: a snapshot per directory hash, `stale`
// against the disk, a.ts → b.ts when both exist and a.ts → pkg:c when a.ts imports 'c'.
const FAKE_ENGINE = String.raw`#!/usr/bin/env node
${fakeCatalog}
const { createHash } = require('node:crypto');
const { appendFileSync, existsSync, readFileSync, readdirSync, writeFileSync } = require('node:fs');
const { join, relative } = require('node:path');
const [root, stateFile] = [process.env.FAKE_ROOT, process.env.FAKE_STATE];
const a = process.argv.slice(2), print = (v) => console.log(JSON.stringify(v));
const out = (data) => print({ results: [{ index: 0, data }] });
const input = JSON.parse(a.find((x) => x.startsWith('{')) ?? '{}'), row = input.queries?.[0] ?? {};
const snapFile = (id) => stateFile + '.' + id + '.json';
const list = (dir) => readdirSync(dir, { recursive: true, withFileTypes: true }).filter((d) => d.isFile()).map((d) => relative(dir, join(d.parentPath, d.name))).filter((p) => !p.startsWith('.octocode'));
const hashes = (dir) => Object.fromEntries(list(dir).map((p) => [p, createHash('sha1').update(readFileSync(join(dir, p))).digest('hex')]));
const code = (dir) => list(dir).filter((p) => p.endsWith('.ts')).toSorted();
const has = (dir, f, text) => existsSync(join(dir, f)) && readFileSync(join(dir, f), 'utf8').includes(text);
function graph(dir) {
  const files = code(dir), W = [];
  if (files.includes('a.ts') && files.includes('b.ts')) W.push({ edge: 'imports', from: 'a.ts', to: 'b.ts', path: 'a.ts', line: 1, confidence: 'high' });
  if (has(dir, 'a.ts', "from 'c'")) W.push({ edge: 'imports', from: 'a.ts', to: 'pkg:c', path: 'a.ts', line: 2, confidence: 'high' });
  return { files, W };
}
const page = (rows) => out({ graph: { id: row.graph }, rows });
const inTarget = (file) => !row.target || row.target.includes(file);
if (a[0] === 'schema') print({ querySchema: { properties: { target: { anyOf: [{ type: 'string' }, { type: 'array', maxItems: 100 }] } } } });
else if (a[0] === 'structureSearch') { const files = list(root); out({ files: [{ dir: root, files }], summary: { files: files.length } }); }
else if (a[0] === 'graphBuild') {
  const h = hashes(row.path), id = 'snap-' + createHash('sha1').update(row.path + JSON.stringify(h)).digest('hex').slice(0, 12);
  appendFileSync(stateFile, relative(root, row.path) + '\n');
  writeFileSync(snapFile(id), JSON.stringify(h));
  out({ graph: id, root: row.path, counts: { nodes: 1, edges: 1 } });
} else if (row.operation === 'stale') {
  const before = existsSync(snapFile(row.graph)) ? JSON.parse(readFileSync(snapFile(row.graph), 'utf8')) : {}, now = hashes(row.path);
  const rows = [...Object.keys(before).filter((p) => now[p] !== before[p]).map((p) => ({ path: p, status: now[p] ? 'changed' : 'missing' })), ...Object.keys(now).filter((p) => !before[p]).map((p) => ({ path: p, status: 'added' }))];
  for (const entry of rows) entry.path = relative(root, join(row.path, entry.path));
  out({ graph: { id: row.graph }, rows, freshness: { fresh: rows.length === 0, analysisCurrent: true } });
} else {
  const g = graph(row.path);
  if (row.operation === 'find') page(g.files.map((id) => ({ id, kind: 'file', language: 'typescript' })));
  else if (row.operation === 'witnesses') page(g.W.filter((w) => inTarget(w.from)));
  else if (row.operation === 'symbols') page([]);
  else out({ graph: { id: row.graph }, rows: [], summary: { health: {}, byDetector: {} } });
}`;

// git as check --since may call it: read-only commands only, each one logged. `main` is the only revision; the diff
// <rev>...HEAD lists FAKE_GIT_CHANGED; the working tree is clean.
const FAKE_GIT = String.raw`#!/usr/bin/env node
const { appendFileSync } = require('node:fs');
const args = process.argv.slice(2);
const cmd = args[0] === '-C' ? args.slice(2) : args;
appendFileSync(process.env.FAKE_GIT_LOG, JSON.stringify(cmd) + '\n');
const SHA = '1111111111111111111111111111111111111111';
const nul = (list) => process.stdout.write(list.map((p) => p + '\0').join(''));
if (cmd[0] === 'rev-parse' && cmd.includes('--verify')) {
  if (cmd.at(-1) !== 'main^{commit}') process.exit(1);
  console.log(SHA);
} else if (cmd[0] === 'merge-base') console.log(SHA);
else if (cmd[0] === 'diff') nul(cmd.some((x) => x.endsWith('...HEAD')) ? (process.env.FAKE_GIT_CHANGED ?? '').split(',').filter(Boolean) : []);
else if (cmd[0] === 'ls-files') nul([]);
else process.exit(1);`;

function overlayRepo() {
  const base = tempDir('graph-check-since-');
  const root = join(base, 'repo');
  const files = {
    'pkg/a/package.json': '{"name":"a"}\n',
    'pkg/a/a.ts': "import { b } from './b';\n",
    'pkg/a/b.ts': 'export function b() {}\n',
    'pkg/c/package.json': '{"name":"c"}\n',
    'pkg/c/c.ts': 'export function c() {}\n',
    '.octocode/graph-rules.json': JSON.stringify({
      forbidden: [
        { name: 'a-no-c', severity: 'error', from: { path: '^pkg/a/' }, to: { path: '^pkg:c$', reachable: true } },
        { name: 'a-no-b', severity: 'error', from: { path: '^pkg/a/a\\.ts$' }, to: { path: '^pkg/a/b\\.ts$' } },
      ],
    }),
  };
  for (const [name, text] of Object.entries(files)) {
    mkdirSync(dirname(join(root, name)), { recursive: true });
    writeFileSync(join(root, name), text);
  }
  const bin = join(base, 'bin');
  mkdirSync(bin);
  writeFileSync(join(base, 'engine.cjs'), FAKE_ENGINE, { mode: 0o755 });
  writeFileSync(join(bin, 'git'), FAKE_GIT, { mode: 0o755 });
  const state = join(base, 'state.json');
  const gitLog = join(base, 'git.jsonl');
  writeFileSync(gitLog, '');
  const env = {
    ...process.env,
    OCTOCODE_CLI: `${process.execPath} ${join(base, 'engine.cjs')}`,
    FAKE_ROOT: root,
    FAKE_STATE: state,
    WORKSPACE_ROOT: root,
    OCTOCODE_HOME: join(base, 'home'),
    FAKE_GIT_LOG: gitLog,
    PATH: `${bin}${delimiter}${process.env.PATH}`,
  };
  const out = join(base, 'result');
  const built = spawnSync(process.execPath, [join(commands, 'build.ts'), root, '--out', out, '--lsp', 'off'], { encoding: 'utf8', cwd: root, env });
  assert.equal(built.status, 0, built.stderr);
  const builds = () => readFileSync(state, 'utf8').trim().split('\n');
  const gitCalls = () => readFileSync(gitLog, 'utf8').trim().split('\n').filter(Boolean).map((l) => JSON.parse(l));
  return { root, out, env, builds, gitCalls };
}

test('V4: check --since refreshes only the changed chunk over the base graph and reports only the new violation', { timeout: 60000 }, () => {
  const r = overlayRepo();
  const before = r.builds().length;
  writeFileSync(join(r.root, 'pkg/a/a.ts'), "import { b } from './b';\nimport c from 'c';\n");
  const since = (changed) => spawnSync(process.execPath, [join(commands, 'check.ts'), r.out, '--since', 'main', '--format', 'json'], { encoding: 'utf8', cwd: r.root, env: { ...r.env, FAKE_GIT_CHANGED: changed } });

  const first = since('pkg/a/a.ts');
  assert.equal(first.status, 1, first.stderr);
  const report = JSON.parse(first.stdout);
  assert.deepEqual(report.violations.map((v) => [v.rule, v.from, v.to]), [['a-no-c', 'pkg/a/a.ts', 'pkg:c']]);
  assert.equal(report.since.preExisting, 1, 'a-no-b holds in the base graph too: not reported');
  assert.match(report.since.base, /before this refresh/);
  assert.deepEqual(report.since.rebuilt, ['pkg-a']);
  assert.deepEqual(r.builds().slice(before), ['pkg/a'], 'only the changed chunk rebuilt');

  // Without the base the first run kept, the graph its refresh replaced (previous.json) is the base: the diff
  // accounts for every file that refresh changed.
  rmSync(join(r.out, 'since-base.json'));
  const previous = JSON.parse(since('pkg/a/a.ts').stdout);
  assert.deepEqual(previous.violations.map((v) => v.rule), ['a-no-c']);
  assert.match(previous.since.base, /previous\.json/);
  // Run again: the base matched to the merge base is kept beside the graph.
  const again = JSON.parse(since('pkg/a/a.ts').stdout);
  assert.deepEqual(again.violations.map((v) => v.rule), ['a-no-c']);
  assert.match(again.since.base, /earlier check kept/);

  // A diff the saved graphs do not match: no base graph, so every violation is reported and the answer says why.
  rmSync(join(r.out, 'since-base.json'));
  const none = since('pkg/c/c.ts');
  assert.equal(none.status, 1);
  const full = JSON.parse(none.stdout);
  assert.equal(full.since.base, null);
  assert.match(full.since.note, /no saved graph of main/);
  assert.deepEqual(full.violations.map((v) => v.rule).toSorted(), ['a-no-b', 'a-no-c']);

  const verbs = new Set(r.gitCalls().map((c) => c[0]));
  assert.deepEqual([...verbs].filter((v) => !['rev-parse', 'merge-base', 'diff', 'ls-files'].includes(v)), [], 'git is only read');
  assert.equal(spawnSync(process.execPath, [join(commands, 'check.ts'), r.out, '--since', 'nope'], { encoding: 'utf8', cwd: r.root, env: r.env }).status, 2);
});
