/**
 * TDD tests for file-state.ts
 *
 * Covers the shared file read-state tracking and mutation queue used by
 * edit-tool.ts, write-tool.ts, and octocode-tools.ts.
 */
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test, beforeEach, afterEach } from 'vitest';
import {
  resolveFilePath,
  atomicWriteUtf8,
  withFileMutationQueue,
  recordFileReadState,
  recordFileReadStateFromContent,
  checkReadState,
  clearReadStatesForTests,
  MAX_RECORDED_READ_STATES,
} from '../src/tools/file-state.js';

let tmpDir: string;

beforeEach(() => {
  tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'file-state-test-'));
  clearReadStatesForTests();
});

afterEach(() => {
  fs.rmSync(tmpDir, { recursive: true, force: true });
  clearReadStatesForTests();
});

// ─── resolveFilePath ──────────────────────────────────────────────────────────

test('resolveFilePath: absolute path is returned as-is', () => {
  const abs = path.join(os.tmpdir(), 'example.txt');
  assert.equal(resolveFilePath(abs, '/some/cwd'), abs);
});

test('resolveFilePath: relative path is resolved against cwd', () => {
  const cwd = '/my/project';
  assert.equal(resolveFilePath('src/index.ts', cwd), '/my/project/src/index.ts');
});

test('resolveFilePath: defaults to process.cwd() when cwd omitted', () => {
  const rel = 'some/file.ts';
  assert.equal(resolveFilePath(rel), path.resolve(rel));
});

// ─── atomicWriteUtf8 ──────────────────────────────────────────────────────────

test('atomicWriteUtf8: creates parent directories and writes UTF-8 content', async () => {
  const file = path.join(tmpDir, 'nested', 'atomic.txt');
  await atomicWriteUtf8(file, 'héllo');
  assert.equal(fs.readFileSync(file, 'utf8'), 'héllo');
});

test('atomicWriteUtf8: uses unique temp files and leaves no shared temp artifact', async () => {
  const file = path.join(tmpDir, 'atomic-collision.txt');
  await Promise.all([
    atomicWriteUtf8(file, 'first'),
    atomicWriteUtf8(file, 'second'),
  ]);
  assert.match(fs.readFileSync(file, 'utf8'), /^(first|second)$/);
  assert.deepEqual(
    fs.readdirSync(tmpDir).filter((name) => name.includes('.octocode-')),
    [],
  );
});

// ─── withFileMutationQueue ────────────────────────────────────────────────────

test('withFileMutationQueue: executes the operation and resolves its value', async () => {
  const result = await withFileMutationQueue('/some/key', () => Promise.resolve(42));
  assert.equal(result, 42);
});

test('withFileMutationQueue: propagates errors from the fn to the caller', async () => {
  await assert.rejects(
    () => withFileMutationQueue('/some/key', () => Promise.reject(new Error('boom'))),
    /boom/,
  );
});

test('withFileMutationQueue: serialises concurrent writes on the same key', async () => {
  const filePath = path.join(tmpDir, 'serial.txt');
  fs.writeFileSync(filePath, 'init');

  const order: number[] = [];
  const op = (n: number, delay: number): Promise<void> =>
    withFileMutationQueue(filePath, () =>
      new Promise((res) => setTimeout(() => { order.push(n); res(); }, delay)),
    );

  // Launch all three simultaneously; expect them to execute in enqueue order.
  await Promise.all([op(1, 30), op(2, 10), op(3, 5)]);
  assert.deepEqual(order, [1, 2, 3]);
});

test('withFileMutationQueue: queue continues after a failed operation', async () => {
  const key = '/some/key-recovery';
  const results: string[] = [];

  await Promise.allSettled([
    withFileMutationQueue(key, () => Promise.reject(new Error('fail'))),
    withFileMutationQueue(key, () => { results.push('ok'); return Promise.resolve(); }),
  ]);
  assert.deepEqual(results, ['ok'], 'second op must run even if first failed');
});

test('withFileMutationQueue: different keys run independently (no blocking)', async () => {
  const starts: string[] = [];
  const done: string[] = [];

  const op = (key: string, delay: number) =>
    withFileMutationQueue(key, () =>
      new Promise<void>((res) => {
        starts.push(key);
        setTimeout(() => { done.push(key); res(); }, delay);
      }),
    );

  await Promise.all([op('a', 30), op('b', 5)]);
  // Both started before either finished (different keys don't serialise each other)
  assert.equal(starts.length, 2);
  assert.equal(done.length, 2);
  // 'b' finishes first because its delay is shorter
  assert.equal(done[0], 'b');
  assert.equal(done[1], 'a');
});

