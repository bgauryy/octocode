import { describe, it, expect } from 'vitest';
import { applyRipgrepVerbosity } from '../../src/tools/local_ripgrep/ripgrepResultBuilder.js';
import { applyFindFilesVerbosity } from '../../src/tools/local_find_files/findFiles.js';
import { applyFetchContentVerbosity } from '../../src/tools/local_fetch_content/fetchContent.js';
import { applyGotoDefinitionVerbosity } from '../../src/tools/lsp_goto_definition/execution.js';
import { applyFindReferencesVerbosity } from '../../src/tools/lsp_find_references/lsp_find_references.js';
import { applyCallHierarchyVerbosity } from '../../src/tools/lsp_call_hierarchy/callHierarchy.js';

describe('applyRipgrepVerbosity', () => {
  const baseResult = {
    files: [
      {
        path: '/repo/src/foo.ts',
        matchCount: 3,
        matches: [{ line: 12, value: 'export function foo' }],
      },
      {
        path: '/repo/src/bar.ts',
        matchCount: 1,
        matches: [{ line: 5, value: 'foo()' }],
      },
    ],
    searchEngine: 'rg' as const,
    pagination: {
      currentPage: 1,
      totalPages: 1,
      filesPerPage: 20,
      totalFiles: 2,
      hasMore: false,
    },
    warnings: [],
    hints: ['Page 1/1'],
  };
  const baseQuery = { pattern: 'foo', path: '/repo' } as never;
  const totals = { totalMatches: 4, totalFiles: 2 };

  it('verbose:false preserves full files[] (research data only)', () => {
    const out = applyRipgrepVerbosity(
      baseResult,
      { ...baseQuery, verbose: false },
      totals
    );
    expect(out.files).toEqual(baseResult.files);
  });

  it('verbose:true also preserves full files[] (metadata is additive)', () => {
    const out = applyRipgrepVerbosity(
      baseResult,
      { ...baseQuery, verbose: true },
      totals
    );
    expect(out.files).toEqual(baseResult.files);
  });

  it('omitted verbose preserves full files[]', () => {
    const out = applyRipgrepVerbosity(baseResult, baseQuery, totals);
    expect(out.files).toEqual(baseResult.files);
  });
});

describe('applyFindFilesVerbosity', () => {
  const baseResult = {
    files: [
      { path: '/repo/src/foo.ts', name: 'foo.ts' },
      { path: '/repo/src/bar.ts', name: 'bar.ts' },
      { path: '/repo/test/baz.test.ts', name: 'baz.test.ts' },
    ],
    pagination: {
      currentPage: 1,
      totalPages: 1,
      filesPerPage: 20,
      totalFiles: 3,
      hasMore: false,
    },
    hints: ['Page 1/1'],
  };
  const baseQuery = { path: '/repo' } as never;
  const totals = { totalFiles: 3 };

  it('verbose:false preserves full files[]', () => {
    const out = applyFindFilesVerbosity(
      baseResult,
      { ...baseQuery, verbose: false },
      totals
    );
    expect(out.files).toEqual(baseResult.files);
  });

  it('verbose:true preserves full files[]', () => {
    const out = applyFindFilesVerbosity(
      baseResult,
      { ...baseQuery, verbose: true },
      totals
    );
    expect(out.files).toEqual(baseResult.files);
  });

  it('verbose:false + sortBy=modified preserves modified timestamps', () => {
    const resultWithTimestamps = {
      ...baseResult,
      files: [
        {
          path: '/repo/src/foo.ts',
          name: 'foo.ts',
          modified: '2026-05-01T10:00:00Z',
          size: 1000,
          permissions: '644',
        },
        {
          path: '/repo/src/bar.ts',
          name: 'bar.ts',
          modified: '2026-04-01T10:00:00Z',
          size: 500,
          permissions: '644',
        },
      ],
    };

    const out = applyFindFilesVerbosity(
      resultWithTimestamps as never,
      { ...baseQuery, sortBy: 'modified', verbose: false } as never,
      totals
    );

    for (const file of out.files ?? []) {
      expect((file as Record<string, unknown>).modified).toBeDefined();
    }
    for (const file of out.files ?? []) {
      expect((file as Record<string, unknown>).size).toBeUndefined();
      expect((file as Record<string, unknown>).permissions).toBeUndefined();
    }
  });

  it('verbose:false without sortBy=modified strips modified timestamps', () => {
    const resultWithTimestamps = {
      ...baseResult,
      files: [
        {
          path: '/repo/src/foo.ts',
          name: 'foo.ts',
          modified: '2026-05-01T10:00:00Z',
        },
      ],
    };

    const out = applyFindFilesVerbosity(
      resultWithTimestamps as never,
      { ...baseQuery, verbose: false } as never,
      totals
    );

    for (const file of out.files ?? []) {
      expect((file as Record<string, unknown>).modified).toBeUndefined();
    }
  });
});

describe('applyFetchContentVerbosity', () => {
  const baseResult = {
    filePath: '/repo/src/foo.ts',
    content: '// '.repeat(400) + 'export function foo() {}\n'.repeat(10),
    hints: ['Read OK'],
  };
  const baseQuery = { path: '/repo/src/foo.ts' } as never;

  it('verbose:false preserves full content', () => {
    const out = applyFetchContentVerbosity(
      baseResult,
      { ...baseQuery, verbose: false },
      420
    );
    expect(out.content).toBe(baseResult.content);
  });

  it('verbose:true preserves full content', () => {
    const out = applyFetchContentVerbosity(
      baseResult,
      { ...baseQuery, verbose: true },
      420
    );
    expect(out.content).toBe(baseResult.content);
  });
});

