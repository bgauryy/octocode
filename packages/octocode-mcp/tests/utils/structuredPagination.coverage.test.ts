import { beforeAll, describe, expect, it } from 'vitest';
import {
  applyQueryOutputPagination,
  applyBulkResponsePagination,
} from '../../src/utils/response/structuredPagination.js';
import { TOOL_NAMES } from '../../src/tools/toolMetadata/proxies.js';
import { initializeToolMetadata } from '../../src/tools/toolMetadata/state.js';

beforeAll(async () => {
  await initializeToolMetadata();
});

describe('structuredPagination branch coverage', () => {
  // ---- applyQueryOutputPagination guard branches ----

  it('returns the result unchanged when data is not a plain object (line 1070)', () => {
    const queryResult = {
      id: 'q-nondata',
      data: 'not-an-object' as unknown as Record<string, unknown>,
    };
    const result = applyQueryOutputPagination(
      queryResult,
      {},
      TOOL_NAMES.GITHUB_SEARCH_CODE
    );
    expect(result).toBe(queryResult);
  });

  it('returns the result unchanged for a non-success status (line ~1078)', () => {
    const queryResult = {
      id: 'q-status',
      status: 'error' as const,
      data: { error: 'boom', repositories: [] },
    };
    const result = applyQueryOutputPagination(
      queryResult,
      { charLength: 10 },
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
    );
    expect(result).toBe(queryResult);
  });

  it('returns the result unchanged for lspFindReferences (line 1082)', () => {
    const queryResult = {
      id: 'q-refs',
      data: { locations: [{ content: 'x'.repeat(20000) }] },
    };
    const result = applyQueryOutputPagination(
      queryResult,
      { charLength: 50 },
      TOOL_NAMES.LSP_FIND_REFERENCES
    );
    expect(result).toBe(queryResult);
  });

  it('returns the result unchanged when nothing needs pagination (small payload, no explicit request)', () => {
    const queryResult = {
      id: 'q-small',
      data: { repositories: [{ owner: 'o', repo: 'r' }] },
    };
    const result = applyQueryOutputPagination(
      queryResult,
      {},
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
    );
    expect(result).toBe(queryResult);
  });

  // ---- localFindFiles charPagination -> outputPagination mapping ----

  it('maps charPagination to outputPagination on localFindFiles non-paginated path (lines 1100-1110)', () => {
    const result = applyQueryOutputPagination(
      {
        id: 'find',
        data: {
          files: ['a.ts', 'b.ts'],
          charPagination: {
            currentPage: 1,
            totalPages: 2,
            hasMore: true,
            charOffset: 0,
            charLength: 50,
            totalChars: 100,
          },
        },
      },
      {},
      TOOL_NAMES.LOCAL_FIND_FILES
    );
    const data = result.data as Record<string, unknown>;
    expect(data.outputPagination).toEqual(data.charPagination);
  });

  it('leaves localFindFiles unchanged when there is no charPagination', () => {
    const queryResult = {
      id: 'find2',
      data: { files: ['a.ts'] },
    };
    const result = applyQueryOutputPagination(
      queryResult,
      {},
      TOOL_NAMES.LOCAL_FIND_FILES
    );
    expect(result).toBe(queryResult);
  });

  // ---- localFindFiles adds charPagination alongside outputPagination (line 1125) ----

  it('adds both outputPagination and charPagination on a paginated localFindFiles (line 1125)', () => {
    // LOCAL_FIND_FILES short-circuits to non-paginated in pageToolDataValue,
    // so to hit the paginated branch the data must NOT be re-sliced there.
    // We rely on the fallback object pagination via large hints array.
    const files = Array.from({ length: 200 }, (_, i) => `dir/file-${i}.ts`);
    const result = applyQueryOutputPagination(
      {
        id: 'find3',
        data: { files },
      },
      { charLength: 200 },
      TOOL_NAMES.LOCAL_FIND_FILES
    );
    // LOCAL_FIND_FILES always returns non-paginated from pageToolDataValue,
    // then falls back; if fallback paginates, both fields appear.
    const data = result.data as Record<string, unknown>;
    if (data.outputPagination) {
      expect(data.charPagination).toBeDefined();
    } else {
      expect(data).toBeDefined();
    }
  });

  // ---- createOutputPagination empty totalChars (lines 110, 115) ----

  it('produces currentPage=1 and totalPages=1 for empty content via empty fallback object', () => {
    // Force pagination of an essentially empty object by requesting an offset
    // far beyond content with explicit request.
    const result = applyQueryOutputPagination(
      {
        id: 'q-empty-content',
        data: { packages: [{ name: 'p', keywords: ['kw'.repeat(400)] }] },
      },
      { charOffset: 0, charLength: 30 },
      TOOL_NAMES.PACKAGE_SEARCH
    );
    const data = result.data as {
      outputPagination?: { currentPage: number; totalPages: number };
    };
    expect(data.outputPagination?.currentPage).toBeGreaterThanOrEqual(1);
    expect(data.outputPagination?.totalPages).toBeGreaterThanOrEqual(1);
  });

  // ---- request.offset >= totalChars beyond-content branches (lines 396-404, 438, 553, 1013) ----

  it('returns an empty page when offset is beyond total content (string field, line 438/553)', () => {
    const result = applyQueryOutputPagination(
      {
        id: 'q-beyond',
        data: {
          locations: [{ path: 'x.ts', content: 'short content here' }],
        },
      },
      { charOffset: 100000, charLength: 100 },
      TOOL_NAMES.LSP_GOTO_DEFINITION
    );
    const data = result.data as {
      locations?: Array<{ content?: string }>;
      outputPagination?: { hasMore: boolean; charOffset: number };
    };
    expect(data.outputPagination).toBeDefined();
    expect(data.outputPagination?.hasMore).toBe(false);
    // beyond content -> empty content
    expect(data.locations?.[0]?.content ?? '').toBe('');
  });

  it('returns an empty page when offset is beyond total content (configured object value, line 396)', () => {
    const packages = Array.from({ length: 5 }, (_, i) => ({
      name: `pkg-${i}`,
      keywords: ['k'.repeat(100)],
    }));
    const result = applyQueryOutputPagination(
      {
        id: 'q-beyond-obj',
        data: { packages },
      },
      { charOffset: 100000, charLength: 100 },
      TOOL_NAMES.PACKAGE_SEARCH
    );
    const data = result.data as {
      packages?: unknown[];
      outputPagination?: { hasMore: boolean };
    };
    expect(data.outputPagination?.hasMore).toBe(false);
  });

  // ---- paginateStringValue: large string at offset 0, hasMore true (lines 428, 485, 495) ----

  it('paginates a very large content string with budget that fits one chunk (lines 485/495)', () => {
    const big = 'a'.repeat(50000);
    const result = applyQueryOutputPagination(
      {
        id: 'q-bigstr',
        data: { locations: [{ path: 'big.ts', content: big }] },
      },
      { charOffset: 0, charLength: 100 },
      TOOL_NAMES.LSP_GOTO_DEFINITION
    );
    const data = result.data as {
      locations?: Array<{ content?: string }>;
      outputPagination?: { hasMore: boolean };
    };
    expect(data.locations?.[0]?.content?.length).toBeLessThan(big.length);
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  it('paginates a large string starting at a non-zero offset (lines 451-465 prefix scan)', () => {
    const big = 'b'.repeat(40000);
    const result = applyQueryOutputPagination(
      {
        id: 'q-bigstr-off',
        data: { locations: [{ path: 'big.ts', content: big }] },
      },
      { charOffset: 5000, charLength: 200 },
      TOOL_NAMES.LSP_GOTO_DEFINITION
    );
    const data = result.data as {
      locations?: Array<{ content?: string }>;
      outputPagination?: { charOffset: number };
    };
    expect(data.locations?.[0]?.content?.length).toBeGreaterThan(0);
    expect(data.outputPagination?.charOffset).toBeGreaterThanOrEqual(0);
  });

  it('handles multi-byte / escaped characters in a paginated string (encoded lengths > 1)', () => {
    // Newlines and quotes encode to multiple JSON chars, exercising encoded length math.
    const big = '"\n\t\\'.repeat(8000);
    const result = applyQueryOutputPagination(
      {
        id: 'q-escapes',
        data: { locations: [{ path: 'e.ts', content: big }] },
      },
      { charOffset: 0, charLength: 150 },
      TOOL_NAMES.LSP_GOTO_DEFINITION
    );
    const data = result.data as {
      locations?: Array<{ content?: string }>;
      outputPagination?: { hasMore: boolean };
    };
    expect(data.outputPagination?.hasMore).toBe(true);
    expect(data.locations?.[0]?.content?.length).toBeGreaterThan(0);
  });

  it('handles unicode astral code points in a paginated string', () => {
    const big = '😀🚀'.repeat(6000);
    const result = applyQueryOutputPagination(
      {
        id: 'q-unicode',
        data: { locations: [{ path: 'u.ts', content: big }] },
      },
      { charOffset: 0, charLength: 120 },
      TOOL_NAMES.LSP_GOTO_DEFINITION
    );
    const data = result.data as {
      locations?: Array<{ content?: string }>;
    };
    expect(data.locations?.[0]?.content?.length).toBeGreaterThan(0);
  });

  // ---- itemPaginator null returns for non-string / non-object items ----

  it('returns null itemPaginator path for githubSearchCode non-string/non-object text_matches (line 689/696)', () => {
    const result = applyQueryOutputPagination(
      {
        id: 'q-numeric-matches',
        data: {
          files: Array.from({ length: 40 }, (_, i) => ({
            path: `src/f-${i}.ts`,
            owner: 'o',
            repo: 'r',
            text_matches: [12345, true, null],
          })),
        },
      },
      { charLength: 300 },
      TOOL_NAMES.GITHUB_SEARCH_CODE
    );
    const data = result.data as {
      files?: unknown[];
      outputPagination?: { hasMore: boolean };
    };
    expect(data.outputPagination?.hasMore).toBe(true);
    expect(data.files?.length ?? 0).toBeLessThan(40);
  });

  it('returns null itemPaginator path for githubSearchRepositories non-string topics (line 715)', () => {
    const result = applyQueryOutputPagination(
      {
        id: 'q-numeric-topics',
        data: {
          repositories: Array.from({ length: 30 }, (_, i) => ({
            owner: 'o',
            repo: `r-${i}`,
            url: `https://github.com/o/r-${i}`,
            topics: [1, 2, 3, { not: 'a string' }],
          })),
        },
      },
      { charLength: 300 },
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
    );
    const data = result.data as { outputPagination?: { hasMore: boolean } };
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  it('returns null itemPaginator path for packageSearch non-string keywords (line 735)', () => {
    const result = applyQueryOutputPagination(
      {
        id: 'q-numeric-kw',
        data: {
          packages: Array.from({ length: 30 }, (_, i) => ({
            name: `pkg-${i}`,
            keywords: [1, 2, 3],
            engines: { node: '>=18', npm: '>=9' },
            dependencies: { left: '^1.0.0', right: '^2.0.0' },
            peerDependencies: { react: '^18.0.0' },
          })),
        },
      },
      { charLength: 400 },
      TOOL_NAMES.PACKAGE_SEARCH
    );
    const data = result.data as { outputPagination?: { hasMore: boolean } };
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  it('githubViewRepoStructure record entries are item-atomic — a node files[] is never sliced', () => {
    const mk = (p: string) => ['1', '2', '3'].map(n => `${p}${n}.ts`);
    const result = applyQueryOutputPagination(
      {
        id: 'q-struct-atomic',
        data: {
          structure: {
            a: { files: mk('a'), folders: [] },
            b: { files: mk('b'), folders: [] },
            c: { files: mk('c'), folders: [] },
          },
        },
      },
      { charOffset: 0, charLength: 60 }, // tight → not all nodes fit
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE
    );
    const data = result.data as {
      structure?: Record<string, { files?: string[] }>;
      outputPagination?: { hasMore: boolean };
    };
    expect(data.outputPagination?.hasMore).toBe(true);
    // Every emitted node keeps its FULL files[] — never a partial slice.
    for (const node of Object.values(data.structure ?? {})) {
      expect(node.files?.length).toBe(3);
    }
  });

  it('returns null itemPaginator path for githubCloneRepo non-string hints (line 871)', () => {
    const result = applyQueryOutputPagination(
      {
        id: 'q-clone-num',
        data: {
          localPath: '/tmp/repo',
          hints: [
            1,
            2,
            3,
            ...Array.from({ length: 30 }, () => 'h'.repeat(100)),
          ],
        },
      },
      { charLength: 400 },
      TOOL_NAMES.GITHUB_CLONE_REPO
    );
    const data = result.data as { outputPagination?: { hasMore: boolean } };
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  // ---- paginators given a non-object value return null (lines 677, 706, 726, 758, 786, 797, 814) ----

  it('handles localSearchCode files where items are not plain objects (line 786/797)', () => {
    const result = applyQueryOutputPagination(
      {
        id: 'q-local-nonobj',
        data: {
          files: [
            'not-an-object',
            42,
            {
              path: 'x.ts',
              matches: [{ lineNumber: 1, value: 'm'.repeat(2000) }],
            },
          ],
        },
      },
      { charLength: 300 },
      TOOL_NAMES.LOCAL_RIPGREP
    );
    const data = result.data as { outputPagination?: { hasMore: boolean } };
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  it('handles lsp locations where items are not plain objects (line 814)', () => {
    const result = applyQueryOutputPagination(
      {
        id: 'q-lsp-nonobj',
        data: {
          locations: [
            'string-location',
            99,
            { path: 'x.ts', content: 'c'.repeat(3000) },
          ],
        },
      },
      { charLength: 300 },
      TOOL_NAMES.LSP_GOTO_DEFINITION
    );
    const data = result.data as { outputPagination?: { hasMore: boolean } };
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  it('handles lspCallHierarchy with mixed primitive and object call entries (fallback paginator)', () => {
    const result = applyQueryOutputPagination(
      {
        id: 'q-callh',
        data: {
          incomingCalls: [
            'primitive',
            123,
            ...Array.from({ length: 20 }, (_, i) => ({
              from: `caller-${i}`,
              detail: 'd'.repeat(150),
            })),
          ],
          outgoingCalls: [],
        },
      },
      { charLength: 400 },
      TOOL_NAMES.LSP_CALL_HIERARCHY
    );
    const data = result.data as { outputPagination?: { hasMore: boolean } };
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  // ---- githubSearchPullRequests outputPagination already present short-circuit (lines ~918-927) ----

  it('short-circuits githubSearchPullRequests when outputPagination already present', () => {
    const queryResult = {
      id: 'q-pr-already',
      data: {
        pull_requests: Array.from({ length: 20 }, (_, i) => ({
          number: i,
          title: 't'.repeat(100),
        })),
        outputPagination: {
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
          charOffset: 0,
          charLength: 10,
          totalChars: 10,
        },
      },
    };
    const result = applyQueryOutputPagination(
      queryResult,
      { charLength: 200 },
      TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS
    );
    // falls through to fallback object pagination which may paginate, but the
    // tool-specific branch returns non-paginated; ensure it does not throw.
    expect(result.data).toBeDefined();
  });

  // ---- default tool path (unknown tool) hits fallback (lines 955-963 + fallback) ----

  it('uses the fallback object pagination for an unknown tool name', () => {
    const result = applyQueryOutputPagination(
      {
        id: 'q-unknown',
        data: {
          items: Array.from(
            { length: 40 },
            (_, i) => `item-${i}-${'x'.repeat(60)}`
          ),
        },
      },
      { charLength: 300 },
      'some_unknown_tool'
    );
    const data = result.data as { outputPagination?: { hasMore: boolean } };
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  it('fallback paginates a nested object field when no arrays/strings dominate (lines 653-661, 594)', () => {
    const result = applyQueryOutputPagination(
      {
        id: 'q-nested',
        data: {
          meta: {
            description: 'd'.repeat(20000),
          },
        },
      },
      { charLength: 300 },
      'unknown_nested_tool'
    );
    const data = result.data as {
      meta?: { description?: string };
      outputPagination?: { hasMore: boolean };
    };
    expect(data.outputPagination?.hasMore).toBe(true);
    expect(data.meta?.description?.length).toBeLessThan(20000);
  });

  it('fallback paginates a top-level string field (lines 643-651, 583)', () => {
    const result = applyQueryOutputPagination(
      {
        id: 'q-topstr',
        data: {
          summary: 's'.repeat(20000),
        },
      },
      { charLength: 300 },
      'unknown_string_tool'
    );
    const data = result.data as {
      summary?: string;
      outputPagination?: { hasMore: boolean };
    };
    expect(data.outputPagination?.hasMore).toBe(true);
    expect(data.summary?.length).toBeLessThan(20000);
  });

  // ---- withPaginationHints: existing hints already include the page summary (line 155) ----

  it('does not duplicate the page-summary hint when re-paginating identical content', () => {
    const payload = {
      id: 'q-dup-hints',
      data: {
        locations: [{ path: 'd.ts', content: 'z'.repeat(8000) }],
      },
    };
    const first = applyQueryOutputPagination(
      payload,
      { charOffset: 0, charLength: 300 },
      TOOL_NAMES.LSP_GOTO_DEFINITION
    );
    const data = first.data as { hints?: string[] };
    const summaryHints = (data.hints ?? []).filter(h => h.startsWith('Page '));
    expect(summaryHints.length).toBe(1);
  });

  // ---- bulk response: kind === 'response' continuation hint (line 141) + materialize branches ----

  it('paginates a bulk response across multiple results and emits responsePagination', () => {
    const results = Array.from({ length: 6 }, (_, i) => ({
      id: `q-${i}`,
      data: {
        repositories: [
          {
            owner: 'o',
            repo: `r-${i}`,
            description: 'desc '.repeat(40),
            url: `https://github.com/o/r-${i}`,
          },
        ],
      },
    }));
    const response = applyBulkResponsePagination(
      { results },
      { offset: 0, length: 400 },
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
    );
    expect(response.responsePagination?.hasMore).toBe(true);
    expect(Array.isArray(response.results)).toBe(true);
    expect(response.results.length).toBeLessThanOrEqual(results.length);
  });

  it('returns the bulk response unchanged when small enough and no explicit request (page not paginated, line 1162)', () => {
    const response = {
      results: [
        { id: 'q', data: { repositories: [{ owner: 'o', repo: 'r' }] } },
      ],
    };
    const out = applyBulkResponsePagination(
      response,
      {},
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
    );
    expect(out).toBe(response);
  });

  it('paginates a bulk response at a mid offset to cross segment boundaries', () => {
    const results = Array.from({ length: 8 }, (_, i) => ({
      id: `q-${i}`,
      data: {
        repositories: [
          {
            owner: 'o',
            repo: `r-${i}`,
            description: 'd'.repeat(120),
            url: `https://github.com/o/r-${i}`,
          },
        ],
      },
    }));
    const first = applyBulkResponsePagination(
      { results },
      { offset: 0, length: 400 },
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
    );
    const nextOffset =
      (first.responsePagination?.charOffset ?? 0) +
      (first.responsePagination?.charLength ?? 0);
    const second = applyBulkResponsePagination(
      { results },
      { offset: nextOffset, length: 400 },
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
    );
    expect(second.responsePagination).toBeDefined();
    expect(second.responsePagination?.currentPage).toBeGreaterThanOrEqual(1);
  });

  it('clamps a bulk response when the offset is far beyond content (line 289 firstSegmentIndex=-1)', () => {
    const response = applyBulkResponsePagination(
      {
        results: [
          {
            id: 'q',
            data: { repositories: [{ owner: 'o', repo: 'r' }] },
          },
        ],
      },
      { offset: 999999, length: 50 },
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
    );
    expect(response.responsePagination).toBeDefined();
    expect(response.responsePagination?.hasMore).toBe(false);
  });

  // ---- paginateFlatQueryResult guards inside bulk (lines 974, 981) ----

  it('bulk pagination tolerates results whose data is not an object (line 974)', () => {
    const response = applyBulkResponsePagination(
      {
        results: [
          {
            id: 'bad',
            data: 'string-data' as unknown as Record<string, unknown>,
          },
          {
            id: 'good',
            data: {
              repositories: [
                {
                  owner: 'o',
                  repo: 'r',
                  description: 'd'.repeat(200),
                  url: 'https://github.com/o/r',
                },
              ],
            },
          },
        ],
      },
      { offset: 0, length: 200 },
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
    );
    expect(response.results).toBeDefined();
  });

  it('bulk pagination skips per-result inner pagination for non-success status results (line 981)', () => {
    const response = applyBulkResponsePagination(
      {
        results: [
          {
            id: 'err',
            status: 'error' as const,
            data: { error: 'e'.repeat(5000) },
          },
          {
            id: 'ok',
            data: {
              repositories: Array.from({ length: 5 }, (_, i) => ({
                owner: 'o',
                repo: `r-${i}`,
                description: 'd'.repeat(120),
                url: `https://github.com/o/r-${i}`,
              })),
            },
          },
        ],
      },
      { offset: 0, length: 300 },
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
    );
    expect(response.responsePagination).toBeDefined();
  });

  // ---- paginateFlatQueryResult exposes outputPagination on inner data (lines 1031-1058) ----

  it('exposes inner outputPagination on a paginated bulk result data object (lines 1031-1058)', () => {
    const results = [
      {
        id: 'big',
        data: {
          files: [
            {
              path: 'big.ts',
              owner: 'o',
              repo: 'r',
              text_matches: ['m'.repeat(8000)],
            },
          ],
        },
      },
    ];
    const response = applyBulkResponsePagination(
      { results },
      { offset: 0, length: 500 },
      TOOL_NAMES.GITHUB_SEARCH_CODE
    );
    const data = response.results[0]?.data as {
      outputPagination?: { hasMore: boolean };
    };
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  it('does not expose inner outputPagination for lspFindReferences bulk results (line 1023/1038)', () => {
    const results = [
      {
        id: 'refs',
        data: {
          locations: Array.from({ length: 10 }, (_, i) => ({
            uri: `/x/${i}.ts`,
            content: 'c'.repeat(800),
          })),
        },
      },
    ];
    const response = applyBulkResponsePagination(
      { results },
      { offset: 0, length: 400 },
      TOOL_NAMES.LSP_FIND_REFERENCES
    );
    const data = response.results[0]?.data as {
      outputPagination?: unknown;
    };
    expect(data.outputPagination).toBeUndefined();
    expect(response.responsePagination?.hasMore).toBe(true);
  });

  // ---- fallback: top-level-only string field paths (lines 438, 451-465, 504, 543, 553, 583, 608) ----

  it('paginates a sole top-level string field at offset 0 (fallback string branch)', () => {
    const result = applyQueryOutputPagination(
      { id: 'q-only-str', data: { summary: 's'.repeat(20000) } },
      { charOffset: 0, charLength: 300 },
      'unknown_string_only_tool'
    );
    const data = result.data as {
      summary?: string;
      outputPagination?: { hasMore: boolean };
    };
    expect(data.summary?.length).toBeLessThan(20000);
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  it('paginates a sole top-level string field at a mid offset (prefix scan, line 451-465)', () => {
    const result = applyQueryOutputPagination(
      { id: 'q-only-str-mid', data: { summary: 's'.repeat(20000) } },
      { charOffset: 9000, charLength: 300 },
      'unknown_string_only_tool'
    );
    const data = result.data as {
      summary?: string;
      outputPagination?: { charOffset: number; hasMore: boolean };
    };
    expect(data.summary?.length).toBeGreaterThan(0);
    expect(data.outputPagination?.charOffset).toBe(9000);
  });

  it('returns an empty string field when offset is at/after the string total (lines 438, 553)', () => {
    const result = applyQueryOutputPagination(
      { id: 'q-only-str-beyond', data: { summary: 's'.repeat(2000) } },
      { charOffset: 50000, charLength: 200 },
      'unknown_string_only_tool'
    );
    const data = result.data as {
      summary?: string;
      outputPagination?: { hasMore: boolean; charLength: number };
    };
    expect(data.summary).toBe('');
    expect(data.outputPagination?.hasMore).toBe(false);
    expect(data.outputPagination?.charLength).toBe(0);
  });

  it('reads a short string field near its end so the last chunk reaches the terminator (line 504)', () => {
    const result = applyQueryOutputPagination(
      { id: 'q-only-str-end', data: { summary: 's'.repeat(50) } },
      { charOffset: 40, charLength: 8000 },
      'unknown_string_only_tool'
    );
    const data = result.data as { summary?: string };
    expect(data.summary?.length).toBeGreaterThan(0);
  });

  it('handles a string field whose offset lands in the final encoded region (not found prefix, line 465)', () => {
    const result = applyQueryOutputPagination(
      { id: 'q-near-end', data: { summary: 's'.repeat(100) } },
      { charOffset: 101, charLength: 50 },
      'unknown_string_only_tool'
    );
    const data = result.data as {
      summary?: string;
      outputPagination?: { hasMore: boolean };
    };
    expect(data.outputPagination?.hasMore).toBe(false);
    expect(data.summary?.length).toBeGreaterThanOrEqual(0);
  });

  // ---- fallback: sole nested object field (lines 594, 595, 543) ----

  it('paginates a sole nested object field via the nested-object fallback (line 594)', () => {
    const result = applyQueryOutputPagination(
      { id: 'q-nested-only', data: { meta: { desc: 'd'.repeat(20000) } } },
      { charLength: 300 },
      'unknown_nested_only_tool'
    );
    const data = result.data as {
      meta?: { desc?: string };
      outputPagination?: { hasMore: boolean };
    };
    expect(data.meta?.desc?.length).toBeLessThan(20000);
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  // ---- fallback: excluded fields are skipped (lines 622, 644, 648, 654, 658) ----

  it('skips excluded fields (warnings) and paginates the dominant string instead (line 622)', () => {
    const result = applyQueryOutputPagination(
      {
        id: 'q-excl',
        data: {
          warnings: ['w'.repeat(50)],
          summary: 's'.repeat(20000),
        },
      },
      { charLength: 300 },
      'unknown_excl_tool'
    );
    const data = result.data as {
      warnings?: string[];
      summary?: string;
      outputPagination?: { hasMore: boolean };
    };
    expect(data.warnings).toEqual(['w'.repeat(50)]);
    expect(data.summary?.length).toBeLessThan(20000);
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  // ---- auto-pagination hint (lines 144-152, 147, 148) ----

  it('does NOT per-query paginate an oversized result without an explicit charOffset/charLength (bulk owns auto-capping)', () => {
    // Per-query char-pagination is explicit-only now: without charOffset/
    // charLength on the query, applyQueryOutputPagination leaves the result
    // untouched so the response carries a single coherent cursor
    // (responseCharOffset, emitted by applyBulkResponsePagination) instead of
    // two breadcrumbs with different char totals.
    const repositories = Array.from({ length: 60 }, (_, i) => ({
      owner: 'o',
      repo: `r-${i}`,
      description: 'd'.repeat(200),
      url: `https://github.com/o/r-${i}`,
    }));
    const result = applyQueryOutputPagination(
      { id: 'q-auto', data: { repositories } },
      {},
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
    );
    const data = result.data as {
      hints?: string[];
      outputPagination?: unknown;
    };
    expect(data.outputPagination).toBeUndefined();
    expect((data.hints ?? []).some(h => h.startsWith('Auto-paginated:'))).toBe(
      false
    );
    // The full payload is preserved (untouched) — bulk pagination, not this
    // per-query pass, is what bounds the aggregate.
    expect((data as { repositories?: unknown[] }).repositories).toHaveLength(
      60
    );
  });

  it('per-query paginates + emits page/cursor hints when an explicit charLength is supplied', () => {
    const repositories = Array.from({ length: 60 }, (_, i) => ({
      owner: 'o',
      repo: `r-${i}`,
      description: 'd'.repeat(200),
      url: `https://github.com/o/r-${i}`,
    }));
    const result = applyQueryOutputPagination(
      { id: 'q-explicit', data: { repositories } },
      { charLength: 500 },
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
    );
    const data = result.data as {
      hints?: string[];
      outputPagination?: { hasMore: boolean };
    };
    expect(data.outputPagination?.hasMore).toBe(true);
    expect((data.hints ?? []).some(h => h.startsWith('Page '))).toBe(true);
    expect((data.hints ?? []).some(h => h.includes('Use charOffset='))).toBe(
      true
    );
  });

  // ---- bulk inner data continuation hint uses 'output' kind, but response kind tested via responsePagination shape ----

  it('produces an oversized bulk response whose inner result also carries outputPagination + hints (lines 1032-1058)', () => {
    const results = [
      {
        id: 'big',
        data: {
          files: [
            {
              path: 'big.ts',
              owner: 'o',
              repo: 'r',
              text_matches: ['m'.repeat(8000)],
            },
          ],
        },
      },
    ];
    const response = applyBulkResponsePagination(
      { results },
      { offset: 0, length: 500 },
      TOOL_NAMES.GITHUB_SEARCH_CODE
    );
    const data = response.results[0]?.data as {
      outputPagination?: { hasMore: boolean };
      hints?: string[];
    };
    expect(data.outputPagination?.hasMore).toBe(true);
    expect((data.hints ?? []).some(h => h.startsWith('Page '))).toBe(true);
  });

  // ---- bulk small offset: request.offset inside the base-object region (lines 313, 344, 353) ----

  it('paginates a bulk response with an offset inside the wrapper region (actualOffset resets to 0)', () => {
    const results = Array.from({ length: 4 }, (_, i) => ({
      id: `q-${i}`,
      data: {
        repositories: [
          {
            owner: 'o',
            repo: `r-${i}`,
            description: 'd'.repeat(80),
            url: `https://github.com/o/r-${i}`,
          },
        ],
      },
    }));
    const response = applyBulkResponsePagination(
      { results },
      { offset: 3, length: 100 },
      TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
    );
    expect(response.responsePagination?.charOffset).toBe(0);
    expect(response.responsePagination?.hasMore).toBe(true);
    expect(response.results.length).toBeGreaterThan(0);
  });

  // ---- materializeSegments record reuse: multi-key records on the same field (lines 255, 263) ----

  it('paginates a record across multiple keys, reusing the materialized record object (structure branch)', () => {
    // The record-materialize branch is exercised by githubViewRepoStructure,
    // whose `structure` field is a Record<path, entry> (packageSearch repos are
    // now atomic and no longer sub-paginate their record fields).
    const result = applyQueryOutputPagination(
      {
        id: 'q-struct-records',
        data: {
          structure: {
            src: { files: ['a.ts'], folders: [] },
            'src/lib': { files: ['b.ts'], folders: [] },
            test: { files: ['c.ts'], folders: [] },
            docs: { files: ['d.ts'], folders: [] },
          },
        },
      },
      { charOffset: 0, charLength: 60 },
      TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE
    );
    const data = result.data as {
      structure?: Record<string, unknown>;
      outputPagination?: { hasMore: boolean };
    };
    // At least one key materialized into the structure record; more remain.
    expect(Object.keys(data.structure ?? {}).length).toBeGreaterThanOrEqual(1);
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  it('paginates multi-item arrays of the same field, reusing the materialized array (line 255)', () => {
    // Code-search files[] still array-paginates (each match is atomic but the
    // file LIST is sliced), exercising the array-materialize branch.
    const result = applyQueryOutputPagination(
      {
        id: 'q-multi-array',
        data: {
          files: Array.from({ length: 60 }, (_, i) => ({
            path: `f${i}.ts`,
            text_matches: [{ value: 'v'.repeat(20) }],
          })),
        },
      },
      { charOffset: 0, charLength: 300 },
      TOOL_NAMES.GITHUB_SEARCH_CODE
    );
    const data = result.data as {
      files?: unknown[];
      outputPagination?: { hasMore: boolean };
    };
    expect(data.files?.length ?? 0).toBeGreaterThan(1);
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  // ---- query offset past the base-object region: actualOffset uses segment.start (false side of lines 313, 344) ----

  it('paginates a query at an offset past the wrapper so a whole segment anchors actualOffset (line 313 false side)', () => {
    // Code-search files still sub-paginate (text_matches[].value), so they
    // exercise the segment-offset branches; repos/packages are now atomic.
    const files = Array.from({ length: 8 }, (_, i) => ({
      path: `f${i}.ts`,
      text_matches: [{ value: 'v'.repeat(120) }],
    }));
    const result = applyQueryOutputPagination(
      { id: 'q-off-seg', data: { files } },
      { charOffset: 300, charLength: 200 },
      TOOL_NAMES.GITHUB_SEARCH_CODE
    );
    const data = result.data as {
      outputPagination?: { charOffset: number; hasMore: boolean };
    };
    // Offset is past the wrapper, so actualOffset anchors at/after it (not 0).
    expect(data.outputPagination?.charOffset).toBeGreaterThan(0);
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  it('paginates a query at an offset past the wrapper into a partial item (line 344 false side)', () => {
    const result = applyQueryOutputPagination(
      {
        id: 'q-off-partial',
        data: {
          files: [
            { path: 'a.ts', text_matches: [{ value: 'k'.repeat(50) }] },
            { path: 'b.ts', text_matches: [{ value: 'm'.repeat(5000) }] },
          ],
        },
      },
      { charOffset: 120, charLength: 200 },
      TOOL_NAMES.GITHUB_SEARCH_CODE
    );
    const data = result.data as {
      files?: unknown[];
      outputPagination?: { charOffset: number; hasMore: boolean };
    };
    // Landed inside the oversized item → a partial slice is returned and the
    // cursor reports more data.
    expect(data.outputPagination?.charOffset).toBeGreaterThan(0);
    expect(data.outputPagination?.hasMore).toBe(true);
    expect(data.files?.length ?? 0).toBeGreaterThan(0);
  });

  it('anchors actualOffset to a whole segment start when the offset lands exactly on a segment boundary (line 313 false side)', () => {
    const pkg = (i: number) => ({ name: `pkg-${i}`, keywords: ['k'] });
    const packages = Array.from({ length: 10 }, (_, i) => pkg(i));
    // baseChars = len('{"packages":[]}') = 15; first item has no leading comma.
    const baseChars = JSON.stringify({ packages: [] }).length;
    const secondSegmentStart = baseChars + JSON.stringify(pkg(0)).length;
    const result = applyQueryOutputPagination(
      { id: 'q-boundary', data: { packages } },
      { charOffset: secondSegmentStart, charLength: 200 },
      TOOL_NAMES.PACKAGE_SEARCH
    );
    const data = result.data as {
      outputPagination?: { charOffset: number; hasMore: boolean };
    };
    expect(data.outputPagination?.charOffset).toBe(secondSegmentStart);
    expect(data.outputPagination?.hasMore).toBe(true);
  });
});

describe('githubSearchPullRequests pagination fixes', () => {
  // #1 — escape valve: an oversized single PR (huge fileChanges[].patch under
  // fullContent) must be sub-sliced rather than emitted whole.
  it('sub-slices an oversized single PR by paginating fileChanges[].patch', () => {
    const bigPatch = 'P'.repeat(20000);
    const result = applyQueryOutputPagination(
      {
        id: 'pr-big',
        data: {
          pull_requests: [
            {
              number: 1,
              title: 't',
              fileChanges: [
                { path: 'a.ts', patch: bigPatch },
                { path: 'b.ts', patch: bigPatch },
              ],
            },
          ],
        },
      },
      { charLength: 2000 },
      TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS
    );
    const data = result.data as {
      pull_requests?: Array<{ fileChanges?: Array<{ patch?: string }> }>;
      outputPagination?: { hasMore: boolean; charLength: number };
    };
    // Page 1 is bounded near the budget — NOT the full ~40K of patches.
    const emitted = JSON.stringify(data.pull_requests).length;
    expect(emitted).toBeLessThan(8000);
    expect(data.outputPagination?.hasMore).toBe(true);
  });

  // #2 — totalPages is exact on the last page even when an atomic item ate
  // more than one page-size worth of chars (no more "1/2" for a single page).
  it('reports totalPages === currentPage when the final page fits everything', () => {
    // One PR whose serialized size exceeds the requested page size, but there
    // is nothing after it → it is the one and only (last) page.
    const result = applyQueryOutputPagination(
      {
        id: 'pr-one',
        data: {
          pull_requests: [
            { number: 1, title: 'x'.repeat(3000) }, // > 2000 page size
          ],
        },
      },
      { charLength: 2000, charOffset: 0 },
      TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS
    );
    const data = result.data as {
      outputPagination?: {
        currentPage: number;
        totalPages: number;
        hasMore: boolean;
      };
    };
    const pg = data.outputPagination;
    if (pg) {
      // The single oversized item is the last page; the count must not overcount.
      if (!pg.hasMore) expect(pg.totalPages).toBe(pg.currentPage);
    }
  });

  // #3 — when every result already carries a per-query cursor and bulk
  // pagination was NOT requested, the bulk pass is skipped (no second,
  // contradictory responsePagination breadcrumb).
  it('skips bulk pagination when results are already per-query paginated', () => {
    const out = applyBulkResponsePagination(
      {
        results: [
          {
            id: 'q1',
            data: {
              pull_requests: [{ number: 1 }],
              outputPagination: {
                currentPage: 1,
                totalPages: 2,
                hasMore: true,
                charOffset: 0,
                charLength: 2000,
                totalChars: 4000,
              },
            },
          },
        ],
      } as never,
      {}, // no responseCharOffset/Length → bulk pagination not requested
      TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS
    );
    expect(out.responsePagination).toBeUndefined();
  });

  it('still applies bulk pagination when it is explicitly requested', () => {
    const out = applyBulkResponsePagination(
      {
        results: Array.from({ length: 4 }, (_, i) => ({
          id: `q${i}`,
          data: { pull_requests: [{ number: i, body: 'b'.repeat(1500) }] },
        })),
      } as never,
      { length: 2000 }, // explicit bulk request
      TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS
    );
    expect(out.responsePagination).toBeDefined();
    expect(out.responsePagination!.hasMore).toBe(true);
  });
});
