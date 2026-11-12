import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from 'node:fs';
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
