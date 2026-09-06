import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { writeFileSync, mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

let tempDir: string;

const mocks = vi.hoisted(() => ({
  getOctokit: vi.fn(),
  getContent: vi.fn(),
  getCommit: vi.fn(),
  getOctocodeDir: vi.fn(),
  readContent: vi.fn(),
}));

vi.mock('../../src/github/client.js', () => ({
  getOctokit: mocks.getOctokit,
  resolveCacheAuthFingerprint: vi.fn(async () => 'test-auth'),
  resolveDefaultBranch: vi.fn(async () => 'main'),
}));

vi.mock('../../src/shared/paths.js', async importOriginal => ({
  ...(await importOriginal<typeof import('../../src/shared/paths.js')>()),
  getOctocodeDir: mocks.getOctocodeDir,
}));
vi.mock('../../src/shared/fs-utils.js', () => ({
  getDirectorySizeBytes: vi.fn(() => 0),
}));

const { fetchDirectoryContents } =
  await import('../../src/github/directoryFetch/fetchDirectoryContents.js');
const { fetchFileContentToDisk } =
  await import('../../src/github/directoryFetch/fetchFileContentToDisk.js');
const { fetchGitHubFileContentAPI } =
  await import('../../src/github/fileContent.js');
const { clearAllCache } =
  await import('../../src/utils/http/cache/management.js');

const COMMIT_A = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
const COMMIT_B = 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';

describe('fetchDirectoryContents — complete/verified semantics', () => {
  beforeEach(() => {
    tempDir = mkdtempSync(join(tmpdir(), 'octocode-dftest-'));
    mocks.getOctocodeDir.mockReturnValue(tempDir);
    mocks.getOctokit.mockResolvedValue({
      rest: {
        repos: {
          getContent: async (params: { path: string }) => {
            const response = await mocks.getContent(params);
            if (!Array.isArray(response.data)) return response;
            const file = response.data.find(
              (entry: { path: string; type: string }) =>
                entry.path === params.path && entry.type === 'file'
            );
            if (!file) return response;
            const content = await mocks.readContent();
            return {
              data: {
                ...file,
                content: Buffer.from(content).toString('base64'),
                encoding: 'base64',
              },
              headers: {},
            };
          },
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

  it('reuses the exact raw response for disk materialization and reports UTF-8 byte size', async () => {
    const content = 'שלום 🌍';
    mocks.getContent.mockResolvedValue({
      data: {
        type: 'file',
        content: Buffer.from(content).toString('base64'),
        size: Buffer.byteLength(content),
        sha: COMMIT_B,
      },
      headers: {},
    });
    await fetchGitHubFileContentAPI({
      owner: 'owner',
      repo: 'repo',
      path: 'greeting.txt',
      branch: COMMIT_A,
      fullContent: true,
      minify: 'none',
      noTimestamp: true,
    });
    const saved = await fetchFileContentToDisk(
      'owner',
      'repo',
      'greeting.txt',
      COMMIT_A
    );
    expect(mocks.getContent).toHaveBeenCalledOnce();
    expect(saved.size).toBe(Buffer.byteLength(content, 'utf8'));
  });

  it('cache hit without a completeness record → complete:false, verified:false', async () => {
    mocks.getContent.mockResolvedValue({ data: [] });
    const initial = await fetchDirectoryContents('owner', 'repo', '', 'main');
    const root = initial.localPath;
    rmSync(join(root, '.octocode-directory-meta.json'));
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
    expect(result.complete).toBe(false);
    expect(result.verified).toBe(false);
  });

  it('cache hit → warning about unverified completeness', async () => {
    mocks.getContent.mockResolvedValue({ data: [] });
    const initial = await fetchDirectoryContents('owner', 'repo', '', 'main');
    const root = initial.localPath;
    rmSync(join(root, '.octocode-directory-meta.json'));

    const result = await fetchDirectoryContents(
      'owner',
      'repo',
      '',
      'main',
      undefined,
      false
    );

    expect(result.warnings).toBeDefined();
    expect(result.warnings!.some(w => w.includes('partial'))).toBe(true);
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
    mocks.readContent.mockResolvedValue('content');

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
    mocks.getContent.mockResolvedValue({ data: [] });
    const initial = await fetchDirectoryContents('owner', 'repo', '', 'main');
    const root = initial.localPath;
    rmSync(join(root, '.octocode-directory-meta.json'));

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

  it.each([false, true])(
    'preserves original completeness and skip counts on cache hits (oversized=%s)',
    async oversized => {
      mocks.getContent.mockResolvedValue({
        data: [
          {
            name: 'a.ts',
            path: 'src/a.ts',
            type: 'file',
            size: oversized ? 400 * 1024 : 10,
            download_url:
              'https://raw.githubusercontent.com/owner/repo/main/src/a.ts',
          },
        ],
      });
      mocks.readContent.mockResolvedValue('content');
      const fresh = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main'
      );
      const cached = await fetchDirectoryContents(
        'owner',
        'repo',
        'src',
        'main'
      );
      expect(fresh.complete).toBe(!oversized);
      expect(cached.cached).toBe(true);
      expect(cached.complete).toBe(fresh.complete);
      expect(cached.verified).toBe(false);
      expect(cached.skipped).toEqual(fresh.skipped);
      expect(cached.directoryEntryCount).toBe(fresh.directoryEntryCount);
      expect(cached.eligibleFileCount).toBe(fresh.eligibleFileCount);
      expect(cached.files).toEqual(fresh.files);
      expect(
        mocks.getContent.mock.calls.filter(([query]) => query.path === 'src')
      ).toHaveLength(1);
    }
  );

  it('does not retain complete:true after a cached file is removed', async () => {
    mocks.getContent.mockResolvedValue({
      data: [
        {
          name: 'a.ts',
          path: 'a.ts',
          type: 'file',
          size: 7,
          download_url:
            'https://raw.githubusercontent.com/owner/repo/main/a.ts',
        },
      ],
    });
    mocks.readContent.mockResolvedValue('content');
    const fresh = await fetchDirectoryContents('owner', 'repo', '', 'main');
    expect(fresh.complete).toBe(true);
    rmSync(join(fresh.localPath, 'a.ts'));
    const cached = await fetchDirectoryContents('owner', 'repo', '', 'main');
    expect(cached.cached).toBe(true);
    expect(cached.complete).toBe(false);
    expect(cached.savedFileCount).toBe(0);
  });

  it.each([
    ['.gitignore', 'node_modules/\n'],
    ['greeting.txt', 'שלום 🌍'],
    ['.github/README.md', 'workflow guidance'],
  ])(
    'preserves complete cached files and byte sizes for %s',
    async (path, content) => {
      const name = path.split('/').pop()!;
      const directory = path.includes('/')
        ? path.slice(0, path.lastIndexOf('/'))
        : '';
      mocks.getContent.mockResolvedValue({
        data: [
          {
            name,
            path,
            type: 'file',
            size: Buffer.byteLength(content, 'utf8'),
            download_url: `https://raw.githubusercontent.com/owner/repo/main/${path}`,
          },
        ],
      });
      mocks.readContent.mockResolvedValue(content);
      const fresh = await fetchDirectoryContents(
        'owner',
        'repo',
        directory,
        'main'
      );
      const cached = await fetchDirectoryContents(
        'owner',
        'repo',
        directory,
        'main'
      );
      expect(fresh.complete).toBe(true);
      expect(cached.cached).toBe(true);
      expect(cached.complete).toBe(true);
      expect(fresh.totalSize).toBe(Buffer.byteLength(content, 'utf8'));
      expect(cached.totalSize).toBe(fresh.totalSize);
      expect(cached.files).toEqual(fresh.files);
      expect(cached.files.map(file => file.path)).toEqual([path]);
    }
  );

  it.each([
    'invalid json',
    JSON.stringify({ commitSha: COMMIT_B, complete: true }),
  ])(
    'treats unusable completeness metadata conservatively: %s',
    async metadata => {
      mocks.getContent.mockResolvedValue({ data: [] });
      const fresh = await fetchDirectoryContents('owner', 'repo', '', 'main');
      writeFileSync(
        join(fresh.localPath, '.octocode-directory-meta.json'),
        metadata
      );
      const cached = await fetchDirectoryContents('owner', 'repo', '', 'main');
      expect(cached.cached).toBe(true);
      expect(cached.complete).toBe(false);
    }
  );
});
