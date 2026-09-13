import assert from 'node:assert/strict';
import fs from 'node:fs';
import * as fsp from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import * as native from '@octocodeai/octocode-extension-rust';
import { afterEach, beforeEach, test, vi } from 'vitest';
import type { ToolDefinition } from '../src/types.js';
import { registerFileTool } from '../src/tools/file-tool.js';
import { atomicWriteUtf8, checkReadState, clearReadStatesForTests, withFileMutationQueue } from '../src/tools/file-state.js';
import { assertPathAllowed } from '../src/tools/path-guard.js';
import { QueryBatchError } from '../src/tools/query-batch-error.js';

vi.mock('node:fs/promises', async (importOriginal) => {
  const actual = await importOriginal<typeof import('node:fs/promises')>();
  return { ...actual };
});
vi.mock('@octocodeai/octocode-extension-rust', async importOriginal => {
  const actual = await importOriginal<typeof import('@octocodeai/octocode-extension-rust')>();
  return { ...actual, replaceFile: vi.fn(actual.replaceFile), deleteFile: vi.fn(actual.deleteFile) };
});

let cwd: string;
let tool: ToolDefinition;

beforeEach(() => {
  cwd = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'write-edge-audit-')));
  clearReadStatesForTests();
  registerFileTool({}, new Set(), (_pi, names, definition) => {
    names.add(definition.name);
    tool = definition;
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  fs.rmSync(cwd, { recursive: true, force: true });
  clearReadStatesForTests();
});

const write = (target: string, content = 'replacement') => ({
  type: 'write', reasoning: 'exercise write edge contract', path: target, content,
});
const call = (...queries: Array<Record<string, unknown>>) =>
  tool.execute('write-edge', { queries }, undefined, undefined, { cwd });

test('write through a dangling symlink preserves the link and creates its target', async () => {
  const target = path.join(cwd, 'missing-target');
  const link = path.join(cwd, 'link');
  fs.symlinkSync(target, link);
  await call(write(link));
  assert.equal(fs.lstatSync(link).isSymbolicLink(), true);
  assert.equal(fs.readFileSync(target, 'utf8'), 'replacement');
});

test('write rejects a FIFO without replacing the filesystem object', async () => {
  const target = path.join(cwd, 'pipe');
  execFileSync('mkfifo', [target]);
  await assert.rejects(call(write(target)), /regular|file|unsupported|FIFO/i);
  assert.equal(fs.lstatSync(target).isFIFO(), true);
});

test('a directory write target fails batch preflight before earlier writes', async () => {
  fs.mkdirSync(path.join(cwd, 'directory'));
  await assert.rejects(call(write('first'), write('directory')));
  assert.equal(fs.existsSync(path.join(cwd, 'first')), false);
});

test('duplicate target paths are rejected even with an absolute dot segment', async () => {
  const target = path.join(cwd, 'target');
  fs.writeFileSync(target, 'original');
  await assert.rejects(call(write(target, 'one'), write(`${cwd}/./target`, 'two')), /duplicate|same path|conflict/i);
  assert.equal(fs.readFileSync(target, 'utf8'), 'original');
});

test('duplicate target paths are rejected through symlink aliases', async () => {
  const target = path.join(cwd, 'target');
  const alias = path.join(cwd, 'alias');
  fs.writeFileSync(target, 'original');
  fs.symlinkSync(target, alias);
  await assert.rejects(call(write(target, 'one'), write(alias, 'two')), /duplicate|same path|conflict/i);
  assert.equal(fs.readFileSync(target, 'utf8'), 'original');
});

test('the process-local mutation queue serializes symlink aliases of the same file', async () => {
  const target = path.join(cwd, 'target');
  const alias = path.join(cwd, 'alias');
  fs.writeFileSync(target, 'original');
  fs.symlinkSync(target, alias);
  let release!: () => void;
  let markEntered!: () => void;
  const entered = new Promise<void>((resolve) => { markEntered = resolve; });
  const gate = new Promise<void>((resolve) => { release = resolve; });
  let released = false;
  let aliasStartedAfterRelease: boolean | undefined;
  const first = withFileMutationQueue(target, async () => { markEntered(); await gate; });
  await entered;
  const second = withFileMutationQueue(alias, async () => { aliasStartedAfterRelease = released; });
  // Yield one event-loop turn so a wrongly independent queue has entered.
  await new Promise<void>((resolve) => { setImmediate(resolve); });
  released = true;
  release();
  await Promise.all([first, second]);
  assert.equal(aliasStartedAfterRelease, true);
});

test('cancellation after native commit retains a successful write receipt and read state', async () => {
  const target = path.join(cwd, 'target');
  fs.writeFileSync(target, 'original');
  const controller = new AbortController();
  const actual = await vi.importActual<typeof import('@octocodeai/octocode-extension-rust')>('@octocodeai/octocode-extension-rust');
  vi.mocked(native.replaceFile).mockImplementationOnce(async (...args) => {
    const receipt = await actual.replaceFile(...args);
    controller.abort();
    return receipt;
  });
  const result = await tool.execute('write-edge-abort', { queries: [write(target)] }, controller.signal, undefined, { cwd });
  assert.equal(result.isError, undefined);
  assert.equal(fs.readFileSync(target, 'utf8'), 'replacement');
  assert.equal((await checkReadState(target, true)).state, 'fresh');
});

// Initial temporary permissions and colliding-name ownership are tested at the
// Rust filesystem boundary, where exclusive temporary creation now lives.
test('overwriting a private file preserves its mode through the native boundary', async () => {
  const target = path.join(cwd, 'private');
  fs.writeFileSync(target, 'original', { mode: 0o600 });
  await atomicWriteUtf8(target, 'private replacement');
  assert.equal(fs.readFileSync(target, 'utf8'), 'private replacement');
  assert.equal(fs.statSync(target).mode & 0o777, 0o600);
});

