import { describe, expect, it, vi } from 'vitest';
import {
  LSP_GET_DIAGNOSTICS_TOOL_NAME,
  LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
} from '../../src/tools/lsp/shared/semanticTypes.js';
import {
  BulkLspGetSemanticContentQuerySchema,
  LspGetSemanticContentQuerySchema,
} from '../../src/tools/lsp/semantic_content/scheme.js';
import { BulkLspGetDiagnosticsQuerySchema } from '../../src/tools/lsp/diagnostics/scheme.js';
import { registerLspGetSemanticContentTool } from '../../src/tools/lsp/semantic_content/register.js';
import { registerLspGetDiagnosticsTool } from '../../src/tools/lsp/diagnostics/register.js';
import { ALL_TOOLS } from '../../src/tools/toolConfig.js';

const legacyLspToolNames = [
  `lsp${'Goto'}Definition`,
  `lsp${'Find'}References`,
  `lsp${'Call'}Hierarchy`,
];

describe('new public LSP tools', () => {
  it('advertises lspGetSemanticContent and lspGetDiagnostics instead of legacy LSP tools', () => {
    const names = ALL_TOOLS.map(tool => tool.name);

    expect(names).toContain(LSP_GET_SEMANTIC_CONTENT_TOOL_NAME);
    expect(names).toContain(LSP_GET_DIAGNOSTICS_TOOL_NAME);
    for (const legacyName of legacyLspToolNames) {
      expect(names).not.toContain(legacyName);
    }
    expect(names).toHaveLength(13);
  });

  it('registers both new tools with read-only annotations', () => {
    const server = { registerTool: vi.fn() };

    registerLspGetSemanticContentTool(server as never);
    registerLspGetDiagnosticsTool(server as never);

    expect(server.registerTool).toHaveBeenCalledWith(
      LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
      expect.objectContaining({
        inputSchema: expect.any(Object),
        outputSchema: expect.any(Object),
        annotations: expect.objectContaining({ readOnlyHint: true }),
      }),
      expect.any(Function)
    );
    expect(server.registerTool).toHaveBeenCalledWith(
      LSP_GET_DIAGNOSTICS_TOOL_NAME,
      expect.objectContaining({
        inputSchema: expect.any(Object),
        outputSchema: expect.any(Object),
        annotations: expect.objectContaining({ readOnlyHint: true }),
      }),
      expect.any(Function)
    );
  });

  it('enforces semantic type anchoring rules', () => {
    expect(
      LspGetSemanticContentQuerySchema.safeParse({
        type: 'documentSymbols',
        uri: '/tmp/a.ts',
      }).success
    ).toBe(true);
    expect(
      LspGetSemanticContentQuerySchema.safeParse({
        type: 'definition',
        uri: '/tmp/a.ts',
        symbolName: 'target',
        lineHint: 1,
      }).success
    ).toBe(true);
    expect(
      LspGetSemanticContentQuerySchema.safeParse({
        type: 'definition',
        uri: '/tmp/a.ts',
      }).success
    ).toBe(false);
  });

  it('bulk schemas parse minimal valid requests', () => {
    expect(
      BulkLspGetSemanticContentQuerySchema.safeParse({
        queries: [
          {
            type: 'definition',
            uri: '/tmp/a.ts',
            symbolName: 'target',
            lineHint: 1,
          },
        ],
      }).success
    ).toBe(true);
    expect(
      BulkLspGetDiagnosticsQuerySchema.safeParse({
        queries: [{ uri: '/tmp/a.ts', severity: 'all' }],
      }).success
    ).toBe(true);
  });
});
