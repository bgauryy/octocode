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
  cleanupStaleCloneArtifacts,
  writeCloneLockMeta,
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

  it('getTreeDir addresses materialized trees by immutable commit SHA', () => {
    const sha = '0123456789abcdef0123456789abcdef01234567';
    const treeDir = getTreeDir(octocodeDir, 'owner', 'repo', sha);
    const treeBase = join(octocodeDir, 'tmp', 'tree');
    expect(treeDir.endsWith(`${sep}owner${sep}repo${sep}${sha}`)).toBe(true);
    expect(segmentCount(treeBase, treeDir)).toBe(3);
  });

  it('getTreeDir rejects mutable refs and malformed commit identities', () => {
    expect(() => getTreeDir(octocodeDir, 'owner', 'repo', 'main')).toThrow(
      'commit SHA'
    );
    expect(() =>
      getTreeDir(octocodeDir, 'owner', 'repo', '../0123456789abcdef')
    ).toThrow('commit SHA');
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
    const slashBranchDir = getCloneDir(
      octocodeDir,
      'microsoft',
      'vscode',
      'release/1.96'
    );
    mkdirSync(slashBranchDir, { recursive: true });
    writeCacheMeta(
      slashBranchDir,
      createCacheMeta('microsoft', 'vscode', 'release/1.96', 'clone')
    );

    // An unrelated, already-expired entry to actually exercise eviction.
    const expiredDir = getCloneDir(
      octocodeDir,
      'octocat',
      'Hello-World',
      'main'
    );
    mkdirSync(expiredDir, { recursive: true });
    const expiredMeta = createCacheMeta(
      'octocat',
      'Hello-World',
      'main',
      'clone'
    );
    expiredMeta.expiresAt = new Date(Date.now() - 1000).toISOString();
    writeCacheMeta(expiredDir, expiredMeta);

    evictExpiredClones(octocodeDir);

    expect(existsSync(slashBranchDir)).toBe(true);
    expect(existsSync(expiredDir)).toBe(false);
  });
});

describe('clone temporary artifact cleanup', () => {
  let octocodeDir: string;

  beforeEach(() => {
    octocodeDir = mkdtempSync(join(tmpdir(), 'octocode-cache-artifacts-'));
  });

  afterEach(() => {
    rmSync(octocodeDir, { recursive: true, force: true });
  });

  it('removes stale clone temporaries but preserves unrelated tmp state', () => {
    const stale = join(octocodeDir, 'tmp', 'clone-tmp', 'stale');
    const sentinel = join(octocodeDir, 'tmp', 'plan', 'keep');
    mkdirSync(stale, { recursive: true });
    mkdirSync(sentinel, { recursive: true });

    const evicted = cleanupStaleCloneArtifacts(
      octocodeDir,
      Date.now() + 20 * 60 * 1000
    );

    expect(evicted).toBe(1);
    expect(existsSync(stale)).toBe(false);
    expect(existsSync(sentinel)).toBe(true);
  });

  it('reclaims a stale dead-owner lock but preserves a live-owner lock', () => {
    const locks = join(octocodeDir, 'tmp', 'clone-locks');
    const dead = join(locks, 'dead');
    const live = join(locks, 'live');
    mkdirSync(dead, { recursive: true });
    mkdirSync(live, { recursive: true });
    const old = Date.now() - 10 * 60 * 1000;
    writeCloneLockMeta(dead, 2_147_483_647, old);
    writeCloneLockMeta(live, process.pid, old);

    const evicted = cleanupStaleCloneArtifacts(octocodeDir);

    expect(evicted).toBe(1);
    expect(existsSync(dead)).toBe(false);
    expect(existsSync(live)).toBe(true);
  });
});
