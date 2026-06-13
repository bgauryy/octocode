import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { MessageConnection } from 'vscode-jsonrpc/node.js';
import {
  Location,
  SymbolKind,
  type Range,
} from 'vscode-languageserver-protocol';
import { LSPDocumentManager } from '../src/lspDocumentManager.js';
import { LSPOperations } from '../src/lspOperations.js';
import { toUri } from '../src/uri.js';
import type { CallHierarchyItem, LanguageServerConfig } from '../src/types.js';

const TEST_ROOT = fileURLToPath(
  new URL('../test-workspaces/', import.meta.url)
);
const tempDirs: string[] = [];

afterEach(async () => {
  while (tempDirs.length > 0) {
    await rm(tempDirs.pop()!, { recursive: true, force: true });
  }
});

type RequestHandler = (
  method: string,
  params: unknown,
  token?: unknown
) => Promise<unknown>;

function connection(handler: RequestHandler): MessageConnection {
  return {
    sendNotification: vi.fn().mockResolvedValue(undefined),
    sendRequest: vi.fn(handler),
  } as unknown as MessageConnection;
}

async function createFixture(content: string): Promise<{
  readonly root: string;
  readonly filePath: string;
  readonly config: LanguageServerConfig;
}> {
  await mkdir(TEST_ROOT, { recursive: true });
  const root = await mkdtemp(path.join(TEST_ROOT, 'ops-'));
  tempDirs.push(root);
  const src = path.join(root, 'src');
  await mkdir(src, { recursive: true });
  const filePath = path.join(src, 'demo.foo');
  await writeFile(filePath, content);
  return {
    root,
    filePath,
    config: {
      command: 'server',
      args: [],
      workspaceRoot: root,
      languageId: 'foo',
    },
  };
}

function operations(
  config: LanguageServerConfig,
  conn: MessageConnection
): LSPOperations {
  const documents = new LSPDocumentManager(config);
  documents.setConnection(conn, true);
  const ops = new LSPOperations(documents, config.workspaceRoot);
  ops.setConnection(conn, true);
  return ops;
}

function range(
  startLine: number,
  startCharacter: number,
  endLine = startLine,
  endCharacter = startCharacter + 1
): Range {
  return {
    start: { line: startLine, character: startCharacter },
    end: { line: endLine, character: endCharacter },
  };
}

function first<T>(items: readonly T[]): T {
  const item = items[0];
  if (item === undefined) {
    throw new Error('Expected at least one item');
  }
  return item;
}

