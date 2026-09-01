import { describe, expect, it } from 'vitest';

import { withSemanticNext } from '../../../src/tools/lsp/semantic_content/semanticNext.js';
import { failedAnchorEnvelope } from '../../../src/tools/lsp/semantic_content/semanticEnvelopes.js';
import type {
  LspGetSemanticsQuery,
  LspSemanticEnvelope,
} from '../../../src/tools/lsp/shared/semanticTypes.js';
import { prepareDirectToolInput } from '../../../src/tools/directToolCatalog.meta.js';

/**
 * The tool description promises: "Empty/incomplete: re-anchor or fall back to
 * localSearch." withSemanticNext must emit that structured fallback for
 * empty results that carry a symbolName.
 */
describe('withSemanticNext — empty-state fallback', () => {
  const symbolNotFound = (
    type: 'definition' | 'references'
  ): { query: LspGetSemanticsQuery; result: LspSemanticEnvelope } => {
    const query = {
      type,
      uri: 'file:///repo/src/foo.ts',
      symbolName: 'doThing',
      lineHint: 10,
    } as LspGetSemanticsQuery;
    const result = failedAnchorEnvelope(
      query,
      'Could not find symbol "doThing"'
    );
    expect(result.payload).toMatchObject({
      kind: 'empty',
      category: 'symbolNotFound',
    });
    return { query, result };
  };

  it('points a symbolNotFound definition to a schema-valid localSearch query', () => {
    const { query, result } = symbolNotFound('definition');
    const withNext = withSemanticNext(query, result) as LspSemanticEnvelope;

    const textSearch = withNext.next?.textSearch;
    expect(textSearch?.tool).toBe('localSearch');
    expect(textSearch?.query).toMatchObject({
      operation: 'text',
      path: '/repo/src/foo.ts',
      searchText: 'doThing',
    });
    expect(textSearch?.query.keywords).toBeUndefined();
    expect(() =>
      prepareDirectToolInput('localSearch', textSearch?.query ?? {}, {
        rejectUnknownFields: true,
      })
    ).not.toThrow();
  });

  it('adds a re-anchor hint for symbolNotFound (references)', () => {
    const { query, result } = symbolNotFound('references');
    const withNext = withSemanticNext(query, result) as LspSemanticEnvelope;

    expect(withNext.next?.textSearch?.query.searchText).toBe('doThing');
    const reAnchor = withNext.next?.reAnchor;
    expect(reAnchor?.tool).toBe('lspGetSemantics');
    expect(reAnchor?.query).toMatchObject({
      type: 'documentSymbols',
      uri: 'file:///repo/src/foo.ts',
    });
  });

  it('points documentSymbols unsupportedOperation to a file-scoped export search (no symbolName needed)', () => {
    // Regression for the benchmark-found gap: documentSymbols never has a
    // symbolName (it lists every symbol in a file), so the symbolName-gated
    // fallback never fired for it — leaving a Flow-typed .js (or any
    // language server without documentSymbolProvider) caller with no pointer
    // to the regex/AST outline workaround.
    const query = {
      type: 'documentSymbols',
      uri: 'file:///repo/src/Big.js',
    } as LspGetSemanticsQuery;
    const result: LspSemanticEnvelope = {
      type: 'documentSymbols',
      uri: 'file:///repo/src/Big.js',
      lsp: { serverAvailable: true },
      payload: {
        kind: 'documentSymbols',
        symbols: [],
        totalSymbols: 0,
        empty: {
          category: 'unsupportedOperation',
          reason: 'documentSymbolProvider unsupported',
        },
      },
    };

    const withNext = withSemanticNext(query, result) as LspSemanticEnvelope;
    const textSearch = withNext.next?.textSearch;

    expect(textSearch?.tool).toBe('localSearch');
    expect(textSearch?.query.path).toBe('/repo/src/Big.js');
    expect(typeof textSearch?.query.searchText).toBe('string');
    expect(textSearch?.query.regex).toBe('perl');
  });

  it('picks a language-appropriate fallback regex per file extension', () => {
    // `^export` finds nothing in Rust/Python/Go — the fallback must speak the
    // file's own declaration idiom or it sends the caller to a guaranteed
    // empty search.
    const cases: Array<{ uri: string; mustMatch: RegExp }> = [
      { uri: 'file:///repo/src/lib.rs', mustMatch: /pub/ },
      { uri: 'file:///repo/src/app.py', mustMatch: /def|class/ },
      { uri: 'file:///repo/src/main.go', mustMatch: /func/ },
      { uri: 'file:///repo/src/Big.js', mustMatch: /export/ },
    ];

    for (const { uri, mustMatch } of cases) {
      const query = { type: 'documentSymbols', uri } as LspGetSemanticsQuery;
      const result: LspSemanticEnvelope = {
        type: 'documentSymbols',
        uri,
        lsp: { serverAvailable: true },
        payload: {
          kind: 'documentSymbols',
          symbols: [],
          totalSymbols: 0,
          empty: {
            category: 'unsupportedOperation',
            reason: 'documentSymbolProvider unsupported',
          },
        },
      };

      const withNext = withSemanticNext(query, result) as LspSemanticEnvelope;
      const searchText = String(
        withNext.next?.textSearch?.query.searchText ?? ''
      );
      expect(searchText, `fallback regex for ${uri}`).toMatch(mustMatch);
    }
  });

  it('uses the explicit workspace root instead of a representative anchor for a workspace-symbol fallback', () => {
    const query = {
      type: 'workspaceSymbol',
      workspaceRoot: '/repo',
      symbolName: 'doThing',
    } as LspGetSemanticsQuery;
    const result: LspSemanticEnvelope = {
      type: 'workspaceSymbol',
      uri: '/repo/src/representative.ts',
      workspaceRoot: '/repo',
      lsp: { serverAvailable: true },
      payload: {
        kind: 'empty',
        category: 'noWorkspaceSymbols',
        reason: 'No workspace symbols found.',
      },
    };

    const withNext = withSemanticNext(query, result) as LspSemanticEnvelope;
    expect(withNext.next?.textSearch?.query).toMatchObject({
      path: '/repo',
      searchText: 'doThing',
    });
  });

  it('emits no fallback when there is no symbolName to search for', () => {
    const query = {
      type: 'documentSymbols',
      uri: 'src/foo.ts',
    } as LspGetSemanticsQuery;
    const result = failedAnchorEnvelope(query, 'anchor failed');
    const withNext = withSemanticNext(query, result) as LspSemanticEnvelope;
    expect(withNext.next).toBeUndefined();
  });

  it('still emits readSite (not the fallback) when a location is present', () => {
    const query = {
      type: 'definition',
      uri: 'src/foo.ts',
      symbolName: 'doThing',
      lineHint: 10,
    } as LspGetSemanticsQuery;
    const result: LspSemanticEnvelope = {
      type: 'definition',
      uri: 'src/foo.ts',
      lsp: { serverAvailable: true },
      payload: {
        kind: 'definition',
        locations: [
          {
            uri: 'file:///abs/foo.ts',
            displayRange: { startLine: 5, endLine: 5 },
          },
        ],
      },
    };
    const withNext = withSemanticNext(query, result) as LspSemanticEnvelope;
    expect(withNext.next?.readSite?.tool).toBe('localGetFileContent');
    expect(withNext.next?.textSearch).toBeUndefined();
  });

  it('turns pagination.nextPage into an executable schema-valid continuation', () => {
    const query = {
      type: 'documentSymbols',
      uri: 'file:///repo/src/foo.ts',
      page: 1,
      pageSize: 1,
      format: 'compact',
      goal: 'auto-filled goal',
      reasoning: 'auto-filled reasoning',
    } as LspGetSemanticsQuery & Record<string, unknown>;
    const result: LspSemanticEnvelope = {
      type: 'documentSymbols',
      uri: query.uri!,
      lsp: { serverAvailable: true },
      payload: {
        kind: 'documentSymbols',
        symbols: [{ name: 'alpha' }],
      },
      pagination: {
        currentPage: 1,
        totalPages: 2,
        totalResults: 2,
        hasMore: true,
        pageSize: 1,
        nextPage: 2,
      },
    };

    const withNext = withSemanticNext(query, result) as LspSemanticEnvelope;
    const nextPage = withNext.next?.nextPage;
    expect(nextPage).toMatchObject({
      tool: 'lspGetSemantics',
      query: {
        type: 'documentSymbols',
        uri: 'file:///repo/src/foo.ts',
        page: 2,
        pageSize: 1,
        format: 'compact',
      },
      confidence: 'exact',
    });
    expect(nextPage?.query).not.toHaveProperty('goal');
    expect(nextPage?.query).not.toHaveProperty('reasoning');
    expect(() =>
      prepareDirectToolInput('lspGetSemantics', nextPage?.query ?? {}, {
        rejectUnknownFields: true,
      })
    ).not.toThrow();
  });

  it('marks page 1000 terminal instead of emitting schema-invalid page 1001', () => {
    const query = {
      type: 'documentSymbols',
      uri: 'file:///repo/src/foo.ts',
      page: 1_000,
      pageSize: 1,
    } as LspGetSemanticsQuery;
    const result: LspSemanticEnvelope = {
      type: 'documentSymbols',
      uri: query.uri!,
      lsp: { serverAvailable: true },
      payload: {
        kind: 'documentSymbols',
        symbols: [{ name: 'item-999' }],
      },
      pagination: {
        currentPage: 1_000,
        totalPages: 1_001,
        totalResults: 1_001,
        hasMore: true,
        pageSize: 1,
        nextPage: 1_001,
      },
    };

    const withNext = withSemanticNext(query, result) as LspSemanticEnvelope;
    expect(withNext.terminalLimit).toBe(true);
    expect(withNext.pagination).not.toHaveProperty('nextPage');
    expect(withNext.next?.nextPage).toBeUndefined();
  });

  it('makes a capped reference warmup partial and provides a workspace search', () => {
    const query = {
      type: 'references',
      uri: 'file:///repo/src/foo.ts',
      workspaceRoot: '/repo',
      symbolName: 'doThing',
      lineHint: 10,
    } as LspGetSemanticsQuery;
    const result: LspSemanticEnvelope = {
      type: 'references',
      uri: query.uri!,
      workspaceRoot: '/repo',
      lsp: { serverAvailable: true },
      payload: {
        kind: 'references',
        locations: [],
        totalReferences: 0,
        totalFiles: 0,
        warmup: {
          candidates: 100,
          warmedFiles: 100,
          skippedLarge: 0,
          possiblyTruncated: true,
        },
      },
    };

    const withNext = withSemanticNext(query, result) as LspSemanticEnvelope;
    expect(withNext).toMatchObject({
      truncated: true,
      partialReasons: ['warmupCap'],
      next: {
        verifyCompleteness: {
          tool: 'localSearch',
          query: {
            operation: 'text',
            path: '/repo',
            searchText: 'doThing',
            wholeWord: true,
          },
        },
      },
    });
  });

  it('expands call depth below the schema maximum and terminalizes fixed budgets', () => {
    const query = {
      type: 'callers',
      uri: 'file:///repo/src/foo.ts',
      symbolName: 'doThing',
      lineHint: 10,
      depth: 2,
    } as LspGetSemanticsQuery;
    const base: LspSemanticEnvelope = {
      type: 'callers',
      uri: query.uri!,
      lsp: { serverAvailable: true },
      payload: {
        kind: 'callers',
        direction: 'incoming',
        calls: [],
        completeness: {
          complete: false,
          truncatedByDepth: true,
          truncatedByBudget: false,
          cycleCount: 0,
          failedRequestCount: 0,
          dynamicCallsExcluded: true,
        },
      },
    };

    const expandable = withSemanticNext(query, base) as LspSemanticEnvelope;
    expect(expandable).toMatchObject({
      truncated: true,
      partialReasons: ['depth'],
      next: {
        expandDepth: {
          tool: 'lspGetSemantics',
          query: { depth: 4, page: 1 },
        },
      },
    });
    expect(expandable.terminalLimit).toBeUndefined();

    const terminal = withSemanticNext(
      { ...query, depth: 20 },
      {
        ...base,
        payload: {
          ...base.payload,
          completeness: {
            ...(
              base.payload as Extract<
                LspSemanticEnvelope['payload'],
                { kind: 'callers' }
              >
            ).completeness,
            truncatedByBudget: true,
          },
        },
      }
    ) as LspSemanticEnvelope;
    expect(terminal.terminalLimit).toBe(true);
    expect(terminal.next?.expandDepth).toBeUndefined();
    expect(terminal.partialReasons).toEqual(['depth', 'budget']);
  });
});
