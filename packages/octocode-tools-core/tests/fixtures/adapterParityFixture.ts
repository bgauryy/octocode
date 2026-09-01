/**
 * Deterministic, network-free contract shared by the tools-core, CLI, and MCP
 * adapter tests. Every query is intentionally a valid public query; execution
 * is supplied by each adapter's test harness.
 */
export const CANONICAL_ADAPTER_TOOL_NAMES = [
  'ghSearch',
  'ghGetFileContent',
  'ghSearchHistory',
  'ghGetHistoryItem',
  'npmSearch',
  'ghCloneRepo',
  'localSearch',
  'localAnalyzeGraph',
  'localGetFileContent',
  'lspGetSemantics',
] as const;

export type CanonicalAdapterToolName =
  (typeof CANONICAL_ADAPTER_TOOL_NAMES)[number];

type AdapterParityQuery = Record<string, unknown>;

export interface AdapterParityCase {
  name: CanonicalAdapterToolName;
  query: AdapterParityQuery;
  input: { queries: [AdapterParityQuery, AdapterParityQuery] };
}

const CASE_QUERIES: Readonly<
  Record<CanonicalAdapterToolName, AdapterParityQuery>
> = {
  ghSearch: {
    operation: 'code',
    keywords: ['adapter-parity'],
    owner: 'fixture-owner',
    repo: 'fixture-repo',
    pageSize: 1,
  },
  ghGetFileContent: {
    owner: 'fixture-owner',
    repo: 'fixture-repo',
    path: 'README.md',
    startLine: 1,
    endLine: 1,
    minify: 'none',
  },
  ghSearchHistory: {
    operation: 'commits',
    owner: 'fixture-owner',
    repo: 'fixture-repo',
    path: 'README.md',
    pageSize: 1,
  },
  ghGetHistoryItem: {
    operation: 'commit',
    owner: 'fixture-owner',
    repo: 'fixture-repo',
    ref: 'fixture-ref',
  },
  npmSearch: { packageName: 'fixture-package' },
  ghCloneRepo: {
    owner: 'fixture-owner',
    repo: 'fixture-repo',
    sparsePath: 'src',
  },
  localSearch: {
    operation: 'text',
    path: '/adapter-parity/fixture',
    searchText: 'fixture-token',
    regex: 'fixed',
    pageSize: 1,
  },
  localAnalyzeGraph: {
    operation: 'dependencies',
    path: '/adapter-parity/fixture',
    file: 'src/index.ts',
    depth: 1,
  },
  localGetFileContent: {
    path: '/adapter-parity/fixture.ts',
    startLine: 1,
    endLine: 1,
    minify: 'none',
  },
  lspGetSemantics: {
    type: 'documentSymbols',
    uri: '/adapter-parity/fixture.ts',
  },
};

export const ADAPTER_PARITY_CASES: readonly AdapterParityCase[] =
  CANONICAL_ADAPTER_TOOL_NAMES.map(name => {
    const query = CASE_QUERIES[name];
    return { name, query, input: { queries: [query, query] } };
  });

export interface AdapterParityResult {
  [key: string]: unknown;
  content: Array<{ type: 'text'; text: string }>;
  structuredContent: Record<string, unknown>;
  isError?: boolean;
}

export function createAdapterParityPageResult(
  testCase: AdapterParityCase,
  page: 1 | 2
): AdapterParityResult {
  const hasMore = page === 1;
  const results: Array<Record<string, unknown>> = [
    {
      index: 0,
      meta: { evidence: { kind: 'exact', confidence: 'high' } },
      data: {
        fixture: `${testCase.name}:page:${page}`,
        pagination: {
          currentPage: page,
          totalPages: 2,
          hasMore,
          ...(hasMore ? { nextPage: 2 } : {}),
        },
        ...(hasMore
          ? {
              next: {
                tool: testCase.name,
                query: testCase.query,
              },
            }
          : {}),
      },
    },
  ];

  if (page === 1) {
    results.push({
      index: 1,
      status: 'error',
      meta: {
        evidence: { kind: 'exact', confidence: 'high' },
        diagnostics: { codes: ['ADAPTER_FIXTURE_ROW_ERROR'] },
      },
      data: {
        error: {
          code: 'ADAPTER_FIXTURE_ROW_ERROR',
          message: `fixture row error for ${testCase.name}`,
        },
      },
    });
  }

  return {
    content: [
      {
        type: 'text',
        text: `adapter-parity:${testCase.name}:page:${page}`,
      },
    ],
    structuredContent: {
      fixtureVersion: 1,
      tool: testCase.name,
      results,
    },
  };
}

export function createAdapterParityErrorResult(
  testCase: AdapterParityCase
): AdapterParityResult {
  const message = `adapter parity fixture failure for ${testCase.name}`;
  return {
    content: [{ type: 'text', text: message }],
    structuredContent: {
      fixtureVersion: 1,
      results: [],
      status: 'error',
      tool: testCase.name,
      code: 'ADAPTER_FIXTURE_ERROR',
      error: { name: 'Error', message, code: 'ADAPTER_FIXTURE_ERROR' },
    },
    isError: true,
  };
}

export function getAdapterParityContinuation(result: {
  structuredContent?: unknown;
}): {
  tool: CanonicalAdapterToolName;
  query: AdapterParityQuery;
} {
  const structured = result.structuredContent as {
    results?: Array<{ data?: { next?: unknown } }>;
  };
  const next = structured.results?.[0]?.data?.next;
  if (!next || typeof next !== 'object') {
    throw new Error('adapter parity result did not expose a continuation');
  }
  return next as {
    tool: CanonicalAdapterToolName;
    query: AdapterParityQuery;
  };
}
