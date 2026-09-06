import { describe, expect, it } from 'vitest';
import { finalizeGraphOutput } from '../../../src/tools/local_analyze_graph/pagination.js';
import type {
  AnalyzeGraphOutput,
  AnalyzeGraphQuery,
} from '../../../src/tools/local_analyze_graph/analysisTypes.js';
import { LocalAnalyzeGraphQuerySchema } from '../../../src/tools/local_analyze_graph/scheme.js';
import type { GraphCoverage } from '../../../src/graph/types.js';

type Diagnostic = GraphCoverage['diagnostics'][number];
const query: AnalyzeGraphQuery = { operation: 'cycles', path: '/repo' };
const diagnostics = (count: number): Diagnostic[] =>
  Array.from({ length: count }, (_, index) => ({
    file: `src/file-${String(index).padStart(4, '0')}.rs`,
    line: index + 1,
    code: index % 2 ? 'unsupported-linking' : 'syntax-only',
    message: `Diagnostic ${index}: ${'context '.repeat(10)}`,
  }));
function output(items: Diagnostic[]): AnalyzeGraphOutput {
  return {
    operation: 'cycles',
    path: '/repo',
    results: [],
    coverage: {
      basis: 'syntactic',
      referenceBasis: 'lexical-occurrence',
      languages: [],
      imports: {
        resolved: 0,
        external: 0,
        unsupported: 0,
        unresolvedInternal: 0,
      },
      diagnostics: items,
    },
  };
}
function run(items: Diagnostic[], input = query) {
  return finalizeGraphOutput(output(items), input, false, 'Continue cycles.');
}
function continuation(
  result: AnalyzeGraphOutput,
  key = 'nextDiagnostics'
): AnalyzeGraphQuery {
  const next = result.next?.[key] as { tool: string; query: unknown };
  expect(next.tool).toBe('localAnalyzeGraph');
  return LocalAnalyzeGraphQuerySchema.parse(next.query);
}