describe('LSPOperations', () => {
  it('converts definitions to safe workspace snippets', async () => {
    const fixture = await createFixture(
      'function target() {\n  return 1;\n}\n'
    );
    const targetRange = range(0, 9, 1, 11);
    let definitionResult: unknown = [
      null,
      {},
      { uri: toUri(fixture.filePath) },
      Location.create(toUri(fixture.filePath), targetRange),
      Location.create(toUri('/etc/passwd'), targetRange),
    ];
    const conn = connection(async method => {
      if (method !== 'textDocument/definition') return null;
      return definitionResult;
    });

    const ops = operations(fixture.config, conn);
    const snippets = await ops.gotoDefinition(fixture.filePath, {
      line: 0,
      character: 12,
    });

    expect(snippets).toHaveLength(1);
    expect(first(snippets)).toMatchObject({
      uri: fixture.filePath,
      range: targetRange,
      displayRange: { startLine: 1, endLine: 2 },
      content: '1\tfunction target() {\n2\t  return 1;',
    });

    definitionResult = Location.create(toUri(fixture.filePath), targetRange);
    await expect(
      ops.gotoDefinition(fixture.filePath, { line: 0, character: 12 })
    ).resolves.toHaveLength(1);
  });

  it('passes reference context and converts location links', async () => {
    const fixture = await createFixture('type Name = string;\n');
    const calls: Array<{ readonly method: string; readonly params: unknown }> =
      [];
    const targetRange = range(0, 5, 0, 9);
    const conn = connection(async (method, params) => {
      calls.push({ method, params });
      if (method === 'textDocument/references') {
        return [Location.create(toUri(fixture.filePath), targetRange)];
      }
      if (
        method === 'textDocument/typeDefinition' ||
        method === 'textDocument/implementation'
      ) {
        return [
          {
            targetUri: toUri(fixture.filePath),
            targetRange,
            targetSelectionRange: targetRange,
          },
        ];
      }
      return null;
    });
    const ops = operations(fixture.config, conn);

    expect(
      await ops.findReferences(
        fixture.filePath,
        { line: 0, character: 6 },
        false
      )
    ).toHaveLength(1);
    expect(first(calls)).toMatchObject({
      method: 'textDocument/references',
      params: { context: { includeDeclaration: false } },
    });
    expect(
      await ops.typeDefinition(fixture.filePath, { line: 0, character: 6 })
    ).toHaveLength(1);
    expect(
      await ops.implementation(fixture.filePath, { line: 0, character: 6 })
    ).toHaveLength(1);
  });

  it('returns hover and document symbols with empty fallback', async () => {
    const fixture = await createFixture('define Foo\n');
    let symbolsResult: unknown = [
      {
        name: 'Foo',
        kind: SymbolKind.Function,
        range: range(0, 0, 0, 10),
        selectionRange: range(0, 7, 0, 10),
      },
    ];
    const conn = connection(async method => {
      if (method === 'textDocument/hover') {
        return {
          contents: { kind: 'markdown', value: 'Foo docs' },
          range: range(0, 7, 0, 10),
        };
      }
      if (method === 'textDocument/documentSymbol') return symbolsResult;
      return null;
    });
    const ops = operations(fixture.config, conn);

    expect(
      await ops.hover(fixture.filePath, { line: 0, character: 8 })
    ).toMatchObject({
      contents: { value: 'Foo docs' },
    });
    expect(await ops.documentSymbols(fixture.filePath)).toHaveLength(1);

    symbolsResult = null;
    expect(await ops.documentSymbols(fixture.filePath)).toEqual([]);
  });

  it('normalizes call hierarchy items and filters malformed calls', async () => {
    const fixture = await createFixture('define Foo\ncall Bar\n');
    const itemRange = range(0, 7, 0, 10);
    const protocolItem = {
      name: 'Foo',
      kind: SymbolKind.Function,
      uri: toUri(fixture.filePath),
      range: itemRange,
      selectionRange: itemRange,
    };
    const looseItem = {
      name: 'Loose',
      kind: SymbolKind.Method,
      uri: toUri(fixture.filePath),
    };
    const partialItem = {
      kind: 999,
      range: { start: {}, end: {} },
      selectionRange: { start: {}, end: {} },
    };
    const conn = connection(async method => {
      if (method === 'textDocument/prepareCallHierarchy') {
        return [protocolItem, looseItem, partialItem];
      }
      if (method === 'callHierarchy/incomingCalls') {
        return [
          {
            from: protocolItem,
            fromRanges: [
              {
                start: { line: 1, character: 0 },
                end: { line: 1, character: 8 },
              },
              {
                start: {},
                end: {},
              },
            ],
          },
          { from: null, fromRanges: [] },
          { from: protocolItem, fromRanges: null },
        ];
      }
      if (method === 'callHierarchy/outgoingCalls') {
        return [
          {
            to: protocolItem,
            fromRanges: [{ start: {}, end: {} }],
          },
          { to: null, fromRanges: [] },
        ];
      }
      return null;
    });
    const ops = operations(fixture.config, conn);

    const prepared = await ops.prepareCallHierarchy(fixture.filePath, {
      line: 0,
      character: 8,
    });

    expect(prepared).toHaveLength(3);
    expect(prepared[0]).toMatchObject({
      name: 'Foo',
      kind: 'function',
      uri: fixture.filePath,
      displayRange: { startLine: 1, endLine: 1 },
    });
    expect(prepared[1]).toMatchObject({
      name: 'Loose',
      kind: 'method',
      range: {
        start: { line: 0, character: 0 },
        end: { line: 0, character: 0 },
      },
    });
    expect(prepared[2]).toMatchObject({
      name: '',
      kind: 'unknown',
      uri: '',
      range: {
        start: { line: 0, character: 0 },
        end: { line: 0, character: 0 },
      },
      selectionRange: {
        start: { line: 0, character: 0 },
        end: { line: 0, character: 0 },
      },
    });

    const incoming = await ops.getIncomingCalls(first(prepared));
    expect(incoming).toHaveLength(1);
    expect(first(incoming).fromRanges[0]).toMatchObject({
      start: { line: 1, character: 0 },
      end: { line: 1, character: 8 },
    });
    expect(first(incoming).fromRanges[1]).toMatchObject({
      start: { line: 0, character: 0 },
      end: { line: 0, character: 0 },
    });

    const outgoing = await ops.getOutgoingCalls(first(prepared));
    expect(outgoing).toHaveLength(1);
    expect(first(outgoing).fromRanges[0]).toMatchObject({
      start: { line: 0, character: 0 },
      end: { line: 0, character: 0 },
    });
    await expect(
      ops.getOutgoingCalls(withoutSelectionRange(first(prepared)))
    ).resolves.toHaveLength(1);
    await expect(
      ops.getIncomingCalls(withoutKind(first(prepared)))
    ).resolves.toHaveLength(1);
  });

  it('requires an initialized connection for requests', async () => {
    const fixture = await createFixture('define Foo\n');
    const documents = new LSPDocumentManager(fixture.config);
    const ops = new LSPOperations(documents, fixture.root);

    await expect(
      ops.getIncomingCalls({
        name: 'Foo',
        kind: 'function',
        uri: fixture.filePath,
        range: range(0, 0),
        selectionRange: range(0, 0),
        displayRange: { startLine: 1, endLine: 1 },
      } satisfies CallHierarchyItem)
    ).rejects.toThrow('LSP client not initialized');
  });
});

function withoutSelectionRange(item: CallHierarchyItem): CallHierarchyItem {
  return {
    name: item.name,
    kind: item.kind,
    uri: item.uri,
    range: item.range,
    ...(item.displayRange && { displayRange: item.displayRange }),
  };
}

function withoutKind(item: CallHierarchyItem): CallHierarchyItem {
  return {
    name: item.name,
    kind: undefined as unknown as CallHierarchyItem['kind'],
    uri: item.uri,
    range: item.range,
    ...(item.selectionRange && { selectionRange: item.selectionRange }),
    ...(item.displayRange && { displayRange: item.displayRange }),
  };
}
