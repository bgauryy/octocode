import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { atomicWriteFile, atomicWriteFileSync } from '../src/shared/atomic.js';
import { envFlag, envInt, ownNonce, PERMISSION_NONCE_FILE_ENV, sameSecret, takeNonce } from '../src/shared/env.js';
import { dirsToRepoRoot, repoKey, workspaceRoot, workspaceScratchDir } from '../src/shared/home.js';
import { findManifest, minimumPiVersion, packageRoot, versionBelow } from '../src/shared/package.js';
import { sweepStaleOutputs } from '../src/shared/sweep.js';
import { capChars, capOutput, clipText, contentText, cutText, finiteNumber, optionalString, settleWithin, stringOrEmpty } from '../src/shared/util.js';
import { tmp } from './helpers.js';
import { nonceFile } from './nonce-file.js';

describe('untyped field guards', () => {
  it('read a string, or nothing, from any value', () => {
    expect(['a', '', 1, null, undefined, {}, ['a']].map(stringOrEmpty)).toEqual(['a', '', '', '', '', '', '']);
    expect(['a', '', 1, null, undefined, {}, ['a']].map(optionalString)).toEqual(['a', '', undefined, undefined, undefined, undefined, undefined]);
  });

  it('read a finite number, never a stand-in zero', () => {
    expect([0, -3.5, 7, Number.NaN, Infinity, -Infinity, '7', null, undefined].map(finiteNumber)).toEqual([0, -3.5, 7, undefined, undefined, undefined, undefined, undefined, undefined]);
  });
});

describe('settleWithin', () => {
  it('ends on the work, the timeout or an abort, never rejects, and removes its abort listener', async () => {
    const started = Date.now();
    await settleWithin(Promise.reject(new Error('boom')), 5_000);
    await settleWithin(new Promise(() => undefined), 30);
    // No limit: only the work or the signal ends the wait.
    const unlimited = new AbortController();
    const open = settleWithin(new Promise(() => undefined), undefined, unlimited.signal);
    let done = false;
    void open.then(() => (done = true));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(done).toBe(false);
    unlimited.abort();
    await open;
    expect(Date.now() - started).toBeLessThan(1_000);
    const controller = new AbortController();
    const remove = vi.spyOn(controller.signal, 'removeEventListener');
    const waiting = settleWithin(new Promise(() => undefined), 60_000, controller.signal);
    controller.abort();
    await waiting;
    await settleWithin(Promise.resolve(), 60_000, new AbortController().signal);
    expect(remove).toHaveBeenCalledWith('abort', expect.any(Function));
    await settleWithin(new Promise(() => undefined), 60_000, controller.signal);
  });
});

describe('capOutput', () => {
  it('keeps the head of a single line over the byte budget instead of returning nothing', () => {
    const out = capOutput('é'.repeat(1_000), 101);
    expect(out.startsWith('é'.repeat(50))).toBe(true);
    expect(out).not.toContain('\uFFFD');
    expect(out).toContain('[Output trimmed: the first line alone is');
    expect(capOutput('a\nb\nc', 1_000, 2)).toContain('[Output trimmed: 2 of 3 lines');
  });
});

describe('envFlag / envInt', () => {
  it('accepts one spelling rule for every switch', () => {
    for (const value of ['1', 'true', 'ON', ' yes ']) expect(envFlag({ X: value }, 'X')).toBe(true);
    for (const value of ['0', 'false', 'Off', 'no']) expect(envFlag({ X: value }, 'X', true)).toBe(false);
    expect(envFlag({}, 'X')).toBe(false);
    expect(envFlag({ X: 'maybe' }, 'X', true)).toBe(true);
  });

  it('parses clamped integers', () => {
    expect(envInt({ N: '7' }, 'N', 3)).toBe(7);
    expect(envInt({ N: '7x' }, 'N', 3)).toBe(3);
    expect(envInt({}, 'N', 3)).toBe(3);
    expect(envInt({ N: '99' }, 'N', 3, { min: 1, max: 10 })).toBe(10);
    expect(envInt({ N: '-5' }, 'N', 3, { min: 1 })).toBe(1);
  });
});

