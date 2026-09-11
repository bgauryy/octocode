import { afterEach, describe, expect, it, vi } from 'vitest';
import { contextUtils } from '../../../../src/utils/contextUtils.js';
import { buildFileGraph } from '../../../../src/graph/buildFileGraph.js';
import { analyzeTopology } from '../../../../src/tools/ast_search/topology/analyzeTopology.js';
import { GraphAnalysisQuerySchema } from '../../../../src/tools/ast_search/topology/scheme.js';

afterEach(() => vi.restoreAllMocks());

function mockScan(
  entries: Array<{ file: string; facts: Record<string, unknown> }>
) {
  vi.spyOn(contextUtils, 'scanGraphFacts').mockResolvedValue({
    candidatePaths: entries.map(entry => entry.file),
    filesSkipped: 0,
    truncated: false,
    entries: entries.map(entry => ({
      relativePath: entry.file,
      factsJson: JSON.stringify(entry.facts),
      referenceCounts: [],
    })),
  });
}

describe('graph coverage survives every public operation', () => {
  it('preserves native scan-skip reason codes as bounded coverage evidence', async () => {
    vi.spyOn(contextUtils, 'scanGraphFacts').mockResolvedValue({
      schemaVersion: 1,
      candidatePaths: ['src/large.ts'],
      filesSkipped: 1,
      truncated: false,
      entries: [],
      skipped: [
        {
          relativePath: 'src/large.ts',
          code: 'graph.scan.fileTooLarge',
          message: 'file exceeds the graph scan byte limit',
        },
      ],
    });

    const built = await buildFileGraph('/nonexistent-graph-fixture', [], 20);

    expect(built.filesSkipped).toBe(1);
    expect(built.coverage?.diagnostics).toEqual([
      {
        file: 'src/large.ts',
        code: 'scan-skip',
        message:
          'graph.scan.fileTooLarge: file exceeds the graph scan byte limit',
      },
    ]);
  });

  it('rejects unsupported fact schemas and reports malformed fact payloads', async () => {
    vi.spyOn(contextUtils, 'scanGraphFacts').mockResolvedValue({
      schemaVersion: 1,
      candidatePaths: ['future.ts', 'malformed.ts'],
      filesSkipped: 0,
      truncated: false,
      skipped: [],
      entries: [
        {
          relativePath: 'future.ts',
          factsJson: JSON.stringify({ schemaVersion: 2, declarations: [] }),
          referenceCounts: [],
        },
        {
          relativePath: 'malformed.ts',
          factsJson: '{not-json',
          referenceCounts: [],
        },
      ],
    });

    const built = await buildFileGraph('/nonexistent-graph-fixture', [], 20);

    expect(built.filesSkipped).toBe(2);
    expect(built.filesScanned).toBe(0);
    expect(built.coverage?.diagnostics).toEqual([
      {
        file: 'future.ts',
        code: 'facts-schema-unsupported',
        message: 'unsupported graph-fact schema version: 2',
      },
      {
        file: 'malformed.ts',
        code: 'facts-decode-failed',
        message: 'native graph facts could not be decoded',
      },
    ]);
  });

  it('rejects an unsupported outer scan envelope even when facts match it', async () => {
    vi.spyOn(contextUtils, 'scanGraphFacts').mockResolvedValue({
      schemaVersion: 2,
      candidatePaths: ['future.ts'],
      filesSkipped: 0,
      truncated: false,
      skipped: [],
      entries: [
        {
          relativePath: 'future.ts',
          factsJson: JSON.stringify({ schemaVersion: 2, declarations: [] }),
          referenceCounts: [],
        },
      ],
    });

    const built = await buildFileGraph('/nonexistent-graph-fixture', [], 20);

    expect(built.filesSkipped).toBe(1);
    expect(built.filesScanned).toBe(0);
    expect(built.coverage?.diagnostics).toEqual([
      {
        file: '.',
        code: 'facts-schema-unsupported',
        message: 'unsupported graph scan schema version: 2',
      },
    ]);
  });

  it('preserves syntax-only guidance without misreporting a parse failure', async () => {
    const message =
      'tree-sitter graph facts are syntax-only; use LSP references/callHierarchy for semantic proof';
    mockScan([
      {
        file: 'src/lib.rs',
        facts: { language: 'rust', diagnostics: [message] },
      },
    ]);
    const built = await buildFileGraph('/nonexistent-graph-fixture', [], 20);
    const output = await analyzeTopology(
      { operation: 'cycles', path: '/nonexistent-graph-fixture' },
      { getGraph: () => built }
    );
    expect(output.coverage?.diagnostics).toEqual([
      { file: 'src/lib.rs', code: 'syntax-only', message },
    ]);
    expect(output.partialReasons).toBeUndefined();
    expect(output.terminalLimit).toBeUndefined();
  });
  it('does not link an overridden module to a coincidentally existing conventional file', async () => {
    mockScan([
      {
        file: 'src/lib.rs',
        facts: {
          modules: [
            {
              name: 'child',
              line: 2,
              scope: [],
              inline: false,
              unsupported: true,
            },
            { name: 'consumer', line: 3, scope: [], inline: false },
          ],
          imports: [
            {
              specifier: 'self::child',
              importKind: 'module',
              resolutionHint: 'unsupported',
              line: 2,
            },
          ],
        },
      },
      {
        file: 'src/consumer.rs',
        facts: { imports: [{ specifier: 'crate::child::Thing', line: 1 }] },
      },
      { file: 'src/child.rs', facts: {} },
    ]);
    const built = await buildFileGraph('/nonexistent-graph-fixture', [], 20);
    expect([...built.fileGraph.get('src/consumer.rs')!.importsFiles]).toEqual(
      []
    );
    expect(built.coverage?.imports.unsupported).toBe(2);
  });

  it('links conventional Rust files and preserves per-import coverage', async () => {
    mockScan([
      {
        file: 'src/lib.rs',
        facts: {
          modules: [{ name: 'structural', line: 1, scope: [], inline: false }],
        },
      },
      {
        file: 'src/structural/mod.rs',
        facts: {
          modules: ['files', 'language', 'query'].map(name => ({
            name,
            line: 1,
            scope: [],
            inline: false,
          })),
        },
      },
      {
        file: 'src/structural/files.rs',
        facts: {
          language: 'rust',
          imports: [
            { specifier: 'super::language::AgLanguage', line: 9 },
            { specifier: 'super::query::Prefilter', line: 11 },
            { specifier: 'std::fs', line: 2 },
            { specifier: 'rayon::prelude::*', line: 7 },
            { specifier: 'crate::missing::Thing', line: 14 },
          ],
        },
      },
      { file: 'src/structural/language.rs', facts: {} },
      { file: 'src/structural/query.rs', facts: {} },
    ]);
    const built = await buildFileGraph('/nonexistent-graph-fixture', [], 20);
    expect(
      [...built.fileGraph.get('src/structural/files.rs')!.importsFiles].sort()
    ).toEqual(['src/structural/language.rs', 'src/structural/query.rs']);
    expect(built.coverage?.imports).toEqual({
      resolved: 2,
      external: 1,
      unresolvedInternal: 1,
      unsupported: 1,
    });
    expect(built.coverage?.referenceBasis).toBe('lexical-occurrence');
    const output = await analyzeTopology(
      {
        operation: 'dependencies',
        path: '/nonexistent-graph-fixture',
        file: 'src/structural/files.rs',
      },
      { getGraph: () => built }
    );
    expect(output.partialReasons).toEqual([
      'unresolvedImports',
      'unsupportedLinking',
    ]);
    expect(output.terminalLimit).toBe(true);
    expect(output.completeness).toEqual({
      results: 'complete',
      graph: 'coverage-incomplete',
      diagnostics: 'complete',
      coverageGapReasons: ['unresolvedImports', 'unsupportedLinking'],
    });
    expect(output.coverage).toMatchObject(built.coverage!);
  });

  it.each(['cycles', 'deadCode'] as const)(
    'preserves parse recovery and unsupported language in %s',
    async operation => {
      const messages = Array.from(
        { length: 70 },
        (_, index) => `parser recovered at ${index}`
      );
      mockScan([
        {
          file: 'index.js',
          facts: {
            language: 'js',
            diagnostics: messages,
            imports: [{ specifier: './missing.js', line: 1 }],
          },
        },
        {
          file: 'other.java',
          facts: {
            language: 'java',
            declarations: [
              {
                id: 'declaration:other.java#lambda@0:function',
                name: 'lambda',
                kind: 'function',
                line: 1,
                exported: true,
              },
            ],
          },
        },
      ]);
      const built = await buildFileGraph('/nonexistent-graph-fixture', [], 20);
      // JavaScript reserved-name filtering must not silently erase other language facts.
      expect(built.facts.get('other.java')?.declarations).toHaveLength(1);
      let output = await analyzeTopology(
        {
          operation,
          path: '/nonexistent-graph-fixture',
          ...(operation === 'deadCode' ? { entrypoints: ['index.js'] } : {}),
        },
        { getGraph: () => built }
      );
      expect(output.partialReasons).toEqual([
        'parseRecovery',
        'unresolvedImports',
        'unsupportedLinking',
        'diagnosticPage',
      ]);
      expect(output.terminalLimit).toBe(true);
      expect(output.completeness).toEqual({
        results: 'complete',
        graph: 'coverage-incomplete',
        diagnostics: 'pageable',
        coverageGapReasons: [
          'parseRecovery',
          'unresolvedImports',
          'unsupportedLinking',
        ],
      });
      if (operation === 'deadCode') {
        expect(output.confidence).toBe('low');
        expect(
          output.warnings?.some(message =>
            message.includes('Incomplete import linking')
          )
        ).toBe(true);
      }
      const collected = [...output.coverage!.diagnostics];
      while (output.coverage?.diagnosticsPagination?.hasMore) {
        const next = output.next!.nextDiagnostics as { query: unknown };
        output = await analyzeTopology(
          GraphAnalysisQuerySchema.parse(next.query),
          { getGraph: () => built }
        );
        collected.push(...output.coverage!.diagnostics);
      }
      expect(
        collected
          .filter(item => item.code === 'parse-recovery')
          .map(item => item.message)
          .sort()
      ).toEqual([...messages].sort());
      expect(output.next?.expandScan).toBeUndefined();
    }
  );
});