test('a native operation failure does not fall back to a Node filesystem mutation', async () => {
  const target = path.join(cwd, 'target');
  fs.writeFileSync(target, 'original');
  vi.mocked(native.replaceFile).mockRejectedValueOnce(new Error('Native filesystem unavailable'));
  await assert.rejects(call(write(target)), /Native filesystem unavailable/);
  assert.equal(fs.readFileSync(target, 'utf8'), 'original');
  assert.deepEqual(fs.readdirSync(cwd), ['target']);
});

test('write refuses a target changed after batch preflight', async () => {
  const target = path.join(cwd, 'target');
  fs.writeFileSync(target, 'original');
  let failure: QueryBatchError | undefined;
  try {
    await tool.execute('changed', { queries: [write(target)] }, undefined, () => {
      fs.writeFileSync(target, 'external replacement');
    }, { cwd });
  } catch (error) {
    failure = error as QueryBatchError;
  }
  assert.ok(failure instanceof QueryBatchError);
  assert.match(failure.message, /changed after preflight/);
  assert.deepEqual(failure.rows[0]?.recovery, {
    tool: 'MCPTool',
    query: {
      queries: [{
        reasoning: 'Refresh the changed file before retrying the mutation.',
        action: 'call',
        server: 'octocode',
        tool: 'localFetch',
        arguments: { queries: [{ path: target }] },
      }],
    },
    why: 'The file changed after preflight; inspect current bytes before preparing a new mutation.',
  });
  assert.equal(fs.readFileSync(target, 'utf8'), 'external replacement');
});

test('create-only commit refuses a competing creator after preflight', async () => {
  const target = path.join(cwd, 'target');
  await assert.rejects(tool.execute('competing-create', { queries: [write(target, 'ours')] }, undefined, () => {
    fs.writeFileSync(target, 'external creation');
  }, { cwd }), /EEXIST|PRECONDITION_FAILED|changed after preflight/);
  assert.equal(fs.readFileSync(target, 'utf8'), 'external creation');
  assert.deepEqual(fs.readdirSync(cwd), ['target']);
});

test('a committed write reports metadata refresh failure without suggesting no write occurred', async () => {
  const target = path.join(cwd, 'target');
  fs.writeFileSync(target, 'original');
  const actual = await vi.importActual<typeof import('@octocodeai/octocode-extension-rust')>('@octocodeai/octocode-extension-rust');
  vi.mocked(native.replaceFile).mockImplementationOnce(async (...args) => {
    const receipt = await actual.replaceFile(...args);
    vi.spyOn(fsp, 'stat').mockRejectedValueOnce(Object.assign(new Error('metadata unavailable'), { code: 'EIO' }));
    return receipt;
  });
  const result = await call(write(target));
  assert.equal(result.isError, undefined);
  assert.equal(fs.readFileSync(target, 'utf8'), 'replacement');
  assert.equal((result.details as { committed?: boolean }).committed, true);
  assert.match(JSON.stringify(result), /metadata unavailable/);
});

test('write rejects ill-formed Unicode content before mutation', async () => {
  await assert.rejects(call(write('target', '\ud800')), /Unicode|surrogate/);
  assert.equal(fs.existsSync(path.join(cwd, 'target')), false);
});

test('committed delete survives read-state cleanup failure', async () => {
  const target = path.join(cwd, 'target');
  fs.writeFileSync(target, 'original');
  const actual = await vi.importActual<typeof import('@octocodeai/octocode-extension-rust')>('@octocodeai/octocode-extension-rust');
  vi.mocked(native.deleteFile).mockImplementationOnce(async (...args) => {
    const receipt = await actual.deleteFile(...args);
    vi.spyOn(fs, 'lstatSync').mockImplementation(() => { throw Object.assign(new Error('metadata unavailable'), { code: 'EIO' }); });
    return receipt;
  });
  const result = await call({ type: 'delete', path: target, reasoning: 'delete the fixture' });
  assert.equal(fs.existsSync(target), false);
  assert.equal(result.isError, undefined);
  assert.equal((result.details as { committed?: boolean }).committed, true);
  assert.match(JSON.stringify(result), /metadata unavailable/);
});

test('path guard resolves ancestors of a dangling link literal target', () => {
  const externalAlias = path.join(cwd, 'external-alias');
  const dangling = path.join(cwd, 'dangling');
  fs.symlinkSync('/usr', externalAlias);
  fs.symlinkSync(path.join(externalAlias, `octocode-missing-${process.pid}`, 'target'), dangling);
  // No write to /usr is attempted; the guard itself must reject this escape.
  assert.throws(() => assertPathAllowed(path.join(dangling, 'child'), cwd, 'file write'), /outside the allowed roots/);
});

test('ordinary symlink writes preserve link and executable target mode', async () => {
  const target = path.join(cwd, 'script');
  const link = path.join(cwd, 'link');
  fs.writeFileSync(target, 'original', { mode: 0o751 });
  fs.symlinkSync(target, link);
  await call(write(link));
  assert.equal(fs.lstatSync(link).isSymbolicLink(), true);
  assert.equal(fs.readFileSync(target, 'utf8'), 'replacement');
  assert.equal(fs.statSync(target).mode & 0o777, 0o751);
});

test('failed rename leaves original directory and no temporary files', async () => {
  const target = path.join(cwd, 'directory');
  fs.mkdirSync(target);
  await assert.rejects(atomicWriteUtf8(target, 'replacement'));
  assert.equal(fs.lstatSync(target).isDirectory(), true);
  assert.deepEqual(fs.readdirSync(cwd), ['directory']);
});
