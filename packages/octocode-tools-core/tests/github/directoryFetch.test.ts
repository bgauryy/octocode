import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mkdirSync, writeFileSync, mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

let tempDir: string;

const mocks = vi.hoisted(() => ({
  getOctokit: vi.fn(),
  getContent: vi.fn(),
  getOctocodeDir: vi.fn(),
  fetch: vi.fn(),
}));

vi.mock('../../src/github/client.js', () => ({
  getOctokit: mocks.getOctokit,
}));

vi.mock('../../src/shared/index.js', () => ({
  getOctocodeDir: mocks.getOctocodeDir,
  // evictExpiredTrees calls getDirectorySizeBytes — stub it
  getDirectorySizeBytes: vi.fn(() => 0),
}));

global.fetch = mocks.fetch as typeof fetch;

const { fetchDirectoryContents } = await import(
  '../../src/github/directoryFetch.js'
);

// Matches getTreeDir: join(octocodeDir, 'tmp', 'tree', owner, repo, branch)
function buildTreeRoot(
  base: string,
  owner: string,
  repo: string,
  branch: string
) {
  return join(base, 'tmp', 'tree', owner, repo, branch);
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
      rest: { repos: { getContent: mocks.getContent } },
    });
  });

  afterEach(() => {
    vi.clearAllMocks();
    rmSync(tempDir, { recursive: true, force: true });
  });

  it('cache hit → complete:true, verified:false', async () => {
    const root = buildTreeRoot(tempDir, 'owner', 'repo', 'main');
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
    const root = buildTreeRoot(tempDir, 'owner', 'repo', 'main');
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
    const root = buildTreeRoot(tempDir, 'owner', 'repo', 'main');
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
});
