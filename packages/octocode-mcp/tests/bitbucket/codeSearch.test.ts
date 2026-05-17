import { describe, it, expect, vi, beforeEach } from 'vitest';
import { clearAllCache } from '../../src/utils/http/cache.js';

const mockGET = vi.fn();
vi.mock('../../src/bitbucket/client.js', () => ({
  getBitbucketClient: vi.fn(() => ({ GET: mockGET })),
  getAuthHeader: vi.fn(() => 'Bearer test-token'),
}));

vi.mock('../../src/bitbucketConfig.js', () => ({
  getBitbucketHost: vi.fn(() => 'https://api.bitbucket.org/2.0'),
  getBitbucketToken: vi.fn(() => 'test-token'),
  getBitbucketUsername: vi.fn(() => null),
}));

import { searchBitbucketCodeAPI } from '../../src/bitbucket/codeSearch.js';

describe('searchBitbucketCodeAPI', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
  });

  it('should return error when workspace is missing', async () => {
    const result = await searchBitbucketCodeAPI({
      workspace: '',
      searchQuery: 'hello',
    });
    expect(result).toHaveProperty('error');
    expect((result as { status: number }).status).toBe(400);
  });

  it('should return error when searchQuery is empty', async () => {
    const result = await searchBitbucketCodeAPI({
      workspace: 'myws',
      searchQuery: '  ',
    });
    expect(result).toHaveProperty('error');
    expect((result as { status: number }).status).toBe(400);
  });

  it('should return successful search results', async () => {
    mockGET.mockResolvedValue({
      data: {
        values: [
          {
            type: 'code_search_result',
            content_matches: [
              {
                lines: [
                  { line: 10, segments: [{ text: 'import foo', match: true }] },
                ],
              },
            ],
            path_matches: [],
            file: {
              path: 'src/index.ts',
              type: 'commit_file',
              links: {
                self: {
                  href: 'https://bitbucket.org/ws/repo/src/main/src/index.ts',
                },
              },
            },
          },
        ],
        size: 1,
        page: 1,
        next: undefined,
      },
    });

    const result = await searchBitbucketCodeAPI({
      workspace: 'myws',
      searchQuery: 'import foo',
    });

    expect(result).toHaveProperty('data');
    const data = (result as { data: { items: unknown[]; totalCount: number } })
      .data;
    expect(data.items).toHaveLength(1);
    expect(data.items[0]).toHaveProperty('file');
    expect(data.totalCount).toBe(1);
  });

  it('should handle missing response fields gracefully', async () => {
    mockGET.mockResolvedValue({ data: {} });

    const result = await searchBitbucketCodeAPI({
      workspace: 'myws',
      searchQuery: 'test',
    });

    expect(result).toHaveProperty('data');
    const data = (result as { data: { items: unknown[]; totalCount: number } })
      .data;
    expect(data.items).toHaveLength(0);
    expect(data.totalCount).toBe(0);
  });

  it('should filter by repoSlug when provided', async () => {
    mockGET.mockResolvedValue({
      data: {
        values: [
          {
            type: 'code_search_result',
            content_matches: [],
            file: {
              path: 'lib/utils.ts',
              type: 'commit_file',
              links: {
                self: {
                  href: 'https://bb.org/ws/target-repo/src/main/lib/utils.ts',
                },
              },
            },
          },
          {
            type: 'code_search_result',
            content_matches: [],
            file: {
              path: 'lib/other.ts',
              type: 'commit_file',
              links: {
                self: {
                  href: 'https://bb.org/ws/other-repo/src/main/lib/other.ts',
                },
              },
            },
          },
        ],
        size: 2,
        page: 1,
      },
    });

    const result = await searchBitbucketCodeAPI({
      workspace: 'myws',
      repoSlug: 'target-repo',
      searchQuery: 'utils',
    });

    const data = (result as { data: { items: unknown[] } }).data;
    expect(data.items).toHaveLength(1);
  });

  it('should handle repoSlug filtering against Bitbucket API self links', async () => {
    mockGET.mockResolvedValue({
      data: {
        values: [
          {
            type: 'code_search_result',
            content_matches: [],
            file: {
              path: 'lib/utils.ts',
              type: 'commit_file',
              links: {
                self: {
                  href: 'https://api.bitbucket.org/2.0/repositories/ws/target-repo/src/main/lib/utils.ts',
                },
              },
            },
          },
          {
            type: 'code_search_result',
            content_matches: [],
            file: {
              path: 'lib/other.ts',
              type: 'commit_file',
              links: {
                self: {
                  href: 'https://api.bitbucket.org/2.0/repositories/ws/other-repo/src/main/lib/other.ts',
                },
              },
            },
          },
        ],
        size: 2,
        page: 1,
      },
    });

    const result = await searchBitbucketCodeAPI({
      workspace: 'myws',
      repoSlug: 'target-repo',
      searchQuery: 'utils',
    });

    const data = (
      result as { data: { items: Array<{ file: { path: string } }> } }
    ).data;
    expect(data.items).toHaveLength(1);
    expect(data.items[0]!.file.path).toBe('lib/utils.ts');
  });

  it('should use provider-native repo and path modifiers in the search query', async () => {
    mockGET.mockResolvedValue({
      data: { values: [], size: 0 },
    });

    await searchBitbucketCodeAPI({
      workspace: 'myws',
      repoSlug: 'target-repo',
      path: 'src/services',
      searchQuery: 'authenticate',
    });

    expect(mockGET).toHaveBeenCalledWith(
      '/workspaces/{workspace}/search/code',
      expect.objectContaining({
        params: expect.objectContaining({
          query: expect.objectContaining({
            search_query: expect.stringContaining('repo:target-repo'),
          }),
        }),
      })
    );
    expect(mockGET).toHaveBeenCalledWith(
      '/workspaces/{workspace}/search/code',
      expect.objectContaining({
        params: expect.objectContaining({
          query: expect.objectContaining({
            search_query: expect.stringContaining('path:"src/services"'),
          }),
        }),
      })
    );
  });

  it('should handle API errors', async () => {
    mockGET.mockRejectedValue(
      Object.assign(new Error('Not Found'), { status: 404 })
    );

    const result = await searchBitbucketCodeAPI({
      workspace: 'myws',
      searchQuery: 'test',
    });

    expect(result).toHaveProperty('error');
    expect((result as { status: number }).status).toBe(404);
  });

  it('should use default page and limit values', async () => {
    mockGET.mockResolvedValue({
      data: { values: [], size: 0 },
    });

    await searchBitbucketCodeAPI({
      workspace: 'myws',
      searchQuery: 'test',
    });

    expect(mockGET).toHaveBeenCalledWith(
      '/workspaces/{workspace}/search/code',
      expect.objectContaining({
        params: expect.objectContaining({
          query: expect.objectContaining({
            page: 1,
            pagelen: 20,
          }),
        }),
      })
    );
  });

  it('should handle pagination with next URL', async () => {
    mockGET.mockResolvedValue({
      data: {
        values: [
          {
            type: 'code_search_result',
            content_matches: [],
            file: { path: 'a.ts', type: 'commit_file' },
          },
        ],
        size: 50,
        page: 2,
        next: 'https://api.bitbucket.org/2.0/workspaces/ws/search/code?page=3',
      },
    });

    const result = await searchBitbucketCodeAPI({
      workspace: 'myws',
      searchQuery: 'test',
      page: 2,
      limit: 10,
    });

    const data = (
      result as {
        data: { pagination: { hasMore: boolean; totalPages: number } };
      }
    ).data;
    expect(data.pagination.hasMore).toBe(true);
    expect(data.pagination.totalPages).toBe(5);
  });

  it('should handle items with empty content_matches and path_matches', async () => {
    mockGET.mockResolvedValue({
      data: {
        values: [
          {
            type: 'code_search_result',
            file: { path: 'empty.ts', type: 'commit_file' },
          },
        ],
        size: 1,
      },
    });

    const result = await searchBitbucketCodeAPI({
      workspace: 'myws',
      searchQuery: 'test',
    });

    const data = (
      result as {
        data: {
          items: Array<{ content_matches: unknown[]; path_matches: undefined }>;
        };
      }
    ).data;
    expect(data.items[0]!.content_matches).toEqual([]);
    expect(data.items[0]!.path_matches).toBeUndefined();
  });

  function makeItem(filePath: string) {
    return {
      type: 'code_search_result',
      content_matches: [],
      file: { path: filePath, type: 'commit_file' },
    };
  }

  it('should filter by path prefix', async () => {
    mockGET.mockResolvedValue({
      data: {
        values: [makeItem('src/index.ts'), makeItem('lib/utils.ts')],
        size: 2,
        page: 1,
      },
    });
    const result = await searchBitbucketCodeAPI({
      workspace: 'myws',
      searchQuery: 'test',
      path: 'src/',
    });
    const data = (result as { data: { items: unknown[] } }).data;
    expect(data.items).toHaveLength(1);
  });

  it('should filter by filename', async () => {
    mockGET.mockResolvedValue({
      data: {
        values: [makeItem('src/index.ts'), makeItem('src/utils.ts')],
        size: 2,
        page: 1,
      },
    });
    const result = await searchBitbucketCodeAPI({
      workspace: 'myws',
      searchQuery: 'test',
      filename: 'index.ts',
    });
    const data = (result as { data: { items: unknown[] } }).data;
    expect(data.items).toHaveLength(1);
  });

  it('should filter by extension', async () => {
    mockGET.mockResolvedValue({
      data: {
        values: [makeItem('src/index.ts'), makeItem('src/utils.js')],
        size: 2,
        page: 1,
      },
    });
    const result = await searchBitbucketCodeAPI({
      workspace: 'myws',
      searchQuery: 'test',
      extension: 'ts',
    });
    const data = (result as { data: { items: unknown[] } }).data;
    expect(data.items).toHaveLength(1);
  });

  it('should return 502 error when API response has unexpected shape (null data)', async () => {
    mockGET.mockResolvedValue({ data: null });
    const result = await searchBitbucketCodeAPI({
      workspace: 'myws',
      searchQuery: 'test',
    });
    expect(result).toHaveProperty('error');
    expect((result as { status: number }).status).toBe(502);
  });
});
