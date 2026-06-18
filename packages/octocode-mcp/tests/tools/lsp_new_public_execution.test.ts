import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';

vi.mock('octocode-lsp/manager', () => ({
  acquirePooledClient: vi.fn(),
  isLanguageServerAvailable: vi.fn(),
}));

vi.mock('octocode-lsp/workspaceRoot', () => ({
  resolveWorkspaceRootForFile: vi.fn().mockResolvedValue('/workspace'),
}));

vi.mock(
  '../../../octocode-tools-core/src/tools/lsp/shared/callHierarchyTraversal.js',
  () => ({
    gatherIncomingCallsRecursive: vi.fn(),
    gatherOutgoingCallsRecursive: vi.fn(),
    createCallItemKey: (item: {
      uri: string;
      range: { start: { line: number } };
      name: string;
    }) => `${item.uri}:${item.range.start.line}:${item.name}`,
  })
);

import {
  acquirePooledClient,
  isLanguageServerAvailable,
} from 'octocode-lsp/manager';
import { executeLspGetSemantics } from '../../../octocode-tools-core/src/tools/lsp/semantic_content/execution.js';
import { hints as semanticToolHints } from '../../../octocode-tools-core/src/tools/lsp/semantic_content/hints.js';
import {
  LspGetSemanticsOutputSchema,
  LspGetSemanticsQuerySchema,
} from '../../../octocode-tools-core/src/tools/lsp/semantic_content/scheme.js';
import {
  gatherIncomingCallsRecursive,
  gatherOutgoingCallsRecursive,
} from '../../../octocode-tools-core/src/tools/lsp/shared/callHierarchyTraversal.js';

const range = {
  start: { line: 0, character: 16 },
  end: { line: 0, character: 22 },
};

let tempDir: string;
let filePath: string;