describe('the subagent nonce', () => {
  it('is read from the file an injected environment names, which is unlinked and no longer named', () => {
    const file = nonceFile('abc');
    const env: NodeJS.ProcessEnv = { [PERMISSION_NONCE_FILE_ENV]: file };
    expect(ownNonce(env)).toBeUndefined();
    expect(takeNonce(env)).toBe('abc');
    expect(fs.existsSync(file)).toBe(false);
    expect(env).not.toHaveProperty(PERMISSION_NONCE_FILE_ENV);
    expect(ownNonce(env)).toBe('abc');
    expect(takeNonce(env)).toBe('abc');
    expect(takeNonce({})).toBeUndefined();
    // A missing file gives no nonce, and nothing throws.
    expect(takeNonce({ [PERMISSION_NONCE_FILE_ENV]: path.join(tmp(), 'gone') })).toBeUndefined();
  });

  it('stays with the process after the file and the variable are gone, so a reload still has it', () => {
    const saved = process.env[PERMISSION_NONCE_FILE_ENV];
    const slot = Symbol.for('octocode.permissionNonce');
    const held = (globalThis as Record<symbol, unknown>)[slot];
    try {
      const file = nonceFile('n-1');
      process.env[PERMISSION_NONCE_FILE_ENV] = file;
      expect(takeNonce()).toBe('n-1');
      expect(process.env[PERMISSION_NONCE_FILE_ENV]).toBeUndefined();
      expect(fs.existsSync(file)).toBe(false);
      expect(ownNonce()).toBe('n-1');
      expect(takeNonce()).toBe('n-1');
    } finally {
      (globalThis as Record<symbol, unknown>)[slot] = held;
      if (saved === undefined) delete process.env[PERMISSION_NONCE_FILE_ENV];
      else process.env[PERMISSION_NONCE_FILE_ENV] = saved;
    }
  });

  it('compares secrets in constant time over equal-length buffers: only the same string matches', () => {
    expect(sameSecret('abc', 'abc')).toBe(true);
    expect(sameSecret('abc', 'abd')).toBe(false);
    expect(sameSecret('abc', 'abcd')).toBe(false);
    expect(sameSecret('abc', undefined)).toBe(false);
    expect(sameSecret('abc', 1)).toBe(false);
    expect(sameSecret('', '')).toBe(false);
  });
});

describe('paths', () => {
  it('walks up to the repository root, else stays at cwd', () => {
    const root = tmp('octocode-paths-');
    fs.mkdirSync(path.join(root, '.git'));
    const deep = path.join(root, 'a', 'b');
    fs.mkdirSync(deep, { recursive: true });
    expect(dirsToRepoRoot(deep)).toEqual([deep, path.join(root, 'a'), root]);
    expect(repoKey(deep).root).toBe(fs.realpathSync(root));
    const loose = tmp('octocode-paths-');
    expect(dirsToRepoRoot(loose)).toEqual([loose]);
    expect(repoKey(loose).root).toBe(fs.realpathSync(loose));
  });

  it('counts a repository root at, above or equal to home, or `/`, as no workspace: cwd is the root', () => {
    const home = tmp('octocode-paths-home-');
    fs.mkdirSync(path.join(home, '.git'));
    const proj = path.join(home, 'proj');
    fs.mkdirSync(proj);
    vi.stubEnv('HOME', home);
    // A dotfiles repository at `~`: a project folder under it is its own root, not `~`.
    expect(repoKey(proj)).toEqual({ key: fs.realpathSync(proj), root: fs.realpathSync(proj), name: 'proj' });
    // A repository nested under home keeps its own git root.
    const nested = path.join(proj, 'app');
    fs.mkdirSync(path.join(nested, '.git'), { recursive: true });
    expect(repoKey(nested).root).toBe(fs.realpathSync(nested));
    // A root above home (here: the folder holding it) is no workspace either.
    const above = tmp('octocode-paths-above-');
    fs.mkdirSync(path.join(above, '.git'));
    const inner = path.join(above, 'me');
    fs.mkdirSync(inner);
    vi.stubEnv('HOME', inner);
    const elsewhere = path.join(above, 'other');
    fs.mkdirSync(elsewhere);
    expect(repoKey(elsewhere).root).toBe(fs.realpathSync(elsewhere));
  });

  it('names no permission workspace for home, a folder above it or `/`, and the repository root elsewhere', () => {
    const home = tmp('octocode-paths-home-');
    const proj = path.join(home, 'proj');
    fs.mkdirSync(path.join(proj, '.git'), { recursive: true });
    vi.stubEnv('HOME', home);
    expect(workspaceRoot(home)).toBeUndefined();
    expect(workspaceRoot(path.dirname(home))).toBeUndefined();
    expect(workspaceRoot('/')).toBeUndefined();
    expect(workspaceRoot(proj)).toBe(fs.realpathSync(proj));
    // Per-repository state still keys on the folder itself.
    expect(repoKey(home).root).toBe(fs.realpathSync(home));
  });

  it('resolves one checkout reached through a symlink to one repository', () => {
    const root = tmp('octocode-paths-');
    fs.mkdirSync(path.join(root, '.git'));
    const link = path.join(tmp('octocode-paths-'), 'link');
    fs.symlinkSync(root, link);
    expect(repoKey(link)).toEqual(repoKey(root));
  });
});

describe('workspace scratch', () => {
  it('creates a self-ignoring folder and sweeps stale entries except kept ones', async () => {
    const workspace = tmp('octocode-scratch-');
    const dir = workspaceScratchDir(workspace, path.join('agents', 'a-1'));
    expect(dir).toBe(path.join(workspace, '.octocode', 'tmp', 'agents', 'a-1'));
    expect(fs.readFileSync(path.join(workspace, '.octocode', 'tmp', '.gitignore'), 'utf8')).toBe('*\n');
    workspaceScratchDir(workspace, path.join('agents', 'a-2'));
    workspaceScratchDir(workspace, path.join('agents', 'a-3'));
    const root = path.dirname(dir);
    const old = new Date(Date.now() - 10_000);
    for (const name of ['a-1', 'a-2']) fs.utimesSync(path.join(root, name), old, old);
    await sweepStaleOutputs(root, 5_000, new Set(['a-2']));
    expect(fs.readdirSync(root).sort()).toEqual(['a-2', 'a-3']);
    await expect(sweepStaleOutputs(path.join(workspace, 'missing'), 0)).resolves.toBeUndefined();
  });
});

