/**
 * Execution-path tests for the OQL open-gap closures that need a clone/inspect/
 * content backend (gaps 7, 8 direct-file lane, 9). The backing tool runner is
 * mocked so these assert OQL's mapping + continuation behavior, not the tools.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';

const { runDirect } = vi.hoisted(() => ({ runDirect: vi.fn() }));
vi.mock('../../src/oql/adapters/runner.js', async importOriginal => ({
  ...(await importOriginal<object>()),
  runDirect,
}));

import { runOqlSearch } from '../../src/oql/run.js';
import {
  isBatchEnvelope,
  type OqlCodeResultRow,
  type OqlContentResultRow,
  type OqlContinuation,
  type OqlRecordResultRow,
  type OqlResultEnvelope,
  type OqlTreeResultRow,
} from '../../src/oql/types.js';

function single(
  r: Awaited<ReturnType<typeof runOqlSearch>>
): OqlResultEnvelope {
  if (isBatchEnvelope(r)) throw new Error('expected single envelope');
  return r;
}
function toolResult(
  data: Record<string, unknown>,
  status = 'success',
  extraStructuredContent: Record<string, unknown> = {}
) {
  return {
    content: [],
    structuredContent: {
      ...extraStructuredContent,
      results: [{ status, data }],
    },
  };
}

beforeEach(() => runDirect.mockReset());

describe('GitHub structure execution filters OQL tree rows', () => {
  it('applies extension filters while preserving directories', async () => {
    runDirect.mockResolvedValue(
      toolResult({
        structure: {
          '.': {
            folders: ['src', 'tests'],
            files: ['README.md', 'package.json', 'index.ts'],
          },
          src: {
            files: ['cli.ts', 'README.md'],
          },
        },
      })
    );

    const env = single(
      await runOqlSearch({
        target: 'structure',
        repo: 'microsoft/playwright-mcp',
        fetch: { tree: { maxDepth: 2, extensions: ['ts'] } },
      })
    );

    expect(runDirect).toHaveBeenCalledWith(
      'ghViewRepoStructure',
      expect.objectContaining({ owner: 'microsoft', repo: 'playwright-mcp' })
    );
    expect(env.results.map(row => (row as OqlTreeResultRow).path)).toEqual([
      'src',
      'tests',
      'index.ts',
      'src/cli.ts',
    ]);
  });
});

/* ----------------- gap 7: materialize checkpoint row -------------------- */

describe('gap 7: target:"materialize" returns a checkpoint row + continuations', () => {
  it('clones once and returns a materialized record row', async () => {
    runDirect.mockResolvedValue(
      toolResult({ localPath: '/cache/facebook/react', cached: false })
    );
    const env = single(
      await runOqlSearch({
        target: 'materialize',
        repo: 'facebook/react',
        path: 'packages/react',
      })
    );
    expect(runDirect).toHaveBeenCalledWith(
      'ghCloneRepo',
      expect.objectContaining({ owner: 'facebook', repo: 'react' })
    );
    const row = env.results[0] as OqlRecordResultRow;
    expect(row.kind).toBe('record');
    expect(row.recordType).toBe('materialized');
    expect(row.id).toBe('/cache/facebook/react');
    expect(row.data.localPath).toBe('/cache/facebook/react');
    expect(row.data.repoRoot).toBe('/cache/facebook/react');
    expect(row.data.complete).toBe(false); // bounded sparse subtree
    expect(env.provenance[0]?.backend).toBe('ghCloneRepo');
    expect(env.provenance[0]?.materializedPath).toBe('/cache/facebook/react');
  });

  it('resolves relative clone localPath against the clone base', async () => {
    runDirect.mockResolvedValue(
      toolResult({ localPath: 'main__sp_abc123', cached: false }, 'success', {
        base: '/cache/facebook/react',
      })
    );
    const env = single(
      await runOqlSearch({
        target: 'materialize',
        repo: 'facebook/react',
        path: 'packages/react',
      })
    );
    const row = env.results[0] as OqlRecordResultRow;
    expect(row.data.localPath).toBe('/cache/facebook/react/main__sp_abc123');
    expect(env.provenance[0]?.materializedPath).toBe(
      '/cache/facebook/react/main__sp_abc123'
    );
    expect(row.next?.['next.structure']?.query).toMatchObject({
      from: { kind: 'local', path: '/cache/facebook/react/main__sp_abc123' },
    });
  });

  it('the checkpoint row carries next.structure / next.files', async () => {
    runDirect.mockResolvedValue(
      toolResult({ localPath: '/cache/react', cached: true })
    );
    const env = single(
      await runOqlSearch({
        target: 'materialize',
        repo: 'facebook/react',
        path: 'packages/react',
      })
    );
    const row = env.results[0] as OqlRecordResultRow;
    expect(row.next?.['next.structure']?.query).toMatchObject({
      target: 'structure',
      from: { kind: 'local', path: '/cache/react' },
    });
    expect(row.next?.['next.files']?.query).toMatchObject({
      target: 'files',
      from: { kind: 'local', path: '/cache/react' },
    });
    // cached clone -> staleCache info diagnostic
    expect(env.diagnostics.map(d => d.code)).toContain('staleCache');
  });
});

