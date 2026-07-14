import { describe, expect, it } from 'vitest';
import { LspGetSemanticsOutputSchema } from '../../src/tools/lsp/semantic_content/scheme.js';

/**
 * Regression: withSemanticNext attaches `next.readSite` on definition hits.
 * Zod strips unknown keys by default, so MCP JSON Schema (additionalProperties:
 * false) rejected live structuredContent until `next` was declared on
 * SemanticDataSchema.
 */
describe('LspGetSemanticsOutputSchema — MCP structuredContent contract', () => {
  const definitionWithNext = {
    results: [
      {
        id: 'b',
        data: {
          type: 'definition',
          uri: 'cache.ts',
          resolvedSymbol: {
            name: 'withDataCacheConditional',
            uri: 'cache.ts',
            foundAtLine: 266,
          },
          lsp: { serverAvailable: true, provider: 'definitionProvider' },
          payload: {
            kind: 'definition' as const,
            locations: [
              {
                uri: 'cache.ts',
                content: 'export async function withDataCacheConditional<T>(',
                displayRange: { startLine: 266, endLine: 266 },
              },
            ],
          },
          pagination: {
            currentPage: 1,
            totalPages: 1,
            totalResults: 1,
            hasMore: false,
            itemsPerPage: 40,
          },
          next: {
            readSite: {
              tool: 'localGetFileContent',
              query: {
                path: '/abs/cache.ts',
                startLine: 263,
                endLine: 276,
              },
              why: 'Read the top result location with surrounding context',
              confidence: 'exact' as const,
            },
          },
        },
      },
    ],
    base: '/abs',
    responsePagination: {
      currentPage: 1,
      totalPages: 1,
      hasMore: false,
      charOffset: 0,
      charLength: 100,
      totalChars: 100,
    },
  };

  it('accepts success results that include next.readSite', () => {
    const parsed = LspGetSemanticsOutputSchema.safeParse(definitionWithNext);
    expect(parsed.success).toBe(true);
    if (!parsed.success) return;
    expect(parsed.data.results[0]).toMatchObject({
      id: 'b',
      data: {
        next: {
          readSite: {
            tool: 'localGetFileContent',
            confidence: 'exact',
          },
        },
      },
    });
  });

  it('keeps next after parse (does not strip)', () => {
    const parsed = LspGetSemanticsOutputSchema.parse(definitionWithNext);
    const data = parsed.results[0]?.data as {
      next?: { readSite?: { tool?: string } };
    };
    expect(data.next?.readSite?.tool).toBe('localGetFileContent');
  });

  it('keeps responsePagination after parse (does not strip)', () => {
    const parsed = LspGetSemanticsOutputSchema.parse(definitionWithNext) as {
      responsePagination?: { totalChars?: number };
    };
    expect(parsed.responsePagination?.totalChars).toBe(100);
  });

  it('still accepts success without next', () => {
    const { next: _n, ...dataSansNext } = definitionWithNext.results[0].data;
    void _n;
    const parsed = LspGetSemanticsOutputSchema.safeParse({
      results: [{ id: 'b', data: dataSansNext }],
    });
    expect(parsed.success).toBe(true);
  });
});
