import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  fetchContent: vi.fn(),
  searchContentRipgrep: vi.fn(),
  cloneRepo: vi.fn(),
}));

vi.mock('@octocodeai/octocode-tools-core', () => ({
  executeBulkOperation: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: '' }],
    isError: false,
  }),
}));

vi.mock('@octocodeai/octocode-tools-core', () => ({
  fetchContent: mocks.fetchContent,
}));

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

vi.mock('@octocodeai/octocode-tools-core', () => ({
  LocalRipgrepQuerySchema: {
    safeParse: vi.fn().mockReturnValue({ success: true, data: {} }),
  },
}));

vi.mock('@octocodeai/octocode-tools-core', () => ({
  LocalFindFilesQuerySchema: {
    safeParse: vi.fn().mockReturnValue({ success: true, data: {} }),
  },
}));

vi.mock('@octocodeai/octocode-tools-core', () => ({
  LocalViewStructureQuerySchema: {
    safeParse: vi.fn().mockReturnValue({ success: true, data: {} }),
  },
}));

vi.mock('@octocodeai/octocode-tools-core', () => ({
  LocalFetchContentQuerySchema: {
    safeParse: vi.fn().mockReturnValue({ success: true, data: {} }),
  },
}));

vi.mock('@octocodeai/octocode-tools-core', () => ({
  searchContentRipgrep: mocks.searchContentRipgrep,
}));

vi.mock('@octocodeai/octocode-tools-core', () => ({
  cloneRepo: mocks.cloneRepo,
}));

vi.mock('@octocodeai/octocode-tools-core', () => ({
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
      await import('@octocodeai/octocode-tools-core');
    const { executeFetchContent } =
      await import('@octocodeai/octocode-tools-core');

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
      await import('@octocodeai/octocode-tools-core');
    const { executeRipgrepSearch } =
      await import('@octocodeai/octocode-tools-core');

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
      await import('@octocodeai/octocode-tools-core');
    const { executeCloneRepo } =
      await import('@octocodeai/octocode-tools-core');

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
