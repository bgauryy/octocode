/**
 * Myers diff correctness + performance regression tests.
 * Baseline (LCS DP): ~230ms at 3k lines. Target: well under 20ms for a
 * single-line change in a 3k-line file.
 */
import assert from 'node:assert/strict';
import { performance } from 'node:perf_hooks';
import { test } from 'vitest';
import {
  diffOps,
  diffOpsJs,
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
  // Avoid trailing-newline empty-line edge: native `similar` and JS `split('\n')`
  // disagree on a final empty segment; edit previews do not need that phantom line.
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

test('PERF: 3000-line single-change Myers diff stays under 5ms', () => {
  const oldContent = makeFile(3000);
  const newContent = oldContent.replace('L001500', 'CHANGED');
  // Warmup
  generateDiffArtifacts('big.ts', oldContent, newContent);
  const times: number[] = [];
  for (let i = 0; i < 7; i++) {
    const t0 = performance.now();
    generateDiffArtifacts('big.ts', oldContent, newContent);
    times.push(performance.now() - t0);
  }
  times.sort((a, b) => a - b);
  const med = times[Math.floor(times.length / 2)]!;
  assert.ok(
    med < 5,
    `expected median Myers+artifacts < 5ms at 3k lines, got ${med.toFixed(2)}ms (was ~230ms+ LCS)`,
  );
});

test('diffOpsJs and native engine agree on changed lines when native is available', () => {
  const js = diffOpsJs('a\nb\nc', 'a\nB\nc').filter((op) => op.type !== 'same');
  assert.deepEqual(js, [
    { type: 'remove', line: 'b' },
    { type: 'add', line: 'B' },
  ]);
  const previous = process.env['OCTOCODE_EDIT_NATIVE_DIFF'];
  process.env['OCTOCODE_EDIT_NATIVE_DIFF'] = '1';
  try {
    const native = diffOps('a\nb\nc', 'a\nB\nc').filter((op) => op.type !== 'same');
    // If native failed to load, diffOps falls back to JS — still a pass.
    assert.deepEqual(native, js);
  } finally {
    if (previous === undefined) delete process.env['OCTOCODE_EDIT_NATIVE_DIFF'];
    else process.env['OCTOCODE_EDIT_NATIVE_DIFF'] = previous;
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
