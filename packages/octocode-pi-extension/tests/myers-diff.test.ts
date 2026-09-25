/**
 * Myers diff correctness + performance regression tests.
 * Baseline (LCS DP): ~230ms at 3k lines, quadratic. Guard: linear scaling for a
 * single-line change (3k vs 300 lines) plus a 50ms backstop at 3k lines.
 */
import assert from 'node:assert/strict';
import { performance } from 'node:perf_hooks';
import { test } from 'vitest';
import {
  diffOps,
  generateDiffArtifacts,
} from '../src/tools/edit-tool.js';

function makeFile(lines: number): string {
  const rows: string[] = [];
  for (let i = 0; i < lines; i++) {
    rows.push(`L${String(i).padStart(6, '0')} ${'x'.repeat(70)}`);
  }
  return rows.join('\n');
}

test('diffOps: single-line change produces remove+add at the right place', () => {
  const oldContent = 'a\nb\nc\n';
  const newContent = 'a\nB\nc\n';
  const ops = diffOps(oldContent, newContent);
  const changed = ops.filter((op) => op.type !== 'same');
  assert.deepEqual(changed, [
    { type: 'remove', line: 'b' },
    { type: 'add', line: 'B' },
  ]);
});

test('diffOps: identical content is all same', () => {
  const content = 'one\ntwo\n';
  const ops = diffOps(content, content);
  assert.ok(ops.every((op) => op.type === 'same'));
});

test('diffOps: append and delete reconstruct both sides', () => {
  const oldContent = 'a\nb';
  const appended = 'a\nb\nc';
  const deleted = 'a';
  const appendOps = diffOps(oldContent, appended);
  assert.deepEqual(
    appendOps.filter((op) => op.type !== 'same'),
    [{ type: 'add', line: 'c' }],
  );
  const rebuiltAppend = appendOps
    .filter((op) => op.type !== 'remove')
    .map((op) => op.line)
    .join('\n');
  assert.equal(rebuiltAppend, appended);

  const deleteOps = diffOps(oldContent, deleted);
  assert.deepEqual(
    deleteOps.filter((op) => op.type !== 'same'),
    [{ type: 'remove', line: 'b' }],
  );
  const rebuiltDelete = deleteOps
    .filter((op) => op.type !== 'remove')
    .map((op) => op.line)
    .join('\n');
  assert.equal(rebuiltDelete, deleted);
});

test('diffOps: multi-hunk change keeps both edits', () => {
  const oldContent = 'a\nb\nc\nd\ne\n';
  const newContent = 'a\nB\nc\nd\nE\n';
  const changed = diffOps(oldContent, newContent).filter((op) => op.type !== 'same');
  assert.deepEqual(changed, [
    { type: 'remove', line: 'b' },
    { type: 'add', line: 'B' },
    { type: 'remove', line: 'e' },
    { type: 'add', line: 'E' },
  ]);
});

test('generateDiffArtifacts: single Myers pass yields both diff and patch', () => {
  const oldContent = 'a\nb\nc\n';
  const newContent = 'a\nB\nc\n';
  const { diff, patch } = generateDiffArtifacts('f.ts', oldContent, newContent);
  assert.match(diff, /^- b$/m);
  assert.match(diff, /^\+ B$/m);
  assert.match(patch, /^--- f\.ts$/m);
  assert.match(patch, /^\+\+\+ f\.ts$/m);
  assert.match(patch, /@@/);
  assert.doesNotMatch(patch, /omitted/);
});

// Wall-clock budgets flake when the suite shares the CPU. The regression this
// guards is algorithmic (LCS DP is quadratic: ~230ms at 3k lines), so measure a
// 10x input growth against a 1x baseline, interleaved so both sizes see the same
// load, and use each size's fastest run (load only ever adds time). Linear Myers
// scales ~10x; quadratic LCS scales ~100x.
test('PERF: single-change Myers diff scales linearly (3000 vs 300 lines)', () => {
  const sizes = [300, 3000] as const;
  const inputs = sizes.map(lines => {
    const oldContent = makeFile(lines);
    const pivot = `L${String(lines / 2).padStart(6, '0')}`;
    return { oldContent, newContent: oldContent.replace(pivot, 'CHANGED') };
  });
  for (const { oldContent, newContent } of inputs) generateDiffArtifacts('big.ts', oldContent, newContent);
  const best = [Infinity, Infinity];
  for (let round = 0; round < 15; round++) {
    inputs.forEach(({ oldContent, newContent }, index) => {
      const t0 = performance.now();
      generateDiffArtifacts('big.ts', oldContent, newContent);
      best[index] = Math.min(best[index]!, performance.now() - t0);
    });
  }
  const [small, large] = best as [number, number];
  const ratio = large / Math.max(small, 0.05);
  assert.ok(
    ratio < 30,
    `expected ~linear scaling for 10x lines, got ${ratio.toFixed(1)}x (${small.toFixed(2)}ms -> ${large.toFixed(2)}ms); quadratic LCS is ~100x`,
  );
  // Absolute backstop an order of magnitude under the LCS baseline (~230ms).
  assert.ok(large < 50, `expected fastest 3k-line Myers+artifacts < 50ms, got ${large.toFixed(2)}ms`);
});

test('native diff reconstructs both inputs including final empty lines and Unicode', () => {
  const cases = ['', '\n', 'a', 'a\n', 'a\n\n', 'a\nb', '😀\r\n界', 'a\na\nb'];
  for (const oldContent of cases) for (const newContent of cases) {
    const ops = diffOps(oldContent, newContent);
    assert.equal(ops.filter(op => op.type !== 'add').map(op => op.line).join('\n'), oldContent);
    assert.equal(ops.filter(op => op.type !== 'remove').map(op => op.line).join('\n'), newContent);
  }
});

test('PERF: 10000-line single-change still returns a real diff (no omit)', () => {
  const oldContent = makeFile(10_000);
  const newContent = oldContent.replace('L005000', 'CHANGED');
  const { diff, patch } = generateDiffArtifacts('huge.ts', oldContent, newContent);
  assert.doesNotMatch(diff, /omitted/);
  assert.doesNotMatch(patch, /omitted/);
  assert.match(diff, /CHANGED|L005000/);
});
