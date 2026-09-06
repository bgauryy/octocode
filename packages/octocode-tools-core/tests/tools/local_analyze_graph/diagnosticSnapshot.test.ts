import { describe, expect, it, vi } from 'vitest';
import * as coverage from '../../../src/graph/coverage.js';
import { prepareGraphDiagnostics } from '../../../src/graph/diagnosticSnapshot.js';
import type { GraphCoverage } from '../../../src/graph/types.js';
import { finalizeGraphOutput } from '../../../src/tools/local_analyze_graph/pagination.js';

const make = (): GraphCoverage['diagnostics'] =>
  Array.from({ length: 110 }, (_, index) => ({
    file: `file${index}.rs`,
    line: index,
    code: 'syntax-only',
    message: `fact ${index}`,
  }));

describe('immutable graph diagnostic preparation', () => {
  it('prepares once for multiple queries sharing one completed graph', () => {
    const spy = vi.spyOn(coverage, 'canonicalGraphDiagnostics');
    try {
      const prepared = prepareGraphDiagnostics(make());
      for (let page = 1; page <= 5; page++) {
        const output = finalizeGraphOutput(
          {
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
              diagnostics: prepared.diagnostics,
            },
          },
          { operation: 'cycles', path: '/repo', diagnosticPage: page },
          false,
          'Continue.'
        );
        expect(output.coverage?.diagnosticsPagination?.resultId).toBe(
          prepared.resultId
        );
        expect(output.coverage?.diagnosticCounts).toEqual({
          'syntax-only': 110,
        });
      }
      expect(spy).toHaveBeenCalledTimes(1);
    } finally {
      spy.mockRestore();
    }
  });

  it('does not cache mutable inputs or retain mutable diagnostic aliases', () => {
    const items = make();
    const first = prepareGraphDiagnostics(items);
    items[0]!.message = 'changed';
    items.push({ file: 'new.rs', code: 'parse-recovery', message: 'new' });
    const second = prepareGraphDiagnostics(items);
    expect(second.resultId).not.toBe(first.resultId);
    expect(first.diagnostics[0]!.message).toBe('fact 0');
    expect(first.diagnostics).toHaveLength(110);
    expect(second.diagnostics).toHaveLength(111);
    expect(Object.isFrozen(first.diagnostics)).toBe(true);
    expect(Object.isFrozen(first.diagnostics[0])).toBe(true);
    expect(Object.isFrozen(first.diagnosticCounts)).toBe(true);
  });
});
