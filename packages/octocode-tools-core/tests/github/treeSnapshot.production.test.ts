import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { fork } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { once } from 'node:events';
import {
  currentTreeSnapshot,
  ensureSnapshotDirectory,
  publishTreeSnapshot,
  withTreeLock,
} from '../../src/github/directoryFetch/snapshot.js';
import {
  getTreeDir,
  getTreeLockDir,
} from '../../src/tools/github_clone_repo/cachePaths.js';
import {
  createCacheMeta,
  evictExpiredTrees,
} from '../../src/tools/github_clone_repo/cache.js';

let home: string;
let root: string;
const SHA = 'a'.repeat(40);
beforeEach(() => {
  home = mkdtempSync(join(tmpdir(), 'octocode-tree-snapshot-'));
  root = getTreeDir(home, 'o', 'r', SHA);
});
afterEach(() => rmSync(home, { recursive: true, force: true }));
const meta = () =>
  createCacheMeta('o', 'r', 'main', 'treeFetch', undefined, undefined, SHA);

describe('tree generation publication and cross-process exclusion', () => {
  it('publishes only complete generations and retains prior returned paths', () => {
    const first = publishTreeSnapshot(home, root, undefined, meta(), stage =>
      writeFileSync(join(stage, 'a.txt'), 'first')
    );
    const second = publishTreeSnapshot(home, root, first, meta(), stage =>
      writeFileSync(join(stage, 'a.txt'), 'second')
    );
    expect(currentTreeSnapshot(root)).toBe(second);
    expect(readFileSync(join(first, 'a.txt'), 'utf8')).toBe('first');
    expect(readFileSync(join(second, 'a.txt'), 'utf8')).toBe('second');
  });

  it('does not publish a failed generation or leave staged files behind', () => {
    const first = publishTreeSnapshot(home, root, undefined, meta(), () => {});
    expect(() =>
      publishTreeSnapshot(home, root, first, meta(), () => {
        throw new Error('disk failure');
      })
    ).toThrow('disk failure');
    expect(currentTreeSnapshot(root)).toBe(first);
  });

  it('rejects snapshot paths through symlinks or parent traversal', () => {
    const outside = join(home, 'outside');
    mkdirSync(outside);
    const stage = join(home, 'stage');
    mkdirSync(stage);
    symlinkSync(outside, join(stage, 'link'), 'dir');
    expect(() => ensureSnapshotDirectory(stage, 'link/src')).toThrow(
      'symbolic links'
    );
    expect(() => ensureSnapshotDirectory(stage, '../outside')).toThrow(
      'Invalid snapshot'
    );
    expect(existsSync(join(outside, 'src'))).toBe(false);
  });

  it('eviction does not remove an entry currently held by a writer', async () => {
    const expired = { ...meta(), expiresAt: new Date(0).toISOString() };
    const snapshot = publishTreeSnapshot(
      home,
      root,
      undefined,
      expired,
      () => {}
    );
    await withTreeLock(home, root, async () => {
      evictExpiredTrees(home);
      expect(existsSync(snapshot)).toBe(true);
    });
    evictExpiredTrees(home);
    expect(existsSync(snapshot)).toBe(false);
  });

  it('waits for another process and recovers its abandoned lock', async () => {
    const lock = getTreeLockDir(home, root);
    const workerPath = join(home, 'worker.cjs');
    writeFileSync(
      workerPath,
      `const fs=require('node:fs'); const path=require('node:path'); const lock=process.argv[2]; fs.mkdirSync(path.dirname(lock),{recursive:true}); fs.mkdirSync(lock); fs.writeFileSync(path.join(lock,'.octocode-lock.json'),JSON.stringify({pid:process.pid,createdAt:Date.now()-600000})); process.send('ready'); setInterval(()=>{},1000);`
    );
    const worker = fork(workerPath, [lock], {
      stdio: ['ignore', 'ignore', 'ignore', 'ipc'],
    });
    try {
      await once(worker, 'message');
      let entered = false;
      const pending = withTreeLock(home, root, async () => {
        entered = true;
      });
      await new Promise(resolve => setTimeout(resolve, 60));
      expect(entered).toBe(false);
      const exited = once(worker, 'exit');
      worker.kill('SIGKILL');
      await exited;
      await pending;
      expect(entered).toBe(true);
      expect(existsSync(lock)).toBe(false);
    } finally {
      worker.kill('SIGKILL');
    }
  });
});
