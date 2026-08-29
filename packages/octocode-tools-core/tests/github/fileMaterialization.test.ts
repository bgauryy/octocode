import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

const COMMIT_A = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
const COMMIT_B = 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';

const mocks = vi.hoisted(() => ({
  home: '',
  getCommit: vi.fn(),
  fetchRaw: vi.fn(),
}));

vi.mock('../../src/shared/index.js', async importOriginal => {
  const actual =
    await importOriginal<typeof import('../../src/shared/index.js')>();
  return {
    ...actual,
    getOctocodeDir: () => mocks.home,
    getDirectorySizeBytes: vi.fn(() => 0),
  };
});

vi.mock('../../src/github/client.js', () => ({
  getOctokit: vi.fn(async () => ({
    rest: { repos: { getCommit: mocks.getCommit } },
  })),
  resolveCacheAuthFingerprint: vi.fn(async () => 'test-auth'),
  resolveDefaultBranch: vi.fn(async () => 'main'),
}));

vi.mock('../../src/github/fileContentRaw.js', () => ({
  fetchRawGitHubFileContent: (...args: unknown[]) => mocks.fetchRaw(...args),
}));

const { fetchFileContentToDisk } =
  await import('../../src/github/directoryFetch.js');
const { clearAllCache } = await import('../../src/utils/http/cache.js');

describe('exact file materialization identity', () => {
  beforeEach(() => {
    mocks.home = mkdtempSync(join(tmpdir(), 'octocode-file-tree-'));
    mocks.getCommit.mockReset();
    mocks.fetchRaw.mockReset();
    mocks.getCommit.mockResolvedValue({ data: { sha: COMMIT_A } });
    mocks.fetchRaw.mockResolvedValue({
      data: { rawContent: 'version-a', branch: COMMIT_A },
      status: 200,
    });
    clearAllCache();
  });

  afterEach(() => {
    clearAllCache();
    rmSync(mocks.home, { recursive: true, force: true });
  });

  it('reuses one commit path and moves to a new path when the ref advances', async () => {
    const first = await fetchFileContentToDisk(
      'owner',
      'repo',
      'src/index.ts',
      'main'
    );
    const cached = await fetchFileContentToDisk(
      'owner',
      'repo',
      'src/index.ts',
      'main'
    );

    expect(first.repoRoot).toContain(COMMIT_A);
    expect(cached.repoRoot).toBe(first.repoRoot);
    expect(cached.cached).toBe(true);
    expect(mocks.fetchRaw).toHaveBeenCalledTimes(1);

    mocks.getCommit.mockResolvedValue({ data: { sha: COMMIT_B } });
    mocks.fetchRaw.mockResolvedValue({
      data: { rawContent: 'version-b', branch: COMMIT_B },
      status: 200,
    });
    const advanced = await fetchFileContentToDisk(
      'owner',
      'repo',
      'src/index.ts',
      'main',
      undefined,
      true
    );

    expect(advanced.repoRoot).toContain(COMMIT_B);
    expect(advanced.repoRoot).not.toBe(first.repoRoot);
    expect(existsSync(first.localPath)).toBe(true);
    expect(existsSync(advanced.localPath)).toBe(true);
  });
});
