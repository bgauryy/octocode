import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mkdirSync, writeFileSync, mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

let tempDir: string;

const mocks = vi.hoisted(() => ({
  getOctokit: vi.fn(),
  getContent: vi.fn(),
  getCommit: vi.fn(),
  getOctocodeDir: vi.fn(),
  fetch: vi.fn(),
}));

vi.mock('../../src/github/client.js', () => ({
  getOctokit: mocks.getOctokit,
  resolveCacheAuthFingerprint: vi.fn(async () => 'test-auth'),
  resolveDefaultBranch: vi.fn(async () => 'main'),
}));

vi.mock('../../src/shared/index.js', () => ({
  getOctocodeDir: mocks.getOctocodeDir,
  // evictExpiredTrees calls getDirectorySizeBytes — stub it
  getDirectorySizeBytes: vi.fn(() => 0),
}));

global.fetch = mocks.fetch as typeof fetch;

const { fetchDirectoryContents } =
  await import('../../src/github/directoryFetch.js');
const { clearAllCache } = await import('../../src/utils/http/cache.js');

const COMMIT_A = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
const COMMIT_B = 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';

// Matches getTreeDir: join(octocodeDir, 'tmp', 'tree', owner, repo, commitSha)
function buildTreeRoot(
  base: string,
  owner: string,
  repo: string,
  commitSha: string
) {
  return join(base, 'tmp', 'tree', owner, repo, commitSha);
}

function seedCacheMeta(
  root: string,
  owner: string,
  repo: string,
  branch: string
): void {
  const expiresAt = new Date(Date.now() + 24 * 60 * 60 * 1000).toISOString();
  writeFileSync(
    join(root, '.octocode-clone-meta.json'),
    JSON.stringify({
      clonedAt: new Date().toISOString(),
      expiresAt,
      owner,
      repo,
      branch,
      commitSha: COMMIT_A,
      source: 'treeFetch',
    }),
    'utf-8'
  );
}

describe('fetchDirectoryContents — complete/verified semantics', () => {
  beforeEach(() => {
    tempDir = mkdtempSync(join(tmpdir(), 'octocode-dftest-'));
    mocks.getOctocodeDir.mockReturnValue(tempDir);
    mocks.getOctokit.mockResolvedValue({
      rest: {
        repos: {
          getContent: mocks.getContent,
          getCommit: mocks.getCommit,
        },
      },
    });
    mocks.getCommit.mockResolvedValue({ data: { sha: COMMIT_A } });
    clearAllCache();
  });

  afterEach(() => {
    vi.clearAllMocks();
    clearAllCache();
    rmSync(tempDir, { recursive: true, force: true });
  });

  it('cache hit → complete:true, verified:false', async () => {
    const root = buildTreeRoot(tempDir, 'owner', 'repo', COMMIT_A);
    mkdirSync(root, { recursive: true });
    seedCacheMeta(root, 'owner', 'repo', 'main');
    writeFileSync(join(root, 'foo.ts'), 'export const x = 1;', 'utf-8');

    const result = await fetchDirectoryContents(
      'owner',
      'repo',
      '',
      'main',
      undefined,
      false
    );

    expect(result.cached).toBe(true);
    expect(result.complete).toBe(true);
    expect(result.verified).toBe(false);
  });

  it('cache hit → warning about unverified completeness', async () => {
    const root = buildTreeRoot(tempDir, 'owner', 'repo', COMMIT_A);
    mkdirSync(root, { recursive: true });
    seedCacheMeta(root, 'owner', 'repo', 'main');

    const result = await fetchDirectoryContents(
      'owner',
      'repo',
      '',
      'main',
      undefined,
      false
    );

    expect(result.warnings).toBeDefined();
    expect(result.warnings!.some(w => w.includes('Cannot verify'))).toBe(true);
  });

  it('fresh fetch with no skips → complete:true, verified:true', async () => {
    mocks.getContent.mockResolvedValue({
      data: [
        {
          name: 'a.ts',
          path: 'a.ts',
          type: 'file',
          size: 10,
          download_url:
            'https://raw.githubusercontent.com/owner/repo/main/a.ts',
        },
      ],
    });
    mocks.fetch.mockResolvedValue({
      ok: true,
      text: async () => 'content',
    });

    const result = await fetchDirectoryContents(
      'owner',
      'repo',
      '',
      'main',
      undefined,
      true
    );

    expect(result.cached).toBe(false);
    expect(result.complete).toBe(true);
    expect(result.verified).toBe(true);
    expect(result.warnings).toBeUndefined();
  });

  it('fresh fetch with oversized file skipped → complete:false, verified:false', async () => {
    mocks.getContent.mockResolvedValue({
      data: [
        {
          name: 'big.ts',
          path: 'big.ts',
          type: 'file',
          size: 400 * 1024,
          download_url:
            'https://raw.githubusercontent.com/owner/repo/main/big.ts',
        },
      ],
    });

    const result = await fetchDirectoryContents(
      'owner',
      'repo',
      '',
      'main',
      undefined,
      true
    );

    expect(result.complete).toBe(false);
    expect(result.verified).toBe(false);
    expect(result.warnings).toBeDefined();
    expect(result.warnings!.some(w => w.includes('partial'))).toBe(true);
  });

  it('forceRefresh bypasses cache → verified:true on clean fetch', async () => {
    const root = buildTreeRoot(tempDir, 'owner', 'repo', COMMIT_A);
    mkdirSync(root, { recursive: true });
    seedCacheMeta(root, 'owner', 'repo', 'main');

    mocks.getContent.mockResolvedValue({ data: [] });

    const result = await fetchDirectoryContents(
      'owner',
      'repo',
      '',
      'main',
      undefined,
      true
    );

    expect(result.cached).toBe(false);
    expect(result.complete).toBe(true);
    expect(result.verified).toBe(true);
  });

  it('materializes different branch tips at different immutable paths', async () => {
    mocks.getContent.mockResolvedValue({ data: [] });
    mocks.getCommit
      .mockResolvedValueOnce({ data: { sha: COMMIT_A } })
      .mockResolvedValueOnce({ data: { sha: COMMIT_B } });

    const first = await fetchDirectoryContents(
      'owner',
      'repo',
      '',
      'main',
      undefined,
      true
    );
    const second = await fetchDirectoryContents(
      'owner',
      'repo',
      '',
      'main',
      undefined,
      true
    );

    expect(first.repoRoot).toContain(COMMIT_A);
    expect(second.repoRoot).toContain(COMMIT_B);
    expect(second.repoRoot).not.toBe(first.repoRoot);
  });
});
