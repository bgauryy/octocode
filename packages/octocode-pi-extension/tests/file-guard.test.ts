import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { describe, expect, it, vi } from 'vitest';
import { contextReads, seenFiles } from '../src/files/reads.js';
import { FileGuard } from '../src/files/guard.js';
import { registerFileTool, returnedContent } from '../src/files/tool.js';
import { canonicalPath, readPaths, resolveToolPath } from '../src/shared/home.js';
import { sha256 } from '../src/shared/atomic.js';
import { projectedSession, toolCallMessage, toolResultMessage, type ToolArguments, type ToolResultMessage } from './doubles.js';
import { fakePermission, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';

const contextCall = (id: string, name: string, args: ToolArguments) => ({ id: `a-${id}`, message: toolCallMessage(id, name, args) });
const contextResult = (id: string, toolName: string, text: string, extra: Partial<ToolResultMessage> = {}) => ({ id: `r-${id}`, message: toolResultMessage(id, toolName, text, { timestamp: 100, ...extra }) });

describe('file guard', () => {
  it('allows writing new files, requires existing files for edit/delete, and a read before changing an existing one', () => {
    const dir = tmp();
    const file = path.join(dir, 'a.txt');
    const guard = new FileGuard();
    expect(guard.check(file, 'write', 'a.txt')).toBeUndefined();
    expect(guard.check(file, 'edit', 'a.txt')).toMatch(/does not exist/);
    fs.writeFileSync(file, 'one');
    for (const mode of ['write', 'edit', 'delete'] as const) expect(guard.check(file, mode, 'a.txt')).toMatch(/already exists and was not read in this session\. Read it with `read`/);
    guard.record(file);
    expect(guard.check(file, 'write', 'a.txt')).toBeUndefined();
    expect(guard.check(file, 'edit', 'a.txt')).toBeUndefined();
  });

  it('detects files changed on disk after the last read and resets after compaction', () => {
    const dir = tmp();
    const file = path.join(dir, 'a.txt');
    fs.writeFileSync(file, 'one');
    const guard = new FileGuard();
    guard.record(file);
    expect(guard.check(file, 'edit', 'a.txt')).toBeUndefined();
    fs.writeFileSync(file, 'changed by someone else');
    expect(guard.check(file, 'write', 'a.txt')).toMatch(/changed on disk/);
    expect(guard.check(file, 'delete', 'a.txt')).toMatch(/changed on disk/);
    expect(guard.inspect(file, 'edit', 'a.txt')).toMatchObject({ stale: true });
    guard.restore([]);
    // After compaction the old read is out of context: the file must be read again, even unchanged.
    expect(guard.check(file, 'edit', 'a.txt')).toMatch(/Read it again/);
    guard.record(file);
    expect(guard.check(file, 'edit', 'a.txt')).toBeUndefined();
  });

  it('restores reads the context still shows: a resumed session needs no re-read, a file modified after its read does', () => {
    const dir = tmp();
    const kept = path.join(dir, 'kept.txt');
    const changed = path.join(dir, 'changed.txt');
    const gone = path.join(dir, 'gone.txt');
    for (const file of [kept, changed, gone]) fs.writeFileSync(file, 'one');
    const readAt = fs.statSync(kept).mtimeMs + 1000;
    fs.utimesSync(changed, (readAt + 5000) / 1000, (readAt + 5000) / 1000);
    const guard = new FileGuard();
    guard.record(gone);
    guard.restore([{ file: kept, at: readAt }, { file: changed, at: readAt }]);
    expect(guard.check(kept, 'edit', 'kept.txt')).toBeUndefined();
    expect(guard.check(changed, 'write', 'changed.txt')).toMatch(/changed on disk/);
    expect(guard.inspect(changed, 'edit', 'changed.txt')).toMatchObject({ stale: true });
    expect(guard.check(gone, 'edit', 'gone.txt')).toMatch(/no longer in your context/);
    guard.record(changed);
    expect(guard.check(changed, 'edit', 'changed.txt')).toBeUndefined();
  });
});

describe('file guard', () => {
  it('finds the files the context shows from read results and file changes, and not errors or MCP reads', () => {
    const messages = [
      contextCall('1', 'read', { path: 'a.ts' }), contextResult('1', 'read', 'a'),
      contextCall('2', 'read', { path: 'b.ts' }), contextResult('2', 'read', 'b', { isError: true }),
      contextCall('3', 'read', { path: 'c.ts' }), contextResult('3', 'read', 'c'),
      contextCall('3b', 'read', { path: 'c.ts', offset: 5 }), contextResult('3b', 'read', 'c', { timestamp: 150 }),
      contextCall('4', 'file', { queries: [] }), contextResult('4', 'file', 'ok', { details: { outcomes: [{ ok: true, path: 'd.ts' }, { ok: false, path: 'e.ts' }] }, timestamp: 200 }),
      contextCall('5', 'localFetch', { path: 'f.ts' }), contextResult('5', 'localFetch', 'f'),
    ];
    const seen = seenFiles(messages, '/w');
    const files = [...seen.keys()];
    files.sort();
    expect(files).toEqual(['/w/a.ts', '/w/c.ts', '/w/d.ts']);
    // A later range does not take back a whole read; the pair dates from the earlier result.
    expect(seen.get('/w/c.ts')).toEqual({ file: '/w/c.ts', id: 'r-3b', at: 100, workedAt: 150, line: 5 });
    expect(contextReads(projectedSession('/w', messages))).toEqual([{ file: '/w/a.ts', at: 100 }, { file: '/w/c.ts', at: 100 }, { file: '/w/d.ts', at: 200 }]);
  });

  it('makes a forgotten (rewound) file need a fresh read, and leaves deleted files alone', () => {
    const dir = tmp();
    const file = path.join(dir, 'a.txt');
    const other = path.join(dir, 'b.txt');
    fs.writeFileSync(file, 'one');
    fs.writeFileSync(other, 'two');
    const guard = new FileGuard();
    guard.record(file);
    guard.forget(file);
    expect(guard.check(file, 'write', 'a.txt')).toMatch(/earlier read of it is no longer in your context[\s\S]*Read it again/);
    expect(guard.check(other, 'edit', 'b.txt')).toMatch(/not read in this session/);
    guard.record(file);
    expect(guard.check(file, 'write', 'a.txt')).toBeUndefined();
    // A rewound file that is gone can be created again without a read.
    guard.forget(file);
    fs.rmSync(file);
    expect(guard.check(file, 'write', 'a.txt')).toBeUndefined();
  });

  it('records a read by stat without reading the file, and still catches a same-size rewrite that kept the mtime', () => {
    const dir = tmp();
    const file = path.join(dir, 'small.txt');
    fs.writeFileSync(file, 'aaaa');
    const pinned = new Date(1_700_000_000_000);
    fs.utimesSync(file, pinned, pinned);
    const guard = new FileGuard();
    const reads = vi.spyOn(fs, 'readFileSync');
    try {
      guard.record(file);
      expect(guard.inspect(file, 'edit', 'small.txt')).toEqual({ current: expect.objectContaining({ size: 4 }) });
      expect(reads).not.toHaveBeenCalled();
    } finally {
      reads.mockRestore();
    }
    fs.writeFileSync(file, 'bbbb');
    fs.utimesSync(file, pinned, pinned);
    expect(guard.check(file, 'write', 'small.txt')).toMatch(/changed on disk/);
  });
});

describe('file guard', () => {
  it('checks a known digest against the content, so a touch passes and a same-size rewrite does not', () => {
    const dir = tmp();
    const file = path.join(dir, 'a.txt');
    fs.writeFileSync(file, 'aaaa');
    const guard = new FileGuard();
    guard.record(file, { sha256: sha256('aaaa'), size: 4 });
    fs.utimesSync(file, new Date(), new Date(Date.now() + 5_000));
    expect(guard.inspect(file, 'edit', 'a.txt')).toMatchObject({ expected: sha256('aaaa') });
    fs.appendFileSync(file, 'x');
    expect(guard.check(file, 'delete', 'a.txt')).toMatch(/changed on disk/);
    expect(guard.inspect(file, 'edit', 'a.txt')).toMatchObject({ stale: true });
    // A digest of a different size (not the file's verbatim text) is not kept.
    guard.record(file, { sha256: sha256('other'), size: 99 });
    expect(guard.inspect(file, 'edit', 'a.txt').expected).toBeUndefined();
  });

  it('counts a symlink or case alias of a read file as read', () => {
    const dir = tmp();
    const file = path.join(dir, 'Real.txt');
    fs.writeFileSync(file, 'one');
    fs.symlinkSync(file, path.join(dir, 'link.txt'));
    const guard = new FileGuard();
    guard.record(path.join(dir, 'link.txt'));
    expect(guard.check(file, 'write', 'Real.txt')).toBeUndefined();
    const insensitive = fs.existsSync(path.join(dir, 'real.txt'));
    if (insensitive) expect(guard.check(path.join(dir, 'real.txt'), 'write', 'real.txt')).toBeUndefined();
    fs.writeFileSync(file, 'bbbb');
    expect(guard.check(file, 'write', 'Real.txt')).toMatch(/changed on disk/);
    guard.forget(path.join(dir, 'link.txt'));
    expect(guard.check(file, 'write', 'Real.txt')).toMatch(/Read it again/);
    // A missing file resolves through its real parent directory; an unresolvable one stays as given.
    expect(canonicalPath(path.join(dir, 'new.txt'))).toBe(path.join(fs.realpathSync.native(dir), 'new.txt'));
    expect(canonicalPath('/no/such/dir/x')).toBe('/no/such/dir/x');
  });

  it('takes the digest of what Pi read returned only when it is the whole file', () => {
    const text = [{ type: 'text', text: 'hello' }];
    expect(returnedContent('read', { path: 'a' }, text, undefined)).toEqual({ sha256: sha256('hello'), size: 5 });
    expect(returnedContent('read', { path: 'a', offset: 2 }, text, undefined)).toBeUndefined();
    expect(returnedContent('read', { path: 'a' }, text, { truncation: {} })).toBeUndefined();
    expect(returnedContent('read', { path: 'a' }, [...text, { type: 'image' }], undefined)).toBeUndefined();
    expect(returnedContent('octocode_localFetch', { path: 'a' }, text, undefined)).toBeUndefined();
  });

  it('records a Pi read from the returned text: a change between the read and the hook is caught', async () => {
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'new!');
    const guard = new FileGuard();
    const { pi, fire } = fakePi();
    registerFileTool(pi, guard, fakePermission());
    // The model saw 'old!'; the disk already holds 'new!' when the hook runs.
    await fire('tool_result', { toolName: 'read', input: { path: 'a.txt' }, content: [{ type: 'text', text: 'old!' }], isError: false }, { cwd });
    expect(guard.inspect(file, 'edit', 'a.txt').expected).toBe(sha256('old!'));
  });

  it('never counts an MCP tool call as a read: the server owns its own read contract', async () => {
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'one');
    const guard = new FileGuard();
    const { pi, fire } = fakePi();
    registerFileTool(pi, guard, fakePermission());
    await fire('tool_result', { toolName: 'mcp__octocode__localFetch', input: { queries: [{ path: file }] }, content: [{ type: 'text', text: 'one' }], structuredContent: { content: [], structuredContent: { results: [{ index: 0, data: { path: 'a.txt', content: '1\tone\n', totalLines: 1 } }] } }, isError: false }, { cwd });
    expect(guard.inspect(file, 'edit', 'a.txt').refusal).toMatch(/not read in this session/);
    await fire('tool_result', { toolName: 'read', input: { path: 'a.txt' }, content: [{ type: 'text', text: 'one' }], isError: false }, { cwd });
    expect(guard.inspect(file, 'edit', 'a.txt').refusal).toBeUndefined();
  });
});

