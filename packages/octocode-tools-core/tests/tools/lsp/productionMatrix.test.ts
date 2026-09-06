import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  available: vi.fn(),
  acquire: vi.fn(),
  legacyAcquire: vi.fn(),
  warm: vi.fn(),
}));
vi.mock('@octocodeai/octocode-engine/lsp/manager', () => ({
  isLanguageServerAvailable: mocks.available,
  acquirePooledClientDetailed: mocks.acquire,
  acquirePooledClient: mocks.legacyAcquire,
  unavailableHintFor: () => 'Install a matching language server.',
}));
vi.mock('../../../src/tools/local_ripgrep/searchContentRipgrep.js', () => ({
  searchContentRipgrep: mocks.warm,
}));
const { executeLspGetSemantics } =
  await import('../../../src/tools/lsp/semantic_content/execution.js');
const { LspGetSemanticsQuerySchema } =
  await import('../../../src/tools/lsp/semantic_content/scheme.js');

// Deliberately independent of dispatch: adding a public operation must update
// acceptance expectations, not silently inherit a passing implementation loop.
const operations = [
  'definition',
  'references',
  'callers',
  'callees',
  'callHierarchy',
  'hover',
  'documentSymbols',
  'typeDefinition',
  'implementation',
  'workspaceSymbol',
  'supertypes',
  'subtypes',
  'diagnostic',
] as const;
let dir: string;
let file: string;
let client: Record<string, ReturnType<typeof vi.fn>>;
let generation: number;
const range = {
  start: { line: 0, character: 4 },
  end: { line: 0, character: 10 },
};
const item = (name: string) => ({
  name,
  kind: 12,
  uri: pathToFileURL(join(dir, `${name}.py`)).href,
  range,
  selectionRange: range,
});
const names = () =>
  generation ? ['added', 'alpha', 'beta', 'gamma'] : ['alpha', 'beta', 'gamma'];

beforeEach(async () => {
  vi.resetAllMocks();
  generation = 0;
  dir = await mkdtemp(join(process.cwd(), '.tmp-lsp-production-'));
  file = join(dir, 'fixture.py');
  await writeFile(file, 'def target():\n    return 1\n');
  const locations = () => names().map(name => ({ uri: item(name).uri, range }));
  client = {
    hasCapability: vi.fn().mockReturnValue(true),
    openDocument: vi.fn(),
    gotoDefinition: vi.fn().mockImplementation(async () => locations()),
    findReferences: vi.fn().mockImplementation(async () => locations()),
    typeDefinition: vi.fn().mockImplementation(async () => locations()),
    implementation: vi.fn().mockImplementation(async () => locations()),
    hover: vi.fn().mockResolvedValue({ contents: 'target() -> int' }),
    documentSymbols: vi
      .fn()
      .mockImplementation(async () => names().map(name => item(name))),
    workspaceSymbol: vi.fn().mockImplementation(async () =>
      names().map(name => ({
        name,
        kind: 12,
        location: { uri: item(name).uri, range },
      }))
    ),
    prepareCallHierarchy: vi.fn().mockResolvedValue([item('target')]),
    getIncomingCalls: vi
      .fn()
      .mockImplementation(async () =>
        names().map(name => ({ from: item(name), fromRanges: [] }))
      ),
    getOutgoingCalls: vi
      .fn()
      .mockImplementation(async () =>
        names().map(name => ({ to: item(name), fromRanges: [] }))
      ),
    prepareTypeHierarchy: vi.fn().mockResolvedValue([item('target')]),
    typeHierarchySupertypes: vi
      .fn()
      .mockImplementation(async () => names().map(name => item(name))),
    typeHierarchySubtypes: vi
      .fn()
      .mockImplementation(async () => names().map(name => item(name))),
    getDiagnostics: vi.fn().mockImplementation(async () => ({
      kind: 'full',
      items: names().map(name => ({ message: name, severity: 1, range })),
    })),
  };
  mocks.available.mockResolvedValue(true);
  mocks.acquire.mockResolvedValue({ ok: true, client });
  mocks.legacyAcquire.mockResolvedValue(client);
  mocks.warm.mockResolvedValue({
    files: [],
    pagination: { totalFiles: 0, hasMore: false },
  });
});
afterEach(async () => {
  await rm(dir, { recursive: true, force: true });
});

