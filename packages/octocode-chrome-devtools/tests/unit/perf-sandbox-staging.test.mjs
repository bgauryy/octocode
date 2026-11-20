import assert from 'node:assert/strict';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  statSync,
  utimesSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';

const { RUN_DIR_MAX_AGE_DAYS, helperSetHash, pruneOldRunDirs, stageHelpers } =
  await import(
    resolve(import.meta.dirname, '../../dist/engine/sandbox-staging.mjs')
  );

const scratch = () => mkdtempSync(join(tmpdir(), 'chrome-perf-staging-'));
const names = ['a.mjs', 'b.mjs', 'missing.mjs'];

function sources(dir) {
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, 'a.mjs'), 'export const a = 1;\n');
  writeFileSync(join(dir, 'b.mjs'), 'export const b = 2;\n');
}

test('staging copies the helper set once; an identical second call copies zero files', () => {
  const root = scratch();
  try {
    sources(join(root, 'src'));
    const target = join(root, 'helpers');
    mkdirSync(target);
    const present = names.filter(n => existsSync(join(root, 'src', n))).length;
    assert.equal(stageHelpers(join(root, 'src'), target, names), present);
    assert.equal(stageHelpers(join(root, 'src'), target, names), 0);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('the hash changes when a helper changes, and a changed helper is restaged', () => {
  const root = scratch();
  try {
    const src = join(root, 'src');
    sources(src);
    const target = join(root, 'helpers');
    mkdirSync(target);
    const before = helperSetHash(src, names);
    stageHelpers(src, target, names);
    writeFileSync(join(src, 'b.mjs'), 'export const b = 3;\n');
    assert.notEqual(helperSetHash(src, names), before);
    assert(stageHelpers(src, target, names) > 0);
    assert.equal(stageHelpers(src, target, names), 0);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('a deleted staged helper is restaged even when the stamp matches', () => {
  const root = scratch();
  try {
    sources(join(root, 'src'));
    const target = join(root, 'helpers');
    mkdirSync(target);
    stageHelpers(join(root, 'src'), target, names);
    rmSync(join(target, 'a.mjs'));
    assert(stageHelpers(join(root, 'src'), target, names) > 0);
    assert(existsSync(join(target, 'a.mjs')));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('an unwritable target falls back to a warning, never a throw', () => {
  const root = scratch();
  try {
    sources(join(root, 'src'));
    const warnings = [];
    assert.equal(
      stageHelpers(join(root, 'src'), join(root, 'nope'), names, m =>
        warnings.push(m)
      ),
      0
    );
    assert(warnings.length > 0);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('old run folders are pruned past the age limit; fresh, current, helper and meta folders stay', () => {
  const root = scratch();
  try {
    const now = Date.now();
    const day = 24 * 60 * 60 * 1000;
    const make = (name, ageMs) => {
      const dir = join(root, name);
      mkdirSync(dir);
      const t = (now - ageMs) / 1000;
      utimesSync(dir, t, t);
      return dir;
    };
    const old = make('2020-01-01-00-00-00', (RUN_DIR_MAX_AGE_DAYS + 1) * day);
    const current = make(
      '2020-01-02-00-00-00',
      (RUN_DIR_MAX_AGE_DAYS + 1) * day
    );
    const fresh = make('2026-01-01-00-00-00', day / 2);
    const helpers = make('helpers', (RUN_DIR_MAX_AGE_DAYS + 5) * day);
    const meta = make('session-meta', (RUN_DIR_MAX_AGE_DAYS + 5) * day);
    const removed = pruneOldRunDirs(root, now, current);
    assert.deepEqual(removed, [old]);
    for (const kept of [current, fresh, helpers, meta])
      assert(statSync(kept).isDirectory());
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
