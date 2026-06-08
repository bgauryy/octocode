import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';

vi.mock('../../src/lsp/manager.js', () => ({
  acquirePooledClient: vi.fn(),
  isLanguageServerAvailable: vi.fn(),
}));

vi.mock('../../src/lsp/workspaceRoot.js', () => ({
  resolveWorkspaceRootForFile: vi.fn().mockResolvedValue('/workspace'),
}));

vi.mock('../../src/tools/lsp/shared/callHierarchyTraversal.js', () => ({
  gatherIncomingCallsRecursive: vi.fn(),
  gatherOutgoingCallsRecursive: vi.fn(),
  createCallItemKey: (item: {
    uri: string;
    range: { start: { line: number } };
    name: string;
  }) => `${item.uri}:${item.range.start.line}:${item.name}`,
}));

import {
  acquirePooledClient,
  isLanguageServerAvailable,
} from '../../src/lsp/manager.js';
import { executeLspGetDiagnostics } from '../../src/tools/lsp/diagnostics/execution.js';
import { hints as diagnosticToolHints } from '../../src/tools/lsp/diagnostics/hints.js';
import { executeLspGetSemanticContent } from '../../src/tools/lsp/semantic_content/execution.js';
import { hints as semanticToolHints } from '../../src/tools/lsp/semantic_content/hints.js';
import { LspGetSemanticContentQuerySchema } from '../../src/tools/lsp/semantic_content/scheme.js';
import { LspGetDiagnosticsQuerySchema } from '../../src/tools/lsp/diagnostics/scheme.js';
import {
  gatherIncomingCallsRecursive,
  gatherOutgoingCallsRecursive,
} from '../../src/tools/lsp/shared/callHierarchyTraversal.js';

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

  it('returns unavailable diagnostics without acquiring a client', async () => {
    vi.mocked(isLanguageServerAvailable).mockResolvedValue(false);
    vi.mocked(acquirePooledClient).mockResolvedValue(null);

    const result = await executeLspGetDiagnostics({
      queries: [{ uri: filePath, severity: 'all' }],
    } as never);

    expect(textOf(result)).toContain('Language server unavailable');
    expect(acquirePooledClient).not.toHaveBeenCalled();
  });

  it('filters diagnostics by severity and source', async () => {
    vi.mocked(acquirePooledClient).mockResolvedValue(
      createClient({
        getDiagnostics: vi.fn().mockResolvedValue({
          source: 'pull',
          diagnostics: [
            {
              range,
              severity: 1,
              message: 'target is broken',
              source: 'typescript',
              code: 'TS1',
            },
            {
              range,
              severity: 2,
              message: 'target is suspicious',
              source: 'eslint',
              code: 'ES1',
            },
          ],
        }),
      }) as never
    );

    const result = await executeLspGetDiagnostics({
      queries: [{ uri: filePath, severity: 'error', source: 'typescript' }],
    } as never);
    const text = textOf(result);

    expect(text).toContain('target is broken');
    expect(text).not.toContain('target is suspicious');
    expect(text).toContain('errors: 1');
  });

  it('summarizes diagnostic severities and unavailable diagnostic sources', async () => {
    vi.mocked(acquirePooledClient).mockResolvedValue(
      createClient({
        getDiagnostics: vi.fn().mockResolvedValue({
          source: 'unavailable',
          diagnostics: [
            { range, message: 'implicit error' },
            { range, severity: 2, message: 'warning' },
            { range, severity: 3, message: 'information' },
            { range, severity: 4, message: 'hint' },
          ],
        }),
      }) as never
    );

    const result = await executeLspGetDiagnostics({
      queries: [{ uri: filePath, severity: 'all' }],
    } as never);
    const text = textOf(result);

    expect(text).toContain('errors: 1');
    expect(text).toContain('warnings: 1');
    expect(text).toContain('information: 1');
    expect(text).toContain('hints: 1');
    expect(text).toContain('Diagnostics unavailable');
  });

  it('returns structured file-read errors for missing files inside the workspace', async () => {
    const missingFile = join(tempDir, 'missing.ts');

    const result = await executeLspGetDiagnostics({
      queries: [{ uri: missingFile, severity: 'all' }],
    } as never);

    expect(textOf(result)).toContain('file_not_found');
  });

  it('returns semantic locations, references, hover, type, and implementation content', async () => {
    const result = await executeLspGetSemanticContent({
      queries: [
        anchored('definition'),
        anchored('references', { groupByFile: true, includeDeclaration: true }),
        anchored('hover'),
        anchored('typeDefinition'),
        anchored('implementation'),
      ],
    } as never);
    const text = textOf(result);

    expect(text).toContain('kind: "definition"');
    expect(text).toContain('kind: "references"');
    expect(text).toContain('kind: "hover"');
    expect(text).toContain('kind: "typeDefinition"');
    expect(text).toContain('kind: "implementation"');
    expect(text).toContain('**target**');
  });

  it('returns document symbols and call-flow payloads', async () => {
    vi.mocked(gatherIncomingCallsRecursive).mockResolvedValue([
      { from: callItem('caller'), fromRanges: [range] },
    ] as never);
    vi.mocked(gatherOutgoingCallsRecursive).mockResolvedValue([
      { to: callItem('callee'), fromRanges: [range] },
    ] as never);

    const result = await executeLspGetSemanticContent({
      queries: [
        { uri: filePath, type: 'documentSymbols' },
        anchored('callers'),
        anchored('callees'),
        anchored('callHierarchy', { depth: 2 }),
      ],
    } as never);
    const text = textOf(result);

    expect(text).toContain('kind: "documentSymbols"');
    expect(text).toContain('direction: "incoming"');
    expect(text).toContain('direction: "outgoing"');
    expect(text).toContain('dynamicCallsExcluded: true');
  });

  it('reports unsupported semantic capabilities explicitly', async () => {
    vi.mocked(acquirePooledClient).mockResolvedValue(
      createClient({
        hasCapability: vi.fn(
          (capability: string) => capability !== 'hoverProvider'
        ),
      }) as never
    );

    const result = await executeLspGetSemanticContent({
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

    const result = await executeLspGetSemanticContent({
      queries: [
        anchored('definition'),
        anchored('callers'),
        anchored('references', { lineHint: 99 }),
        { uri: filePath, type: 'documentSymbols' },
      ],
    } as never);
    const text = textOf(result);

    expect(text).toContain('definitionProvider returned no locations');
    expect(text).toContain('callHierarchyProvider unsupported');
    expect(text).toContain('symbol_not_found');
    expect(text).toContain('Language server unavailable');
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

    const result = await executeLspGetSemanticContent({
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

  it('handles invalid path input and schema aliases', async () => {
    const diagnosticsParse = LspGetDiagnosticsQuerySchema.safeParse({
      filePath,
    });
    const semanticParse = LspGetSemanticContentQuerySchema.safeParse({
      filePath,
      type: 'documentSymbols',
    });

    expect(diagnosticsParse.success).toBe(true);
    expect(semanticParse.success).toBe(true);
    if (diagnosticsParse.success)
      expect(diagnosticsParse.data.uri).toBe(filePath);
    if (semanticParse.success) expect(semanticParse.data.uri).toBe(filePath);

    const result = await executeLspGetDiagnostics({
      queries: [{ severity: 'all' }],
    } as never);

    expect(textOf(result)).toContain('pathValidationFailed');
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

    const result = await executeLspGetSemanticContent({
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

  it('exposes direct hint branches for the new public LSP tools', () => {
    expect(diagnosticToolHints.empty({ uri: filePath })).toContain(
      `No diagnostics returned for ${filePath}.`
    );
    expect(diagnosticToolHints.error({ errorType: 'lsp_unavailable' })).toEqual(
      expect.arrayContaining([
        expect.stringContaining('Language server unavailable'),
      ])
    );
    expect(semanticToolHints.empty({ symbolName: 'target' })).toEqual(
      expect.arrayContaining([expect.stringContaining('target')])
    );
    expect(semanticToolHints.empty({ type: 'hover' } as never)).toEqual(
      expect.arrayContaining([expect.stringContaining('type="hover"')])
    );
    expect(semanticToolHints.error({ errorType: 'lsp_unavailable' })).toEqual(
      expect.arrayContaining([
        expect.stringContaining('Language server unavailable'),
      ])
    );
    expect(semanticToolHints.error({ errorType: 'symbol_not_found' })).toEqual(
      expect.arrayContaining([expect.stringContaining('Symbol was not found')])
    );
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
    getDiagnostics: vi.fn().mockResolvedValue({
      source: 'publish',
      diagnostics: [],
    }),
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
