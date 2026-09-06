import { afterEach, describe, expect, it, vi } from 'vitest';
import { contextUtils } from '../../../src/utils/contextUtils.js';
import { executeAnalyzeGraph } from '../../../src/tools/local_analyze_graph/execution.js';
import { LocalAnalyzeGraphQuerySchema } from '../../../src/tools/local_analyze_graph/scheme.js';

afterEach(() => vi.restoreAllMocks());

describe('public graph diagnostic completeness', () => {
  it.each(['graph.parse.deadlineExceeded', 'graph.traversal.deadlineExceeded'])(
    'reports native %s as terminal partial analysis, never successful absence',
    async message => {
      vi.spyOn(contextUtils, 'scanGraphFacts').mockResolvedValue({
        candidatePaths: ['src/lib.rs'],
        filesSkipped: 0,
        truncated: false,
        entries: [
          {
            relativePath: 'src/lib.rs',
            referenceCounts: [],
            factsJson: JSON.stringify({
              language: 'rust',
              declarations: [],
              imports: [],
              calls: [],
              modules: [],
              rustRootUnsupported: true,
              diagnostics: [message],
            }),
          },
        ],
      });
      const result = await executeAnalyzeGraph({
        queries: [{ operation: 'cycles', path: process.cwd() }],
      });
      const row = (
        result.structuredContent as {
          results: Array<{
            meta: { diagnostics?: { partial?: boolean } };
            data: Record<string, unknown>;
          }>;
        }
      ).results[0]!;
      expect(row.meta.diagnostics?.partial).toBe(true);
      expect(row.data).toMatchObject({
        terminalLimit: true,
        coverage: { diagnostics: [{ code: 'parse-recovery', message }] },
      });
    }
  );

  it('rescans on a separate invocation and rejects a changed diagnostic snapshot', async () => {
    const scan = vi.spyOn(contextUtils, 'scanGraphFacts');
    const fixture = (suffix: string) => ({
      candidatePaths: ['src/lib.rs'],
      filesSkipped: 0,
      truncated: false,
      entries: [
        {
          relativePath: 'src/lib.rs',
          referenceCounts: [],
          factsJson: JSON.stringify({
            language: 'rust',
            diagnostics: Array.from(
              { length: 30 },
              (_, i) =>
                `tree-sitter graph facts are syntax-only; ${i} ${suffix}`
            ),
          }),
        },
      ],
    });
    scan
      .mockResolvedValueOnce(fixture('first'))
      .mockResolvedValueOnce(fixture('changed'));
    const first = await executeAnalyzeGraph({
      queries: [{ operation: 'cycles', path: process.cwd() }],
    });
    type Row = {
      data: { errorCode?: string; next?: Record<string, { query: unknown }> };
    };
    const row = (first.structuredContent as { results: Row[] }).results[0]!;
    const next = LocalAnalyzeGraphQuerySchema.parse(
      row.data.next!.nextDiagnostics!.query
    );
    const second = await executeAnalyzeGraph({ queries: [next] });
    expect(scan).toHaveBeenCalledTimes(2);
    const changed = (second.structuredContent as { results: Row[] })
      .results[0]!;
    expect(changed.data.errorCode).toBe('graphDiagnosticsChanged');
    expect(changed.data.next?.restartDiagnostics).toBeDefined();
  });

  it('marks a page of informational diagnostics partial and executes its public continuation', async () => {
    vi.spyOn(contextUtils, 'scanGraphFacts').mockResolvedValue({
      candidatePaths: ['src/lib.rs'],
      filesSkipped: 0,
      truncated: false,
      entries: [
        {
          relativePath: 'src/lib.rs',
          referenceCounts: [],
          factsJson: JSON.stringify({
            language: 'rust',
            diagnostics: Array.from(
              { length: 30 },
              (_, index) =>
                `tree-sitter graph facts are syntax-only; informational diagnostic ${index}`
            ),
          }),
        },
      ],
    });
    const first = await executeAnalyzeGraph({
      queries: [{ operation: 'cycles', path: process.cwd() }],
    });
    type Row = {
      meta: { diagnostics?: { partial?: boolean } };
      data: {
        truncated?: boolean;
        partialReasons?: string[];
        coverage: { diagnostics: unknown[] };
        next?: { nextDiagnostics?: { tool: string; query: unknown } };
      };
    };
    const row = (first.structuredContent as { results: Row[] }).results[0]!;
    expect(row.meta.diagnostics?.partial).toBe(true);
    expect(row.data).toMatchObject({
      truncated: true,
      partialReasons: ['diagnosticPage'],
    });
    expect(row.data.coverage.diagnostics).toHaveLength(25);
    const next = row.data.next!.nextDiagnostics!;
    expect(next.tool).toBe('localAnalyzeGraph');
    const second = await executeAnalyzeGraph({
      queries: [LocalAnalyzeGraphQuerySchema.parse(next.query)],
    });
    const last = (second.structuredContent as { results: Row[] }).results[0]!;
    expect(last.data.coverage.diagnostics).toHaveLength(5);
    expect(last.data.truncated).toBeUndefined();
    expect(last.meta.diagnostics?.partial).toBeUndefined();
  });
});