describe('package', () => {
  it('finds manifests by name', () => {
    const root = packageRoot();
    expect(fs.existsSync(path.join(root, 'package.json'))).toBe(true);
    expect(findManifest(path.join(root, 'src', 'shared'), '@octocodeai/pi-extension')?.dir).toBe(root);
    expect(findManifest(root, 'no-such-package', 2)).toBeUndefined();
  });
});

describe('util', () => {
  it('caps characters with a note', () => {
    expect(capChars('short', 10)).toBe('short');
    expect(capChars('abcdef', 3)).toBe('abc\n[… 3 more characters cut]');
    expect(capChars('abcdef', 3, 'read the file')).toBe('abc\n[… 3 more characters cut; read the file]');
  });

  it('cuts text by bytes or characters, head or tail, never splitting a character', () => {
    const emoji = 'a😀b'; // 😀: 4 UTF-8 bytes, a surrogate pair
    expect(cutText(emoji, 3)).toBe('a');
    expect(cutText(emoji, 5)).toBe('a😀');
    expect(cutText(emoji, 3, { tail: true })).toBe('b');
    expect(cutText('héllo', 2)).toBe('h');
    expect(cutText(emoji, 2, { chars: true })).toBe('a');
    expect(cutText(emoji, 2, { chars: true, tail: true })).toBe('b');
    expect(cutText(emoji, 3, { chars: true, tail: true })).toBe('😀b');
    expect(cutText('abc', 10)).toBe('abc');
    expect(clipText('ab😀cd', 4)).toBe('ab…');
    const cut = 4;
    expect(capChars('ab😀cd', 3)).toBe(`ab\n[… ${cut} more characters cut]`);
  });

  it('joins message content text', () => {
    const content = [{ type: 'text', text: 'a' }, { type: 'image', data: 'x' }, { type: 'text', text: 'b' }, 'junk'];
    expect(contentText(content)).toBe('a\nb');
    expect(contentText(content, { separator: '', image: '[image]' })).toBe('a[image]b');
    expect(contentText('plain')).toBe('plain');
    expect(contentText(undefined)).toBe('');
  });
});

describe('atomic writes', () => {
  it.skipIf(process.platform === 'win32')('applies an explicit mode and leaves no temp file', async () => {
    const dir = tmp('octocode-atomic-');
    const file = path.join(dir, 'secret.json');
    fs.writeFileSync(file, 'old', { mode: 0o644 });
    await atomicWriteFile(file, 'new', undefined, { mode: 0o600 });
    expect(fs.statSync(file).mode & 0o777).toBe(0o600);
    const sync = path.join(dir, 'sync.json');
    atomicWriteFileSync(sync, 'x', { mode: 0o600 });
    expect(fs.statSync(sync).mode & 0o777).toBe(0o600);
    expect(fs.readdirSync(dir).sort()).toEqual(['secret.json', 'sync.json']);
  });

  it('keeps an existing file\'s mode and writes through a symlink, synchronously and asynchronously', async () => {
    const dir = tmp('octocode-atomic-');
    for (const [name, write] of [['sync', async (file: string) => atomicWriteFileSync(file, 'new')], ['async', (file: string) => atomicWriteFile(file, 'new')]] as const) {
      const real = path.join(dir, `${name}.sh`);
      const link = path.join(dir, `${name}-link.sh`);
      fs.writeFileSync(real, 'old');
      fs.chmodSync(real, 0o750);
      fs.symlinkSync(real, link);
      await write(link);
      expect(fs.readFileSync(real, 'utf8'), name).toBe('new');
      expect(fs.statSync(real).mode & 0o777, name).toBe(0o750);
      expect(fs.lstatSync(link).isSymbolicLink(), name).toBe(true);
    }
    expect(fs.readdirSync(dir).sort()).toEqual(['async-link.sh', 'async.sh', 'sync-link.sh', 'sync.sh']);
  });

  it('removes the temp file when the write fails', () => {
    const dir = tmp('octocode-atomic-');
    fs.mkdirSync(path.join(dir, 'target'));
    expect(() => atomicWriteFileSync(path.join(dir, 'target'), 'x')).toThrow();
    expect(fs.readdirSync(dir)).toEqual(['target']);
  });
});

describe('Pi version floor', () => {
  it('compares dotted versions and reads the floor of the peer range', () => {
    expect(versionBelow('1.1.0', '1.2.0')).toBe(true);
    expect(versionBelow('1.2.0', '1.2.0')).toBe(false);
    expect(versionBelow('1.10.0', '1.2.0')).toBe(false);
    expect(versionBelow('1.2.0-beta.1', '1.2.0')).toBe(false);
    expect(minimumPiVersion()).toMatch(/^\d+\.\d+\.\d+$/);
  });
});
