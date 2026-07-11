import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { mkdtempSync, existsSync, rmSync, mkdirSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import { tmpdir } from 'node:os';

import {
  getCloneDir,
  getTreeDir,
  getCloneBaseDir,
  createCacheMeta,
  writeCacheMeta,
  evictExpiredClones,
} from '../../../src/tools/github_clone_repo/cache.js';

describe('slash-branch cache directory encoding', () => {
  let octocodeDir: string;

  beforeEach(() => {
    octocodeDir = mkdtempSync(join(tmpdir(), 'octocode-cache-test-'));
  });

  afterEach(() => {
    rmSync(octocodeDir, { recursive: true, force: true });
  });

  function segmentCount(base: string, dir: string): number {
    return relative(base, dir).split(sep).filter(Boolean).length;
  }

  it('getCloneDir keeps a slash-branch clone as a single directory level under owner/repo', () => {
    const cloneDir = getCloneDir(octocodeDir, 'owner', 'repo', 'release/1.96');
    const base = getCloneBaseDir(octocodeDir);
    // owner + repo + <one branch segment> = 3 levels, never 4+.
    expect(segmentCount(base, cloneDir)).toBe(3);
  });

  it('getTreeDir keeps a slash-branch tree as a single directory level under owner/repo', () => {
    const treeDir = getTreeDir(octocodeDir, 'owner', 'repo', 'dependabot/npm/foo');
    const treeBase = join(octocodeDir, 'tmp', 'tree');
    // owner + repo + <one branch segment> = 3 levels, never 4+.
    expect(segmentCount(treeBase, treeDir)).toBe(3);
  });

  it('getCloneDir stays byte-identical for a plain (non-slash) branch name', () => {
    const cloneDir = getCloneDir(octocodeDir, 'owner', 'repo', 'main');
    expect(cloneDir.endsWith(`${sep}owner${sep}repo${sep}main`)).toBe(true);
  });

  it('two different slash-branches never collide on the same directory', () => {
    const a = getCloneDir(octocodeDir, 'owner', 'repo', 'release/1.96');
    const b = getCloneDir(octocodeDir, 'owner', 'repo', 'release_1.96');
    expect(a).not.toBe(b);
  });

  it('regression: a valid slash-branch clone survives GC eviction of an unrelated expired entry', () => {
    // The bug: walkCloneDirs assumed exactly owner/repo/<branch> (3 levels).
    // A slash-branch used to create owner/repo/release/1.96 (4 levels), so
    // the walker found no meta at the "release" level and deleted it whole —
    // wiping the valid nested clone. getCloneDir now collapses it back to a
    // single segment, so this must no longer happen.
    const slashBranchDir = getCloneDir(octocodeDir, 'microsoft', 'vscode', 'release/1.96');
    mkdirSync(slashBranchDir, { recursive: true });
    writeCacheMeta(
      slashBranchDir,
      createCacheMeta('microsoft', 'vscode', 'release/1.96', 'clone')
    );

    // An unrelated, already-expired entry to actually exercise eviction.
    const expiredDir = getCloneDir(octocodeDir, 'octocat', 'Hello-World', 'main');
    mkdirSync(expiredDir, { recursive: true });
    const expiredMeta = createCacheMeta('octocat', 'Hello-World', 'main', 'clone');
    expiredMeta.expiresAt = new Date(Date.now() - 1000).toISOString();
    writeCacheMeta(expiredDir, expiredMeta);

    evictExpiredClones(octocodeDir);

    expect(existsSync(slashBranchDir)).toBe(true);
    expect(existsSync(expiredDir)).toBe(false);
  });
});