// ─── recordFileReadState / checkReadState ─────────────────────────────────────

test('recordFileReadState then checkReadState returns "fresh" for unchanged file', async () => {
  const file = path.join(tmpDir, 'track.txt');
  fs.writeFileSync(file, 'hello world');
  await recordFileReadState(file, tmpDir);
  const result = await checkReadState(file, false);
  assert.equal(result.state, 'fresh');
});

test('checkReadState returns "missing" when file was never recorded', async () => {
  const file = path.join(tmpDir, 'untracked.txt');
  fs.writeFileSync(file, 'content');
  const result = await checkReadState(file, false);
  assert.equal(result.state, 'missing');
});

test('checkReadState throws when requireRecentRead is true and no state is recorded', async () => {
  const file = path.join(tmpDir, 'no-state.txt');
  fs.writeFileSync(file, 'content');
  await assert.rejects(
    () => checkReadState(file, true),
    /No prior localGetFileContent read state recorded/,
  );
});

test('checkReadState throws "changed" error when file content is modified after recording', async () => {
  const file = path.join(tmpDir, 'stale.txt');
  fs.writeFileSync(file, 'original content');
  await recordFileReadState(file, tmpDir);

  // Modify content — this will change both mtime and content hash
  fs.writeFileSync(file, 'modified content');

  await assert.rejects(
    () => checkReadState(file, false),
    /File changed since last recorded read/,
  );
});

test('clearReadStatesForTests removes all recorded states', async () => {
  const file = path.join(tmpDir, 'clear-test.txt');
  fs.writeFileSync(file, 'data');
  await recordFileReadState(file, tmpDir);
  clearReadStatesForTests();
  // After clearing, state is missing → should not throw with requireRecentRead=false
  const result = await checkReadState(file, false);
  assert.equal(result.state, 'missing');
});

test('recordFileReadState evicts the oldest read state when the cache exceeds its cap', async () => {
  const files = Array.from({ length: MAX_RECORDED_READ_STATES + 1 }, (_, index) =>
    path.join(tmpDir, `tracked-${index}.txt`),
  );
  for (const [index, file] of files.entries()) {
    fs.writeFileSync(file, `content-${index}`);
    await recordFileReadState(file, tmpDir);
  }

  assert.equal((await checkReadState(files[0]!, false)).state, 'missing');
  assert.equal((await checkReadState(files.at(-1)!, true)).state, 'fresh');
}, 15_000);

test('recordFileReadState accepts an absolute path (cwd unused)', async () => {
  const file = path.join(tmpDir, 'abs.txt');
  fs.writeFileSync(file, 'absolute');
  // Pass absolute path; cwd is irrelevant
  await recordFileReadState(file);
  const result = await checkReadState(file, true);
  assert.equal(result.state, 'fresh');
});

test('recordFileReadState resolves relative path against provided cwd', async () => {
  const file = path.join(tmpDir, 'relative.txt');
  fs.writeFileSync(file, 'relative path test');
  // Pass relative filename + cwd
  await recordFileReadState('relative.txt', tmpDir);
  const result = await checkReadState(file, true);
  assert.equal(result.state, 'fresh');
});

test('checkReadState returns "fresh" even when mtime changes but content is identical', async () => {
  const file = path.join(tmpDir, 'touch-test.txt');
  const content = 'same content both times';
  fs.writeFileSync(file, content);
  await recordFileReadState(file, tmpDir);

  // Simulate editor "touch" — change mtime without changing content
  // We write the exact same bytes so content hash matches
  await new Promise((res) => setTimeout(res, 10)); // ensure mtime would differ
  fs.writeFileSync(file, content);

  // No error — content hash matches, so not stale
  const result = await checkReadState(file, false);
  assert.equal(result.state, 'fresh');
});

test('checkReadState: content-anchored edit proceeds (advisory) when file changed since read', async () => {
  const f = path.join(tmpDir, 'a.txt');
  fs.writeFileSync(f, 'one\n', 'utf8');
  await recordFileReadState(f, tmpDir);
  // Simulate an external change since the recorded read.
  fs.writeFileSync(f, 'two\n', 'utf8');

  // content-anchored (oldText edits) → advisory 'stale', no throw.
  const soft = await checkReadState(f, false, { contentAnchored: true });
  assert.equal(soft.state, 'stale');

  // position-anchored (lineRange) → still hard-fails.
  await assert.rejects(
    () => checkReadState(f, false, { contentAnchored: false }),
    /File changed since last recorded read/,
  );

  // explicit requireRecentRead → hard-fails even when content-anchored.
  await assert.rejects(
    () => checkReadState(f, true, { contentAnchored: true }),
    /File changed since last recorded read/,
  );
});

