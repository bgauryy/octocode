import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const fixtureExecution = vi.hoisted(() => vi.fn());

vi.mock('../../src/cacheMaintenance.js', () => ({
  runCacheMaintenanceIfDue: vi.fn().mockResolvedValue(undefined),
}));
vi.mock('../../src/providers/factory.js', () => ({
  initializeProviders: vi.fn().mockResolvedValue([]),
}));
vi.mock('../../src/tools/github_clone_repo/execution.js', () => ({
  executeCloneRepo: (input: unknown) => fixtureExecution('ghCloneRepo', input),
}));
vi.mock('../../src/tools/github_fetch_content/execution.js', () => ({
  fetchMultipleGitHubFileContents: (input: unknown) =>
    fixtureExecution('ghGetFileContent', input),
}));
vi.mock('../../src/tools/github_search/execution.js', () => ({
  executeGitHubSearch: (input: unknown) => fixtureExecution('ghSearch', input),
}));
vi.mock(
  '../../src/tools/github_search_pull_requests/historyExecutions.js',
  () => ({
    searchMultipleGitHubHistory: (input: unknown) =>
      fixtureExecution('ghSearchHistory', input),
    getMultipleGitHubHistoryItems: (input: unknown) =>
      fixtureExecution('ghGetHistoryItem', input),
  })
);
vi.mock('../../src/tools/package_search/execution.js', () => ({
  searchPackages: (input: unknown) => fixtureExecution('npmSearch', input),
}));
vi.mock('../../src/tools/local_fetch_content/execution.js', () => ({
  executeFetchContent: (input: unknown) =>
    fixtureExecution('localGetFileContent', input),
}));
vi.mock('../../src/tools/local_analyze_graph/execution.js', () => ({
  executeAnalyzeGraph: (input: unknown) =>
    fixtureExecution('localAnalyzeGraph', input),
}));
vi.mock('../../src/tools/local_search/execution.js', () => ({
  executeLocalSearch: (input: unknown) =>
    fixtureExecution('localSearch', input),
}));
vi.mock('../../src/tools/lsp/semantic_content/execution.js', () => ({
  executeLspGetSemantics: (input: unknown) =>
    fixtureExecution('lspGetSemantics', input),
}));

import { DIRECT_TOOL_SPECIFICATIONS } from '../../src/tools/directToolCatalog/toolSpecifications.js';
import {
  _overrideInitialize,
  _resetInitialize,
  executeDirectTool,
} from '../../src/tools/directToolCatalog.exec.js';
import { cleanup } from '../../src/serverConfig.js';
import {
  ADAPTER_PARITY_CASES,
  CANONICAL_ADAPTER_TOOL_NAMES,
  createAdapterParityErrorResult,
  createAdapterParityPageResult,
  getAdapterParityContinuation,
} from '../fixtures/adapterParityFixture.js';

describe('canonical adapter parity fixture', () => {
  beforeEach(() => {
    process.env.ENABLE_LOCAL = 'true';
    delete process.env.TOOLS_TO_RUN;
    delete process.env.DISABLE_TOOLS;
    _overrideInitialize(async () => undefined);
    const invocationCount = new Map<string, number>();
    fixtureExecution.mockImplementation(async (name: string) => {
      const count = (invocationCount.get(name) ?? 0) + 1;
      invocationCount.set(name, count);
      const testCase = ADAPTER_PARITY_CASES.find(item => item.name === name)!;
      if (count === 1) return createAdapterParityPageResult(testCase, 1);
      if (count === 2) return createAdapterParityPageResult(testCase, 2);
      return createAdapterParityErrorResult(testCase);
    });
  });

  afterEach(() => {
    delete process.env.ENABLE_LOCAL;
    delete process.env.TOOLS_TO_RUN;
    delete process.env.DISABLE_TOOLS;
    fixtureExecution.mockReset();
    _resetInitialize();
    cleanup();
  });

  it('covers exactly the ten tools owned by the direct-tool specification', () => {
    expect(CANONICAL_ADAPTER_TOOL_NAMES).toEqual(
      DIRECT_TOOL_SPECIFICATIONS.map(tool => tool.name)
    );
    expect(ADAPTER_PARITY_CASES.map(testCase => testCase.name)).toEqual(
      CANONICAL_ADAPTER_TOOL_NAMES
    );
  });

  it.each(ADAPTER_PARITY_CASES)(
    '$name preserves schema, result, row error, outer error, and continuation contracts',
    async testCase => {
      const specification = DIRECT_TOOL_SPECIFICATIONS.find(
        tool => tool.name === testCase.name
      )!;
      expect(specification.inputSchema.safeParse(testCase.input).success).toBe(
        true
      );

      const first = await executeDirectTool(testCase.name, testCase.input);
      expect(first).toEqual(createAdapterParityPageResult(testCase, 1));
      expect(first.structuredContent).toMatchObject({
        results: [
          { index: 0 },
          {
            index: 1,
            status: 'error',
            data: { error: { code: 'ADAPTER_FIXTURE_ROW_ERROR' } },
          },
        ],
      });

      const next = getAdapterParityContinuation(first);
      expect(next.tool).toBe(testCase.name);
      const continuationInput = { queries: [next.query] };
      expect(
        specification.inputSchema.safeParse(continuationInput).success
      ).toBe(true);
      await expect(
        executeDirectTool(next.tool, continuationInput)
      ).resolves.toEqual(createAdapterParityPageResult(testCase, 2));

      await expect(
        executeDirectTool(testCase.name, testCase.input)
      ).resolves.toEqual(createAdapterParityErrorResult(testCase));
    }
  );
});
