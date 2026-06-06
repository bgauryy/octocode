import { describe, it, expect, vi, beforeEach } from 'vitest';
import { searchGitHubCodeAPI } from '../../src/github/codeSearch.js';
import { getOctokit } from '../../src/github/client.js';
import { clearAllCache } from '../../src/utils/http/cache.js';

vi.mock('../../src/github/client.js');
vi.mock('../../src/session.js', () => ({
  logSessionError: vi.fn(() => Promise.resolve()),
}));

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
      mockOctokit as unknown as ReturnType<typeof getOctokit>
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
      keywordsToSearch: ['test'],
    });

    expect(result.status).toBe(200);
    if ('data' in result) {
      expect(result.data.total_count).toBe(1234);
      expect(result.data.items.length).toBe(1);
    }
  });
});