test('checkReadState: unchanged file is fresh regardless of anchoring', async () => {
  const f = path.join(tmpDir, 'b.txt');
  fs.writeFileSync(f, 'same\n', 'utf8');
  await recordFileReadState(f, tmpDir);
  const r = await checkReadState(f, false, { contentAnchored: true });
  assert.equal(r.state, 'fresh');
});

// ─── Explicit recent-read contract ——————————————————————————————————

test('checkReadState: requireRecentRead:true + no state throws even for content-anchored edits', async () => {
  const f = path.join(tmpDir, 'no-state-anchored.txt');
  fs.writeFileSync(f, 'hello\nworld\n', 'utf8');
  // No recordFileReadState call — state is intentionally absent.
  await assert.rejects(
    () => checkReadState(f, true, { contentAnchored: true }),
    /No prior localGetFileContent read state/,
  );
});

test('checkReadState: requireRecentRead:true + no state + contentAnchored:false → still throws', async () => {
  // Position-anchored edits (lineRange without oldText) genuinely need fresh line numbers;
  // the absence of a prior read is a hard error even when requireRecentRead is set.
  const f = path.join(tmpDir, 'no-state-position.txt');
  fs.writeFileSync(f, 'line1\nline2\n', 'utf8');
  await assert.rejects(
    () => checkReadState(f, true, { contentAnchored: false }),
    /No prior localGetFileContent read state/,
  );
});

test('checkReadState: requireRecentRead:true + no state + no opts still throws', async () => {
  // When contentAnchored is not provided (undefined/falsy), the conservative path applies.
  const f = path.join(tmpDir, 'no-state-noopt.txt');
  fs.writeFileSync(f, 'data', 'utf8');
  await assert.rejects(
    () => checkReadState(f, true),
    /No prior localGetFileContent read state/,
  );
});

test('recordFileReadStateFromContent: subsequent checkReadState sees fresh without re-reading file', async () => {
  const f = path.join(tmpDir, 'from-content.txt');
  const content = 'written content\n';
  fs.writeFileSync(f, content, 'utf8');
  await recordFileReadStateFromContent(f, content);
  const check = await checkReadState(f, false);
  assert.equal(check.state, 'fresh');
});

test('recordFileReadStateFromContent: reports stale when file later changes', async () => {
  const f = path.join(tmpDir, 'from-content-stale.txt');
  const content = 'original\n';
  fs.writeFileSync(f, content, 'utf8');
  await recordFileReadStateFromContent(f, content);
  // Change the file content externally
  fs.writeFileSync(f, 'modified\n', 'utf8');
  // Content-anchored false: should throw stale
  await assert.rejects(
    () => checkReadState(f, false, { contentAnchored: false }),
    /File changed since last recorded read/,
  );
});

test('checkReadState: mtime/size mismatch with different content throws for non-anchored edits', async () => {
  // When content differs AND contentAnchored is false, the stale error must propagate.
  const f = path.join(tmpDir, 'mismatch-throws.txt');
  fs.writeFileSync(f, 'line1\nline2\n', 'utf8');
  await recordFileReadState(f);
  // Replace with visibly different content (triggers mtime + content change)
  fs.writeFileSync(f, 'completely different content\n', 'utf8');
  await assert.rejects(
    () => checkReadState(f, false, { contentAnchored: false }),
    /File changed since last recorded read/,
  );
});

test('checkReadState: same-content editor touch returns fresh even for non-anchored edits', async () => {
  // A same-mtime-skipped or same-content rewrite should never fail as stale.
  const f = path.join(tmpDir, 'same-content-touch.txt');
  const content = 'hello\nworld\n';
  fs.writeFileSync(f, content, 'utf8');
  await recordFileReadState(f);
  // Overwrite with identical bytes (editor save without changes) — triggers mtime change
  fs.writeFileSync(f, content, 'utf8');
  const check = await checkReadState(f, false, { contentAnchored: false });
  assert.equal(check.state, 'fresh', 'identical rewrite must not be falsely reported stale');
});
