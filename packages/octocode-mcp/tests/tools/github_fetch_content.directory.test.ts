/**
 * Tests for the directory fetch mode of githubGetFileContent execution layer.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { existsSync, rmSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

const mockGetOctokit = vi.hoisted(() =>
  vi.fn().mockResolvedValue({
    rest: {
      repos: {
        getContent: vi.fn(),
      },
    },
  })
);

const mockResolveDefaultBranch = vi.hoisted(() =>
  vi.fn().mockResolvedValue('main')
);

vi.mock('../../src/github/client.js', () => ({
  getOctokit: mockGetOctokit,
  resolveDefaultBranch: mockResolveDefaultBranch,
}));

const mockGetOctocodeDir = vi.hoisted(() => vi.fn());
vi.mock('octocode-shared', () => ({
  getOctocodeDir: mockGetOctocodeDir,
  getConfigSync: vi.fn(() => ({
    local: {
      enabled: false,
      enableClone: false,
      allowedPaths: [],
      workspaceRoot: '/tmp',
    },
    output: { format: 'yaml', pagination: { defaultCharLength: 2000 } },
  })),
  DEFAULT_OUTPUT_CONFIG: {
    format: 'yaml',
    pagination: { defaultCharLength: 2000 },
  },
  incrementToolCharSavings: vi.fn(() => ({ success: true })),
}));

const mockIsCloneEnabled = vi.hoisted(() => vi.fn().mockReturnValue(true));
const mockGetActiveProvider = vi.hoisted(() =>
  vi.fn().mockReturnValue('github')
);
const mockGetProvider = vi.hoisted(() => vi.fn());

vi.mock('../../src/serverConfig.js', () => ({
  getActiveProviderConfig: vi.fn(() => ({
    provider: mockGetActiveProvider(),
    baseUrl: undefined,
    token: 'mock-token',
  })),
  getActiveProvider: mockGetActiveProvider,
  isLoggingEnabled: vi.fn(() => false),
  isCloneEnabled: mockIsCloneEnabled,
}));

vi.mock('../../src/providers/factory.js', () => ({
  getProvider: mockGetProvider,
}));

// Mock global fetch for download_url
const mockFetch = vi.fn();
vi.stubGlobal('fetch', mockFetch);

import { fetchMultipleGitHubFileContents } from '../../src/tools/github_fetch_content/execution.js';

let testDir: string;

function createTestDir(): string {
  const dir = join(
    tmpdir(),
    `octocode-exec-test-${Date.now()}-${Math.random().toString(36).slice(2)}`
  );
  mkdirSync(dir, { recursive: true });
  return dir;
}

function mockDirectoryListing(
  files: Array<{ name: string; path: string; size: number }>
) {
  const data = files.map(f => ({
    name: f.name,
    path: f.path,
    type: 'file',
    size: f.size,
    download_url: `https://raw.githubusercontent.com/owner/repo/main/${f.path}`,
    sha: 'abc123',
  }));
  mockGetOctokit.mockResolvedValue({
    rest: {
      repos: {
        getContent: vi.fn().mockResolvedValue({ data }),
      },
    },
  });
}

function createMockProvider(type?: string) {
  return {
    capabilities: {
      cloneRepo: type === 'github',
      fetchDirectoryToDisk: type === 'github',
      requiresScopedCodeSearch: type !== 'github',
      supportsMergedState: type !== 'github',
      supportsMultiTopicSearch: type === 'github',
    },
    getFileContent: vi.fn().mockResolvedValue({
      data: {
        path: 'src/file.ts',
        content: 'const x = 1;',
        encoding: 'utf-8',
        size: 12,
        ref: 'main',
      },
      status: 200,
      provider: 'github',
    }),
  };
}

describe('fetchMultipleGitHubFileContents - directory mode', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    testDir = createTestDir();
    mockGetOctocodeDir.mockReturnValue(testDir);
    mockIsCloneEnabled.mockReturnValue(true);
    mockGetActiveProvider.mockReturnValue('github');
    mockGetProvider.mockImplementation((type?: string) =>
      createMockProvider(type)
    );
  });

  afterEach(() => {
    try {
      if (existsSync(testDir)) {
        rmSync(testDir, { recursive: true, force: true });
      }
    } catch {
      // best-effort
    }
  });

  it('should reject directory fetch when ENABLE_CLONE is disabled', async () => {
    mockIsCloneEnabled.mockReturnValue(false);

    const result = await fetchMultipleGitHubFileContents({
      queries: [
        {
          owner: 'owner',
          repo: 'repo',
          path: 'src',
          branch: 'main',
          type: 'directory',
          mainResearchGoal: 'test',
          researchGoal: 'test',
          reasoning: 'test',
        },
      ],
    });

    const text =
      result.content?.map(c => ('text' in c ? c.text : '')).join('') || '';
    expect(text).toContain('ENABLE_LOCAL=true');
    expect(text).toContain('ENABLE_CLONE=true');
    expect(text).toContain('error');
  });

  it('should allow file mode even when ENABLE_CLONE is disabled', async () => {
    mockIsCloneEnabled.mockReturnValue(false);

    const result = await fetchMultipleGitHubFileContents({
      queries: [
        {
          owner: 'owner',
          repo: 'repo',
          path: 'src/file.ts',
          branch: 'main',
          type: 'file',
          mainResearchGoal: 'test',
          researchGoal: 'test',
          reasoning: 'test',
        },
      ],
    });

    const text =
      result.content?.map(c => ('text' in c ? c.text : '')).join('') || '';
    expect(text).not.toContain('ENABLE_CLONE');
  });

  it('should handle directory type query', async () => {
    mockDirectoryListing([
      { name: 'index.ts', path: 'src/index.ts', size: 100 },
      { name: 'utils.ts', path: 'src/utils.ts', size: 200 },
    ]);
    mockFetch.mockImplementation(async (url: string) => {
      if (url.includes('index.ts')) {
        return { ok: true, text: async () => 'export const main = true;' };
      }
      if (url.includes('utils.ts')) {
        return { ok: true, text: async () => 'export function helper() {}' };
      }
      return { ok: false, status: 404 };
    });

    const result = await fetchMultipleGitHubFileContents({
      queries: [
        {
          owner: 'owner',
          repo: 'repo',
          path: 'src',
          branch: 'main',
          type: 'directory',
          mainResearchGoal: 'test',
          researchGoal: 'test',
          reasoning: 'test',
        },
      ],
    });

    expect(result.isError).toBeFalsy();
    const text =
      result.content?.map(c => ('text' in c ? c.text : '')).join('') || '';
    expect(text).toContain('localPath');
    expect(text).toContain('fileCount');
  });

  it('should handle directory type with cache hit', async () => {
    // First call: populate
    mockDirectoryListing([{ name: 'file.ts', path: 'src/file.ts', size: 50 }]);
    mockFetch.mockResolvedValue({
      ok: true,
      text: async () => 'content',
    });

    await fetchMultipleGitHubFileContents({
      queries: [
        {
          owner: 'owner',
          repo: 'repo',
          path: 'src',
          branch: 'main',
          type: 'directory',
          mainResearchGoal: 'test',
          researchGoal: 'test',
          reasoning: 'test',
        },
      ],
    });

    // Second call: should be cache hit
    const result = await fetchMultipleGitHubFileContents({
      queries: [
        {
          owner: 'owner',
          repo: 'repo',
          path: 'src',
          branch: 'main',
          type: 'directory',
          mainResearchGoal: 'test',
          researchGoal: 'test',
          reasoning: 'test',
        },
      ],
    });

    expect(result.isError).toBeFalsy();
    const text =
      result.content?.map(c => ('text' in c ? c.text : '')).join('') || '';
    expect(text).toContain('fileCount:');
  });

  it('should return real fileCount and totalSize on cache hit', async () => {
    // First call: populate with 2 files
    mockDirectoryListing([
      { name: 'index.ts', path: 'src/index.ts', size: 100 },
      { name: 'utils.ts', path: 'src/utils.ts', size: 200 },
    ]);
    mockFetch.mockImplementation(async (url: string) => {
      if (url.includes('index.ts')) {
        return { ok: true, text: async () => 'export const main = true;' };
      }
      if (url.includes('utils.ts')) {
        return { ok: true, text: async () => 'export function helper() {}' };
      }
      return { ok: false, status: 404 };
    });

    const firstResult = await fetchMultipleGitHubFileContents({
      queries: [
        {
          owner: 'owner',
          repo: 'repo',
          path: 'src',
          branch: 'main',
          type: 'directory',
          mainResearchGoal: 'test',
          researchGoal: 'test',
          reasoning: 'test',
        },
      ],
    });

    const firstText =
      firstResult.content?.map(c => ('text' in c ? c.text : '')).join('') || '';
    expect(firstText).toContain('fileCount:');

    const cachedResult = await fetchMultipleGitHubFileContents({
      queries: [
        {
          owner: 'owner',
          repo: 'repo',
          path: 'src',
          branch: 'main',
          type: 'directory',
          mainResearchGoal: 'test',
          researchGoal: 'test',
          reasoning: 'test',
        },
      ],
    });

    expect(cachedResult.isError).toBeFalsy();
    const cachedText =
      cachedResult.content?.map(c => ('text' in c ? c.text : '')).join('') ||
      '';
    expect(cachedText).toContain('fileCount: 2');
    expect(cachedText).not.toContain('fileCount: 0');
    expect(cachedText).not.toContain('totalSize: 0');
  });

  it('should handle directory fetch errors gracefully', async () => {
    mockGetOctokit.mockResolvedValue({
      rest: {
        repos: {
          getContent: vi.fn().mockRejectedValue(new Error('API error')),
        },
      },
    });

    const result = await fetchMultipleGitHubFileContents({
      queries: [
        {
          owner: 'owner',
          repo: 'repo',
          path: 'nonexistent',
          branch: 'main',
          type: 'directory',
          mainResearchGoal: 'test',
          researchGoal: 'test',
          reasoning: 'test',
        },
      ],
    });

    expect(result).toBeDefined();
    const text =
      result.content?.map(c => ('text' in c ? c.text : '')).join('') || '';
    expect(text).toContain('error');
  });

  it('should resolve default branch via API when not specified', async () => {
    mockResolveDefaultBranch.mockResolvedValue('main');
    mockDirectoryListing([{ name: 'file.ts', path: 'src/file.ts', size: 50 }]);
    mockFetch.mockResolvedValue({
      ok: true,
      text: async () => 'content',
    });

    const result = await fetchMultipleGitHubFileContents({
      queries: [
        {
          owner: 'owner',
          repo: 'repo',
          path: 'src',
          type: 'directory',
          mainResearchGoal: 'test',
          researchGoal: 'test',
          reasoning: 'test',
        },
      ],
    });

    expect(result.isError).toBeFalsy();
    const text =
      result.content?.map(c => ('text' in c ? c.text : '')).join('') || '';
    expect(text).toContain('localPath');
    expect(mockResolveDefaultBranch).toHaveBeenCalledWith(
      'owner',
      'repo',
      undefined
    );
  });
});
