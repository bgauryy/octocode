import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { join } from 'node:path';
import {
  mkdirSync,
  writeFileSync,
  readFileSync,
  existsSync,
  rmSync,
} from 'node:fs';
import { tmpdir } from 'node:os';

import {
  getCloneDir,
  getReposBaseDir,
  readCacheMeta,
  writeCacheMeta,
  isCacheValid,
  isCacheHit,
  createCacheMeta,
  ensureCloneParentDir,
  removeCloneDir,
  evictExpiredClones,
  getCacheTTL,
  getMaxCacheSizeBytes,
  getMaxCloneCount,
  startCacheGC,
  stopCacheGC,
} from '../../../octocode-tools-core/src/tools/github_clone_repo/cache.js';

describe('github_clone_repo cache', () => {
  const testBaseDir = join(tmpdir(), `octocode-cache-test-${Date.now()}`);

  afterEach(() => {
    if (existsSync(testBaseDir)) {
      rmSync(testBaseDir, { recursive: true, force: true });
    }
  });

  describe('getReposBaseDir', () => {
    it('appends repos to octocode dir', () => {
      expect(getReposBaseDir('/home/.octocode')).toBe('/home/.octocode/repos');
    });
  });

  describe('getCloneDir', () => {
    it('builds path for full clone', () => {
      const dir = getCloneDir('/home/.octocode', 'facebook', 'react', 'main');
      expect(dir).toBe('/home/.octocode/repos/facebook/react/main');
    });

    it('builds path with sparse suffix for partial clone', () => {
      const dir = getCloneDir(
        '/home/.octocode',
        'facebook',
        'react',
        'main',
        'packages/core'
      );
      expect(dir).toContain('/home/.octocode/repos/facebook/react/main__sp_');
      const dir2 = getCloneDir(
        '/home/.octocode',
        'facebook',
        'react',
        'main',
        'packages/other'
      );
      expect(dir).not.toBe(dir2);
    });

    it('same sparsePath produces same suffix (deterministic)', () => {
      const dir1 = getCloneDir(
        '/home/.octocode',
        'fb',
        'r',
        'main',
        'src/core'
      );
      const dir2 = getCloneDir(
        '/home/.octocode',
        'fb',
        'r',
        'main',
        'src/core'
      );
      expect(dir1).toBe(dir2);
    });
  });

  describe('readCacheMeta / writeCacheMeta', () => {
    it('returns null for non-existent directory', () => {
      expect(readCacheMeta('/non/existent/dir')).toBeNull();
    });

    it('returns null for corrupt metadata', () => {
      const dir = join(testBaseDir, 'corrupt');
      mkdirSync(dir, { recursive: true });
      writeFileSync(
        join(dir, '.octocode-clone-meta.json'),
        'NOT JSON',
        'utf-8'
      );
      expect(readCacheMeta(dir)).toBeNull();
    });

    it('returns null when source field is missing', () => {
      const dir = join(testBaseDir, 'no-source');
      mkdirSync(dir, { recursive: true });
      writeFileSync(
        join(dir, '.octocode-clone-meta.json'),
        JSON.stringify({
          clonedAt: new Date().toISOString(),
          expiresAt: new Date(Date.now() + 60_000).toISOString(),
          owner: 'o',
          repo: 'r',
          branch: 'main',
        }),
        'utf-8'
      );
      expect(readCacheMeta(dir)).toBeNull();
    });

    it('round-trips metadata', () => {
      const dir = join(testBaseDir, 'roundtrip');
      mkdirSync(dir, { recursive: true });
      const meta = createCacheMeta('fb', 'react', 'main', 'clone');
      writeCacheMeta(dir, meta);
      const loaded = readCacheMeta(dir);
      expect(loaded).toEqual(meta);
    });

    it('round-trips metadata with sparsePath', () => {
      const dir = join(testBaseDir, 'sparse-roundtrip');
      mkdirSync(dir, { recursive: true });
      const meta = createCacheMeta('fb', 'react', 'main', 'clone', 'src/core');
      writeCacheMeta(dir, meta);
      const loaded = readCacheMeta(dir);
      expect(loaded?.sparsePath).toBe('src/core');
    });
  });

  describe('isCacheValid', () => {
    it('returns true for unexpired cache', () => {
      const meta = createCacheMeta('fb', 'react', 'main', 'clone');
      expect(isCacheValid(meta)).toBe(true);
    });

    it('returns false for expired cache', () => {
      const meta = createCacheMeta('fb', 'react', 'main', 'clone');
      meta.expiresAt = new Date(Date.now() - 1000).toISOString();
      expect(isCacheValid(meta)).toBe(false);
    });
  });

  describe('isCacheHit', () => {
    it('returns hit=true when meta valid and dir exists', () => {
      const dir = join(testBaseDir, 'cache-hit');
      mkdirSync(dir, { recursive: true });
      const meta = createCacheMeta('fb', 'react', 'main', 'clone');
      writeCacheMeta(dir, meta);

      const result = isCacheHit(dir);
      expect(result.hit).toBe(true);
      if (result.hit) {
        expect(result.meta.owner).toBe('fb');
      }
    });

    it('returns hit=false when no meta file exists', () => {
      const result = isCacheHit('/non/existent/path');
      expect(result.hit).toBe(false);
    });

    it('returns hit=false when meta is expired', () => {
      const dir = join(testBaseDir, 'cache-expired');
      mkdirSync(dir, { recursive: true });
      const meta = createCacheMeta('fb', 'react', 'main', 'clone');
      meta.expiresAt = new Date(Date.now() - 1000).toISOString();
      writeCacheMeta(dir, meta);

      const result = isCacheHit(dir);
      expect(result.hit).toBe(false);
    });

    it('returns hit=false when directory was externally deleted', () => {
      const dir = join(testBaseDir, 'cache-gone');
      mkdirSync(dir, { recursive: true });
      const meta = createCacheMeta('fb', 'react', 'main', 'clone');
      writeCacheMeta(dir, meta);
      rmSync(dir, { recursive: true, force: true });

      const result = isCacheHit(dir);
      expect(result.hit).toBe(false);
    });
  });

  describe('createCacheMeta', () => {
    it('creates metadata with 24h TTL', () => {
      const before = Date.now();
      const meta = createCacheMeta('fb', 'react', 'main', 'clone');
      const after = Date.now();

      const clonedAt = new Date(meta.clonedAt).getTime();
      const expiresAt = new Date(meta.expiresAt).getTime();
      const twentyFourHours = 24 * 60 * 60 * 1000;

      expect(clonedAt).toBeGreaterThanOrEqual(before);
      expect(clonedAt).toBeLessThanOrEqual(after);
      expect(expiresAt - clonedAt).toBe(twentyFourHours);
    });

    it('omits sparsePath when not provided', () => {
      const meta = createCacheMeta('fb', 'react', 'main', 'clone');
      expect(meta).not.toHaveProperty('sparsePath');
    });

    it('includes sparsePath when provided', () => {
      const meta = createCacheMeta('fb', 'react', 'main', 'clone', 'src/core');
      expect(meta.sparsePath).toBe('src/core');
    });

    it('sets source to clone', () => {
      const meta = createCacheMeta('fb', 'react', 'main', 'clone');
      expect(meta.source).toBe('clone');
    });

    it('sets source to directoryFetch', () => {
      const meta = createCacheMeta('fb', 'react', 'main', 'directoryFetch');
      expect(meta.source).toBe('directoryFetch');
    });

    it('includes sizeBytes when provided', () => {
      const meta = createCacheMeta(
        'org',
        'repo',
        'main',
        'clone',
        undefined,
        12345
      );
      expect(meta.sizeBytes).toBe(12345);
    });

    it('omits sizeBytes when not provided', () => {
      const meta = createCacheMeta('org', 'repo', 'main', 'clone');
      expect(meta).not.toHaveProperty('sizeBytes');
    });
  });

  describe('ensureCloneParentDir', () => {
    it('creates parent directories', () => {
      const dir = join(testBaseDir, 'deep', 'nested', 'clone');
      ensureCloneParentDir(dir);
      expect(existsSync(join(testBaseDir, 'deep', 'nested'))).toBe(true);
    });

    it('does nothing when parent already exists', () => {
      const dir = join(testBaseDir, 'already-exists', 'clone');
      mkdirSync(join(testBaseDir, 'already-exists'), { recursive: true });
      ensureCloneParentDir(dir);
      expect(existsSync(join(testBaseDir, 'already-exists'))).toBe(true);
    });
  });

  describe('removeCloneDir', () => {
    it('removes existing directory', () => {
      const dir = join(testBaseDir, 'to-remove');
      mkdirSync(dir, { recursive: true });
      writeFileSync(join(dir, 'file.txt'), 'hello', 'utf-8');
      removeCloneDir(dir);
      expect(existsSync(dir)).toBe(false);
    });

    it('does nothing for non-existent directory', () => {
      const nonExistent = join(testBaseDir, 'does-not-exist');
      expect(() => removeCloneDir(nonExistent)).not.toThrow();
      expect(existsSync(nonExistent)).toBe(false);
    });
  });

  describe('getCacheTTL', () => {
    const origEnv = process.env.OCTOCODE_CACHE_TTL_MS;
    afterEach(() => {
      if (origEnv === undefined) {
        delete process.env.OCTOCODE_CACHE_TTL_MS;
      } else {
        process.env.OCTOCODE_CACHE_TTL_MS = origEnv;
      }
    });

    it('returns 24 hours by default', () => {
      delete process.env.OCTOCODE_CACHE_TTL_MS;
      expect(getCacheTTL()).toBe(24 * 60 * 60 * 1000);
    });

    it('reads OCTOCODE_CACHE_TTL_MS env var', () => {
      process.env.OCTOCODE_CACHE_TTL_MS = '3600000';
      expect(getCacheTTL()).toBe(3600000);
    });

    it('ignores non-numeric env value', () => {
      process.env.OCTOCODE_CACHE_TTL_MS = 'not-a-number';
      expect(getCacheTTL()).toBe(24 * 60 * 60 * 1000);
    });

    it('ignores zero', () => {
      process.env.OCTOCODE_CACHE_TTL_MS = '0';
      expect(getCacheTTL()).toBe(24 * 60 * 60 * 1000);
    });

    it('ignores negative values', () => {
      process.env.OCTOCODE_CACHE_TTL_MS = '-5000';
      expect(getCacheTTL()).toBe(24 * 60 * 60 * 1000);
    });
  });

  describe('cache size and count limits', () => {
    const origSize = process.env.OCTOCODE_MAX_CACHE_SIZE;
    const origCount = process.env.OCTOCODE_MAX_CLONES;

    afterEach(() => {
      if (origSize === undefined) {
        delete process.env.OCTOCODE_MAX_CACHE_SIZE;
      } else {
        process.env.OCTOCODE_MAX_CACHE_SIZE = origSize;
      }

      if (origCount === undefined) {
        delete process.env.OCTOCODE_MAX_CLONES;
      } else {
        process.env.OCTOCODE_MAX_CLONES = origCount;
      }
    });

    it('uses defaults when env vars are not set', () => {
      delete process.env.OCTOCODE_MAX_CACHE_SIZE;
      delete process.env.OCTOCODE_MAX_CLONES;

      expect(getMaxCacheSizeBytes()).toBe(2 * 1024 * 1024 * 1024);
      expect(getMaxCloneCount()).toBe(50);
    });

    it('reads positive env var overrides', () => {
      process.env.OCTOCODE_MAX_CACHE_SIZE = '12345';
      process.env.OCTOCODE_MAX_CLONES = '7';

      expect(getMaxCacheSizeBytes()).toBe(12345);
      expect(getMaxCloneCount()).toBe(7);
    });

    it('ignores invalid env var overrides', () => {
      process.env.OCTOCODE_MAX_CACHE_SIZE = '-1';
      process.env.OCTOCODE_MAX_CLONES = '0';

      expect(getMaxCacheSizeBytes()).toBe(2 * 1024 * 1024 * 1024);
      expect(getMaxCloneCount()).toBe(50);
    });

    it('evicts oldest clones when clone count exceeds max', () => {
      const dir = join(testBaseDir, 'count-limit-test');
      process.env.OCTOCODE_MAX_CLONES = '2';
      process.env.OCTOCODE_MAX_CACHE_SIZE = String(10 * 1024 * 1024);

      const oldDir = join(dir, 'repos', 'owner', 'repo', 'old');
      const midDir = join(dir, 'repos', 'owner', 'repo', 'mid');
      const newDir = join(dir, 'repos', 'owner', 'repo', 'new');

      mkdirSync(oldDir, { recursive: true });
      mkdirSync(midDir, { recursive: true });
      mkdirSync(newDir, { recursive: true });

      writeFileSync(join(oldDir, 'a.txt'), 'old', 'utf-8');
      writeFileSync(join(midDir, 'a.txt'), 'mid', 'utf-8');
      writeFileSync(join(newDir, 'a.txt'), 'new', 'utf-8');

      const oldMeta = createCacheMeta('owner', 'repo', 'old', 'clone');
      oldMeta.clonedAt = new Date(Date.now() - 30_000).toISOString();
      writeCacheMeta(oldDir, oldMeta);

      const midMeta = createCacheMeta('owner', 'repo', 'mid', 'clone');
      midMeta.clonedAt = new Date(Date.now() - 20_000).toISOString();
      writeCacheMeta(midDir, midMeta);

      const newMeta = createCacheMeta('owner', 'repo', 'new', 'clone');
      newMeta.clonedAt = new Date(Date.now() - 10_000).toISOString();
      writeCacheMeta(newDir, newMeta);

      const evicted = evictExpiredClones(dir);
      expect(evicted).toBe(1);
      expect(existsSync(oldDir)).toBe(false);
      expect(existsSync(midDir)).toBe(true);
      expect(existsSync(newDir)).toBe(true);
    });

    it('evicts oldest clones when cache size exceeds max', () => {
      const dir = join(testBaseDir, 'size-limit-test');
      process.env.OCTOCODE_MAX_CLONES = '10';
      process.env.OCTOCODE_MAX_CACHE_SIZE = '300';

      const oldDir = join(dir, 'repos', 'owner', 'repo', 'old');
      const newDir = join(dir, 'repos', 'owner', 'repo', 'new');

      mkdirSync(oldDir, { recursive: true });
      mkdirSync(newDir, { recursive: true });

      writeFileSync(join(oldDir, 'a.txt'), 'x'.repeat(120), 'utf-8');
      writeFileSync(join(newDir, 'a.txt'), 'y'.repeat(120), 'utf-8');

      const oldMeta = createCacheMeta('owner', 'repo', 'old', 'clone');
      oldMeta.clonedAt = new Date(Date.now() - 20_000).toISOString();
      writeCacheMeta(oldDir, oldMeta);

      const newMeta = createCacheMeta('owner', 'repo', 'new', 'clone');
      newMeta.clonedAt = new Date(Date.now() - 10_000).toISOString();
      writeCacheMeta(newDir, newMeta);

      const evicted = evictExpiredClones(dir);
      expect(evicted).toBe(1);
      expect(existsSync(oldDir)).toBe(false);
      expect(existsSync(newDir)).toBe(true);
    });
  });

  describe('createCacheMeta with custom TTL', () => {
    const origEnv = process.env.OCTOCODE_CACHE_TTL_MS;
    afterEach(() => {
      if (origEnv === undefined) {
        delete process.env.OCTOCODE_CACHE_TTL_MS;
      } else {
        process.env.OCTOCODE_CACHE_TTL_MS = origEnv;
      }
    });

    it('uses custom TTL from env var', () => {
      process.env.OCTOCODE_CACHE_TTL_MS = '60000';
      const meta = createCacheMeta('fb', 'react', 'main', 'clone');
      const clonedAt = new Date(meta.clonedAt).getTime();
      const expiresAt = new Date(meta.expiresAt).getTime();
      expect(expiresAt - clonedAt).toBe(60000);
    });
  });

  describe('startCacheGC / stopCacheGC', () => {
    afterEach(() => {
      stopCacheGC();
    });

    it('runs immediate eviction on start', () => {
      const dir = join(testBaseDir, 'gc-test');
      const reposDir = join(dir, 'repos', 'owner', 'repo', 'main');
      mkdirSync(reposDir, { recursive: true });
      const expiredMeta = createCacheMeta('owner', 'repo', 'main', 'clone');
      expiredMeta.expiresAt = new Date(Date.now() - 1000).toISOString();
      writeCacheMeta(reposDir, expiredMeta);

      startCacheGC(dir);

      expect(existsSync(reposDir)).toBe(false);
    });

    it('is idempotent — second call is a no-op', () => {
      mkdirSync(testBaseDir, { recursive: true });
      expect(() => startCacheGC(testBaseDir)).not.toThrow();
      expect(() => startCacheGC(testBaseDir)).not.toThrow();
      stopCacheGC();
    });

    it('stopCacheGC is safe when GC was never started', () => {
      expect(() => stopCacheGC()).not.toThrow();
    });
  });

  describe('evictExpiredClones', () => {
    it('removes expired entries and keeps valid ones', () => {
      const dir = join(testBaseDir, 'evict-test');

      const expiredDir = join(dir, 'repos', 'owner', 'repo', 'old-branch');
      mkdirSync(expiredDir, { recursive: true });
      const expiredMeta = createCacheMeta(
        'owner',
        'repo',
        'old-branch',
        'clone'
      );
      expiredMeta.expiresAt = new Date(Date.now() - 1000).toISOString();
      writeCacheMeta(expiredDir, expiredMeta);

      const validDir = join(dir, 'repos', 'owner', 'repo', 'main');
      mkdirSync(validDir, { recursive: true });
      const validMeta = createCacheMeta('owner', 'repo', 'main', 'clone');
      writeCacheMeta(validDir, validMeta);

      const count = evictExpiredClones(dir);

      expect(count).toBe(1);
      expect(existsSync(expiredDir)).toBe(false);
      expect(existsSync(validDir)).toBe(true);
    });

    it('returns 0 when repos dir does not exist', () => {
      expect(evictExpiredClones('/non/existent/path')).toBe(0);
    });
  });
});

