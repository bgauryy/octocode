import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  fetchContent: vi.fn(),
  searchContentRipgrep: vi.fn(),
  processCallHierarchy: vi.fn(),
  cloneRepo: vi.fn(),
}));

vi.mock('../../src/utils/response/bulk.js', () => ({
  executeBulkOperation: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: '' }],
    isError: false,
  }),
}));

vi.mock('../../src/tools/local_fetch_content/fetchContent.js', () => ({
  fetchContent: mocks.fetchContent,
}));

const minimalCompleteMetadata = {
  toolNames: {},
  tools: {},
  baseSchema: {
    mainResearchGoal: '',
    researchGoal: '',
    reasoning: '',
    bulkQuery: () => '',
  },
  baseHints: { hasResults: [], empty: [] },
  genericErrorHints: [],
  instructions: '',
  prompts: {},
  bulkOperations: {},
};

vi.mock('@octocodeai/octocode-core', () => ({
  FetchContentQuerySchema: {
    safeParse: vi.fn().mockReturnValue({ success: true }),
  },
  RipgrepQuerySchema: {
    safeParse: vi.fn().mockReturnValue({ success: true }),
  },
  completeMetadata: minimalCompleteMetadata,
}));

vi.mock('../../src/scheme/localSchemaOverlay.js', () => ({
  RipgrepQuerySchema: {
    safeParse: vi.fn().mockReturnValue({ success: true, data: {} }),
  },
  FindFilesQuerySchema: {
    safeParse: vi.fn().mockReturnValue({ success: true, data: {} }),
  },
  ViewStructureQuerySchema: {
    safeParse: vi.fn().mockReturnValue({ success: true, data: {} }),
  },
}));

vi.mock('../../src/tools/local_ripgrep/searchContentRipgrep.js', () => ({
  searchContentRipgrep: mocks.searchContentRipgrep,
}));

vi.mock('../../src/tools/lsp_call_hierarchy/callHierarchy.js', () => ({
  processCallHierarchy: mocks.processCallHierarchy,
}));

vi.mock('../../src/tools/github_clone_repo/cloneRepo.js', () => ({
  cloneRepo: mocks.cloneRepo,
}));

vi.mock('../../src/tools/providerExecution.js', () => ({
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
      await import('../../src/utils/response/bulk.js');
    const { executeFetchContent } =
      await import('../../src/tools/local_fetch_content/execution.js');

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
      await import('../../src/utils/response/bulk.js');
    const { executeRipgrepSearch } =
      await import('../../src/tools/local_ripgrep/execution.js');

    mocks.searchContentRipgrep.mockRejectedValueOnce(
      new Error('ripgrep failed')
    );

    await executeRipgrepSearch({
      queries: [{ path: '/tmp', pattern: 'x' }] as any,
    });

    const callback = vi.mocked(executeBulkOperation).mock.calls[0]![1] as (
      query: unknown,
      index: number
    ) => Promise<{ status: string }>;

    const result = await callback({ path: '/tmp', pattern: 'x' }, 0);
    expect(result.status).toBe('error');
  });

  it('returns structured error when lsp_call_hierarchy callback throws', async () => {
    const { executeBulkOperation } =
      await import('../../src/utils/response/bulk.js');
    const { executeCallHierarchy } =
      await import('../../src/tools/lsp_call_hierarchy/execution.js');

    mocks.processCallHierarchy.mockRejectedValueOnce(new Error('lsp failed'));

    await executeCallHierarchy({ queries: [{ uri: '/tmp/a.ts' }] as any });

    const callback = vi.mocked(executeBulkOperation).mock.calls[0]![1] as (
      query: unknown,
      index: number
    ) => Promise<{ status: string }>;

    const result = await callback({ uri: '/tmp/a.ts' }, 0);
    expect(result.status).toBe('error');
  });

  it('returns structured error when github_clone_repo callback throws', async () => {
    const { executeBulkOperation } =
      await import('../../src/utils/response/bulk.js');
    const { executeCloneRepo } =
      await import('../../src/tools/github_clone_repo/execution.js');

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
