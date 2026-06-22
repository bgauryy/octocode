import { describe, it, expect, vi, beforeEach } from 'vitest';
import { searchGitHubCodeAPI } from '../../../octocode-tools-core/src/github/codeSearch.js';
import { getOctokit } from '../../../octocode-tools-core/src/github/client.js';
import { clearAllCache } from '../../../octocode-tools-core/src/utils/http/cache.js';

vi.mock('../../../octocode-tools-core/src/github/client.js');
describe('Code Search - Total Count', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
  });

  it('should return original total_count from API even if items are filtered or paginated', async () => {
    const searchCodeMock = vi.fn();

    const mockOctokit = {
      rest: {
        search: {
          code: searchCodeMock,
        },
      },
    };

    vi.mocked(getOctokit).mockResolvedValue(
      mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
    );

    searchCodeMock.mockResolvedValue({
      data: {
        total_count: 1234,
        items: [
          {
            name: 'file1.ts',
            path: 'src/file1.ts',
            repository: {
              full_name: 'test/repo',
              url: 'repo_url',
              owner: { login: 'test' },
            },
            url: 'file_url',
            html_url: 'html_url',
            sha: 'sha',
          },
        ],
        incomplete_results: false,
      },
      headers: {},
    });

    const result = await searchGitHubCodeAPI({
      keywords: ['test'],
    });

    expect(result.status).toBe(200);
    if ('data' in result) {
      expect(result.data.total_count).toBe(1234);
      expect(result.data.items.length).toBe(1);
    }
  });
});
