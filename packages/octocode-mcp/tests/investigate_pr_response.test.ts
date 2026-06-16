import { describe, it, expect, vi, beforeEach } from 'vitest';
import { searchGitHubPullRequestsAPI } from '../../octocode-tools-core/src/github/pullRequestSearch';
import * as client from '../../octocode-tools-core/src/github/client';

vi.mock('../../octocode-tools-core/src/github/client', () => ({
  getOctokit: vi.fn(),
  OctokitWithThrottling: class {},
}));

describe('GitHub PR Search Tool Refactor (Mocked)', () => {
  const mockOctokit = {
    rest: {
      pulls: {
        get: vi.fn(),
        listFiles: vi.fn(),
      },
      issues: {
        listComments: vi.fn(),
      },
    },
  };

  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(client.getOctokit).mockResolvedValue(
      mockOctokit as unknown as Awaited<ReturnType<typeof client.getOctokit>>
    );
  });

  const mockPRData = {
    number: 35234,
    title: 'Test PR',
    html_url: 'https://github.com/facebook/react/pull/35234',
    state: 'closed',
    user: { login: 'eps1lon' },
    created_at: '2023-01-01T00:00:00Z',
    updated_at: '2023-01-02T00:00:00Z',
    head: { ref: 'feature', sha: 'abc', repo: { full_name: 'facebook/react' } },
    base: { ref: 'main', sha: 'def', repo: { full_name: 'facebook/react' } },
    body: 'Test Body',
  };

  const mockFilesData = [
    {
      filename:
        'packages/react-devtools-shared/src/__tests__/storeComponentFilters-test.js',
      status: 'modified',
      additions: 1,
      deletions: 1,
      changes: 2,
      patch:
        "@@ -753,7 +753,7 @@ describe('Store component filters', () => {\n      });\n    });\n  \n-  // @reactVersion >= 16.6\n+  // @reactVersion >= 18.0\n    it('resets forced error and fallback states when filters are changed', async () => {\n      store.componentFilters = [];\n      class ErrorBoundary extends React.Component {",
    },
    {
      filename: 'other/file.js',
      status: 'added',
      additions: 10,
      deletions: 0,
      changes: 10,
      patch: '@@ -0,0 +1,10 @@\n+New File Content',
    },
  ];

  it('should support type="metadata" and return file list without patch', async () => {
    mockOctokit.rest.pulls.get.mockResolvedValue({ data: mockPRData });
    mockOctokit.rest.pulls.listFiles.mockResolvedValue({ data: mockFilesData });

    const result = await searchGitHubPullRequestsAPI({
      owner: 'facebook',
      repo: 'react',
      prNumber: 35234,
      content: { changedFiles: true },
    });

    expect(result.error).toBeUndefined();
    expect(result.pull_requests).toHaveLength(1);
    const pr = result.pull_requests![0]!;

    expect(pr.file_changes).toBeDefined();
    expect(pr.file_changes).toHaveLength(2);
    pr.file_changes?.forEach(file => {
      expect(file.patch).toBeUndefined();
    });
  });

  it('should support type="fullContent" and return file list WITH patch', async () => {
    mockOctokit.rest.pulls.get.mockResolvedValue({ data: mockPRData });
    mockOctokit.rest.pulls.listFiles.mockResolvedValue({ data: mockFilesData });

    const result = await searchGitHubPullRequestsAPI({
      owner: 'facebook',
      repo: 'react',
      prNumber: 35234,
      content: { changedFiles: true, patches: { mode: 'all' } },
    });

    expect(result.error).toBeUndefined();
    expect(result.pull_requests).toHaveLength(1);
    const pr = result.pull_requests![0]!;

    expect(pr.file_changes).toBeDefined();
    expect(pr.file_changes).toHaveLength(2);
    expect(pr.file_changes![0]!.patch).toBeDefined();
    expect(pr.file_changes![1]!.patch).toBeDefined();
  });

  it('should support selected patch content and return filtered patch', async () => {
    mockOctokit.rest.pulls.get.mockResolvedValue({ data: mockPRData });
    mockOctokit.rest.pulls.listFiles.mockResolvedValue({ data: mockFilesData });

    const filename =
      'packages/react-devtools-shared/src/__tests__/storeComponentFilters-test.js';

    const result = await searchGitHubPullRequestsAPI({
      owner: 'facebook',
      repo: 'react',
      prNumber: 35234,
      content: {
        patches: {
          mode: 'selected',
          ranges: [
            {
              file: filename,
              additions: [756],
            },
          ],
        },
      },
    });

    expect(result.error).toBeUndefined();
    expect(result.pull_requests).toHaveLength(1);
    const pr = result.pull_requests![0]!;

    expect(pr.file_changes).toBeDefined();
    expect(pr.file_changes).toHaveLength(1);
    expect(pr.file_changes![0]!.filename).toBe(filename);

    const patch = pr.file_changes![0]!.patch;
    expect(patch).toBeDefined();

    expect(patch).toContain('+756:');
    expect(patch).toContain('@reactVersion >= 18.0');
  });
});
