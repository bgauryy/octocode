/**
 * Verbosity apply-function contract tests.
 *
 * Verbosity trimming is intentionally disabled: `isConcise` and `isCompact`
 * both return false so every tool returns the full payload regardless of the
 * requested verbosity tier. This prevents unpredictable LLM behavior caused
 * by each tool producing a different shape under "concise".
 *
 * Invariants:
 *   1. All verbosity tiers (undefined / "basic" / "compact" / "concise")
 *      preserve the full data payload.
 *   2. `groupByFile:true` on lspFindReferences is a product mode (not
 *      verbosity) and still produces a byFile rollup.
 *   3. Empty/error results pass through unchanged regardless of verbosity.
 */

import { describe, it, expect } from 'vitest';
import { applyRipgrepVerbosity } from '../../src/tools/local_ripgrep/ripgrepResultBuilder.js';
import { applyFindFilesVerbosity } from '../../src/tools/local_find_files/findFiles.js';
import { applyFetchContentVerbosity } from '../../src/tools/local_fetch_content/fetchContent.js';
import { applyGotoDefinitionVerbosity } from '../../src/tools/lsp_goto_definition/execution.js';
import { applyFindReferencesVerbosity } from '../../src/tools/lsp_find_references/lsp_find_references.js';
import { applyCallHierarchyVerbosity } from '../../src/tools/lsp_call_hierarchy/callHierarchy.js';
import type { Verbosity } from '../../src/scheme/localSchemaOverlay.js';

// All verbosity tiers — trimming is disabled, so every tier must preserve
// the full data payload.
const ALL_VERBOSITIES: Array<Verbosity | undefined> = [
  undefined,
  'basic',
  'compact',
  'concise',
];

// ---------------------------------------------------------------------------
// localSearchCode (ripgrep)
// ---------------------------------------------------------------------------

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

  it.each(ALL_VERBOSITIES)(
    'verbosity=%s always preserves full files[] (trimming disabled)',
    verbosity => {
      const out = applyRipgrepVerbosity(
        baseResult,
        { ...baseQuery, verbosity },
        totals
      );
      expect(out.files).toEqual(baseResult.files);
    }
  );
});

// ---------------------------------------------------------------------------
// localFindFiles
// ---------------------------------------------------------------------------

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

  it.each(ALL_VERBOSITIES)(
    'verbosity=%s always preserves full files[] (trimming disabled)',
    verbosity => {
      const out = applyFindFilesVerbosity(
        baseResult,
        { ...baseQuery, verbosity },
        totals
      );
      expect(out.files).toEqual(baseResult.files);
    }
  );
});

// ---------------------------------------------------------------------------
// localGetFileContent (fetchContent)
// ---------------------------------------------------------------------------

describe('applyFetchContentVerbosity', () => {
  const baseResult = {
    filePath: '/repo/src/foo.ts',
    content: '// '.repeat(400) + 'export function foo() {}\n'.repeat(10),
    hints: ['Read OK'],
  };
  const baseQuery = { path: '/repo/src/foo.ts' } as never;

  it.each(ALL_VERBOSITIES)(
    'verbosity=%s always preserves full content (trimming disabled)',
    verbosity => {
      const out = applyFetchContentVerbosity(
        baseResult,
        { ...baseQuery, verbosity },
        420
      );
      expect(out.content).toBe(baseResult.content);
    }
  );
});

// ---------------------------------------------------------------------------
// lspGotoDefinition
// ---------------------------------------------------------------------------

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

  it.each(ALL_VERBOSITIES)(
    'verbosity=%s always preserves locations[].content (trimming disabled)',
    verbosity => {
      const out = applyGotoDefinitionVerbosity(baseResult, {
        ...baseQuery,
        verbosity,
      });
      expect(out.locations?.[0]?.content).toBe(
        baseResult.locations[0]!.content
      );
    }
  );
});

// ---------------------------------------------------------------------------
// lspFindReferences
// ---------------------------------------------------------------------------

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

  it.each(ALL_VERBOSITIES)(
    'verbosity=%s always preserves full locations[] (trimming disabled)',
    verbosity => {
      const result = { locations: makeRefs(10) };
      const out = applyFindReferencesVerbosity(result, {
        ...baseQuery,
        verbosity,
      });
      expect(out.locations).toEqual(result.locations);
    }
  );

  it('groupByFile:true (product mode, not verbosity) — rollup regardless of verbosity', () => {
    const result = { locations: makeRefs(20, 3) };
    const out = applyFindReferencesVerbosity(result, {
      ...baseQuery,
      groupByFile: true,
    });
    expect(out.locations).toEqual([]);
    expect(out.byFile).toBeDefined();
    expect(out.totalReferences).toBe(20);
  });

  it('empty results — pass-through unchanged', () => {
    const result = { locations: [] };
    const out = applyFindReferencesVerbosity(result, {
      ...baseQuery,
      verbosity: 'concise',
    });
    expect(out).toEqual(result);
  });
});

// ---------------------------------------------------------------------------
// lspCallHierarchy
// ---------------------------------------------------------------------------

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
          { start: { line: 14, character: 4 }, end: { line: 14, character: 7 } },
          { start: { line: 20, character: 4 }, end: { line: 20, character: 7 } },
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

  it.each(ALL_VERBOSITIES)(
    'verbosity=%s always preserves full calls[] with content (trimming disabled)',
    verbosity => {
      const out = applyCallHierarchyVerbosity(baseResult, {
        ...baseQuery,
        verbosity,
      });
      expect(out.calls).toEqual(baseResult.calls);
      expect(out.calls?.[0]?.from.content).toBeDefined();
    }
  );
});