describe('applyGotoDefinitionVerbosity', () => {
  const baseResult = {
    locations: [
      {
        uri: '/repo/src/foo.ts',
        range: {
          start: { line: 11, character: 9 },
          end: { line: 11, character: 12 },
        },
        content: '   12| export function foo() {}',
      },
      {
        uri: '/repo/src/foo.ts',
        range: {
          start: { line: 41, character: 0 },
          end: { line: 41, character: 3 },
        },
        content: '   42| const foo = 1',
      },
    ],
    resolvedPosition: { line: 11, character: 9 },
    searchRadius: 5,
    hints: ['Found 2'],
  };
  const baseQuery = {
    uri: '/repo/src/foo.ts',
    symbolName: 'foo',
    lineHint: 12,
  } as never;

  it('verbose:false preserves locations[].content', () => {
    const out = applyGotoDefinitionVerbosity(baseResult, {
      ...baseQuery,
      verbose: false,
    });
    expect(out.locations?.[0]?.content).toBe(baseResult.locations[0]!.content);
  });

  it('verbose:true preserves locations[].content', () => {
    const out = applyGotoDefinitionVerbosity(baseResult, {
      ...baseQuery,
      verbose: true,
    });
    expect(out.locations?.[0]?.content).toBe(baseResult.locations[0]!.content);
  });
});

describe('applyFindReferencesVerbosity', () => {
  function makeRefs(n: number, filesCount = 4) {
    return Array.from({ length: n }).map((_, i) => ({
      uri: `/repo/src/file${i % filesCount}.ts`,
      range: {
        start: { line: i, character: 0 },
        end: { line: i, character: 3 },
      },
      value: 'foo',
    }));
  }

  const baseQuery = {
    uri: '/repo/src/file0.ts',
    symbolName: 'foo',
    lineHint: 1,
  } as never;

  it('verbose:false preserves full locations[]', () => {
    const result = { locations: makeRefs(10) };
    const out = applyFindReferencesVerbosity(result, {
      ...baseQuery,
      verbose: false,
    });
    expect(out.locations).toEqual(result.locations);
  });

  it('verbose:true preserves full locations[]', () => {
    const result = { locations: makeRefs(10) };
    const out = applyFindReferencesVerbosity(result, {
      ...baseQuery,
      verbose: true,
    });
    expect(out.locations).toEqual(result.locations);
  });

  it('groupByFile:true — rollup produces byFile with required schema fields', () => {
    const result = { locations: makeRefs(20, 3) };
    const out = applyFindReferencesVerbosity(result, {
      ...baseQuery,
      groupByFile: true,
    });
    expect(out.locations).toEqual([]);
    expect(out.byFile).toBeDefined();
    expect(out.totalReferences).toBe(20);
    const firstEntry = out.byFile?.[0];
    expect(typeof firstEntry?.count).toBe('number');
    expect(typeof firstEntry?.firstLine).toBe('number');
    expect(typeof firstEntry?.firstCharacter).toBe('number');
    expect(out.byFile?.length).toBe(3);
  });

  it('empty results — pass-through unchanged', () => {
    const result = { locations: [] };
    const out = applyFindReferencesVerbosity(result, {
      ...baseQuery,
      verbose: false,
    });
    expect(out).toEqual(result);
  });
});

describe('applyCallHierarchyVerbosity', () => {
  const baseResult = {
    direction: 'incoming' as const,
    depth: 1,
    root: {
      symbol: {
        name: 'doWork',
        uri: '/repo/src/foo.ts',
        range: {
          start: { line: 9, character: 0 },
          end: { line: 9, character: 6 },
        },
      },
    },
    calls: [
      {
        from: {
          name: 'serve',
          uri: '/repo/src/server.ts',
          content: 'function serve() {\n  doWork();\n}',
          range: {
            start: { line: 12, character: 0 },
            end: { line: 12, character: 5 },
          },
        },
        fromRanges: [
          {
            start: { line: 14, character: 4 },
            end: { line: 14, character: 7 },
          },
          {
            start: { line: 20, character: 4 },
            end: { line: 20, character: 7 },
          },
        ],
      },
      {
        from: {
          name: 'main',
          uri: '/repo/src/main.ts',
          content: 'function main() {\n  doWork();\n}',
          range: {
            start: { line: 1, character: 0 },
            end: { line: 1, character: 4 },
          },
        },
        fromRanges: [
          { start: { line: 3, character: 0 }, end: { line: 3, character: 3 } },
        ],
      },
    ],
    hints: ['hierarchy'],
  };
  const baseQuery = {
    uri: '/repo/src/foo.ts',
    symbolName: 'doWork',
    lineHint: 10,
    direction: 'incoming',
  } as never;

  it('verbose:false preserves full calls[] with content', () => {
    const out = applyCallHierarchyVerbosity(baseResult, {
      ...baseQuery,
      verbose: false,
    });
    expect(out.calls).toEqual(baseResult.calls);
    expect(out.calls?.[0]?.from.content).toBeDefined();
  });

  it('verbose:true preserves full calls[] with content', () => {
    const out = applyCallHierarchyVerbosity(baseResult, {
      ...baseQuery,
      verbose: true,
    });
    expect(out.calls).toEqual(baseResult.calls);
    expect(out.calls?.[0]?.from.content).toBeDefined();
  });
});
