import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { LSP_ERROR_CODES } from '@octocodeai/octocode-engine/lsp/lspErrorCodes';

const mocks = vi.hoisted(() => ({
  available: vi.fn(),
  acquire: vi.fn(),
  references: vi.fn(),
  warmSearch: vi.fn(),
  capability: vi.fn(),
  implementation: vi.fn(),
  outgoingCalls: vi.fn(),
  incomingCalls: vi.fn(),
  definition: vi.fn(),
  workspaceSymbols: vi.fn(),
}));
vi.mock('@octocodeai/octocode-engine/lsp/manager', () => ({
  isLanguageServerAvailable: mocks.available,
  acquirePooledClientDetailed: mocks.acquire,
  acquirePooledClient: vi.fn(),
  unavailableHintFor: () => 'Install a language server.',
}));
vi.mock('../../../src/tools/local_ripgrep/searchContentRipgrep.js', () => ({
  searchContentRipgrep: mocks.warmSearch,
}));

const { executeLspGetSemantics } =
  await import('../../../src/tools/lsp/semantic_content/execution.js');
let dir: string;
let file: string;

function location(uri: string, line = 0, character = 16) {
  return {
    uri,
    range: {
      start: { line, character },
      end: { line, character: character + 6 },
    },
    content: 'export function target() {}',
  };
}
async function execute(query: Record<string, unknown>) {
  const result = await executeLspGetSemantics({ queries: [query] } as never);
  return (
    result.structuredContent as {
      results: Array<{
        status?: string;
        meta?: Record<string, unknown>;
        data: Record<string, any>;
      }>;
    }
  ).results[0]!;
}
function query(extra: Record<string, unknown> = {}) {
  return {
    type: 'references',
    uri: file,
    symbolName: 'target',
    lineHint: 1,
    workspaceRoot: dir,
    ...extra,
  };
}

it('reports oversized semantic source as an error without claiming symbol absence', async () => {
  await writeFile(
    file,
    `export function target() {}\n${' '.repeat(1_000_000)}`
  );
  const result = await execute(query({ type: 'definition' }));
  expect(result.status).toBe('error');
  expect(result.data.error).toContain('[lspSourceTooLarge]');
  expect(result.data.errorType).not.toBe('symbol_not_found');
  expect(mocks.acquire).not.toHaveBeenCalled();
});

beforeEach(async () => {
  vi.resetAllMocks();
  dir = await mkdtemp(join(process.cwd(), '.tmp-lsp-integrity-'));
  file = join(dir, 'source.ts');
  await writeFile(file, 'export function target() {}\n');
  mocks.available.mockResolvedValue(true);
  mocks.capability.mockReturnValue(true);
  mocks.implementation.mockResolvedValue([]);
  mocks.outgoingCalls.mockResolvedValue([]);
  mocks.incomingCalls.mockResolvedValue([]);
  mocks.definition.mockResolvedValue([location(pathToFileURL(file).href)]);
  mocks.workspaceSymbols.mockResolvedValue([]);
  mocks.acquire.mockResolvedValue({
    ok: true,
    client: {
      hasCapability: mocks.capability,
      findReferences: mocks.references,
      openDocument: vi.fn(),
      implementation: mocks.implementation,
      hover: vi.fn().mockResolvedValue({ contents: 'target(): void' }),
      gotoDefinition: mocks.definition,
      workspaceSymbol: mocks.workspaceSymbols,
      prepareCallHierarchy: vi.fn().mockResolvedValue([
        {
          name: 'target',
          kind: 12,
          uri: pathToFileURL(file).href,
          range: location(pathToFileURL(file).href).range,
          selectionRange: location(pathToFileURL(file).href).range,
        },
      ]),
      getIncomingCalls: mocks.incomingCalls,
      getOutgoingCalls: mocks.outgoingCalls,
    },
  });
  mocks.warmSearch.mockResolvedValue({
    files: [],
    pagination: { totalFiles: 0, hasMore: false },
  });
});