describe('new public LSP tool execution', () => {
  beforeEach(async () => {
    vi.mocked(isLanguageServerAvailable).mockReset();
    vi.mocked(acquirePooledClient).mockReset();
    vi.mocked(gatherIncomingCallsRecursive).mockReset();
    vi.mocked(gatherOutgoingCallsRecursive).mockReset();
    tempDir = await mkdtemp(join(process.cwd(), '.tmp-octocode-lsp-tools-'));
    filePath = join(tempDir, 'fixture.ts');
    await writeFile(
      filePath,
      [
        'export function target() {',
        '  return 1;',
        '}',
        'export function caller() {',
        '  return target();',
        '}',
      ].join('\n')
    );
    vi.mocked(isLanguageServerAvailable).mockResolvedValue(true);
    vi.mocked(acquirePooledClient).mockResolvedValue(createClient() as never);
  });

  afterEach(async () => {
    vi.clearAllMocks();
    await rm(tempDir, { recursive: true, force: true });
  });

  it('returns semantic locations, references, hover, type, and implementation content', async () => {
    const result = await executeLspGetSemantics({
      queries: [
        anchored('definition'),
        anchored('references', { groupByFile: true, includeDeclaration: true }),
        anchored('hover'),
        anchored('typeDefinition'),
        anchored('implementation'),
      ],
    } as never);
    const text = textOf(result);

    expect(text).toContain('kind: definition');
    expect(text).toContain('kind: references');
    expect(text).toContain('kind: hover');
    expect(text).toContain('kind: typeDefinition');
    expect(text).toContain('kind: implementation');
    expect(text).toContain('**target**');
  });

  it('passes cached file content into LSP client calls', async () => {
    const client = createClient();
    vi.mocked(acquirePooledClient).mockResolvedValue(client as never);
    const expectedContent = [
      'export function target() {',
      '  return 1;',
      '}',
      'export function caller() {',
      '  return target();',
      '}',
    ].join('\n');

    await executeLspGetSemantics({
      queries: [
        anchored('definition'),
        anchored('references', { includeDeclaration: false }),
        anchored('hover'),
        anchored('typeDefinition'),
        anchored('implementation'),
        { uri: filePath, type: 'documentSymbols' },
        anchored('callers'),
      ],
    } as never);

    expect(client.gotoDefinition).toHaveBeenCalledWith(
      filePath,
      expect.objectContaining({ line: 0, character: 16 }),
      expectedContent
    );
    expect(client.findReferences).toHaveBeenCalledWith(
      filePath,
      expect.objectContaining({ line: 0, character: 16 }),
      false,
      expectedContent
    );
    expect(client.hover).toHaveBeenCalledWith(
      filePath,
      expect.objectContaining({ line: 0, character: 16 }),
      expectedContent
    );
    expect(client.typeDefinition).toHaveBeenCalledWith(
      filePath,
      expect.objectContaining({ line: 0, character: 16 }),
      expectedContent
    );
    expect(client.implementation).toHaveBeenCalledWith(
      filePath,
      expect.objectContaining({ line: 0, character: 16 }),
      expectedContent
    );
    expect(client.documentSymbols).toHaveBeenCalledWith(
      filePath,
      expectedContent
    );
    expect(client.prepareCallHierarchy).toHaveBeenCalledWith(
      filePath,
      expect.objectContaining({ line: 0, character: 16 }),
      expectedContent
    );
  });

  it('returns document symbols and call-flow payloads', async () => {
    vi.mocked(gatherIncomingCallsRecursive).mockResolvedValue({
      calls: [{ from: callItem('caller'), fromRanges: [range] }],
      truncatedByDepth: true,
      cycleCount: 0,
      failedRequestCount: 0,
    } as never);
    vi.mocked(gatherOutgoingCallsRecursive).mockResolvedValue({
      calls: [{ to: callItem('callee'), fromRanges: [range] }],
      truncatedByDepth: false,
      cycleCount: 1,
      failedRequestCount: 0,
    } as never);

    const result = await executeLspGetSemantics({
      queries: [
        { uri: filePath, type: 'documentSymbols' },
        anchored('callers'),
        anchored('callees'),
        anchored('callHierarchy', { depth: 2 }),
      ],
    } as never);
    const text = textOf(result);

    expect(text).toContain('kind: documentSymbols');
    expect(text).toContain('direction: incoming');
    expect(text).toContain('direction: outgoing');
    expect(text).toContain('dynamicCallsExcluded: true');
    expect(text).toContain('truncatedByDepth: true');
    expect(text).toContain('cycleCount: 1');
  });

  it('supports compact semantic format for high-volume rows', async () => {
    vi.mocked(gatherIncomingCallsRecursive).mockResolvedValue({
      calls: [
        {
          from: { ...callItem('callerFn'), content: 'function callerFn() {}' },
          fromRanges: [range, range],
        },
      ],
      truncatedByDepth: false,
      cycleCount: 0,
      failedRequestCount: 0,
    } as never);
    vi.mocked(gatherOutgoingCallsRecursive).mockResolvedValue({
      calls: [],
      truncatedByDepth: false,
      cycleCount: 0,
      failedRequestCount: 0,
    } as never);

    const result = await executeLspGetSemantics({
      queries: [
        { uri: filePath, type: 'documentSymbols', format: 'compact' },
        anchored('references', { groupByFile: true, format: 'compact' }),
        anchored('callers', { contextLines: 2, format: 'compact' }),
      ],
    } as never);
    const text = textOf(result);

    expect(text).toContain('format: compact');
    expect(text).toContain('- 1:16');
    expect(text).toContain('count=2 lines=');
    expect(text).toContain('incoming callerFn');
    expect(text).not.toContain('childCount:');
    const parsed = LspGetSemanticsOutputSchema.safeParse(
      result.structuredContent
    );
    expect(
      parsed.success,
      parsed.success ? '' : JSON.stringify(parsed.error.issues, null, 2)
    ).toBe(true);
  });

  it('documentSymbols pagination hasMore=true does not affect completeness', async () => {
    const manySymbols = Array.from({ length: 50 }, (_, i) => ({
      name: `sym${i}`,
      kind: 12,
      range: {
        start: { line: i, character: 0 },
        end: { line: i, character: 5 },
      },
    }));
    vi.mocked(acquirePooledClient).mockResolvedValue(
      createClient({
        documentSymbols: vi.fn().mockResolvedValue(manySymbols),
      }) as never
    );
    const result = await executeLspGetSemantics({
      queries: [
        { uri: filePath, type: 'documentSymbols', page: 1, itemsPerPage: 10 },
      ],
    } as never);
    const text = textOf(result);
    expect(text).toContain('hasMore: true');
    expect(text).not.toContain('Result pagination has more results');
  });

  it('references and calls return zero results without error', async () => {
    vi.mocked(acquirePooledClient).mockResolvedValue(
      createClient({
        findReferences: vi.fn().mockResolvedValue([]),
        prepareCallHierarchy: vi.fn().mockResolvedValue([callItem('target')]),
      }) as never
    );
    vi.mocked(gatherIncomingCallsRecursive).mockResolvedValue({
      calls: [],
      truncatedByDepth: false,
      cycleCount: 0,
      failedRequestCount: 0,
    } as never);
    vi.mocked(gatherOutgoingCallsRecursive).mockResolvedValue({
      calls: [],
      truncatedByDepth: false,
      cycleCount: 0,
      failedRequestCount: 0,
    } as never);

    const result = await executeLspGetSemantics({
      queries: [anchored('references'), anchored('callers')],
    } as never);
    const text = textOf(result);

    expect(text).not.toContain('status: error');
    expect(text).toContain('kind: references');
    expect(text).toContain('kind: callers');
  });

  it('reports unsupported semantic capabilities explicitly', async () => {
    vi.mocked(acquirePooledClient).mockResolvedValue(
      createClient({
        hasCapability: vi.fn(
          (capability: string) => capability !== 'hoverProvider'
        ),
      }) as never
    );

    const result = await executeLspGetSemantics({
      queries: [anchored('hover')],
    } as never);

    expect(textOf(result)).toContain('hoverProvider unsupported');
  });

  it('reports semantic empty, unsupported, unavailable, and symbol-not-found paths', async () => {
    vi.mocked(isLanguageServerAvailable)
      .mockResolvedValueOnce(true)
      .mockResolvedValueOnce(true)
      .mockResolvedValueOnce(false);
    vi.mocked(acquirePooledClient).mockResolvedValue(
      createClient({
        gotoDefinition: vi.fn().mockResolvedValue([]),
        hasCapability: vi.fn(
          (capability: string) => capability !== 'callHierarchyProvider'
        ),
      }) as never
    );

    const result = await executeLspGetSemantics({
      queries: [
        anchored('definition'),
        anchored('callers'),
        anchored('references', { lineHint: 99 }),
        { uri: filePath, type: 'documentSymbols' },
      ],
    } as never);
    const text = textOf(result);

    expect(text).toContain('definitionProvider returned no locations');
    expect(text).toContain('category: noLocations');
    expect(text).toContain('callHierarchyProvider unsupported');
    expect(text).toContain('category: unsupportedOperation');
    expect(text).toContain('Could not find symbol');
    expect(text).toContain('category: symbolNotFound');
    expect(text).toContain('Language server unavailable');
    expect(text).toContain('category: serverUnavailable');
  });

  it('reports unsupported providers and missing call hierarchy roots', async () => {
    vi.mocked(acquirePooledClient).mockResolvedValue(
      createClient({
        hasCapability: vi.fn(
          (capability: string) =>
            ![
              'implementationProvider',
              'referencesProvider',
              'typeDefinitionProvider',
            ].includes(capability)
        ),
        prepareCallHierarchy: vi.fn().mockResolvedValue([]),
      }) as never
    );

    const result = await executeLspGetSemantics({
      queries: [
        anchored('implementation'),
        anchored('references'),
        anchored('typeDefinition'),
        anchored('callHierarchy'),
      ],
    } as never);
    const text = textOf(result);

    expect(text).toContain('implementationProvider unsupported');
    expect(text).toContain('referencesProvider unsupported');
    expect(text).toContain('typeDefinitionProvider unsupported');
    expect(text).toContain('No callable symbol found');
  });

  it('requires uri and does not rewrite filePath', async () => {
    const semanticParse = LspGetSemanticsQuerySchema.safeParse({
      filePath,
      type: 'documentSymbols',
    });

    expect(semanticParse.success).toBe(false);
  });

  it('normalizes hover content variants', async () => {
    vi.mocked(acquirePooledClient).mockResolvedValue(
      createClient({
        hover: vi
          .fn()
          .mockResolvedValueOnce({ contents: 'plain hover' })
          .mockResolvedValueOnce({
            contents: ['one', { value: 'two' }, 3],
          })
          .mockResolvedValueOnce({
            contents: { kind: 'plaintext', value: 'typed hover' },
          })
          .mockResolvedValueOnce({ contents: { kind: 'markdown' } })
          .mockResolvedValueOnce(null),
      }) as never
    );

    const result = await executeLspGetSemantics({
      queries: [
        anchored('hover'),
        anchored('hover'),
        anchored('hover'),
        anchored('hover'),
        anchored('hover'),
      ],
    } as never);
    const text = textOf(result);

    expect(text).toContain('plain hover');
    expect(text).toContain('one');
    expect(text).toContain('two');
    expect(text).toContain('typed hover');
    expect(text).toContain('hoverProvider returned no hover content');
  });

  it('handles serverAvailable=false for symbol-anchored queries', async () => {
    vi.mocked(isLanguageServerAvailable).mockResolvedValue(false);
    const result = await executeLspGetSemantics({
      queries: [anchored('definition'), anchored('references')],
    } as never);
    const text = textOf(result);
    expect(text).toContain('Language server unavailable');
  });

  it('handles acquirePooledClient returning null for symbol queries', async () => {
    vi.mocked(acquirePooledClient).mockResolvedValue(null);
    const result = await executeLspGetSemantics({
      queries: [anchored('definition')],
    } as never);
    expect(textOf(result)).toContain('Language server unavailable');
  });

  it('handles documentSymbols with an invalid/missing path', async () => {
    const result = await executeLspGetSemantics({
      queries: [{ uri: join(tempDir, 'missing.ts'), type: 'documentSymbols' }],
    } as never);
    expect(textOf(result)).toContain('file_not_found');
  });

  it('handles documentSymbols when language server is unavailable', async () => {
    vi.mocked(isLanguageServerAvailable).mockResolvedValue(false);
    const result = await executeLspGetSemantics({
      queries: [{ uri: filePath, type: 'documentSymbols' }],
    } as never);
    const text = textOf(result);
    expect(text).toContain('Language server unavailable');
    expect(text).toContain('kind: documentSymbols');
  });

  it('paginates document symbols across pages', async () => {
    const manySymbols = Array.from({ length: 50 }, (_, i) => ({
      name: `sym${i}`,
      kind: 12,
      range: {
        start: { line: i, character: 0 },
        end: { line: i, character: 10 },
      },
    }));
    vi.mocked(acquirePooledClient).mockResolvedValue(
      createClient({
        documentSymbols: vi.fn().mockResolvedValue(manySymbols),
      }) as never
    );
    const result = await executeLspGetSemantics({
      queries: [
        { uri: filePath, type: 'documentSymbols', page: 1, itemsPerPage: 10 },
      ],
    } as never);
    const text = textOf(result);
    expect(text).toContain('hasMore: true');
    expect(text).toContain('nextPage: 2');
  });

  it('paginates reference and location lists with next-page hints', async () => {
    const manyLocations = [
      location('one', 1),
      location('two', 2),
      location('three', 3),
    ];
    vi.mocked(acquirePooledClient).mockResolvedValue(
      createClient({
        gotoDefinition: vi.fn().mockResolvedValue(manyLocations),
        findReferences: vi.fn().mockResolvedValue(manyLocations),
      }) as never
    );

    const result = await executeLspGetSemantics({
      queries: [
        anchored('definition', { page: 1, itemsPerPage: 1 }),
        anchored('references', { page: 1, itemsPerPage: 1 }),
      ],
    } as never);
    const text = textOf(result);

    expect(text).toContain('Page 1/3 (1 of 3 locations). Next: page=2');
    expect(text).toContain('Page 1/3 (1 of 3 references). Next: page=2');
    expect(text.match(/hasMore: true/g)?.length).toBeGreaterThanOrEqual(2);
    expect(text).toContain('nextPage: 2');
  });

  it('summary.kinds reflects total symbol count across all pages, not only the current page slice', async () => {
    const symbols = [
      ...Array.from({ length: 30 }, (_, i) => ({
        name: `fn${i}`,
        kind: 12,
        range: {
          start: { line: i, character: 0 },
          end: { line: i, character: 5 },
        },
      })),
      ...Array.from({ length: 5 }, (_, i) => ({
        name: `Cls${i}`,
        kind: 5,
        range: {
          start: { line: 30 + i, character: 0 },
          end: { line: 30 + i, character: 5 },
        },
      })),
    ];
    vi.mocked(acquirePooledClient).mockResolvedValue(
      createClient({
        documentSymbols: vi.fn().mockResolvedValue(symbols),
      }) as never
    );
    const result = await executeLspGetSemantics({
      queries: [
        { uri: filePath, type: 'documentSymbols', page: 1, itemsPerPage: 10 },
      ],
    } as never);
    const text = textOf(result);
    expect(text).toContain('function: 30');
    expect(text).toContain('class: 5');
    expect(text).toContain('totalSymbols: 35');
  });

  it('paginates call results with a next-page hint when contextLines=0', async () => {
    const manyCalls = Array.from({ length: 15 }, (_, i) => callItem(`fn${i}`));
    vi.mocked(gatherIncomingCallsRecursive).mockResolvedValue({
      calls: manyCalls.map(c => ({ from: c, fromRanges: [range, range] })),
      truncatedByDepth: false,
      cycleCount: 0,
      failedRequestCount: 0,
    } as never);
    vi.mocked(gatherOutgoingCallsRecursive).mockResolvedValue({
      calls: [],
      truncatedByDepth: false,
      cycleCount: 0,
      failedRequestCount: 0,
    } as never);
    const result = await executeLspGetSemantics({
      queries: [anchored('callers', { page: 1, itemsPerPage: 5 })],
    } as never);
    const text = textOf(result);
    expect(text).toContain('hasMore: true');
    expect(text).toContain('nextPage: 2');
    expect(text).toContain('Page 1/3 (5 of 15 calls). Next: page=2');
  });

  it('renders contentPreview and deduplicates ranges when contextLines>0', async () => {
    vi.mocked(gatherIncomingCallsRecursive).mockResolvedValue({
      calls: [
        {
          from: { ...callItem('callerFn'), content: 'function callerFn() {}' },
          fromRanges: [range, range, range],
        },
      ],
      truncatedByDepth: false,
      cycleCount: 0,
      failedRequestCount: 0,
    } as never);
    vi.mocked(gatherOutgoingCallsRecursive).mockResolvedValue({
      calls: [],
      truncatedByDepth: false,
      cycleCount: 0,
      failedRequestCount: 0,
    } as never);
    const result = await executeLspGetSemantics({
      queries: [anchored('callers', { contextLines: 2 })],
    } as never);
    const text = textOf(result);
    expect(text).toContain('callerFn');
    expect(text).toContain('rangeCount: 3');
    expect(text).toContain('rangeSampleCount: 1');
  });

  it('renders nested document symbols (children structure)', async () => {
    const nestedSymbols = [
      {
        name: 'MyClass',
        kind: 5,
        range: {
          start: { line: 0, character: 0 },
          end: { line: 20, character: 1 },
        },
        children: [
          {
            name: 'myMethod',
            kind: 6,
            range: {
              start: { line: 2, character: 2 },
              end: { line: 5, character: 3 },
            },
          },
          { notASymbol: true },
        ],
      },
      {
        name: undefined,
        kind: 12,
        location: {
          range: {
            start: { line: 10, character: 0 },
            end: { line: 10, character: 5 },
          },
        },
      },
    ];
    vi.mocked(acquirePooledClient).mockResolvedValue(
      createClient({
        documentSymbols: vi.fn().mockResolvedValue(nestedSymbols),
      }) as never
    );
    const result = await executeLspGetSemantics({
      queries: [{ uri: filePath, type: 'documentSymbols' }],
    } as never);
    const text = textOf(result);
    expect(text).toContain('MyClass');
    expect(text).toContain('myMethod');
    expect(text).toContain('containerName: MyClass');
  });

  it('renders various symbolKindName values covering all switch cases', async () => {
    const allKinds = [
      1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
      23, 24, 25, 26, 999,
    ];
    const kindSymbols = allKinds.map((kind, i) => ({
      name: `sym${i}`,
      kind,
      range: {
        start: { line: i, character: 0 },
        end: { line: i, character: 5 },
      },
    }));
    vi.mocked(acquirePooledClient).mockResolvedValue(
      createClient({
        documentSymbols: vi.fn().mockResolvedValue(kindSymbols),
      }) as never
    );
    const result = await executeLspGetSemantics({
      queries: [{ uri: filePath, type: 'documentSymbols' }],
    } as never);
    const text = textOf(result);
    expect(text).toContain('file');
    expect(text).toContain('module');
    expect(text).toContain('class');
    expect(text).toContain('array');
    expect(text).toContain('operator');
    expect(text).toContain('unknown');
  });

  it('buildReferencesByFile handles same-file multi-location with isDefinition', async () => {
    vi.mocked(acquirePooledClient).mockResolvedValue(
      createClient({
        findReferences: vi.fn().mockResolvedValue([
          {
            uri: filePath,
            range: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 6 },
            },
            content: 'export',
            isDefinition: true,
          },
          {
            uri: filePath,
            range: {
              start: { line: 2, character: 0 },
              end: { line: 2, character: 6 },
            },
            content: 'usage',
          },
        ]),
      }) as never
    );
    const result = await executeLspGetSemantics({
      queries: [anchored('references', { groupByFile: true })],
    } as never);
    const text = textOf(result);
    expect(text).toContain('hasDefinition: true');
    expect(text).toContain('count: 2');
  });

  it('returns a schema-valid empty envelope when the symbol is not found near lineHint', async () => {
    const result = await executeLspGetSemantics({
      queries: [
        {
          uri: filePath,
          type: 'definition',
          symbolName: 'nonExistentSymbol',
          lineHint: 99,
        },
      ],
    } as never);

    const parsed = LspGetSemanticsOutputSchema.safeParse(
      result.structuredContent
    );
    expect(
      parsed.success,
      parsed.success ? '' : JSON.stringify(parsed.error.issues, null, 2)
    ).toBe(true);
    if (!parsed.success) return;

    const firstResult = parsed.data.results[0];
    expect(firstResult).toBeDefined();
    expect(firstResult).not.toHaveProperty('status');
    expect(firstResult!.data).toMatchObject({
      type: 'definition',
      uri: 'fixture.ts',
      lsp: {},
      payload: expect.objectContaining({ kind: 'empty' }),
    });
    expect(
      (firstResult!.data as { lsp?: Record<string, unknown> }).lsp
    ).not.toHaveProperty('serverAvailable');
  });

  it('handles empty queries array', async () => {
    const result = await executeLspGetSemantics({
      queries: [],
    } as never);
    expect(result).toBeDefined();
  });

  it('exposes direct hint branches for the public LSP tool', () => {
    expect(semanticToolHints.empty({ symbolName: 'target' })).toEqual(
      expect.arrayContaining([expect.stringContaining('localSearchCode')])
    );
    expect(semanticToolHints.empty({ type: 'hover' } as never)).toEqual(
      expect.arrayContaining([expect.stringContaining('localSearchCode')])
    );
    expect(semanticToolHints.error({ errorType: 'lsp_unavailable' })).toEqual(
      expect.arrayContaining([
        expect.stringContaining('Language server unavailable'),
      ])
    );
    expect(semanticToolHints.error({ errorType: 'symbol_not_found' })).toEqual(
      expect.arrayContaining([expect.stringContaining('localSearchCode')])
    );
  });

  it('sets isAmbiguous when symbol occurs multiple times and lineHint is far from resolved position', async () => {
    // Write a larger fixture where 'target' is at line 0 and line 9.
    // lineHint: 5 => resolver picks nearest occurrence; |foundAtLine - 5| > 3 for whichever is found.
    const largerFixture = join(tempDir, 'ambiguous.ts');
    await writeFile(
      largerFixture,
      [
        'export function target() {', // line 0
        '  return 1;', // line 1
        '}', // line 2
        '// padding line a', // line 3
        '// padding line b', // line 4
        '// padding line c', // line 5
        '// padding line d', // line 6
        '// padding line e', // line 7
        '// padding line f', // line 8
        'export function reuse() { return target(); }', // line 9
      ].join('\n')
    );
    // lineHint: 5 is far from both line 0 (|0-5|=5>3) and line 9 (|9-5|=4>3).
    // The resolver finds nearest occurrence (line 0 or line 9), lineDeviation >3.
    const result = await executeLspGetSemantics({
      queries: [
        {
          uri: largerFixture,
          type: 'definition',
          symbolName: 'target',
          lineHint: 5,
        },
      ],
    } as never);
    const text = textOf(result);
    expect(text).toContain('isAmbiguous: true');
  });

  it('does NOT set isAmbiguous when symbol is unique or lineHint is close', async () => {
    // 'target' at line 0, lineHint: 1 => lineDeviation = 1 ≤ 3 => isAmbiguous omitted
    const result = await executeLspGetSemantics({
      queries: [anchored('definition')],
    } as never);
    const text = textOf(result);
    expect(text).not.toContain('isAmbiguous: true');
  });
});

