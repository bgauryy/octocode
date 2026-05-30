import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  walkDirectory,
  type WalkStats,
} from '../../../src/tools/local_view_structure/structureWalker.js';
import type { DirectoryEntry } from '../../../src/tools/local_view_structure/structureFilters.js';
import fs from 'fs';
import path from 'path';
import os from 'os';

describe('walkDirectory - WalkStats error tracking', () => {
  let tmpDir: string;

  beforeEach(async () => {
    tmpDir = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'walker-test-'));
  });

  it('should increment stats.skipped on lstat error', async () => {
    // Create a file, then make lstat fail by removing read permission on parent
    // Simpler: mock fs.promises.lstat to fail for a specific path
    await fs.promises.writeFile(path.join(tmpDir, 'good.txt'), 'hello');
    await fs.promises.writeFile(path.join(tmpDir, 'bad.txt'), 'world');

    const originalLstat = fs.promises.lstat;
    vi.spyOn(fs.promises, 'lstat').mockImplementation(
      async (p: fs.PathLike) => {
        if (String(p).endsWith('bad.txt')) {
          throw new Error('Permission denied');
        }
        return originalLstat(p);
      }
    );

    const entries: DirectoryEntry[] = [];
    const stats: WalkStats = { skipped: 0, permissionDenied: 0 };

    await walkDirectory({
      basePath: tmpDir,
      currentPath: tmpDir,
      depth: 0,
      maxDepth: 1,
      entries,
      maxEntries: 100,
      showHidden: false,
      showModified: false,
      stats,
    });

    expect(stats.skipped).toBe(1);
    expect(entries.length).toBe(1);
    expect(entries[0]!.name).toBe('good.txt');

    vi.restoreAllMocks();
  });

  it('should set stats.rootError on readdir error at depth 0', async () => {
    const originalReaddir = fs.promises.readdir;
    vi.spyOn(fs.promises, 'readdir').mockImplementation(
      async (p: fs.PathLike, ...args: unknown[]) => {
        if (String(p) === tmpDir) {
          throw new Error('Permission denied');
        }
        return originalReaddir(p, ...(args as [never])) as never;
      }
    );

    const entries: DirectoryEntry[] = [];
    const stats: WalkStats = { skipped: 0, permissionDenied: 0 };

    await walkDirectory({
      basePath: tmpDir,
      currentPath: tmpDir,
      depth: 0,
      maxDepth: 1,
      entries,
      maxEntries: 100,
      showHidden: false,
      showModified: false,
      stats,
    });

    // Root-level errors are captured in rootError, not counted as skipped entries
    expect(stats.rootError).toBeDefined();
    expect(stats.skipped).toBe(0);
    expect(entries.length).toBe(0);

    vi.restoreAllMocks();
  });

  it('should return early when depth >= maxDepth (line 35)', async () => {
    await fs.promises.mkdir(path.join(tmpDir, 'subdir'));
    await fs.promises.writeFile(
      path.join(tmpDir, 'subdir', 'nested.txt'),
      'content'
    );

    const readdirSpy = vi.spyOn(fs.promises, 'readdir');
    const entries: DirectoryEntry[] = [];
    const stats: WalkStats = { skipped: 0, permissionDenied: 0 };

    // Call with depth=1, maxDepth=1 - should return immediately without reading subdir
    await walkDirectory({
      basePath: tmpDir,
      currentPath: path.join(tmpDir, 'subdir'),
      depth: 1,
      maxDepth: 1,
      entries,
      maxEntries: 100,
      showHidden: false,
      showModified: false,
      stats,
    });

    // When depth >= maxDepth, we return before readdir - so subdir's contents are never read
    expect(entries.length).toBe(0);
    readdirSpy.mockRestore();
  });

  it('should list files when stats tracks skips', async () => {
    await fs.promises.writeFile(path.join(tmpDir, 'file.txt'), 'content');

    const entries: DirectoryEntry[] = [];
    const stats: WalkStats = { skipped: 0, permissionDenied: 0 };

    await walkDirectory({
      basePath: tmpDir,
      currentPath: tmpDir,
      depth: 0,
      maxDepth: 1,
      entries,
      maxEntries: 100,
      showHidden: false,
      showModified: false,
      stats,
    });

    expect(entries.length).toBe(1);
    expect(stats.skipped).toBe(0);
  });

  it('should collect non-error entries alongside errors', async () => {
    await fs.promises.writeFile(path.join(tmpDir, 'a.txt'), 'aaa');
    await fs.promises.writeFile(path.join(tmpDir, 'b.txt'), 'bbb');
    await fs.promises.writeFile(path.join(tmpDir, 'c.txt'), 'ccc');

    const originalLstat = fs.promises.lstat;
    vi.spyOn(fs.promises, 'lstat').mockImplementation(
      async (p: fs.PathLike) => {
        if (String(p).endsWith('b.txt')) {
          throw new Error('Permission denied');
        }
        return originalLstat(p);
      }
    );

    const entries: DirectoryEntry[] = [];
    const stats: WalkStats = { skipped: 0, permissionDenied: 0 };

    await walkDirectory({
      basePath: tmpDir,
      currentPath: tmpDir,
      depth: 0,
      maxDepth: 1,
      entries,
      maxEntries: 100,
      showHidden: false,
      showModified: false,
      stats,
    });

    expect(stats.skipped).toBe(1);
    expect(entries.length).toBe(2);
    const names = entries.map(e => e.name);
    expect(names).toContain('a.txt');
    expect(names).toContain('c.txt');

    vi.restoreAllMocks();
  });
});