describe('snapshot-safe semantic pagination', () => {
  it('passes explicit Rust context to the provider and preserves it in executable pagination', async () => {
    const rustFile = join(dir, 'main.rs');
    await writeFile(rustFile, 'fn target() {}\n');
    mocks.references.mockResolvedValue([
      location(rustFile, 0),
      location(rustFile, 1),
    ]);
    const rustContext = { features: ['enabled'], cfgs: ['custom'] };
    const first = await execute(
      query({ uri: rustFile, rustContext, pageSize: 1 })
    );
    expect(mocks.acquire).toHaveBeenLastCalledWith(
      dir,
      rustFile,
      expect.objectContaining({
        features: ['enabled'],
        cfgs: ['custom'],
        buildScripts: false,
        procMacros: false,
      })
    );
    expect(first.data.rustContext).toMatchObject({
      features: ['enabled'],
      fingerprint: expect.stringMatching(/^rust-v1:/),
    });
    const continuation = first.data.next.nextPage;
    expect(continuation.query.rustContext.features).toEqual(['enabled']);
    const second = await execute(continuation.query);
    expect(second.data.payload.locations).toHaveLength(1);
    const changed = await execute({
      ...continuation.query,
      rustContext: { features: ['other'] },
    });
    expect(changed.data.payload.category).toBe('paginationChanged');
    expect(changed.data.payload.locations).toBeUndefined();
    expect(changed.data.rustContext.features).toEqual(['other']);
  });

  const cases = [
    'references',
    'workspaceSymbol',
    'documentSymbols',
    'callers',
    'callees',
    'callHierarchy',
  ];
  async function fixture(type: string) {
    let names = ['alpha', 'beta', 'gamma'];
    let reverse = false;
    const ordered = () => {
      reverse = !reverse;
      return reverse ? [...names].reverse() : [...names];
    };
    const target = (name: string) => ({
      name,
      kind: 12,
      uri: pathToFileURL(join(dir, `${name}.ts`)).href,
      range: location(file).range,
      selectionRange: location(file).range,
    });
    mocks.references.mockImplementation(async () =>
      ordered().map(name => location(target(name).uri))
    );
    mocks.workspaceSymbols.mockImplementation(async () =>
      ordered().map(name => ({
        name,
        kind: 12,
        location: { uri: target(name).uri, range: target(name).range },
      }))
    );
    mocks.incomingCalls.mockImplementation(async () =>
      ordered().map(name => ({ from: target(name), fromRanges: [] }))
    );
    mocks.outgoingCalls.mockImplementation(async () =>
      ordered().map(name => ({ to: target(name), fromRanges: [] }))
    );
    const writeSymbols = () =>
      writeFile(
        file,
        names.map(name => `export function ${name}() {}`).join('\n')
      );
    if (type === 'documentSymbols') await writeSymbols();
    return async () => {
      names = ['added', ...names];
      if (type === 'documentSymbols') await writeSymbols();
    };
  }
  const rows = (data: Record<string, any>) =>
    data.payload.locations ?? data.payload.symbols ?? data.payload.calls;

  it.each(
    cases.flatMap(type =>
      ['structured', 'compact'].map(format => ({ type, format }))
    )
  )(
    'executes all $type pages from one stable result identity ($format)',
    async ({ type, format }) => {
      await fixture(type);
      const first = await execute(
        query({ type, format, pageSize: 1, depth: 1 })
      );
      const snapshot = first.data.pagination.snapshot;
      expect(snapshot).toMatch(/^lsp-v1:[a-f0-9]{64}$/);
      const collected = [...rows(first.data)];
      let current = first;
      while (current.data.next?.nextPage) {
        expect(current.data.next.nextPage.query.snapshot).toBe(snapshot);
        current = await execute(current.data.next.nextPage.query);
        expect(current.data.pagination.snapshot).toBe(snapshot);
        collected.push(...rows(current.data));
        expect(collected.length).toBeLessThanOrEqual(6);
      }
      const full = await execute(
        query({ type, format, pageSize: 100, depth: 1 })
      );
      expect(full.data.pagination.snapshot).toBe(snapshot);
      expect(collected).toEqual(rows(full.data));
      expect(new Set(collected.map(row => JSON.stringify(row))).size).toBe(
        collected.length
      );
    }
  );

  it.each(cases)(
    'returns no stale %s rows on mutation and executes restart recovery',
    async type => {
      const mutate = await fixture(type);
      const first = await execute(query({ type, pageSize: 1, depth: 1 }));
      await mutate();
      const changed = await execute(first.data.next.nextPage.query);
      expect(changed.status).toBe('empty');
      expect(changed.meta?.evidence).toMatchObject({ confidence: 'low' });
      expect(changed.data.payload).toMatchObject({
        kind: 'empty',
        category: 'paginationChanged',
      });
      expect(rows(changed.data)).toBeUndefined();
      expect(changed.data.next.nextPage).toBeUndefined();
      expect(
        changed.data.next.restartPagination.query.snapshot
      ).toBeUndefined();
      const restarted = await execute(
        changed.data.next.restartPagination.query
      );
      expect(restarted.data.pagination.currentPage).toBe(1);
      expect(restarted.data.pagination.snapshot).not.toBe(
        first.data.pagination.snapshot
      );
      expect(rows(restarted.data)).toHaveLength(1);
    }
  );

  it('binds the token to the query and requires it for later pages', async () => {
    await fixture('references');
    const first = await execute(query({ pageSize: 1 }));
    const changedQuery = await execute({
      ...first.data.next.nextPage.query,
      contextLines: 5,
    });
    expect(changedQuery.data.payload.category).toBe('paginationChanged');
    expect(rows(changedQuery.data)).toBeUndefined();
    const unguarded = await execute(query({ pageSize: 1, page: 2 }));
    expect(unguarded.data.payload.category).toBe('paginationSnapshotRequired');
    expect(unguarded.data.next.restartPagination.query.page).toBe(1);
    expect(rows(unguarded.data)).toBeUndefined();
  });

  it('keeps one snapshot across presentation formats and rejects changed compact pages', async () => {
    const mutate = await fixture('references');
    const first = await execute(query({ pageSize: 1, format: 'structured' }));
    const compact = await execute({
      ...first.data.next.nextPage.query,
      format: 'compact',
    });
    expect(compact.data.pagination.snapshot).toBe(
      first.data.pagination.snapshot
    );
    expect(typeof compact.data.payload.locations[0]).toBe('string');
    await mutate();
    const changed = await execute(compact.data.next.nextPage.query);
    expect(changed.data.payload.category).toBe('paginationChanged');
    expect(changed.data.format).toBe('compact');
    expect(rows(changed.data)).toBeUndefined();
  });

  it('guards every underlying reference range in grouped pages', async () => {
    const a = location(pathToFileURL(join(dir, 'a.ts')).href, 1, 0);
    const b = location(pathToFileURL(join(dir, 'b.ts')).href, 1, 0);
    const secondB = location(b.uri, 3, 2);
    mocks.references.mockResolvedValueOnce([a, b, secondB]);
    const first = await execute(query({ groupByFile: true, pageSize: 1 }));
    mocks.references.mockResolvedValueOnce([a, b, location(b.uri, 3, 8)]);
    const changed = await execute(first.data.next.nextPage.query);
    expect(changed.data.payload.category).toBe('paginationChanged');
    expect(changed.data.payload.byFile).toBeUndefined();
  });
});
afterEach(async () => {
  await rm(dir, { recursive: true, force: true });
});