/* ---------------- #4: typed record-row data contracts ------------------ */

describe('#4 typed record rows: data carries the documented fields', () => {
  it('repository row data exposes typed fields', async () => {
    runDirect.mockResolvedValue(
      toolResult({
        repositories: [
          {
            fullName: 'facebook/react',
            stars: 1000,
            language: 'JavaScript',
            topics: ['ui'],
          },
        ],
      })
    );
    const env = single(
      await runOqlSearch({
        target: 'repositories',
        params: { keywords: ['react'] },
      })
    );
    const row = env.results[0] as OqlRecordResultRow;
    expect(row.recordType).toBe('repository');
    expect(row.id).toBe('facebook/react');
    expect(row.data.stars).toBe(1000);
    expect(row.data.language).toBe('JavaScript');
  });
});

/* ---------------- provider output regressions from CLI dogfooding -------- */

describe('provider regressions: GitHub content/structure and proof gates', () => {
  it('GitHub content forwards type:file so exact file reads do not look unindexed', async () => {
    runDirect.mockResolvedValue({
      content: [],
      structuredContent: {
        results: [
          {
            id: 'vercel/ai',
            files: [
              {
                path: 'packages/ai/package.json',
                content: '{"name":"ai"}',
              },
            ],
          },
        ],
      },
    });
    const env = single(
      await runOqlSearch({
        target: 'content',
        repo: 'vercel/ai',
        path: 'packages/ai/package.json',
      })
    );
    expect(runDirect).toHaveBeenCalledWith(
      'ghGetFileContent',
      expect.objectContaining({ type: 'file' })
    );
    expect(env.results[0]?.kind).toBe('content');
    expect(env.diagnostics.map(d => d.code)).not.toContain('providerUnindexed');
  });

  it('GitHub content preserves per-file char pagination for next.charRange', async () => {
    runDirect.mockResolvedValue({
      content: [],
      structuredContent: {
        results: [
          {
            id: 'microsoft/playwright-mcp',
            files: [
              {
                path: 'README.md',
                content: 'abc',
                pagination: {
                  currentPage: 1,
                  totalPages: 3,
                  hasMore: true,
                  charOffset: 0,
                  charLength: 3,
                  totalChars: 9,
                },
              },
            ],
          },
        ],
      },
    });

    const env = single(
      await runOqlSearch({
        target: 'content',
        repo: 'microsoft/playwright-mcp',
        path: 'README.md',
        fetch: {
          content: { contentView: 'none', charOffset: 0, charLength: 3 },
        },
      })
    );
    const row = env.results[0] as OqlContentResultRow & {
      next?: Record<string, OqlContinuation>;
    };

    expect(row.range?.charOffset).toBe(0);
    expect(row.range?.charLength).toBe(3);
    expect(env.pagination?.hasMore).toBe(true);
    expect(env.pagination?.totalItemsKind).toBe('chars');
    expect(env.next?.['next.page']).toBeUndefined();
    expect(row.next?.['next.charRange']?.query.fetch?.content?.charOffset).toBe(
      3
    );
  });

  it('empty GitHub provider results block proof/answerReady', async () => {
    runDirect.mockResolvedValue(toolResult({ results: [] }, 'empty'));
    const env = single(
      await runOqlSearch({
        target: 'content',
        repo: 'vercel/ai',
        path: 'packages/ai/package.json',
      })
    );
    const providerDiag = env.diagnostics.find(
      d => d.code === 'providerUnindexed'
    );
    expect(providerDiag?.blocksAnswer).toBe(true);
    expect(env.evidence.answerReady).toBe(false);
  });

  it('empty GitHub code search keeps scoped local-proof continuations actionable', async () => {
    runDirect.mockResolvedValue(toolResult({ results: [] }, 'empty'));

    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'github', repo: 'facebook/react' },
        scope: { path: 'packages/react/src', language: 'js' },
        where: { kind: 'text', value: 'useState' },
        view: 'discovery',
      })
    );
    const providerDiag = env.diagnostics.find(
      d => d.code === 'providerUnindexed'
    );
    const materialize = env.next?.['next.materialize'];

    expect(providerDiag?.message).toContain('clone owner/repo');
    expect(providerDiag?.message).toContain('cache fetch owner/repo');
    expect(providerDiag?.message).toContain('--materialize required');
    expect(materialize?.query).toMatchObject({
      target: 'materialize',
      from: { kind: 'github', repo: 'facebook/react' },
      scope: { path: 'packages/react/src', language: 'js' },
      materialize: { mode: 'required' },
    });
    expect(materialize?.why).toContain(
      'search useState packages/react/src --repo facebook/react --materialize required'
    );
    expect(materialize?.why).toContain(
      'clone facebook/react/packages/react/src'
    );
    expect(materialize?.why).toContain(
      'cache fetch facebook/react packages/react/src --depth tree'
    );
  });

  it('GitHub structure maps array payloads to clean repo paths and top-level pagination', async () => {
    runDirect.mockResolvedValue(
      toolResult({
        structure: [
          { dir: '.', files: [], folders: ['ai', 'src'] },
          { dir: 'ai', files: ['package.json'], folders: ['src'] },
        ],
        pagination: {
          currentPage: 1,
          totalPages: 2,
          hasMore: true,
          entriesPerPage: 100,
          totalEntries: 3,
        },
      })
    );
    const env = single(
      await runOqlSearch({
        target: 'structure',
        repo: 'vercel/ai',
        path: 'packages',
        itemsPerPage: 2,
      })
    );
    expect(runDirect).toHaveBeenCalledWith(
      'ghViewRepoStructure',
      expect.objectContaining({ itemsPerPage: 2 })
    );
    expect(env.results).toHaveLength(2);
    const paths = env.results.map(r => (r as { path: string }).path);
    expect(paths).toEqual(['packages/ai', 'packages/src']);
    expect(paths.some(path => /^\d+\//.test(path))).toBe(false);
    expect(env.pagination?.hasMore).toBe(true);
    expect(env.next?.['next.page']).toBeDefined();
  });

  it('GitHub commit rows preserve parent context and full pagination metadata', async () => {
    runDirect.mockResolvedValue(
      toolResult({
        type: 'file',
        owner: 'langchain-ai',
        repo: 'langchainjs',
        path: 'libs/langchain-core/src/runnables',
        commits: [
          {
            sha: '6cf39fe9636804f6280db0b98c4a4c72d5b103a0',
            messageHeadline: 'chore(core): deprecate streamEvents',
          },
        ],
        pagination: {
          currentPage: 1,
          perPage: 2,
          totalMatches: 23,
          reportedTotalMatches: 31,
          reachableTotalMatches: 20,
          totalMatchesKind: 'reported',
          totalMatchesCapped: false,
          hasMore: true,
          nextPage: 2,
        },
      })
    );
    const env = single(
      await runOqlSearch({
        target: 'commits',
        repo: 'langchain-ai/langchainjs',
        params: {
          path: 'libs/langchain-core/src/runnables',
          perPage: 2,
        },
      })
    );

    const row = env.results[0] as OqlRecordResultRow;
    expect(row.recordType).toBe('commit');
    expect(row.data.sha).toBe('6cf39fe9636804f6280db0b98c4a4c72d5b103a0');
    expect(row.metadata).toEqual({
      type: 'file',
      owner: 'langchain-ai',
      repo: 'langchainjs',
      path: 'libs/langchain-core/src/runnables',
    });
    expect(env.pagination).toMatchObject({
      currentPage: 1,
      nextPage: 2,
      itemsPerPage: 2,
      totalItems: 23,
      reportedTotalItems: 31,
      reachableTotalItems: 20,
      totalItemsKind: 'reported',
      totalItemsCapped: false,
      hasMore: true,
    });
    expect(env.next?.['next.page']?.query).toMatchObject({ page: 2 });
  });

  it('GitHub PR list next.page lowers OQL page into backing tool params', async () => {
    runDirect
      .mockResolvedValueOnce(
        toolResult({
          pull_requests: [{ number: 1, title: 'first page' }],
          pagination: {
            currentPage: 1,
            totalPages: 2,
            perPage: 2,
            hasMore: true,
            nextPage: 2,
          },
        })
      )
      .mockResolvedValueOnce(
        toolResult({
          pull_requests: [{ number: 2, title: 'second page' }],
          pagination: {
            currentPage: 2,
            totalPages: 2,
            perPage: 2,
            hasMore: false,
          },
        })
      );

    const first = single(
      await runOqlSearch({
        target: 'pullRequests',
        repo: 'langchain-ai/langchainjs',
        params: { keywordsToSearch: ['streamEvents'], limit: 2 },
      })
    );
    const nextQuery = first.next?.['next.page']?.query;
    expect(nextQuery).toMatchObject({ page: 2 });

    const second = single(await runOqlSearch(nextQuery!));
    expect(runDirect).toHaveBeenLastCalledWith(
      'ghHistoryResearch',
      expect.objectContaining({ page: 2, limit: 2 })
    );
    expect(second.pagination?.currentPage).toBe(2);
    expect((second.results[0] as OqlRecordResultRow).id).toBe('#2');
  });

  it('GitHub code rows preserve query metadata and provider match indices', async () => {
    runDirect.mockResolvedValue(
      toolResult({
        files: [
          {
            owner: 'langchain-ai',
            repo: 'langchainjs',
            queryId: 'ghSearchCode-1',
            path: 'libs/providers/langchain-openai/src/chat_models/completions.ts',
            matches: [
              {
                value: 'async *_streamChatModelEvents(...)',
                matchIndices: [{ start: 7, end: 29, lineOffset: 0 }],
              },
            ],
          },
        ],
        pagination: {
          currentPage: 1,
          totalPages: 8,
          perPage: 3,
          totalMatches: 23,
          uniqueFileCount: 3,
          hasMore: true,
          nextPage: 2,
        },
      })
    );
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'github', repo: 'langchain-ai/langchainjs' },
        scope: { language: 'TypeScript' },
        where: { kind: 'text', value: '_streamChatModelEvents' },
        limit: 3,
      })
    );

    expect(runDirect).toHaveBeenCalledWith(
      'ghSearchCode',
      expect.objectContaining({
        owner: 'langchain-ai',
        repo: 'langchainjs',
        keywords: ['_streamChatModelEvents'],
        language: 'TypeScript',
      })
    );
    expect(runDirect.mock.calls[0]?.[1]).not.toHaveProperty('extension');

    const row = env.results[0] as OqlCodeResultRow;
    expect(row.path).toBe(
      'libs/providers/langchain-openai/src/chat_models/completions.ts'
    );
    expect(row.snippet).toBe('async *_streamChatModelEvents(...)');
    expect(row.matchIndices).toEqual([{ start: 7, end: 29, lineOffset: 0 }]);
    expect(row.metadata).toEqual({
      owner: 'langchain-ai',
      repo: 'langchainjs',
      queryId: 'ghSearchCode-1',
    });
    expect(env.pagination).toMatchObject({
      currentPage: 1,
      totalPages: 8,
      nextPage: 2,
      itemsPerPage: 3,
      totalItems: 23,
      uniqueFileCount: 3,
      hasMore: true,
    });
    expect(row.next?.['next.fetch']?.query).toMatchObject({
      target: 'content',
      fetch: {
        content: {
          contentView: 'none',
          match: { text: '_streamChatModelEvents' },
        },
      },
    });
    expect(
      env.diagnostics.some(d => d.code === 'providerSemanticsApproximate')
    ).toBe(true);
    expect(env.evidence.kind).toBe('candidate');
    expect(env.evidence.answerReady).toBe(false);
  });

  it('LSP unavailable and nested semantic pagination are partial, not proof', async () => {
    runDirect.mockResolvedValue(
      toolResult({
        lsp: { serverAvailable: false, source: 'native' },
        symbols: [{ name: 'StateGraph', uri: '/tmp/x.ts', line: 1 }],
        pagination: { currentPage: 1, totalPages: 2, hasMore: true },
      })
    );
    const env = single(
      await runOqlSearch({
        target: 'semantics',
        from: { kind: 'local', path: '/tmp/x.ts' },
        params: { type: 'documentSymbols' },
      })
    );
    expect(env.diagnostics.map(d => d.code)).toContain('lspUnavailable');
    expect(env.diagnostics.map(d => d.code)).toContain('partialResult');
    expect(env.evidence.answerReady).toBe(false);
    expect(env.pagination?.hasMore).toBe(true);
    expect(
      (env.next?.['next.page']?.query as { params?: { page?: number } }).params
        ?.page
    ).toBe(2);
  });

  it('uses params.uri as the local LSP anchor when from is a workspace root', async () => {
    runDirect.mockResolvedValue(
      toolResult({
        type: 'documentSymbols',
        uri: '/workspace/src/index.ts',
        symbols: [{ name: 'runCLI', uri: '/workspace/src/index.ts', line: 42 }],
      })
    );

    const env = single(
      await runOqlSearch({
        target: 'semantics',
        from: { kind: 'local', path: '/workspace' },
        params: {
          type: 'documentSymbols',
          uri: '/workspace/src/index.ts',
          format: 'compact',
        },
      })
    );

    expect(runDirect).toHaveBeenCalledWith(
      'lspGetSemantics',
      expect.objectContaining({
        type: 'documentSymbols',
        uri: '/workspace/src/index.ts',
        format: 'compact',
      })
    );
    const row = env.results[0] as OqlRecordResultRow;
    // per-row source is stripped when uniform (token noise) — it lives in provenance
    expect(row.source).toBeUndefined();
    expect(env.provenance[0]?.source).toEqual({
      kind: 'local',
      path: '/workspace/src/index.ts',
    });
  });

  it('routes workspaceSymbol from a directory through workspaceRoot, not a directory uri', async () => {
    runDirect.mockResolvedValue(
      toolResult({
        type: 'workspaceSymbol',
        uri: '/workspace/src/normalize.ts',
        payload: {
          kind: 'workspaceSymbol',
          symbols: [
            {
              name: 'normalizeQuery',
              uri: '/workspace/src/normalize.ts',
              line: 12,
            },
          ],
        },
        lsp: { serverAvailable: true },
      })
    );

    const env = single(
      await runOqlSearch({
        target: 'semantics',
        from: { kind: 'local', path: '.' },
        params: {
          type: 'workspaceSymbol',
          symbolName: 'normalizeQuery',
          workspaceRoot: '/workspace',
        },
      })
    );

    const [, args] = runDirect.mock.calls[0]!;
    expect(args).toMatchObject({
      type: 'workspaceSymbol',
      symbolName: 'normalizeQuery',
      workspaceRoot: '/workspace',
    });
    expect(args).not.toHaveProperty('uri');
    expect(env.diagnostics.map(d => d.code)).not.toContain('lspUnavailable');
    expect(env.results[0]).toMatchObject({
      kind: 'record',
      recordType: 'semantics',
      data: { name: 'normalizeQuery' },
    });
  });

  it('uses scope.path as the remote LSP sparse path and file anchor', async () => {
    runDirect.mockImplementation((tool: string) => {
      if (tool === 'ghCloneRepo') {
        return Promise.resolve(
          toolResult({ localPath: 'main__sp_123' }, 'success', {
            base: '/cache/microsoft/TypeScript',
          })
        );
      }
      return Promise.resolve(
        toolResult({
          type: 'documentSymbols',
          uri: '/cache/microsoft/TypeScript/main__sp_123/src/compiler/program.ts',
          symbols: [
            {
              name: 'createProgram',
              uri: '/cache/microsoft/TypeScript/main__sp_123/src/compiler/program.ts',
              line: 1,
            },
          ],
        })
      );
    });

    const env = single(
      await runOqlSearch({
        target: 'semantics',
        from: { kind: 'github', repo: 'microsoft/TypeScript' },
        scope: { path: 'src/compiler/program.ts' },
        params: { type: 'documentSymbols' },
      })
    );

    expect(runDirect).toHaveBeenCalledWith(
      'ghCloneRepo',
      expect.objectContaining({
        owner: 'microsoft',
        repo: 'TypeScript',
        sparsePath: 'src/compiler/program.ts',
      })
    );
    expect(runDirect).toHaveBeenCalledWith(
      'lspGetSemantics',
      expect.objectContaining({
        type: 'documentSymbols',
        uri: '/cache/microsoft/TypeScript/main__sp_123/src/compiler/program.ts',
      })
    );
    const row = env.results[0] as OqlRecordResultRow;
    expect(row.next?.['next.fetch']?.query).toMatchObject({
      target: 'content',
      from: {
        kind: 'local',
        path: '/cache/microsoft/TypeScript/main__sp_123/src/compiler/program.ts',
      },
    });
    expect(env.diagnostics.some(d => d.code === 'invalidQuery')).toBe(false);
  });

  it('uses scope.path as the materialized LSP file anchor', async () => {
    runDirect.mockResolvedValue(
      toolResult({
        type: 'documentSymbols',
        uri: '/cache/repo/src/index.ts',
        symbols: [{ name: 'main', uri: '/cache/repo/src/index.ts', line: 1 }],
      })
    );

    await runOqlSearch({
      target: 'semantics',
      from: { kind: 'materialized', localPath: '/cache/repo' },
      scope: { path: 'src/index.ts' },
      params: { type: 'documentSymbols' },
    });

    expect(runDirect).toHaveBeenCalledWith(
      'lspGetSemantics',
      expect.objectContaining({
        type: 'documentSymbols',
        uri: '/cache/repo/src/index.ts',
      })
    );
  });
});