const mockSpawnWithTimeout = vi.hoisted(() => vi.fn());
const mockGetOctokit = vi.hoisted(() => vi.fn());
const mockGetOctocodeDir = vi.hoisted(() => vi.fn());

vi.mock(
  '../../../octocode-tools-core/src/utils/exec/spawn.js',
  async importOriginal => {
    const actual = (await importOriginal()) as Record<string, unknown>;
    return {
      ...actual,
      spawnWithTimeout: mockSpawnWithTimeout,
    };
  }
);

const mockResolveDefaultBranch = vi.hoisted(() => vi.fn());

vi.mock('../../../octocode-tools-core/src/github/client.js', () => ({
  getOctokit: mockGetOctokit,
  resolveDefaultBranch: mockResolveDefaultBranch,
}));

vi.mock('@octocodeai/octocode-tools-core/paths', async importOriginal => {
  const actual =
    await importOriginal<
      typeof import('@octocodeai/octocode-tools-core/paths')
    >();
  return {
    ...actual,
    getOctocodeDir: mockGetOctocodeDir,
  };
});

import { cloneRepo } from '../../../octocode-tools-core/src/tools/github_clone_repo/cloneRepo.js';

describe('cloneRepo', () => {
  let testDir: string;

  beforeEach(() => {
    vi.clearAllMocks();
    testDir = join(
      tmpdir(),
      `octocode-clone-test-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
    );
    mockGetOctocodeDir.mockReturnValue(testDir);

    mockSpawnWithTimeout.mockImplementation(
      async (_cmd: string, args: string[]) => {
        if (args.includes('clone')) {
          const targetDir = args[args.length - 1]!;
          if (targetDir && !existsSync(targetDir)) {
            mkdirSync(targetDir, { recursive: true });
          }
        }
        if (args.includes('sparse-checkout')) {
          const cIdx = args.indexOf('-C');
          const targetDir = cIdx !== -1 ? args[cIdx + 1] : undefined;
          const sparsePath = args[args.length - 1];
          if (targetDir && sparsePath) {
            mkdirSync(join(targetDir, sparsePath), { recursive: true });
          }
        }
        return { success: true, stdout: 'ok', stderr: '', exitCode: 0 };
      }
    );

    mockResolveDefaultBranch.mockResolvedValue('main');

    mockGetOctokit.mockResolvedValue({
      rest: {
        repos: {
          get: vi.fn().mockResolvedValue({
            data: { default_branch: 'main' },
          }),
        },
      },
    });
  });

  afterEach(() => {
    if (existsSync(testDir)) {
      rmSync(testDir, { recursive: true, force: true });
    }
  });

  it('performs full clone when no sparsePath', async () => {
    const result = await cloneRepo({
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'facebook',
      repo: 'react',
      branch: 'main',
    });

    expect(result.cached).toBe(false);
    expect(result.owner).toBe('facebook');
    expect(result.repo).toBe('react');
    expect(result.branch).toBe('main');
    expect(result.localPath).toContain('facebook/react/main');
    expect(result.sparsePath).toBeUndefined();

    const cloneCall = mockSpawnWithTimeout.mock.calls.find(
      (call: unknown[]) => {
        const a = call[1] as string[];
        return a.includes('clone');
      }
    );
    expect(cloneCall).toBeDefined();
    const cloneArgs = cloneCall![1] as string[];
    expect(cloneArgs).toContain('--depth');
    expect(cloneArgs).toContain('--single-branch');
    expect(cloneArgs).toContain('--');
  });

  it('performs sparse clone when sparsePath given', async () => {
    const result = await cloneRepo({
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'facebook',
      repo: 'react',
      branch: 'main',
      sparsePath: 'packages/core',
    });

    expect(result.sparsePath).toBe('packages/core');
    expect(result.localPath).toContain('__sp_');

    expect(mockSpawnWithTimeout).toHaveBeenCalledTimes(3);

    const sparseCall = mockSpawnWithTimeout.mock.calls.find(
      (call: unknown[]) => {
        const a = call[1] as string[];
        return a.includes('sparse-checkout');
      }
    );
    expect(sparseCall).toBeDefined();
    const sparseArgs = sparseCall![1] as string[];
    const dashDashIdx = sparseArgs.indexOf('--');
    const pathIdx = sparseArgs.indexOf('packages/core');
    expect(dashDashIdx).toBeGreaterThan(-1);
    expect(pathIdx).toBe(dashDashIdx + 1);
  });

  it('does not pass auth token to sparse-checkout (local operation)', async () => {
    await cloneRepo(
      {
        mainResearchGoal: 'test',
        researchGoal: 'test',
        reasoning: 'test',
        owner: 'facebook',
        repo: 'react',
        branch: 'main',
        sparsePath: 'src',
      },
      undefined,
      'ghp_secret_token_123'
    );

    const sparseCall = mockSpawnWithTimeout.mock.calls.find(
      (call: unknown[]) => {
        const a = call[1] as string[];
        return a.includes('sparse-checkout');
      }
    );
    const sparseArgs = sparseCall![1] as string[];
    const hasAuth = sparseArgs.some(
      (arg: string) => arg.includes('Authorization') || arg.includes('Bearer')
    );
    expect(hasAuth).toBe(false);
  });

  it('passes auth token to clone (network operation)', async () => {
    await cloneRepo(
      {
        mainResearchGoal: 'test',
        researchGoal: 'test',
        reasoning: 'test',
        owner: 'org',
        repo: 'monorepo',
        branch: 'main',
        sparsePath: 'packages/core',
      },
      undefined,
      'ghp_mytoken'
    );

    const cloneCall = mockSpawnWithTimeout.mock.calls.find(
      (call: unknown[]) => {
        const a = call[1] as string[];
        return a.includes('clone');
      }
    );
    expect(cloneCall).toBeDefined();
    const cloneArgStr = (cloneCall![1] as string[]).join(' ');
    expect(cloneArgStr).toContain('Bearer ghp_mytoken');

    const sparseCall = mockSpawnWithTimeout.mock.calls.find(
      (call: unknown[]) => {
        const a = call[1] as string[];
        return a.includes('sparse-checkout');
      }
    );
    expect(sparseCall).toBeDefined();
    const sparseArgStr = (sparseCall![1] as string[]).join(' ');
    expect(sparseArgStr).not.toContain('Bearer');
    expect(sparseArgStr).not.toContain('ghp_mytoken');
  });

  it('returns cached result when cache is valid', async () => {
    await cloneRepo({
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'facebook',
      repo: 'react',
      branch: 'main',
    });

    mockSpawnWithTimeout.mockClear();
    mockSpawnWithTimeout.mockResolvedValue({
      success: true,
      stdout: 'git version 2.40.0',
      stderr: '',
      exitCode: 0,
    });

    const result = await cloneRepo({
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'facebook',
      repo: 'react',
      branch: 'main',
    });

    expect(result.cached).toBe(true);
    const cloneCalls = mockSpawnWithTimeout.mock.calls.filter(
      (call: unknown[]) => {
        const a = call[1] as string[];
        return a.includes('clone');
      }
    );
    expect(cloneCalls).toHaveLength(0);
  });

  it('forceRefresh: true bypasses valid cache and re-clones', async () => {
    await cloneRepo({
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'facebook',
      repo: 'react',
      branch: 'main',
    });

    mockSpawnWithTimeout.mockClear();
    mockSpawnWithTimeout.mockImplementation(
      async (_cmd: string, args: string[]) => {
        if (args.includes('clone')) {
          const targetDir = args[args.length - 1]!;
          if (targetDir && !existsSync(targetDir)) {
            mkdirSync(targetDir, { recursive: true });
          }
        }
        return { success: true, stdout: 'ok', stderr: '', exitCode: 0 };
      }
    );

    const result = await cloneRepo({
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'facebook',
      repo: 'react',
      branch: 'main',
      forceRefresh: true,
    });

    expect(result.cached).toBe(false);
    const cloneCalls = mockSpawnWithTimeout.mock.calls.filter(
      (call: unknown[]) => {
        const a = call[1] as string[];
        return a.includes('clone');
      }
    );
    expect(cloneCalls.length).toBeGreaterThan(0);
  });

  it('rejects directoryFetch cache and re-clones', async () => {
    const cloneDir = getCloneDir(testDir, 'facebook', 'react', 'main');
    mkdirSync(cloneDir, { recursive: true });
    const dirFetchMeta = createCacheMeta(
      'facebook',
      'react',
      'main',
      'directoryFetch'
    );
    writeCacheMeta(cloneDir, dirFetchMeta);

    const result = await cloneRepo({
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'facebook',
      repo: 'react',
      branch: 'main',
    });

    expect(result.cached).toBe(false);
    const cloneCalls = mockSpawnWithTimeout.mock.calls.filter(
      (call: unknown[]) => {
        const a = call[1] as string[];
        return a.includes('clone');
      }
    );
    expect(cloneCalls.length).toBeGreaterThan(0);
  });

  it('writes source: clone in cache metadata', async () => {
    const result = await cloneRepo({
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'facebook',
      repo: 'react',
      branch: 'main',
    });

    const metaPath = join(result.localPath, '.octocode-clone-meta.json');
    expect(existsSync(metaPath)).toBe(true);
    const meta = JSON.parse(readFileSync(metaPath, 'utf-8'));
    expect(meta.source).toBe('clone');
  });

  it('re-clones when cache meta exists but is expired', async () => {
    const first = await cloneRepo({
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'facebook',
      repo: 'react',
      branch: 'main',
    });
    expect(first.cached).toBe(false);
    expect(existsSync(first.localPath)).toBe(true);

    const expiredMeta = createCacheMeta('facebook', 'react', 'main', 'clone');
    expiredMeta.expiresAt = new Date(Date.now() - 1000).toISOString();
    writeCacheMeta(first.localPath, expiredMeta);

    mockSpawnWithTimeout.mockClear();
    mockSpawnWithTimeout.mockImplementation(
      async (_cmd: string, args: string[]) => {
        if (args.includes('clone')) {
          const targetDir = args[args.length - 1]!;
          if (targetDir && !existsSync(targetDir)) {
            mkdirSync(targetDir, { recursive: true });
          }
        }
        return { success: true, stdout: 'ok', stderr: '', exitCode: 0 };
      }
    );

    const second = await cloneRepo({
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'facebook',
      repo: 'react',
      branch: 'main',
    });
    expect(second.cached).toBe(false);
    const cloneCalls = mockSpawnWithTimeout.mock.calls.filter(
      (call: unknown[]) => {
        const a = call[1] as string[];
        return a.includes('clone');
      }
    );
    expect(cloneCalls.length).toBeGreaterThan(0);
  });

  it('re-clones when directory was externally deleted despite valid cache meta', async () => {
    const first = await cloneRepo({
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'facebook',
      repo: 'react',
      branch: 'main',
    });
    expect(first.cached).toBe(false);

    rmSync(first.localPath, { recursive: true, force: true });
    mockSpawnWithTimeout.mockClear();
    mockSpawnWithTimeout.mockImplementation(
      async (_cmd: string, args: string[]) => {
        if (args.includes('clone')) {
          const targetDir = args[args.length - 1]!;
          if (targetDir && !existsSync(targetDir)) {
            mkdirSync(targetDir, { recursive: true });
          }
        }
        return { success: true, stdout: 'ok', stderr: '', exitCode: 0 };
      }
    );

    const second = await cloneRepo({
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'facebook',
      repo: 'react',
      branch: 'main',
    });
    expect(second.cached).toBe(false);
    const cloneCalls = mockSpawnWithTimeout.mock.calls.filter(
      (call: unknown[]) => {
        const a = call[1] as string[];
        return a.includes('clone');
      }
    );
    expect(cloneCalls.length).toBeGreaterThan(0);
  });

  it('resolves default branch from API when not specified', async () => {
    mockResolveDefaultBranch.mockResolvedValue('develop');

    const result = await cloneRepo({
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'org',
      repo: 'project',
    });

    expect(result.branch).toBe('develop');
    expect(mockResolveDefaultBranch).toHaveBeenCalledWith(
      'org',
      'project',
      undefined
    );
  });

  it('uses branch from resolveDefaultBranch when query.branch is omitted', async () => {
    mockResolveDefaultBranch.mockResolvedValue('main');

    const result = await cloneRepo({
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'org',
      repo: 'project',
    });

    expect(result.branch).toBe('main');
    expect(mockResolveDefaultBranch).toHaveBeenCalledWith(
      'org',
      'project',
      undefined
    );
  });

  it('throws clear error when git is not available', async () => {
    mockSpawnWithTimeout.mockResolvedValueOnce({
      success: false,
      stdout: '',
      stderr: 'command not found: git',
      exitCode: 127,
    });

    await expect(
      cloneRepo({
        mainResearchGoal: 'test',
        researchGoal: 'test',
        reasoning: 'test',
        owner: 'fb',
        repo: 'react',
        branch: 'main',
      })
    ).rejects.toThrow('git is not installed');
  });

  it('throws on clone failure with scrubbed error', async () => {
    mockSpawnWithTimeout
      .mockResolvedValueOnce({
        success: true,
        stdout: 'git version 2.40.0',
        stderr: '',
        exitCode: 0,
      })
      .mockResolvedValueOnce({
        success: false,
        stdout: '',
        stderr: 'fatal: Authorization: Bearer ghp_secret123 failed',
        exitCode: 128,
      });

    await expect(
      cloneRepo(
        {
          mainResearchGoal: 'test',
          researchGoal: 'test',
          reasoning: 'test',
          owner: 'fb',
          repo: 'react',
          branch: 'main',
        },
        undefined,
        'ghp_secret123'
      )
    ).rejects.toThrow('[REDACTED]');
  });

  it('scrubs Authorization header patterns from error messages', async () => {
    mockSpawnWithTimeout
      .mockResolvedValueOnce({
        success: true,
        stdout: 'git version 2.40.0',
        stderr: '',
        exitCode: 0,
      })
      .mockResolvedValueOnce({
        success: false,
        stdout: '',
        stderr:
          'fatal: could not read Username: Authorization: Bearer sometoken123',
        exitCode: 128,
      });

    try {
      await cloneRepo({
        mainResearchGoal: 'test',
        researchGoal: 'test',
        reasoning: 'test',
        owner: 'fb',
        repo: 'react',
        branch: 'main',
      });
      expect.fail('should have thrown');
    } catch (e) {
      const msg = (e as Error).message;
      expect(msg).not.toContain('sometoken123');
      expect(msg).toContain('[REDACTED]');
    }
  });

  it('uses full clone args correctly (--depth 1 --single-branch --)', async () => {
    await cloneRepo({
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'fb',
      repo: 'react',
      branch: 'main',
    });

    const cloneCall = mockSpawnWithTimeout.mock.calls.find(
      (call: unknown[]) => {
        const a = call[1] as string[];
        return a.includes('clone') && !a.includes('--sparse');
      }
    );
    expect(cloneCall).toBeDefined();
    const args = cloneCall![1] as string[];

    expect(args).toContain('--depth');
    expect(args[args.indexOf('--depth') + 1]).toBe('1');
    expect(args).toContain('--single-branch');
    expect(args).toContain('--branch');
    expect(args).toContain('--');

    const dashDashIdx = args.indexOf('--');
    expect(args[dashDashIdx + 1]).toContain('github.com/fb/react.git');
  });

  it('uses sparse clone args correctly (--filter --sparse --depth 1)', async () => {
    await cloneRepo({
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'fb',
      repo: 'react',
      branch: 'main',
      sparsePath: 'packages/core',
    });

    const cloneCall = mockSpawnWithTimeout.mock.calls.find(
      (call: unknown[]) => {
        const a = call[1] as string[];
        return a.includes('clone') && a.includes('--sparse');
      }
    );
    expect(cloneCall).toBeDefined();
    const args = cloneCall![1] as string[];

    expect(args).toContain('--filter');
    expect(args[args.indexOf('--filter') + 1]).toBe('blob:none');
    expect(args).toContain('--sparse');
    expect(args).toContain('--depth');
    expect(args).toContain('--');
  });

  it('prefers authInfo.token over explicit token', async () => {
    const authInfo = { token: 'oauth_token_from_authinfo' } as any;

    await cloneRepo(
      {
        mainResearchGoal: 'test',
        researchGoal: 'test',
        reasoning: 'test',
        owner: 'fb',
        repo: 'react',
        branch: 'main',
      },
      authInfo,
      'fallback_token'
    );

    const cloneCall = mockSpawnWithTimeout.mock.calls.find(
      (call: unknown[]) => {
        const a = call[1] as string[];
        return a.includes('clone');
      }
    );
    const cloneArgStr = (cloneCall![1] as string[]).join(' ');
    expect(cloneArgStr).toContain('oauth_token_from_authinfo');
    expect(cloneArgStr).not.toContain('fallback_token');
  });

  it('works without any token (public repo)', async () => {
    await cloneRepo({
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'fb',
      repo: 'react',
      branch: 'main',
    });

    const cloneCall = mockSpawnWithTimeout.mock.calls.find(
      (call: unknown[]) => {
        const a = call[1] as string[];
        return a.includes('clone');
      }
    );
    const cloneArgStr = (cloneCall![1] as string[]).join(' ');
    expect(cloneArgStr).not.toContain('Authorization');
    expect(cloneArgStr).not.toContain('Bearer');
  });

  it('returns cached result with sparsePath when cache is valid', async () => {
    await cloneRepo({
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'facebook',
      repo: 'react',
      branch: 'main',
      sparsePath: 'packages/core',
    });

    mockSpawnWithTimeout.mockClear();
    mockSpawnWithTimeout.mockResolvedValue({
      success: true,
      stdout: 'git version 2.40.0',
      stderr: '',
      exitCode: 0,
    });

    const result = await cloneRepo({
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'facebook',
      repo: 'react',
      branch: 'main',
      sparsePath: 'packages/core',
    });

    expect(result.cached).toBe(true);
    expect(result.sparsePath).toBe('packages/core');
  });

  it('throws clear error when spawn itself throws (git not on PATH)', async () => {
    mockSpawnWithTimeout.mockRejectedValueOnce(new Error('spawn git ENOENT'));

    await expect(
      cloneRepo({
        mainResearchGoal: 'test',
        researchGoal: 'test',
        reasoning: 'test',
        owner: 'fb',
        repo: 'react',
        branch: 'main',
      })
    ).rejects.toThrow('git is not installed');
  });

  it('scrubs "Authorization: token" pattern (not just Bearer)', async () => {
    mockSpawnWithTimeout
      .mockResolvedValueOnce({
        success: true,
        stdout: 'git version 2.40.0',
        stderr: '',
        exitCode: 0,
      })
      .mockResolvedValueOnce({
        success: false,
        stdout: '',
        stderr: 'Authorization: token ghp_abc123 in request',
        exitCode: 128,
      });

    try {
      await cloneRepo({
        mainResearchGoal: 'test',
        researchGoal: 'test',
        reasoning: 'test',
        owner: 'fb',
        repo: 'react',
        branch: 'main',
      });
      expect.fail('should have thrown');
    } catch (e) {
      const msg = (e as Error).message;
      expect(msg).not.toContain('ghp_abc123');
      expect(msg).toContain('[REDACTED]');
    }
  });

  it('handles clone failure with empty stderr', async () => {
    mockSpawnWithTimeout
      .mockResolvedValueOnce({
        success: true,
        stdout: 'git version 2.40.0',
        stderr: '',
        exitCode: 0,
      })
      .mockResolvedValueOnce({
        success: false,
        stdout: '',
        stderr: '',
        exitCode: 128,
      });

    await expect(
      cloneRepo({
        mainResearchGoal: 'test',
        researchGoal: 'test',
        reasoning: 'test',
        owner: 'fb',
        repo: 'react',
        branch: 'main',
      })
    ).rejects.toThrow('git full clone of fb/react failed');
  });

  it('sets GIT_TERMINAL_PROMPT=0 to prevent interactive prompts', async () => {
    await cloneRepo({
      mainResearchGoal: 'test',
      researchGoal: 'test',
      reasoning: 'test',
      owner: 'fb',
      repo: 'react',
      branch: 'main',
    });

    for (const call of mockSpawnWithTimeout.mock.calls) {
      const opts = call[2] as { env?: Record<string, string> };
      expect(opts.env?.GIT_TERMINAL_PROMPT).toBe('0');
    }
  });
});

const mockGetActiveProvider = vi.hoisted(() => vi.fn());
const mockGetActiveProviderConfig = vi.hoisted(() => vi.fn());
const mockGetProvider = vi.hoisted(() => vi.fn());

vi.mock('../../../octocode-tools-core/src/serverConfig.js', () => ({
  getActiveProvider: mockGetActiveProvider,
  getActiveProviderConfig: mockGetActiveProviderConfig,
}));

vi.mock('../../../octocode-tools-core/src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

import { executeCloneRepo } from '../../../octocode-tools-core/src/tools/github_clone_repo/execution.js';
import { registerGitHubCloneRepoTool } from '../../src/tools/github_clone_repo/github_clone_repo.js';
import { createMockMcpServer } from '../fixtures/mcp-fixtures.js';

function createMockProviderCapabilities(type?: string) {
  return {
    cloneRepo: type === 'github',
    fetchDirectoryToDisk: type === 'github',
    requiresScopedCodeSearch: type !== 'github',
    supportsMergedState: type !== 'github',
    supportsMultiTopicSearch: type === 'github',
  };
}

describe('registerGitHubCloneRepoTool', () => {
  it('registers the tool with correct name and metadata', () => {
    const mockServer = createMockMcpServer();
    registerGitHubCloneRepoTool(mockServer.server);

    expect(mockServer.server.registerTool).toHaveBeenCalledTimes(1);
    const [toolName, options] = (mockServer.server.registerTool as any).mock
      .calls[0];
    expect(toolName).toBe('ghCloneRepo');
    expect(options.description).toContain('Clone');
    expect(options.annotations.idempotentHint).toBe(true);
    expect(options.annotations.readOnlyHint).toBe(false);
  });

  it('registered handler invokes executeCloneRepo', async () => {
    const execTestDir = join(
      tmpdir(),
      `octocode-reg-test-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
    );
    mockGetOctocodeDir.mockReturnValue(execTestDir);
    mockGetActiveProvider.mockReturnValue('github');
    mockGetActiveProviderConfig.mockReturnValue({ token: 'test-token' });
    mockGetProvider.mockImplementation((type?: string) => ({
      capabilities: createMockProviderCapabilities(type),
    }));

    mockSpawnWithTimeout.mockImplementation(
      async (_cmd: string, args: string[]) => {
        if (args.includes('clone')) {
          const targetDir = args[args.length - 1]!;
          if (targetDir && !existsSync(targetDir)) {
            mkdirSync(targetDir, { recursive: true });
          }
        }
        return { success: true, stdout: 'ok', stderr: '', exitCode: 0 };
      }
    );

    mockGetOctokit.mockResolvedValue({
      rest: {
        repos: {
          get: vi.fn().mockResolvedValue({ data: { default_branch: 'main' } }),
        },
      },
    });

    const mockServer = createMockMcpServer();
    registerGitHubCloneRepoTool(mockServer.server);

    const result = await mockServer.callTool('ghCloneRepo', {
      queries: [
        {
          mainResearchGoal: 'test',
          researchGoal: 'test',
          reasoning: 'test',
          owner: 'fb',
          repo: 'react',
          branch: 'main',
        },
      ],
    });

    const text = result.content
      .filter((c): c is { type: 'text'; text: string } => c.type === 'text')
      .map(c => c.text)
      .join('\n');
    expect(text).toContain('localPath');

    if (existsSync(execTestDir)) {
      rmSync(execTestDir, { recursive: true, force: true });
    }
  });
});

