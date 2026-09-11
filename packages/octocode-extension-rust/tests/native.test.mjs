import { afterEach, test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, realpath, readFile, writeFile, mkdir, rm, symlink, lstat, chmod, rename, readdir, copyFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import {
  NativeCancellation, NativeErrorCodes, NativeOperationError, nativeErrorCode,
  snapshotFile, replaceFile, deleteFile, computeLineDiff, computeLineDiffAsync, generateDiffArtifactsAsync,
} from '../index.js';

const windows = process.platform === 'win32';
const roots = [];
async function fixture() {
  const root = await realpath(await mkdtemp(join(tmpdir(), 'octocode-native-')));
  roots.push(root);
  return root;
}
afterEach(async () => { await Promise.all(roots.splice(0).map(root => rm(root, { recursive: true, force: true }))); });
const maximum = 4 * 1024 * 1024;

test('snapshot, recursive create, replacement, and delete use real native receipts', async () => {
  const root = await fixture();
  const path = join(root, 'nested', 'deep', 'file');
  const missing = await snapshotFile(path, maximum, true);
  assert.equal(missing.kind, 'missing');
  const bytes = Buffer.from('\ufeffhello\r\nworld\rthird\n');
  const created = await replaceFile(path, bytes, missing.version, maximum, 0o600);
  assert.equal(created.committed, true);
  assert.equal(created.bytes, bytes.length);
  assert.equal(typeof created.durable, 'boolean');
  if (!windows) assert.deepEqual(created, { committed: true, durable: true, warnings: [], bytes: bytes.length });
  else {
    assert.equal(created.durable, false);
    assert.ok(created.warnings.length > 0);
  }
  if (!windows) assert.equal((await lstat(path)).mode & 0o777, 0o600);
  const current = await snapshotFile(path, maximum, true);
  assert.deepEqual(current.content, bytes);
  assert.equal(current.digest, createHash('sha256').update(bytes).digest('hex'));
  assert.equal((await snapshotFile(path, maximum, false)).content, undefined);
  const replacement = Buffer.from('changed');
  await replaceFile(path, replacement, current.version, maximum);
  assert.deepEqual(await readFile(path), replacement);
  if (!windows) assert.equal((await lstat(path)).mode & 0o777, 0o600);
  const last = await snapshotFile(path, maximum, false);
  assert.equal((await deleteFile(path, last.version, maximum)).committed, true);
  assert.equal((await snapshotFile(path, maximum, false)).exists, false);
});

test('preconditions detect changed bytes, recreated inode, missing ancestor replacement, and changed mode', async () => {
  const root = await fixture();
  const path = join(root, 'file');
  await writeFile(path, 'before');
  let prior = await snapshotFile(path, maximum, false);
  await writeFile(path, 'after');
  await assert.rejects(replaceFile(path, Buffer.from('wrong'), prior.version, maximum), error => {
    assert.ok(error instanceof NativeOperationError);
    assert.equal(error.code, NativeErrorCodes.PRECONDITION_FAILED);
    assert.equal(nativeErrorCode(error), NativeErrorCodes.PRECONDITION_FAILED);
    assert.match(error.message, /PRECONDITION_FAILED/);
    return true;
  });
  prior = await snapshotFile(path, maximum, false);
  await rename(path, join(root, 'old'));
  await writeFile(path, 'after');
  await assert.rejects(deleteFile(path, prior.version, maximum), /PRECONDITION_FAILED/);
  if (!windows) {
    prior = await snapshotFile(path, maximum, false);
    await chmod(path, 0o700);
    await assert.rejects(replaceFile(path, Buffer.from('wrong'), prior.version, maximum), /PRECONDITION_FAILED/);
  }
  const parent = join(root, 'parent');
  await mkdir(parent);
  const absent = join(parent, 'missing', 'leaf');
  prior = await snapshotFile(absent, maximum, false);
  await rename(parent, join(root, 'previous-parent'));
  await mkdir(parent);
  await assert.rejects(replaceFile(absent, Buffer.from('wrong'), prior.version, maximum), /PRECONDITION_FAILED/);
  assert.equal((await snapshotFile(absent, maximum, false)).exists, false);
});

test('explicit private mode overrides an existing public mode', { skip: windows && 'POSIX permission bits do not represent Windows ACLs' }, async () => {
  const root = await fixture();
  const path = join(root, 'file');
  await writeFile(path, 'public', { mode: 0o644 });
  const before = await snapshotFile(path, maximum, false);
  await replaceFile(path, Buffer.from('private'), before.version, maximum, 0o600);
  if (!windows) assert.equal((await lstat(path)).mode & 0o777, 0o600);
});

test('private parent creation preserves existing directories and applies to every new level', async () => {
  const root = await fixture();
  const existing = join(root, 'existing');
  await mkdir(existing, { mode: 0o750 });
  const existingMode = (await lstat(existing)).mode;
  const path = join(existing, 'private', 'deep', 'file');
  const before = await snapshotFile(path, maximum, false);
  const result = await replaceFile(path, Buffer.from('private'), before.version, maximum, 0o600, undefined, 0o700);
  assert.equal(result.committed, true);
  assert.equal(await readFile(path, 'utf8'), 'private');
  assert.equal((await lstat(existing)).mode, existingMode);
  if (!windows) {
    assert.equal((await lstat(join(existing, 'private'))).mode & 0o777, 0o700);
    assert.equal((await lstat(dirname(path))).mode & 0o777, 0o700);
    assert.equal((await lstat(path)).mode & 0o777, 0o600);
  }
});

test('invalid parent modes are rejected before any parent or file mutation', async () => {
  const root = await fixture();
  const path = join(root, 'missing', 'file');
  const before = await snapshotFile(path, maximum, false);
  for (const mode of [0, 0o600, 0o777, -1, 0.5, NaN, Infinity, null, '0700']) {
    await assert.rejects(replaceFile(path, Buffer.from('wrong'), before.version, maximum, undefined, undefined, mode), /parentMode/);
    assert.deepEqual(await readdir(root), []);
  }
});

test('cancelled private recursive creation leaves missing parents absent', async () => {
  const root = await fixture();
  const path = join(root, 'private', 'deep', 'file');
  const before = await snapshotFile(path, maximum, false);
  const cancellation = new NativeCancellation(); cancellation.cancel();
  await assert.rejects(replaceFile(path, Buffer.from('wrong'), before.version, maximum, 0o600, cancellation, 0o700), /CANCELLED/);
  assert.deepEqual(await readdir(root), []);
});

test('native direct calls reject relative paths, dot components and symlink traversal', async () => {
  const root = await fixture();
  await writeFile(join(root, 'file'), 'target');
  await symlink(root, join(root, 'alias'), 'dir');
  await symlink(join(root, 'file'), join(root, 'leaf'), 'file');
  for (const path of ['relative', `${root}/./file`, `${root}/../file`, `${root}//file`, `${root}/alias/file`, `${root}/leaf`]) {
    await assert.rejects(snapshotFile(path, maximum, true));
  }
  const leaf = await snapshotFile(join(root, 'leaf'), maximum, false, true);
  assert.equal(leaf.kind, 'symlink');
  await assert.rejects(replaceFile(join(root, 'leaf'), Buffer.from('wrong'), leaf.version, maximum), /NOT_REGULAR_FILE/);
  await deleteFile(join(root, 'leaf'), leaf.version, maximum);
  assert.equal(await readFile(join(root, 'file'), 'utf8'), 'target');
});

test('replacing a validated parent with a symlink fails without writing elsewhere', async () => {
  const root = await fixture();
  const parent = join(root, 'parent');
  const outside = join(root, 'outside');
  await mkdir(parent);
  await mkdir(outside);
  const path = join(parent, 'file');
  const prior = await snapshotFile(path, maximum, false);
  await rename(parent, join(root, 'old-parent'));
  await symlink(outside, parent, 'dir');
  await assert.rejects(replaceFile(path, Buffer.from('wrong'), prior.version, maximum), /UNSAFE_PATH/);
  assert.deepEqual(await readdir(outside), []);
});

test('bounded reads/writes include empty files and reject invalid limits without coercion', async () => {
  const root = await fixture();
  const path = join(root, 'file');
  await writeFile(path, '12345');
  const before = await snapshotFile(path, 5, true);
  await assert.rejects(snapshotFile(path, 4, true), /TOO_LARGE/);
  await assert.rejects(replaceFile(path, Buffer.from('123456'), before.version, 5), error => {
    assert.equal(nativeErrorCode(error), NativeErrorCodes.TOO_LARGE);
    return true;
  });
  for (const limit of [-1, 1.5, NaN, Infinity, 0x100000000]) {
    await assert.rejects(snapshotFile(path, limit, false), /maxBytes/);
  }
  await writeFile(path, '');
  assert.deepEqual((await snapshotFile(path, 0, true)).content, Buffer.alloc(0));
});

test('directory targets are rejected', async () => {
  const root = await fixture();
  await assert.rejects(snapshotFile(root, maximum, true), /NOT_REGULAR_FILE/);
});

test('FIFO targets fail promptly without blocking reads', { skip: windows && 'Windows has no POSIX FIFO nodes' }, async () => {
  const root = await fixture();
  const fifo = join(root, 'fifo');
  execFileSync('mkfifo', [fifo]);
  await assert.rejects(snapshotFile(fifo, maximum, true), /NOT_REGULAR_FILE/);
  assert.equal((await lstat(fifo)).isFIFO(), true);
});

test('cancellation before commit leaves original bytes and no temporary files', async () => {
  const root = await fixture();
  const path = join(root, 'file');
  await writeFile(path, 'original');
  const before = await snapshotFile(path, maximum, false);
  const cancellation = new NativeCancellation();
  cancellation.cancel();
  await assert.rejects(replaceFile(path, Buffer.from('wrong'), before.version, maximum, undefined, cancellation), /CANCELLED/);
  await assert.rejects(deleteFile(path, before.version, maximum, cancellation), /CANCELLED/);
  assert.equal(await readFile(path, 'utf8'), 'original');
  assert.deepEqual(await readdir(root), ['file']);
});

test('concurrent identical expected versions permit only one commit', async () => {
  const root = await fixture();
  const path = join(root, 'file');
  const before = await snapshotFile(path, maximum, false);
  const results = await Promise.allSettled([
    replaceFile(path, Buffer.from('one'), before.version, maximum),
    replaceFile(path, Buffer.from('two'), before.version, maximum),
  ]);
  assert.equal(results.filter(result => result.status === 'fulfilled').length, 1);
  assert.equal(results.filter(result => result.status === 'rejected').length, 1);
  assert.ok(['one', 'two'].includes(await readFile(path, 'utf8')));
  assert.deepEqual(await readdir(root), ['file']);
});

test('sync and async native line diff preserve trailing empty lines and reconstruct both inputs', async () => {
  for (const [oldText, newText] of [['', ''], ['a\n', 'b\n'], ['a\r\nb\r', 'a\r\nc\r'], ['a\nb\na', 'b\na\nb'], ['😀\n', '😀\nlast']]) {
    const operations = computeLineDiff(oldText, newText);
    assert.deepEqual(await computeLineDiffAsync(oldText, newText), operations);
    assert.equal(operations.filter(value => value.opType !== 'add').map(value => value.line).join('\n'), oldText);
    assert.equal(operations.filter(value => value.opType !== 'remove').map(value => value.line).join('\n'), newText);
  }
});

test('loader fails clearly without a native binary instead of falling back to Node writes', async () => {
  const root = await fixture();
  const packageRoot = dirname(dirname(fileURLToPath(import.meta.url)));
  await copyFile(join(packageRoot, 'index.cjs'), join(root, 'index.cjs'));
  await copyFile(join(packageRoot, 'platforms.cjs'), join(root, 'platforms.cjs'));
  assert.throws(() => execFileSync(process.execPath, ['-e', 'require(process.argv[1])', join(root, 'index.cjs')], { stdio: 'pipe' }), /NATIVE_UNAVAILABLE/);
});

test('native worker returns compact diff and exact single-hunk patch without unchanged edges', async () => {
  assert.deepEqual(await generateDiffArtifactsAsync('file.ts', 'same\nold\ninside\nremove\nsame-end\n', 'same\nnew\ninside\nadded\nsame-end\n'), {
    diff: '- old\n+ new\n- remove\n+ added',
    patch: '--- file.ts\n+++ file.ts\n@@ -2,3 +2,3 @@\n-old\n+new\n inside\n-remove\n+added\n',
  });
  assert.deepEqual(await generateDiffArtifactsAsync('file', 'unchanged\n', 'unchanged\n'), { diff: '', patch: '--- file\n+++ file\n@@ -3,0 +3,0 @@\n' });
  assert.deepEqual(await generateDiffArtifactsAsync('file', 'a\n', 'a\nb\n'), {
    diff: '+ b', patch: '--- file\n+++ file\n@@ -2,0 +2,1 @@\n+b\n',
  });
});

test('rebuilding publishes a fresh inode while the current process retains a working loaded addon', { skip: windows && 'Windows locks loaded DLLs; rebuild after the host exits' }, () => {
  const packageRoot = dirname(dirname(fileURLToPath(import.meta.url)));
  execFileSync(process.execPath, [join(packageRoot, 'scripts', 'build.mjs'), '--release'], { stdio: 'pipe' });
  assert.deepEqual(computeLineDiff('before', 'after'), [
    { opType: 'remove', line: 'before' }, { opType: 'add', line: 'after' },
  ]);
});

test('ill-formed Unicode paths cannot alias replacement-character filenames across N-API', async () => {
  const root = await fixture();
  const actualPath = join(root, 'x\ufffd');
  const malformedPath = join(root, 'x\ud800');
  await writeFile(actualPath, 'untouched');
  const before = await snapshotFile(actualPath, maximum, false);
  await assert.rejects(snapshotFile(malformedPath, maximum, false), /INVALID_PATH/);
  await assert.rejects(replaceFile(malformedPath, Buffer.from('wrong'), before.version, maximum), /INVALID_PATH/);
  await assert.rejects(deleteFile(malformedPath, before.version, maximum), /INVALID_PATH/);
  assert.equal(await readFile(actualPath, 'utf8'), 'untouched');
  assert.deepEqual(await readdir(root), ['x\ufffd']);
  assert.throws(() => computeLineDiff('\ud800', '\ufffd'), /INVALID_TEXT/);
  await assert.rejects(computeLineDiffAsync('valid', '\udfff'), /INVALID_TEXT/);
  await assert.rejects(generateDiffArtifactsAsync('file', '\ud800', 'valid'), /INVALID_TEXT/);
});