describe('semantic execution integrity', () => {
  it.each(['structured', 'compact'])(
    'classifies unsupported and unresolved results as low-confidence empty evidence (%s)',
    async format => {
      mocks.capability.mockReturnValue(false);
      const unsupported = await execute(query({ type: 'hover', format }));
      expect(unsupported.status).toBe('empty');
      expect(unsupported.meta?.evidence).toMatchObject({ confidence: 'low' });
      expect(unsupported.data.payload.category).toBe('unsupportedOperation');

      const unresolved = await execute(
        query({ type: 'hover', format, symbolName: 'missingSymbol' })
      );
      expect(unresolved.status).toBe('empty');
      expect(unresolved.meta?.evidence).toMatchObject({ confidence: 'low' });
      expect(unresolved.data.payload.category).toBe('symbolNotFound');
    }
  );

  it.each(['structured', 'compact'])(
    'preserves high-confidence evidence for a supported semantic result (%s)',
    async format => {
      const row = await execute(query({ type: 'hover', format }));
      expect(row.meta?.evidence).toMatchObject({ confidence: 'high' });
      expect(row.status).toBeUndefined();
    }
  );

  it.each(['structured', 'compact'])(
    'offers an executable workspace verification for a local import binding (%s)',
    async format => {
      await writeFile(
        file,
        'import {\n  target,\n} from "./target.js";\ntarget();\n'
      );
      mocks.definition.mockResolvedValue([
        { ...location(pathToFileURL(file).href, 1, 2), content: '  target,' },
      ]);
      const row = await execute(
        query({ type: 'definition', lineHint: 4, format })
      );
      expect(row.data.payload.locations).toHaveLength(1);
      expect(JSON.stringify(row.data.payload.locations)).toContain('target,');
      expect(row.data.next.verifyDefinition).toMatchObject({
        tool: 'lspGetSemantics',
        query: {
          type: 'workspaceSymbol',
          symbolName: 'target',
          workspaceRoot: dir,
        },
      });
      expect(row.data.next.verifyDefinition.why).toContain('cold server');
      expect(row.data.next.searchDefinitionCandidates).toMatchObject({
        tool: 'localSearch',
        confidence: 'low',
        query: {
          operation: 'text',
          path: dir,
          searchText: 'target',
          wholeWord: true,
          maxFiles: 100,
          pageSize: 20,
          maxMatchesPerFile: 20,
        },
      });
      const verified = await execute(row.data.next.verifyDefinition.query);
      expect(verified.status).not.toBe('error');
      expect(verified.data.type).toBe('workspaceSymbol');
      expect(mocks.workspaceSymbols).toHaveBeenCalledWith('target');
    }
  );

  it('does not add import verification to a resolved declaration', async () => {
    const row = await execute(query({ type: 'definition' }));
    expect(row.data.next.verifyDefinition).toBeUndefined();
    expect(row.data.next.readSite).toBeDefined();
  });

  it('preserves an unresolved provider import binding even when a lexical target exists', async () => {
    const content =
      'import { actual as target } from "./actual.js";\ntarget();\n';
    await writeFile(file, content);
    await writeFile(join(dir, 'actual.ts'), 'export function actual() {}\n');
    mocks.definition.mockResolvedValue([
      {
        ...location(pathToFileURL(file).href, 0, 19),
        content: content.split('\n')[0],
      },
    ]);
    const row = await execute(query({ type: 'definition', lineHint: 2 }));
    expect(row.data.payload.locations[0].content).toContain(
      'import { actual as target }'
    );
    expect(row.data.next.verifyDefinition).toBeDefined();
  });

  const outgoingCall = (name: string, uri: string) => ({
    to: {
      name,
      kind: 12,
      uri,
      range: location(uri).range,
      selectionRange: location(uri).range,
    },
    fromRanges: [],
  });

  it.each(
    ['structured', 'compact'].flatMap(format =>
      [1, 2].map(depth => ({ format, depth }))
    )
  )(
    'does not expand excluded stdlib callees at depth $depth ($format)',
    async ({ format, depth }) => {
      mocks.outgoingCalls.mockResolvedValue([
        outgoingCall(
          'push',
          pathToFileURL(join(dir, 'node_modules/typescript/lib/lib.es5.d.ts'))
            .href
        ),
      ]);
      const row = await execute(query({ type: 'callees', format, depth }));
      expect(row.data.payload.outgoingCalls).toBe(0);
      expect(row.data.payload.completeness).toMatchObject({
        complete: true,
        truncatedByDepth: false,
        stdlibCallsExcluded: 1,
        requestCount: 1,
      });
      expect(row.data.truncated).toBeUndefined();
      expect(row.data.next?.expandDepth).toBeUndefined();
      expect(row.data.payload.empty.reason).toContain(
        'excluding TypeScript standard-library'
      );
      expect(mocks.outgoingCalls).toHaveBeenCalledTimes(1);
    }
  );

  it('keeps project callees and excludes stdlib leaves before computing depth completeness', async () => {
    const project = outgoingCall(
      'helper',
      pathToFileURL(join(dir, 'helper.ts')).href
    );
    const stdlib = outgoingCall(
      'push',
      pathToFileURL(join(dir, 'node_modules/typescript/lib/lib.es5.d.ts')).href
    );
    mocks.outgoingCalls
      .mockResolvedValueOnce([project])
      .mockResolvedValueOnce([stdlib]);
    const row = await execute(query({ type: 'callees', depth: 2 }));
    expect(row.data.payload.outgoingCalls).toBe(1);
    expect(row.data.payload.calls[0].item.name).toBe('helper');
    expect(row.data.payload.completeness).toMatchObject({
      complete: true,
      truncatedByDepth: false,
      stdlibCallsExcluded: 1,
      requestCount: 2,
    });
    expect(row.data.next?.expandDepth).toBeUndefined();
    expect(mocks.outgoingCalls).toHaveBeenCalledTimes(2);
  });

  it('retains a depth continuation for unexpanded project callees', async () => {
    mocks.outgoingCalls.mockResolvedValue([
      outgoingCall('helper', pathToFileURL(join(dir, 'helper.ts')).href),
    ]);
    const row = await execute(query({ type: 'callees', depth: 1 }));
    expect(row.data.payload.outgoingCalls).toBe(1);
    expect(row.data.payload.completeness.truncatedByDepth).toBe(true);
    expect(row.data.next.expandDepth.query.depth).toBe(2);
  });

  it.each(['structured', 'compact'])(
    'keeps implementation hits partial when consumer warmup is incomplete (%s)',
    async format => {
      mocks.implementation.mockResolvedValue([
        location(pathToFileURL(file).href),
      ]);
      mocks.warmSearch.mockResolvedValue({
        status: 'error',
        error: 'search failed',
      });
      const row = await execute(query({ type: 'implementation', format }));
      expect(row.data.payload.kind).toBe('implementation');
      expect(row.data.payload.locations).toHaveLength(1);
      expect(row.data.payload.warmup.possiblyTruncated).toBe(true);
      expect(row.data.truncated).toBe(true);
      expect(row.data.next.verifyCompleteness.tool).toBe('localSearch');
    }
  );

  it.each(['callers', 'callees', 'callHierarchy'])(
    'keeps complete consumer coverage complete for %s',
    async type => {
      const row = await execute(query({ type }));
      expect(row.data.payload.warmup.possiblyTruncated).toBe(false);
      expect(row.data.payload.completeness.complete).toBe(true);
      expect(
        row.data.payload.completeness.consumerWarmupIncomplete
      ).toBeUndefined();
      expect(row.data.truncated).toBeUndefined();
      expect(row.data.next?.verifyCompleteness).toBeUndefined();
    }
  );

  it.each(
    [
      'references',
      'callers',
      'callees',
      'callHierarchy',
      'implementation',
    ].flatMap(type =>
      ['structured', 'compact'].map(format => ({ type, format }))
    )
  )(
    'preserves incomplete consumer coverage for $type/$format',
    async ({ type, format }) => {
      mocks.references.mockResolvedValue([]);
      mocks.warmSearch.mockResolvedValue({
        files: [],
        stats: { capped: true },
        pagination: { totalFiles: 101, hasMore: false },
      });
      const row = await execute(query({ type, format }));
      expect(row.status).not.toBe('error');
      expect(row.data.payload.warmup).toMatchObject({
        candidates: 101,
        possiblyTruncated: true,
      });
      expect(row.data.truncated).toBe(true);
      expect(row.data.next.verifyCompleteness).toMatchObject({
        tool: 'localSearch',
        query: { operation: 'text', path: dir, searchText: 'target' },
      });
      if (['callers', 'callees', 'callHierarchy'].includes(type)) {
        expect(row.data.payload.completeness).toMatchObject({
          complete: false,
          consumerWarmupIncomplete: true,
        });
      }
    }
  );

  it.each(['definition', 'hover'])(
    'does not attach consumer coverage to %s',
    async type => {
      const row = await execute(query({ type, format: 'compact' }));
      expect(row.status).not.toBe('error');
      expect(mocks.warmSearch).not.toHaveBeenCalled();
      expect(row.data.payload.warmup).toBeUndefined();
      expect(row.data.next?.verifyCompleteness).toBeUndefined();
    }
  );

  it.each([
    'references',
    'callers',
    'callees',
    'callHierarchy',
    'implementation',
  ])(
    'does not mistake unsupported %s for incomplete provider results',
    async type => {
      mocks.capability.mockReturnValue(false);
      mocks.warmSearch.mockResolvedValue({
        status: 'error',
        error: 'warmup failed',
      });
      const row = await execute(query({ type }));
      expect(row.data.payload.category).toBe('unsupportedOperation');
      expect(row.data.payload.warmup).toBeUndefined();
      expect(row.data.next?.verifyCompleteness).toBeUndefined();
    }
  );

  it('keeps compact reference evidence and executable recovery continuations', async () => {
    mocks.references.mockResolvedValue([location(pathToFileURL(file).href)]);
    mocks.warmSearch.mockResolvedValue({
      files: [],
      stats: { capped: true },
      pagination: { totalFiles: 101, hasMore: false },
    });
    const row = await execute(query({ format: 'compact' }));
    expect(row.data.payload.definitionOnly).toBeUndefined();
    expect(row.data.payload.warmup).toMatchObject({
      candidates: 101,
      possiblyTruncated: true,
    });
    expect(row.data.truncated).toBe(true);
    expect(row.data.next.verifyCompleteness).toMatchObject({
      tool: 'localSearch',
      query: { path: dir, operation: 'text', searchText: 'target' },
    });
    expect(row.data.next.readSite).toMatchObject({
      tool: 'localGetFileContent',
      query: { path: file },
    });

    mocks.references.mockResolvedValue([]);
    const empty = await execute(
      query({ format: 'compact', includeDeclaration: false })
    );
    expect(empty.data.payload.empty.category).toBe('noReferences');
    expect(empty.data.next.textSearch.query).toMatchObject({
      path: file,
      operation: 'text',
      searchText: 'target',
    });
  });

  it.each(['references', 'diagnostic', 'workspaceSymbol'])(
    'returns a typed missing-file error before server selection for %s',
    async type => {
      const row = await execute(query({ type, uri: join(dir, 'missing.ts') }));
      expect(row.status).toBe('error');
      expect(row.data.errorCode).toBe(LSP_ERROR_CODES.LSP_REQUEST_FAILED);
      expect(row.data.error).toContain('File not found');
      expect(row.data.payload).toBeUndefined();
      expect(mocks.available).not.toHaveBeenCalled();
    }
  );

  it.each([false, true])(
    'recovers all reference pages when provider order changes (groupByFile=%s)',
    async groupByFile => {
      const a = location(pathToFileURL(join(dir, 'a.ts')).href, 2, 0);
      const b = location(pathToFileURL(join(dir, 'b.ts')).href, 4, 0);
      mocks.references
        .mockResolvedValueOnce([b, a, b])
        .mockResolvedValueOnce([a, b, a]);
      const first = await execute(
        query({ includeDeclaration: false, pageSize: 1, groupByFile })
      );
      const second = await execute(first.data.next.nextPage.query);
      const field = groupByFile ? 'byFile' : 'locations';
      const recovered = [
        ...first.data.payload[field],
        ...second.data.payload[field],
      ];
      expect(recovered).toHaveLength(2);
      expect(new Set(recovered.map(item => item.path ?? item.uri)).size).toBe(
        2
      );
      expect(first.data.pagination.totalResults).toBe(2);
      expect(second.data.pagination.hasMore).toBe(false);
      if (groupByFile)
        expect(recovered.map(item => item.count)).toEqual([1, 1]);
    }
  );
});
