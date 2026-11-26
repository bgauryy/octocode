import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { HARNESS, HarnessError, checkManifest, checkPins, codePoints, expandIds, fill, loadManifest, pool, questionText, removeTree, seeded, shuffled } from './lib.mjs';
import { roundIn, scratch } from './testing.mjs';

test('expandIds expands ranges, lists and "all"', () => {
  assert.deepEqual(expandIds('Q1-Q3'), ['Q1', 'Q2', 'Q3']);
  assert.deepEqual(expandIds('W9-10,Q2'), ['W9', 'W10', 'Q2']);
  assert.deepEqual(expandIds('all', ['A', 'B']), ['A', 'B']);
  assert.throws(() => expandIds('all'), HarnessError);
  assert.throws(() => expandIds('Q5-Q1'), HarnessError);
});

test('codePoints counts a character outside the BMP once', () => {
  const text = 'a😀b';
  assert.equal(codePoints(text), [...text].length);
  assert.notEqual(codePoints(text), text.length);
});

test('every round manifest in rounds/ loads', () => {
  const files = fs.readdirSync(path.join(HARNESS, 'rounds')).filter(name => name.endsWith('.json'));
  assert.ok(files.length > 0);
  for (const file of files) {
    const manifest = loadManifest(path.join(HARNESS, 'rounds', file));
    for (const [a, b] of manifest.compare) assert.ok(manifest.arms[a] && manifest.arms[b]);
    for (const pin of Object.values(manifest.pins)) assert.ok(path.isAbsolute(pin.path));
  }
});

test('a manifest is refused when an arm names an undefined pin, or a set an undefined arm', () => {
  const base = { round: 'r', neutralCwd: '~/n', arms: { a: { kind: 'shell', instruction: 'x' } }, sets: { s: { questions: 'q', ids: 'Q1', arms: { a: 'all' } } } };
  assert.equal(checkManifest(base).sets.s.arms.a.has('Q1'), true);
  assert.throws(() => checkManifest({ ...base, arms: { a: { kind: 'mcp', instruction: 'x', server: { command: 'node' }, pin: 'missing' } } }), /pin "missing"/);
  assert.throws(() => checkManifest({ ...base, sets: { s: { questions: 'q', ids: 'Q1', arms: { other: 'all' } } } }), /arm "other"/);
  assert.throws(() => checkManifest({ ...base, passes: 2, passDir: 'flat' }), /passDir/);
  assert.throws(() => checkManifest({ ...base, sets: { s: { questions: 'q', ids: 'Q1', arms: { a: 'Q2' } } } }), /outside the set/);
});

test('checkPins refuses a binary whose SHA-256 is not the recorded one', () => {
  const { dir, cleanup } = scratch('pins');
  try {
    const file = path.join(dir, 'bin');
    fs.writeFileSync(file, 'one build');
    const manifest = roundIn(dir, { extra: { pins: { p: { path: file, sha256: 'a'.repeat(64) } } } });
    assert.throws(() => checkPins(manifest), /has SHA-256/);
  } finally {
    cleanup();
  }
});

test('fill gives each arm its own pin and refuses a placeholder with no value', () => {
  const { dir, cleanup } = scratch('fill');
  try {
    const manifest = roundIn(dir, { extra: { pins: { base: { path: path.join(dir, 'base'), sha256: 'a'.repeat(64) }, cand: { path: path.join(dir, 'cand'), sha256: 'b'.repeat(64) } } } });
    assert.equal(fill('{pin}', manifest, { pin: manifest.pins.cand.path }), manifest.pins.cand.path);
    assert.equal(fill({ a: ['{pin:base}'] }, manifest).a[0], manifest.pins.base.path);
    assert.throws(() => fill('{home}', manifest), HarnessError);
    assert.throws(() => fill('{pin}', manifest), HarnessError);
  } finally {
    cleanup();
  }
});

test('questionText keeps the question and drops the sections after it', () => {
  const { dir, cleanup } = scratch('question');
  try {
    const manifest = roundIn(dir);
    const text = questionText(path.join(manifest.sets.s.dir, 'Q1.md'), { clones: '/clones' });
    assert.match(text, /What is Q1\?/);
    assert.doesNotMatch(text, /hidden from runners/);
  } finally {
    cleanup();
  }
});

test('a seed fixes the sequence; another seed changes it', () => {
  const draw = seed => Array.from({ length: 8 }, seeded(seed));
  assert.deepEqual(draw('s'), draw('s'));
  assert.notDeepEqual(draw('s'), draw('t'));
  const items = Array.from({ length: 20 }, (_, i) => i);
  assert.deepEqual(shuffled(items, seeded(1)).toSorted((a, b) => a - b), items);
});

test('pool starts items in order and never runs more than the limit at once', async () => {
  let running = 0;
  let peak = 0;
  const started = [];
  await pool(Array.from({ length: 9 }, (_, i) => i), 3, async item => {
    started.push(item);
    running += 1;
    peak = Math.max(peak, running);
    await new Promise(resolve => setTimeout(resolve, 2));
    running -= 1;
  });
  assert.deepEqual(started, started.toSorted((a, b) => a - b));
  assert.ok(peak <= 3);
});

test('removeTree removes a tree whose directories are read-only (a Go module cache)', () => {
  const { dir, cleanup } = scratch('readonly');
  try {
    const cache = path.join(dir, 'home', 'go', 'pkg', 'mod', 'example.com@v1');
    fs.mkdirSync(cache, { recursive: true });
    fs.writeFileSync(path.join(cache, 'a.go'), 'package a');
    fs.chmodSync(path.join(cache, 'a.go'), 0o444);
    fs.chmodSync(cache, 0o555);
    fs.chmodSync(path.dirname(cache), 0o555);
    removeTree(path.join(dir, 'home'));
    assert.equal(fs.existsSync(path.join(dir, 'home')), false);
  } finally {
    cleanup();
  }
});
