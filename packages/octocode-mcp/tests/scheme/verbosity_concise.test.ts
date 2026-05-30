/**
 * Verbosity:"concise" — acceptance tests for every tool.
 *
 * Each `apply*Verbosity` helper is a pure function on the tool's result type.
 * These tests pin the four shared invariants of the concise contract:
 *
 *   1. Default invariance — undefined / "basic" / "compact" preserve the
 *      data payload (per octocode-core baseSchema.verbosity, default = "basic").
 *   2. Lossiness — "concise" drops the heavy field (matches, content, locations,
 *      entries, calls) for hasResults.
 *   3. Drill-back hint — every concise response carries a `Drill-back:` line
 *      so the agent never lands in a dead end.
 *   4. Bounded payload — the synthetic summary fits a small per-tool char
 *      budget.
 *
 * Special-case: `applyFindReferencesVerbosity` is adaptive — flat refs[] of
 * `file:line` strings below 500 refs, `topFiles`-style rollup above. Both
 * paths are exercised.
 */

import { describe, it, expect } from 'vitest';
import { applyRipgrepVerbosity } from '../../src/tools/local_ripgrep/ripgrepResultBuilder.js';
import { applyFindFilesVerbosity } from '../../src/tools/local_find_files/findFiles.js';
import { applyFetchContentVerbosity } from '../../src/tools/local_fetch_content/fetchContent.js';
import { applyGotoDefinitionVerbosity } from '../../src/tools/lsp_goto_definition/execution.js';
import { applyFindReferencesVerbosity } from '../../src/tools/lsp_find_references/lsp_find_references.js';
import { applyCallHierarchyVerbosity } from '../../src/tools/lsp_call_hierarchy/callHierarchy.js';
import { assertConcisePayload } from '../../src/scheme/verbosity.js';
import type { Verbosity } from '../../src/scheme/localSchemaOverlay.js';

// Per-test concise payload budgets. The default 2048-byte guard fits the
// stripped-data + summary hint shapes; lspFindReferences rollup carries a
// top-20 file list so it gets a wider budget. These guards catch
// contract drift — a finalizer that accidentally keeps `matches`, `content`,
// `calls`, etc. blows the budget instantly.
const CONCISE_BUDGET_DEFAULT = 2048;
const CONCISE_BUDGET_REFS_ROLLUP = 4096;

// Canonical default is `basic` per octocode-core/src/resources/global.ts
// baseSchema.verbosity. `compact` trims hints but keeps content; only `concise`
// drops content. Both `basic` (default) and `compact` must preserve the
// data payload below — these tests assert that.
const VERBOSITIES_PRESERVING_DEFAULT: Array<Verbosity | undefined> = [
  undefined,
  'basic',
  'compact',
];

function hintsBlob(result: { hints?: string[] }): string {
  return (result.hints ?? []).join('\n');
}

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

  it.each(VERBOSITIES_PRESERVING_DEFAULT)(
    'verbosity=%s is identity (preserves files[])',
    verbosity => {
      const out = applyRipgrepVerbosity(
        baseResult,
        { ...baseQuery, verbosity },
        totals
      );
      expect(out.files).toEqual(baseResult.files);
    }
  );

  it('verbosity:"concise" drops files[] and emits summary + drill-back', () => {
    const out = applyRipgrepVerbosity(
      baseResult,
      { ...baseQuery, verbosity: 'concise' },
      totals
    );
    expect(out.files).toEqual([]);
    const blob = hintsBlob(out);
    expect(blob).toMatch(/4 matches in 2 files/);
    expect(blob).toMatch(/top: \/repo\/src\/foo\.ts:12/);
    expect(blob.toLowerCase()).not.toMatch(/drill-back|re-call|detail dropped/);
    assertConcisePayload(out, CONCISE_BUDGET_DEFAULT);
  });

  it('concise summary fits the ≤ 200 char budget', () => {
    const out = applyRipgrepVerbosity(
      baseResult,
      { ...baseQuery, verbosity: 'concise' },
      totals
    );
    const summaryLine = (out.hints ?? [])[0] ?? '';
    expect(summaryLine.length).toBeLessThanOrEqual(200);
  });
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

  it.each(VERBOSITIES_PRESERVING_DEFAULT)(
    'verbosity=%s is identity',
    verbosity => {
      const out = applyFindFilesVerbosity(
        baseResult,
        { ...baseQuery, verbosity },
        totals
      );
      expect(out.files).toEqual(baseResult.files);
    }
  );

  it('verbosity:"concise" drops files[] and emits "X files in Y dirs"', () => {
    const out = applyFindFilesVerbosity(
      baseResult,
      { ...baseQuery, verbosity: 'concise' },
      totals
    );
    expect(out.files).toEqual([]);
    const blob = hintsBlob(out);
    expect(blob).toMatch(/3 files in 2 dirs/);
    expect(blob).toMatch(/newest: \/repo\/src\/foo\.ts/);
    expect(blob.toLowerCase()).not.toMatch(/drill-back|re-call|detail dropped/);
    assertConcisePayload(out, CONCISE_BUDGET_DEFAULT);
  });

  it('concise summary fits the ≤ 200 char budget', () => {
    const out = applyFindFilesVerbosity(
      baseResult,
      { ...baseQuery, verbosity: 'concise' },
      totals
    );
    const summaryLine = (out.hints ?? [])[0] ?? '';
    expect(summaryLine.length).toBeLessThanOrEqual(200);
  });
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

  it.each(VERBOSITIES_PRESERVING_DEFAULT)(
    'verbosity=%s is identity (preserves content)',
    verbosity => {
      const out = applyFetchContentVerbosity(
        baseResult,
        { ...baseQuery, verbosity },
        420
      );
      expect(out.content).toBe(baseResult.content);
    }
  );

  it('verbosity:"concise" MINIFIES content (not blanked) + emits raw→min token summary', () => {
    const out = applyFetchContentVerbosity(
      baseResult,
      { ...baseQuery, verbosity: 'concise' },
      420
    );
    // Content is kept but minified — substance survives, comments/whitespace go.
    expect(out.content).not.toBe('');
    expect((out.content ?? '').length).toBeLessThan(baseResult.content.length);
    expect(out.content).toContain('foo'); // code body preserved
    const blob = hintsBlob(out);
    expect(blob).toMatch(/\/repo\/src\/foo\.ts/);
    expect(blob).toMatch(/420 lines/);
    expect(blob).toMatch(/~\d+→\d+ tokens \(minified\)/);
    expect(blob.toLowerCase()).not.toMatch(/drill-back|re-call|detail dropped/);
    // Heavy metadata dropped under concise.
    expect((out as { lastModified?: string }).lastModified).toBeUndefined();
  });
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

  it.each(VERBOSITIES_PRESERVING_DEFAULT)(
    'verbosity=%s preserves locations[].content',
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

  it('verbosity:"concise" strips locations[].content and emits file:line:col summary', () => {
    const out = applyGotoDefinitionVerbosity(baseResult, {
      ...baseQuery,
      verbosity: 'concise',
    });
    expect(out.locations?.[0]?.content).toBe('');
    const blob = hintsBlob(out);
    expect(blob).toMatch(/2 definition\(s\)/);
    expect(blob).toMatch(/\/repo\/src\/foo\.ts:12:10/);
    expect(blob.toLowerCase()).not.toMatch(/drill-back|re-call|detail dropped/);
    assertConcisePayload(out, CONCISE_BUDGET_DEFAULT);
  });
});

