import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  fetchContent: vi.fn(),
  searchContentRipgrep: vi.fn(),
  cloneRepo: vi.fn(),
}));

vi.mock('../../../octocode-tools-core/src/utils/response/bulk.js', () => ({
  executeBulkOperation: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: '' }],
    isError: false,
  }),
}));

vi.mock(
  '../../../octocode-tools-core/src/tools/local_fetch_content/fetchContent.js',
  () => ({
    fetchContent: mocks.fetchContent,
  })
);

vi.mock('@octocodeai/octocode-core', () => ({
  FetchContentQuerySchema: {
    safeParse: vi.fn().mockReturnValue({ success: true }),
  },
  RipgrepQuerySchema: {
    safeParse: vi.fn().mockReturnValue({ success: true }),
  },
  completeMetadata: {
    instructions: '',
    prompts: {},
    toolNames: {},
    baseSchema: {
      mainResearchGoal: '',
      researchGoal: '',
      reasoning: '',
      bulkQuery: () => '',
    },
    tools: {},
    baseHints: { hasResults: [], empty: [] },
    genericErrorHints: [],
    bulkOperations: {},
  },
}));

vi.mock(
  '../../../octocode-tools-core/src/tools/local_ripgrep/scheme.js',
  () => ({
    LocalRipgrepQuerySchema: {
      safeParse: vi.fn().mockReturnValue({ success: true, data: {} }),
    },
  })
);

vi.mock(
  '../../../octocode-tools-core/src/tools/local_find_files/scheme.js',
  () => ({
    LocalFindFilesQuerySchema: {
      safeParse: vi.fn().mockReturnValue({ success: true, data: {} }),
    },
  })
);

vi.mock(
  '../../../octocode-tools-core/src/tools/local_view_structure/scheme.js',
  () => ({
    LocalViewStructureQuerySchema: {
      safeParse: vi.fn().mockReturnValue({ success: true, data: {} }),
    },
  })
);

vi.mock(
  '../../../octocode-tools-core/src/tools/local_fetch_content/scheme.js',
  () => ({
    LocalFetchContentQuerySchema: {
      safeParse: vi.fn().mockReturnValue({ success: true, data: {} }),
    },
  })
);

vi.mock(
  '../../../octocode-tools-core/src/tools/local_ripgrep/searchContentRipgrep.js',
  () => ({
    searchContentRipgrep: mocks.searchContentRipgrep,
  })
);

vi.mock(
  '../../../octocode-tools-core/src/tools/github_clone_repo/cloneRepo.js',
  () => ({
    cloneRepo: mocks.cloneRepo,
  })
);

vi.mock('../../../octocode-tools-core/src/tools/providerExecution.js', () => ({
  createProviderExecutionContext: vi.fn().mockReturnValue({
    providerType: 'github',
    token: 'test-token',
  }),
  createLazyProviderContext: vi.fn(() =>
    vi.fn().mockReturnValue({
      providerType: 'github',
      token: 'test-token',
    })
  ),
  providerSupports: vi.fn().mockReturnValue(true),
}));

describe('Execution boundary guards in target RFC flows', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('returns structured error when local_fetch_content callback throws', async () => {
    const { executeBulkOperation } =
      await import('../../../octocode-tools-core/src/utils/response/bulk.js');
    const { executeFetchContent } =
      await import('../../../octocode-tools-core/src/tools/local_fetch_content/execution.js');

    mocks.fetchContent.mockRejectedValueOnce(new Error('fetch failed'));

    await executeFetchContent({ queries: [{ path: '/tmp/a.ts' }] as any });

    const callback = vi.mocked(executeBulkOperation).mock.calls[0]![1] as (
      query: unknown,
      index: number
    ) => Promise<{ status: string }>;

    const result = await callback({ path: '/tmp/a.ts' }, 0);
    expect(result.status).toBe('error');
  });

  it('returns structured error when local_ripgrep callback throws', async () => {
    const { executeBulkOperation } =
      await import('../../../octocode-tools-core/src/utils/response/bulk.js');
    const { executeRipgrepSearch } =
      await import('../../../octocode-tools-core/src/tools/local_ripgrep/execution.js');

    mocks.searchContentRipgrep.mockRejectedValueOnce(
      new Error('ripgrep failed')
    );

    await executeRipgrepSearch({
      queries: [{ path: '/tmp', keywords: 'x' }] as any,
    });

    const callback = vi.mocked(executeBulkOperation).mock.calls[0]![1] as (
      query: unknown,
      index: number
    ) => Promise<{ status: string }>;

    const result = await callback({ path: '/tmp', keywords: 'x' }, 0);
    expect(result.status).toBe('error');
  });

  it('returns structured error when github_clone_repo callback throws', async () => {
    const { executeBulkOperation } =
      await import('../../../octocode-tools-core/src/utils/response/bulk.js');
    const { executeCloneRepo } =
      await import('../../../octocode-tools-core/src/tools/github_clone_repo/execution.js');

    mocks.cloneRepo.mockRejectedValueOnce(new Error('clone failed'));

    await executeCloneRepo({
      authInfo: { provider: 'github' } as any,
      queries: [{ owner: 'octocat', repo: 'hello-world' }] as any,
    });

    const callback = vi.mocked(executeBulkOperation).mock.calls[0]![1] as (
      query: unknown,
      index: number
    ) => Promise<{ status: string }>;

    const result = await callback({ owner: 'octocat', repo: 'hello-world' }, 0);
    expect(result.status).toBe('error');
  });
});