async function execute(query: Record<string, unknown>) {
  const result = await executeLspGetSemantics({ queries: [query] } as never);
  return (
    result.structuredContent as {
      results: Array<{
        status?: string;
        meta?: { evidence?: { kind?: string; confidence?: string } };
        data: Record<string, any>;
      }>;
    }
  ).results[0]!;
}
function query(type: string, extra: Record<string, unknown> = {}) {
  return {
    type,
    uri: file,
    workspaceRoot: dir,
    symbolName: 'target',
    lineHint: 1,
    ...extra,
  };
}
const rows = (data: Record<string, any>) =>
  data.payload.locations ??
  data.payload.symbols ??
  data.payload.calls ??
  data.payload.items ??
  data.payload.diagnostics;

describe('production LSP operation matrix', () => {
  it.each(operations)(
    '%s distinguishes missing servers from missing capabilities',
    async type => {
      mocks.available.mockResolvedValue(false);
      const unavailable = await execute(query(type));
      expect(unavailable.status).toBe('error');
      expect(unavailable.data.errorCode).toBe('lspServerUnavailable');
      expect(mocks.acquire).not.toHaveBeenCalled();
      mocks.available.mockResolvedValue(true);
      client.hasCapability!.mockReturnValue(false);
      const unsupported = await execute(query(type));
      expect(unsupported.status).toBe('empty');
      expect(mocks.warm).not.toHaveBeenCalled();
      expect(
        unsupported.data.payload.category ??
          unsupported.data.payload.empty?.category
      ).toBe('unsupportedOperation');
    }
  );

  it.each(operations)(
    '%s preserves initialization failure details',
    async type => {
      mocks.acquire.mockResolvedValue({
        ok: false,
        kind: 'startupFailed',
        message: 'initialize handshake failed',
      });
      mocks.legacyAcquire.mockResolvedValue(null);
      const result = await execute(query(type));
      expect(result.status).toBe('error');
      expect(result.data.errorCode).toBe('lspServerUnavailable');
      expect(result.data.error).toContain('initialize handshake failed');
    }
  );

  it.each(
    operations.flatMap(type =>
      ['structured', 'compact'].map(format => ({ type, format }))
    )
  )(
    '$type executes complete stable pages and restarts after mutation ($format)',
    async ({ type, format }) => {
      let current = await execute(
        query(type, { format, pageSize: 1, depth: 1 })
      );
      expect(current.status).not.toBe('error');
      if (type === 'hover') {
        expect(current.data.payload.kind).toBe('hover');
        return;
      }
      const first = current;
      const collected = [...rows(current.data)];
      while (current.data.next?.nextPage) {
        expect(
          LspGetSemanticsQuerySchema.safeParse(current.data.next.nextPage.query)
            .success
        ).toBe(true);
        current = await execute(current.data.next.nextPage.query);
        expect(current.status).not.toBe('error');
        collected.push(...rows(current.data));
        expect(collected.length).toBeLessThanOrEqual(8);
      }
      const complete = await execute(
        query(type, { format, pageSize: 100, depth: 1 })
      );
      expect(collected).toEqual(rows(complete.data));
      expect(collected.length).toBeGreaterThan(1);
      generation = 1;
      const changed = await execute(first.data.next.nextPage.query);
      expect(changed.data.payload.category).toBe('paginationChanged');
      expect(rows(changed.data)).toBeUndefined();
      const restarted = await execute(
        changed.data.next.restartPagination.query
      );
      expect(restarted.status).not.toBe('error');
      expect(restarted.data.pagination.currentPage).toBe(1);
    }
  );

  it('returns native outlines without acquiring or probing an LSP server', async () => {
    file = join(dir, 'fixture.ts');
    await writeFile(file, 'export function target() {}\n');
    const result = await execute(query('documentSymbols'));
    expect(result.data.lsp.source).toBe('native');
    expect(result.meta?.evidence).toEqual({
      kind: 'syntactic',
      confidence: 'medium',
    });
    expect(result.data.lsp.serverAvailable).toBeUndefined();
    expect(mocks.available).not.toHaveBeenCalled();
    expect(mocks.acquire).not.toHaveBeenCalled();
    expect(mocks.legacyAcquire).not.toHaveBeenCalled();
  });

  it('keeps a failed workspace request an execution error, not an unsupported capability', async () => {
    client.workspaceSymbol!.mockRejectedValue(new Error('request timed out'));
    const result = await execute(query('workspaceSymbol'));
    expect(result.status).toBe('error');
    expect(result.data.error).toContain('request timed out');
    expect(result.data.payload).toBeUndefined();
  });
});