describe('file guard', () => {
  it('learns reads from Pi read only', () => {
    expect(readPaths('read', { path: '@src/x.ts' }, '/r')).toEqual(['/r/src/x.ts']);
    // Any range of Pi's read counts (Pi's own guard semantics).
    expect(readPaths('read', { path: 'src/x.ts', offset: 5, limit: 2 }, '/r')).toEqual(['/r/src/x.ts']);
    expect(readPaths('mcp__octocode__localFetch', { queries: [{ path: '/r/whole.ts' }] }, '/r')).toEqual([]);
    expect(resolveToolPath('/repo', '@src/x.ts')).toBe('/repo/src/x.ts');
  });

  it('resolves paths as Pi tools do: ~, file:// URLs, Unicode spaces and a leading @', () => {
    const home = tmp();
    vi.stubEnv('HOME', home);
    try {
      expect(resolveToolPath('/r', '~')).toBe(home);
      expect(resolveToolPath('/r', '~/.bashrc')).toBe(path.join(home, '.bashrc'));
      expect(resolveToolPath('/r', '@~/x')).toBe(path.join(home, 'x'));
      expect(resolveToolPath('~/repo', 'a.ts')).toBe(path.join(home, 'repo', 'a.ts'));
      expect(resolveToolPath('/r', pathToFileURL('/tmp/a b.ts').href)).toBe('/tmp/a b.ts');
      expect(resolveToolPath('/r', 'my\u00a0file\u202f.txt')).toBe('/r/my file .txt');
      expect(resolveToolPath('/r', '~user/x')).toBe('/r/~user/x');
      expect(readPaths('read', { path: '~/notes.md' }, '/r')).toEqual([path.join(home, 'notes.md')]);
    } finally {
      vi.unstubAllEnvs();
    }
  });
});