describe('graph coverage diagnostic pagination', () => {
  it('executes schema-valid continuations whose union preserves all 638 diagnostics', () => {
    const all = diagnostics(638);
    const original = output(all);
    let result = finalizeGraphOutput(
      original,
      query,
      false,
      'Continue cycles.'
    );
    expect(result.coverage?.diagnostics).toHaveLength(25);
    expect(JSON.stringify(result).length).toBeLessThan(
      JSON.stringify(original).length * 0.25
    );
    expect(original.coverage?.diagnostics).toHaveLength(638);
    const collected: Diagnostic[] = [];
    while (true) {
      expect(result.coverage?.diagnosticsPagination?.totalEntries).toBe(638);
      expect(result.partialReasons).toEqual(
        result.coverage?.diagnosticsPagination?.hasMore
          ? ['unsupportedLinking', 'diagnosticPage']
          : ['unsupportedLinking']
      );
      collected.push(...result.coverage!.diagnostics);
      if (!result.coverage?.diagnosticsPagination?.hasMore) break;
      result = run(all, continuation(result));
    }
    expect(collected).toEqual(all);
    expect(result.next?.nextDiagnostics).toBeUndefined();
  });

  it('deduplicates identical diagnostics while retaining distinct source occurrences', () => {
    const all = diagnostics(57);
    all.push(all[0]!);
    all.push({ ...all[0]!, line: 99 });
    const first = run(all);
    const second = run([...all].reverse(), continuation(first));
    expect(second.status).not.toBe('error');
    expect(second.coverage?.diagnosticsPagination?.resultId).toBe(
      first.coverage?.diagnosticsPagination?.resultId
    );
    const third = run(all, continuation(second));
    const union = [first, second, third].flatMap(
      result => result.coverage!.diagnostics
    );
    expect(union).toHaveLength(58);
    expect(union.filter(item => item.file === all[0]!.file)).toHaveLength(2);
  });

  it('computes diagnostic counts and incompleteness from rows beyond the current page', () => {
    const all = diagnostics(26).map(item => ({
      ...item,
      code: 'syntax-only' as const,
    }));
    const last: Diagnostic = { ...all[25]!, code: 'parse-recovery' };
    const result = run([...all.slice(0, 25), last]);
    expect(
      result.coverage?.diagnostics.every(item => item.code === 'syntax-only')
    ).toBe(true);
    expect(result.coverage?.diagnosticCounts).toEqual({
      'syntax-only': 25,
      'parse-recovery': 1,
    });
    expect(result.partialReasons).toEqual(['parseRecovery', 'diagnosticPage']);
    expect(result.terminalLimit).toBe(true);
    expect(result.next?.nextDiagnostics).toBeDefined();
  });

  it.each([0, 3])(
    'keeps %s diagnostics complete without a spurious continuation',
    count => {
      const all = diagnostics(count).map(item => ({
        ...item,
        code: 'syntax-only' as const,
      }));
      const result = run(all);
      expect(result.coverage?.diagnostics).toEqual(all);
      expect(result.coverage?.diagnosticsPagination).toMatchObject({
        totalEntries: count,
        hasMore: false,
      });
      expect(result.next?.nextDiagnostics).toBeUndefined();
      expect(result.truncated).toBeUndefined();
    }
  );

  it('returns explicit stale-snapshot failure and an executable restart', () => {
    const all = diagnostics(30);
    const first = run(all);
    const changed = [...all, { ...all[0]!, line: 99 }];
    const stale = run(changed, continuation(first));
    expect(stale).toMatchObject({
      status: 'error',
      errorCode: 'graphDiagnosticsChanged',
      results: [],
    });
    expect(stale.coverage?.diagnostics).toEqual([]);
    const restarted = run(changed, continuation(stale, 'restartDiagnostics'));
    expect(restarted.status).not.toBe('error');
    expect(restarted.coverage?.diagnosticsPagination?.currentPage).toBe(1);
  });

  it('reports an out-of-range page and returns a restart continuation', () => {
    const result = run(diagnostics(30), { ...query, diagnosticPage: 99 });
    expect(result.coverage?.diagnosticsPagination).toMatchObject({
      currentPage: 2,
      outOfRange: true,
      hasMore: false,
    });
    expect(result.coverage?.diagnostics).toHaveLength(5);
    expect(continuation(result, 'restartDiagnostics').diagnosticPage).toBe(1);
  });

  it('marks a terminal diagnostic-page bound without emitting an invalid next call', () => {
    const result = run(diagnostics(100001), {
      ...query,
      diagnosticPage: 1000,
      diagnosticPageSize: 100,
    });
    expect(result.coverage?.diagnosticsPagination).toMatchObject({
      currentPage: 1000,
      totalEntries: 100001,
      hasMore: true,
      terminalLimit: true,
    });
    expect(result.terminalLimit).toBe(true);
    expect(result.next?.nextDiagnostics).toBeUndefined();
  });

  it('expands diagnostic pages before declaring a repairable page bound terminal', () => {
    const result = run(
      diagnostics(1001).map(item => ({
        ...item,
        code: 'syntax-only' as const,
      })),
      { ...query, diagnosticPage: 1000, diagnosticPageSize: 1 }
    );
    expect(result.terminalLimit).toBeUndefined();
    expect(continuation(result)).toMatchObject({
      diagnosticPage: 501,
      diagnosticPageSize: 2,
    });
    const next = run(
      diagnostics(1001).map(item => ({
        ...item,
        code: 'syntax-only' as const,
      })),
      continuation(result)
    );
    expect(next.coverage?.diagnostics).toHaveLength(1);
    expect(next.coverage?.diagnostics[0]?.file).toBe('src/file-1000.rs');
  });

  it('preserves result and scan continuations alongside diagnostic continuation', () => {
    const base = output(diagnostics(30));
    base.pagination = {
      currentPage: 1,
      totalPages: 2,
      entriesPerPage: 1,
      totalEntries: 2,
      hasMore: true,
    };
    const result = finalizeGraphOutput(
      base,
      { ...query, maxFiles: 10 },
      true,
      'Continue cycles.'
    );
    expect(continuation(result, 'nextPage').page).toBe(2);
    expect(continuation(result, 'expandScan').maxFiles).toBe(20);
    expect(continuation(result).diagnosticPage).toBe(2);
  });

  it('preserves a supplied diagnostic snapshot on result pagination and resets it for scan expansion', () => {
    const all = diagnostics(60);
    const snapshot = run(all).coverage!.diagnosticsPagination!.resultId;
    const base = output(all);
    base.pagination = {
      currentPage: 1,
      totalPages: 2,
      entriesPerPage: 1,
      totalEntries: 2,
      hasMore: true,
    };
    const result = finalizeGraphOutput(
      base,
      {
        ...query,
        maxFiles: 10,
        diagnosticSnapshot: snapshot,
        diagnosticPage: 2,
        rustWorkspace: 'cargo',
      },
      true,
      'Continue cycles.'
    );
    expect(continuation(result, 'nextPage')).toMatchObject({
      diagnosticSnapshot: snapshot,
      diagnosticPage: 2,
      rustWorkspace: 'cargo',
    });
    expect(continuation(result, 'expandScan')).toMatchObject({
      diagnosticPage: 1,
      rustWorkspace: 'cargo',
    });
    expect(
      continuation(result, 'expandScan').diagnosticSnapshot
    ).toBeUndefined();
  });
});