/* ---------------- gap 8: direct two-ref file diff lane ------------------ */

describe('gap 8: direct file diff lane (baseRef/headRef/path)', () => {
  it('reads both refs and returns a computed line diff', async () => {
    runDirect.mockImplementation((tool: string, q: { branch?: string }) => {
      if (tool === 'ghGetFileContent') {
        return Promise.resolve(
          toolResult({ content: q.branch === 'main' ? 'a\nb\nc' : 'a\nB\nc' })
        );
      }
      return Promise.resolve(toolResult({}));
    });
    const env = single(
      await runOqlSearch({
        target: 'diff',
        repo: 'facebook/react',
        params: { baseRef: 'main', headRef: 'next', path: 'x.ts' },
      })
    );
    const row = env.results[0] as OqlRecordResultRow;
    expect(row.recordType).toBe('diff');
    expect(row.data.baseRef).toBe('main');
    expect(row.data.headRef).toBe('next');
    expect(row.data.additions).toBe(1);
    expect(row.data.deletions).toBe(1);
    expect(row.data.unchanged).toBe(2);
    // direct-file lane uses ghGetFileContent, not ghHistoryResearch
    expect(runDirect).toHaveBeenCalledWith(
      'ghGetFileContent',
      expect.objectContaining({ branch: 'main' })
    );
  });

  it('returns an error instead of an empty identical diff when refs cannot be read', async () => {
    runDirect.mockResolvedValue(
      toolResult({ error: 'Repository, resource, or path not found' }, 'empty')
    );
    const env = single(
      await runOqlSearch({
        target: 'diff',
        repo: 'facebook/react',
        params: {
          baseRef: 'missing-base',
          headRef: 'missing-head',
          path: 'README.md',
        },
      })
    );

    expect(env.results).toHaveLength(0);
    expect(env.diagnostics[0]).toMatchObject({
      code: 'invalidQuery',
      severity: 'error',
      backend: 'ghGetFileContent',
      blocksAnswer: true,
    });
    expect(env.diagnostics[0]?.message).toContain('not found');
    expect(env.evidence.answerReady).toBe(false);
  });
});