describe('executeCloneRepo', () => {
  let execTestDir: string;

  beforeEach(() => {
    vi.clearAllMocks();
    mockGetActiveProvider.mockReturnValue('github');
    mockGetActiveProviderConfig.mockReturnValue({ token: 'mock-token' });
    mockGetProvider.mockImplementation((type?: string) => ({
      capabilities: createMockProviderCapabilities(type),
    }));
    execTestDir = join(
      tmpdir(),
      `octocode-exec-test-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
    );
    mockGetOctocodeDir.mockReturnValue(execTestDir);

    mockSpawnWithTimeout.mockImplementation(
      async (_cmd: string, args: string[]) => {
        if (args.includes('clone')) {
          const targetDir = args[args.length - 1]!;
          if (targetDir && !existsSync(targetDir)) {
            mkdirSync(targetDir, { recursive: true });
          }
        }
        return { success: true, stdout: 'ok', stderr: '', exitCode: 0 };
      }
    );

    mockGetOctokit.mockResolvedValue({
      rest: {
        repos: {
          get: vi.fn().mockResolvedValue({ data: { default_branch: 'main' } }),
        },
      },
    });
  });

  afterEach(() => {
    if (existsSync(execTestDir)) {
      rmSync(execTestDir, { recursive: true, force: true });
    }
  });

  it('returns success result for github provider', async () => {
    const result = await executeCloneRepo({
      queries: [
        {
          mainResearchGoal: 'test',
          researchGoal: 'test',
          reasoning: 'test',
          owner: 'fb',
          repo: 'react',
          branch: 'main',
        },
      ],
    });

    const text = result.content
      .filter((c): c is { type: 'text'; text: string } => c.type === 'text')
      .map(c => c.text)
      .join('\n');
    expect(text).toContain('localPath');
    expect(text).toContain('fb');
  });

  it('includes resolvedBranch when clone resolves to different branch than requested', async () => {
    mockResolveDefaultBranch.mockResolvedValue('develop');

    const result = await executeCloneRepo({
      queries: [
        {
          mainResearchGoal: 'test',
          researchGoal: 'test',
          reasoning: 'test',
          owner: 'fb',
          repo: 'react',
        },
      ],
    });

    const text = result.content
      .filter((c): c is { type: 'text'; text: string } => c.type === 'text')
      .map(c => c.text)
      .join('\n');
    expect(text).toContain('resolvedBranch');
    expect(text).toContain('develop');
  });

  it('includes sparse hints for sparse clone', async () => {
    const result = await executeCloneRepo({
      queries: [
        {
          mainResearchGoal: 'test',
          researchGoal: 'test',
          reasoning: 'test',
          owner: 'fb',
          repo: 'react',
          branch: 'main',
          sparsePath: 'packages/core',
        },
      ],
    });

    const text = result.content
      .filter((c): c is { type: 'text'; text: string } => c.type === 'text')
      .map(c => c.text)
      .join('\n');
    expect(text.toLowerCase()).toContain('sparse');
  });

  it('includes cache hint when returning cached result', async () => {
    await executeCloneRepo({
      queries: [
        {
          mainResearchGoal: 'test',
          researchGoal: 'test',
          reasoning: 'test',
          owner: 'fb',
          repo: 'react',
          branch: 'main',
        },
      ],
    });

    const result = await executeCloneRepo({
      queries: [
        {
          mainResearchGoal: 'test',
          researchGoal: 'test',
          reasoning: 'test',
          owner: 'fb',
          repo: 'react',
          branch: 'main',
        },
      ],
    });

    const text = result.content
      .filter((c): c is { type: 'text'; text: string } => c.type === 'text')
      .map(c => c.text)
      .join('\n');
    expect(text.toLowerCase()).toContain('cache');
  });

  it('handles clone failure gracefully', async () => {
    mockSpawnWithTimeout
      .mockResolvedValueOnce({
        success: true,
        stdout: 'git version 2.40.0',
        stderr: '',
        exitCode: 0,
      })
      .mockResolvedValueOnce({
        success: false,
        stdout: '',
        stderr: 'Repository not found',
        exitCode: 128,
      });

    const result = await executeCloneRepo({
      queries: [
        {
          mainResearchGoal: 'test',
          researchGoal: 'test',
          reasoning: 'test',
          owner: 'fb',
          repo: 'nonexistent',
          branch: 'main',
        },
      ],
    });

    const text = result.content
      .filter((c): c is { type: 'text'; text: string } => c.type === 'text')
      .map(c => c.text)
      .join('\n');
    expect(text.toLowerCase()).toContain('failed');
  });
});
