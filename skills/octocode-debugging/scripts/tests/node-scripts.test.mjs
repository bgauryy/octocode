import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, realpathSync, rmSync, writeFileSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const nodeBreak = fileURLToPath(new URL('../node-break.mjs', import.meta.url));
const profileTop = fileURLToPath(new URL('../profile-top.mjs', import.meta.url));
const run = (args, cwd) => spawnSync(process.execPath, args, { cwd, encoding: 'utf8' });
const events = (stdout) => stdout.split('\n').filter((line) => line.startsWith('{')).map((line) => JSON.parse(line));

test('node-break reports every pause and whole values unless bounded', (t) => {
  const dir = mkdtempSync(join(tmpdir(), 'node-break-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  writeFileSync(join(dir, 'app.mjs'), "const long = 'x'.repeat(500);\nfunction step(i) {\n  return i + long.length;\n}\nfor (let i = 0; i < 30; i++) step(i);\n");
  const all = events(run([nodeBreak, '--break', 'app.mjs:3', '--eval', 'long', '--', 'app.mjs'], dir).stdout);
  assert.equal(all.filter((e) => e.event === 'hit').length, 30);
  assert.equal(all.find((e) => e.event === 'hit').values.long.length, 500);
  assert.equal(all.at(-1).code, 0);
  const bounded = events(run([nodeBreak, '--break', 'app.mjs:3', '--max-hits', '3', '--', 'app.mjs'], dir).stdout);
  assert.equal(bounded.filter((e) => e.event === 'hit').length, 3);
  assert.ok(bounded.some((e) => e.event === 'max-hits'));
});

test('node-break help succeeds and bad bounds are refused', () => {
  const help = run([nodeBreak, '--help']);
  assert.equal(help.status, 0);
  assert.match(help.stdout, /Usage/);
  assert.equal(run([nodeBreak, '--max-hits', '0', '--', 'app.mjs']).status, 2);
  assert.equal(run([nodeBreak, '--timeout', '0', '--', 'app.mjs']).status, 2);
});

test('profile-top discloses the rows shown, the profiles found and unreadable input', (t) => {
  const dir = mkdtempSync(join(tmpdir(), 'profile-top-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const frame = (id, functionName, children = []) => ({ id, callFrame: { functionName, url: '', lineNumber: 0 }, children });
  const profile = { nodes: [frame(1, '(root)', [2, 3]), frame(2, 'a'), frame(3, 'b')], samples: [2, 3, 2], timeDeltas: [0, 1000, 1000], startTime: 0, endTime: 3000 };
  mkdirSync(join(dir, 'prof'));
  writeFileSync(join(dir, 'prof', 'one.cpuprofile'), JSON.stringify(profile));
  writeFileSync(join(dir, 'prof', 'two.cpuprofile'), JSON.stringify(profile));
  const out = run([profileTop, 'prof', '--top', '1'], dir);
  assert.equal(out.status, 0, out.stderr);
  assert.match(out.stdout, /newest of 2 profiles/);
  assert.match(out.stdout, /1 of 2 functions/);
  assert.equal(run([profileTop, '--help']).status, 0);
  writeFileSync(join(dir, 'bad.cpuprofile'), '{');
  const bad = run([profileTop, 'bad.cpuprofile'], dir);
  assert.equal(bad.status, 2);
  assert.match(bad.stderr, /^profile-top: bad\.cpuprofile/);
  assert.equal(run([profileTop, 'missing'], dir).status, 2);
});

test('profile-top slices a time window, charges costs to the owner and lists caller chains', (t) => {
  const dir = mkdtempSync(join(tmpdir(), 'profile-top-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const frame = (id, functionName, url, children = []) => ({ id, callFrame: { functionName, url, lineNumber: 0 }, children });
  // root > app (ours) > parse (a library) > scan; root > idle. Samples: app at 0 s, scan at 1 s and 2 s, idle at 3 s.
  const profile = {
    nodes: [frame(1, '(root)', '', [2, 5]), frame(2, 'app', 'file:///p/app.mjs', [3]), frame(3, 'parse', 'file:///p/node_modules/lib.mjs', [4]), frame(4, 'scan', 'file:///p/node_modules/lib.mjs'), frame(5, '(idle)', '')],
    samples: [2, 4, 4, 5, 5],
    timeDeltas: [0, 1_000_000, 1_000_000, 1_000_000, 1_000_000],
    startTime: 0,
    endTime: 5_000_000,
  };
  writeFileSync(join(dir, 'run.cpuprofile'), JSON.stringify(profile));
  const window = run([profileTop, 'run.cpuprofile', '--from', '0.5', '--to', '2.5'], dir);
  assert.equal(window.status, 0, window.stderr);
  assert.match(window.stdout, /window 0\.5-2\.5 s, 2000 ms sampled busy/);
  const owned = run([profileTop, 'run.cpuprofile', '--owner', 'app.mjs', '--top', '1'], dir);
  assert.match(owned.stdout, /3000\.0\s+100\.0%\s+3000\.0\s+100\.0%\s+app/);
  const chains = run([profileTop, 'run.cpuprofile', '--callers', 'parse'], dir);
  assert.match(chains.stdout, /2000\.0\s+66\.7%\s+parse \S*lib\.mjs:1 < app \S*app\.mjs:1/);
  writeFileSync(join(dir, 'live.heapprofile'), JSON.stringify({ head: { callFrame: { functionName: '(root)', url: '', lineNumber: 0 }, selfSize: 0, children: [] } }));
  assert.equal(run([profileTop, 'live.heapprofile', '--from', '1'], dir).status, 2);
});

test('node-alloc records allocations the GC already collected, which a live-only heap profile misses', (t) => {
  const dir = mkdtempSync(join(tmpdir(), 'node-alloc-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  // `churn` allocates about 40 MB of short-lived arrays and keeps none of them.
  writeFileSync(join(dir, 'app.mjs'), 'function churn() { let n = 0; for (let i = 0; i < 4000; i++) n += new Array(1250).fill(i).length; return n; }\nconsole.log(churn());\n');
  const nodeAlloc = fileURLToPath(new URL('../node-alloc.mjs', import.meta.url));
  const out = spawnSync(process.execPath, ['--import', nodeAlloc, 'app.mjs'], { cwd: dir, encoding: 'utf8', env: { ...process.env, NODE_ALLOC_DIR: dir } });
  assert.equal(out.status, 0, out.stderr);
  const summary = run([profileTop, dir, '--filter', 'churn', '--top', '1'], dir);
  assert.equal(summary.status, 0, summary.stderr);
  const bytes = Number(/^\s*(\d+)\s/m.exec(summary.stdout.split('\n').slice(3).join('\n'))?.[1]);
  assert.ok(bytes > 10_000_000, `churn sampled ${bytes} bytes:\n${summary.stdout}`);
  assert.equal(spawnSync(process.execPath, ['--import', nodeAlloc, 'app.mjs'], { cwd: dir, encoding: 'utf8', env: { ...process.env, NODE_ALLOC_DIR: join(dir, 'missing') } }).status, 1);
});

test('profile-top matches --owner against absolute paths and divides costs by --ops', (t) => {
  const dir = mkdtempSync(join(tmpdir(), 'profile-top-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const app = new URL(`file://${join(dir, 'src', 'app.mjs')}`).href;
  const frame = (id, functionName, url, children = []) => ({ id, callFrame: { functionName, url, lineNumber: 0 }, children });
  const profile = { nodes: [frame(1, '(root)', '', [2]), frame(2, 'app', app, [3]), frame(3, 'scan', 'file:///lib/x.mjs')], samples: [3, 3, 3, 3], timeDeltas: [0, 1000, 1000, 1000], startTime: 0, endTime: 4000 };
  writeFileSync(join(dir, 'run.cpuprofile'), JSON.stringify(profile));
  const owned = run([profileTop, 'run.cpuprofile', '--owner', join(dir, 'src'), '--ops', '4', '--top', '1'], dir);
  assert.equal(owned.status, 0, owned.stderr);
  assert.match(owned.stdout, /per operation over 4 operations/);
  const [, cells] = owned.stdout.split('\n').slice(2).map((line) => line.trim().split(/\s+/));
  assert.equal(cells.at(-2), 'app', `the absolute owner path matches a frame shown relative to cwd:\n${owned.stdout}`);
  assert.equal(Number(cells[4]), Number(cells[0]) / 4, 'self per op is self divided by --ops');
  assert.equal(run([profileTop, 'run.cpuprofile', '--ops', '0'], dir).status, 2);
});

test('profile-top --sync-io groups warnings by phase, runtime API and the first frame of the owner', (t) => {
  // The real path, so frames under it show relative to the working directory (macOS tmpdir is a symlink).
  const dir = realpathSync(mkdtempSync(join(tmpdir(), 'sync-io-test-')));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const app = new URL(`file://${join(dir, 'src', 'app.mjs')}`).href;
  const warning = (line) => `(node:1) WARNING: Detected use of sync API\n    at openSync (node:fs:697:18)\n    at readFileSync (node:fs:543:35)\n    at load (${app}:${line}:5)\n    at file:///lib/run.mjs:1:1\n`;
  writeFileSync(join(dir, 'sync.log'), `${warning(2)}\n${warning(2)}\nphase: steady\n${warning(9)}\nunrelated stderr\n`);
  const out = run([profileTop, 'sync.log', '--sync-io', '--owner', join(dir, 'src')], dir);
  assert.equal(out.status, 0, out.stderr);
  assert.match(out.stdout, /3 synchronous operations in 2 phases/);
  const rows = out.stdout.split('\n').slice(3).filter(Boolean).map((line) => line.trim().split(/\s+/));
  assert.deepEqual(rows.map((cells) => [cells[0], cells[2], cells[3], cells.at(-1)]), [['2', '(start)', 'readFileSync', 'src/app.mjs:2'], ['1', 'steady', 'readFileSync', 'src/app.mjs:9']]);
  assert.equal(run([profileTop, 'sync.log', '--sync-io', '--callers', 'x'], dir).status, 2);
  writeFileSync(join(dir, 'empty.log'), 'nothing here\n');
  assert.equal(run([profileTop, 'empty.log', '--sync-io'], dir).status, 2);
});

test('the preloads answer --help when run directly and leave the program\'s own --help alone', (t) => {
  const dir = mkdtempSync(join(tmpdir(), 'preload-help-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  writeFileSync(join(dir, 'app.mjs'), "console.log('app', process.argv.slice(2).join(' '));\n");
  for (const name of ['node-alloc.mjs', 'node-metrics.mjs']) {
    const preload = fileURLToPath(new URL(`../${name}`, import.meta.url));
    const help = run([preload, '--help']);
    assert.equal(help.status, 0, help.stderr);
    assert.match(help.stdout, /Inputs \(environment\)/);
    assert.match(help.stdout, /Output:/);
    assert.equal(run([preload]).status, 2);
    const env = { ...process.env, NODE_ALLOC_DIR: dir, NODE_METRICS_DIR: dir };
    const app = spawnSync(process.execPath, ['--import', preload, 'app.mjs', '--help'], { cwd: dir, encoding: 'utf8', env });
    assert.equal(app.status, 0, app.stderr);
    assert.equal(app.stdout, 'app --help\n');
  }
});