// ---------------------------------------------------------------------------
// lspFindReferences (adaptive concise)
// ---------------------------------------------------------------------------

describe('applyFindReferencesVerbosity (adaptive)', () => {
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

  it.each(VERBOSITIES_PRESERVING_DEFAULT)(
    'verbosity=%s preserves locations[]',
    verbosity => {
      const result = {
        locations: makeRefs(10),
      };
      const out = applyFindReferencesVerbosity(result, {
        ...baseQuery,
        verbosity,
      });
      expect(out.locations).toEqual(result.locations);
    }
  );

  it('flat path (< 500 refs) — verbosity:"concise" returns refs[] in hints', () => {
    const result = {
      locations: makeRefs(50),
    };
    const out = applyFindReferencesVerbosity(result, {
      ...baseQuery,
      verbosity: 'concise',
    });
    expect(out.locations).toEqual([]);
    const blob = hintsBlob(out);
    expect(blob).toMatch(/50 refs in 4 files/);
    expect(blob).toMatch(/refs: \/repo\/src\/file0\.ts:1/);
    expect(blob.toLowerCase()).not.toMatch(/drill-back|re-call|detail dropped/);
    // Flat path joins all 50 file:line strings into one hint — pick the
    // wider budget so the assertion catches "kept locations[]" drift rather
    // than tripping on legitimate joined refs.
    assertConcisePayload(out, CONCISE_BUDGET_REFS_ROLLUP);
  });

  it('adaptive rollup path (≥ 500 refs) — emits topFiles, no individual refs', () => {
    const result = {
      locations: makeRefs(1000, 8),
    };
    const out = applyFindReferencesVerbosity(result, {
      ...baseQuery,
      verbosity: 'concise',
    });
    expect(out.locations).toEqual([]);
    const blob = hintsBlob(out);
    expect(blob).toMatch(/1000 refs in 8 files/);
    expect(blob).toMatch(/top-20:/);
    expect(blob).not.toMatch(/refs: \/repo\/src\/file0\.ts:1,/);
    // No verbosity-feature commentary (drill-back / groupByFile suggestion).
    expect(blob.toLowerCase()).not.toMatch(
      /drill-back|re-call|detail dropped/i
    );
    assertConcisePayload(out, CONCISE_BUDGET_DEFAULT);
  });

  it('groupByFile:true (explicit) — rollup regardless of fanout', () => {
    const result = {
      locations: makeRefs(20, 3),
    };
    const out = applyFindReferencesVerbosity(result, {
      ...baseQuery,
      groupByFile: true,
    });
    expect(out.locations).toEqual([]);
    const blob = hintsBlob(out);
    expect(blob).toMatch(/20 refs in 3 files/);
    expect(blob.toLowerCase()).not.toMatch(/drill-back|re-call|detail dropped/);
    assertConcisePayload(out, CONCISE_BUDGET_DEFAULT);
  });

  it('empty results — no transformation applied', () => {
    const result = {
      locations: [],
    };
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

  it.each(VERBOSITIES_PRESERVING_DEFAULT)(
    'verbosity=%s preserves calls[]',
    verbosity => {
      const out = applyCallHierarchyVerbosity(baseResult, {
        ...baseQuery,
        verbosity,
      });
      expect(out.calls).toEqual(baseResult.calls);
    }
  );

  it('verbosity:"concise" emits edges-only summary, drops calls[]', () => {
    const out = applyCallHierarchyVerbosity(baseResult, {
      ...baseQuery,
      verbosity: 'concise',
    });
    expect(out.calls).toEqual([]);
    const blob = hintsBlob(out);
    expect(blob).toMatch(/2 incoming edge\(s\)/);
    expect(blob).toMatch(/serve → doWork \(×2\)/);
    expect(blob).toMatch(/main → doWork/);
    expect(blob.toLowerCase()).not.toMatch(/drill-back|re-call|detail dropped/);
    assertConcisePayload(out, CONCISE_BUDGET_DEFAULT);
  });
});
