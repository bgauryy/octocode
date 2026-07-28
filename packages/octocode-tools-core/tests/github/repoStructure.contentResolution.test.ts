import { beforeEach, describe, expect, it, vi } from 'vitest';
import { RequestError } from 'octokit';

// Mock client before importing contentResolution
vi.mock('../../src/github/client.js', () => ({
  getOctokit: vi.fn(),
  resolveDefaultBranch: vi.fn(async () => 'main'),
  OctokitWithThrottling: class {},
  resolveCacheAuthFingerprint: vi.fn(async () => 'anon'),
}));

import { resolveDefaultBranch } from '../../src/github/client.js';
import {
  mapApiItems,
  resolveContentWithBranchFallback,
} from '../../src/github/repoStructure/contentResolution.js';

const mockResolveDefaultBranch = vi.mocked(resolveDefaultBranch);

// ---------------------------------------------------------------------------
// mapApiItems (pure)
// ---------------------------------------------------------------------------

describe('mapApiItems', () => {
  it('maps raw API items to GitHubApiFileItem shape', () => {
    const raw = [
      {
        name: 'index.ts',
        path: 'src/index.ts',
        type: 'file',
        size: 100,
        download_url: 'https://raw.github.com/...',
        url: 'https://api.github.com/...',
        html_url: 'https://github.com/...',
        git_url: 'https://api.github.com/git/...',
        sha: 'abc123',
      },
    ];
    const items = mapApiItems(raw);
    expect(items).toHaveLength(1);
    const item = items[0]!;
    expect(item.name).toBe('index.ts');
    expect(item.path).toBe('src/index.ts');
    expect(item.type).toBe('file');
    expect(item.size).toBe(100);
    expect(item.download_url).toBe('https://raw.github.com/...');
    expect(item.sha).toBe('abc123');
  });

  it('maps directory items', () => {
    const raw = [
      {
        name: 'src',
        path: 'src',
        type: 'dir',
        url: 'https://api.github.com/...',
        html_url: 'https://github.com/...',
        git_url: 'https://api.github.com/git/...',
        sha: 'def456',
      },
    ];
    const items = mapApiItems(raw);
    expect(items[0]!.type).toBe('dir');
    expect(items[0]!.size).toBeUndefined();
    expect(items[0]!.download_url).toBeUndefined();
  });

  it('returns an empty array for empty input', () => {
    expect(mapApiItems([])).toHaveLength(0);
  });

  it('maps multiple items', () => {
    const raw = [
      { name: 'a.ts', path: 'a.ts', type: 'file', size: 10, url: '', html_url: '', git_url: '', sha: '1' },
      { name: 'b.ts', path: 'b.ts', type: 'file', size: 20, url: '', html_url: '', git_url: '', sha: '2' },
    ];
    expect(mapApiItems(raw)).toHaveLength(2);
  });
});

// ---------------------------------------------------------------------------
// resolveContentWithBranchFallback
// ---------------------------------------------------------------------------

function makeOctokit(getContentImpl: () => Promise<unknown>) {
  return {
    rest: {
      repos: {
        getContent: vi.fn().mockImplementation(getContentImpl),
      },
    },
  } as never;
}

describe('resolveContentWithBranchFallback', () => {
  beforeEach(() => {
    mockResolveDefaultBranch.mockClear();
  });

  it('resolves branch from default when none is specified', async () => {
    mockResolveDefaultBranch.mockResolvedValue('main');
    const octokit = makeOctokit(async () => ({
      data: [{ name: 'src', path: 'src', type: 'dir', sha: '1', url: '', html_url: '', git_url: '' }],
      headers: {},
    }));

    const result = await resolveContentWithBranchFallback(
      octokit, 'facebook', 'react', '', undefined
    );
    expect((result as { workingBranch: string }).workingBranch).toBe('main');
    expect((result as { repoDefaultBranch: string }).repoDefaultBranch).toBe('main');
  });

  it('uses the provided branch directly (no resolveDefaultBranch call)', async () => {
    mockResolveDefaultBranch.mockResolvedValue('should-not-use');
    const octokit = makeOctokit(async () => ({
      data: [],
      headers: {},
    }));

    const result = await resolveContentWithBranchFallback(
      octokit, 'facebook', 'react', 'src', 'my-branch'
    );
    expect((result as { workingBranch: string }).workingBranch).toBe('my-branch');
    // repoDefaultBranch should be absent since branch was pinned
    expect((result as Record<string, unknown>).repoDefaultBranch).toBeUndefined();
    expect(mockResolveDefaultBranch).not.toHaveBeenCalled();
  });

  it('returns an error when resolveDefaultBranch throws', async () => {
    mockResolveDefaultBranch.mockRejectedValue(
      Object.assign(new Error('Not Found'), { status: 404 })
    );
    const octokit = makeOctokit(async () => ({}));

    const result = await resolveContentWithBranchFallback(
      octokit, 'facebook', 'badrepo', '', undefined
    );
    expect((result as { error: string }).error).toBeDefined();
  });

  it('returns notModified:true on HTTP 304', async () => {
    mockResolveDefaultBranch.mockResolvedValue('main');
    const req304 = new RequestError('Not Modified', 304, {
      request: { method: 'GET', url: 'https://api.github.com/x', headers: {} },
      response: { status: 304, url: 'https://api.github.com/x', headers: {}, data: {} },
    });

    const octokit = makeOctokit(async () => { throw req304; });

    const result = await resolveContentWithBranchFallback(
      octokit, 'facebook', 'react', '', undefined, undefined, '"etag-123"'
    );
    expect((result as { notModified: boolean }).notModified).toBe(true);
    expect((result as { data: unknown }).data).toBeNull();
  });

  it('returns a 404 path-not-found error on HTTP 404', async () => {
    mockResolveDefaultBranch.mockResolvedValue('main');
    const req404 = new RequestError('Not Found', 404, {
      request: { method: 'GET', url: 'https://api.github.com/x', headers: {} },
      response: { status: 404, url: 'https://api.github.com/x', headers: {}, data: {} },
    });

    const octokit = makeOctokit(async () => { throw req404; });

    const result = await resolveContentWithBranchFallback(
      octokit, 'facebook', 'react', 'nonexistent/path', undefined
    );
    expect((result as { error: string }).error).toMatch(/nonexistent\/path|not found/i);
  });

  it('returns access-failed error on non-404 API error', async () => {
    mockResolveDefaultBranch.mockResolvedValue('main');
    const req403 = new RequestError('Forbidden', 403, {
      request: { method: 'GET', url: 'https://api.github.com/x', headers: {} },
      response: { status: 403, url: 'https://api.github.com/x', headers: {}, data: {} },
    });

    const octokit = makeOctokit(async () => { throw req403; });

    const result = await resolveContentWithBranchFallback(
      octokit, 'facebook', 'react', '', undefined
    );
    expect((result as { error: string }).error).toBeDefined();
    expect((result as { status: number }).status).toBe(403);
  });

  it('includes etag in result when returned in response headers', async () => {
    mockResolveDefaultBranch.mockResolvedValue('main');
    const octokit = makeOctokit(async () => ({
      data: [],
      headers: { etag: '"v1"' },
    }));

    const result = await resolveContentWithBranchFallback(
      octokit, 'facebook', 'react', '', undefined
    );
    expect((result as { etag: string }).etag).toBe('"v1"');
  });
});