function anchored(type: string, extra: Record<string, unknown> = {}) {
  return {
    uri: filePath,
    type,
    symbolName: 'target',
    lineHint: 1,
    ...extra,
  };
}

function createClient(overrides: Record<string, unknown> = {}) {
  return {
    hasCapability: vi.fn(() => true),
    gotoDefinition: vi.fn().mockResolvedValue([location('definition target')]),
    findReferences: vi
      .fn()
      .mockResolvedValue([
        location('definition target'),
        location('target();', 4),
      ]),
    hover: vi.fn().mockResolvedValue({
      contents: { kind: 'markdown', value: '**target**: () => number' },
    }),
    typeDefinition: vi.fn().mockResolvedValue([location('type target')]),
    implementation: vi
      .fn()
      .mockResolvedValue([location('implementation target')]),
    documentSymbols: vi.fn().mockResolvedValue([callItem('target')]),
    prepareCallHierarchy: vi.fn().mockResolvedValue([callItem('target')]),
    ...overrides,
  };
}

function location(content: string, line = 0) {
  return {
    uri: filePath,
    range: {
      start: { line, character: 16 },
      end: { line, character: 22 },
    },
    content,
  };
}

function callItem(name: string) {
  return {
    name,
    kind: 12,
    uri: filePath,
    range,
    selectionRange: range,
  };
}

function textOf(result: CallToolResult): string {
  return result.content
    .map(item => (item.type === 'text' && 'text' in item ? item.text : ''))
    .join('\n');
}
