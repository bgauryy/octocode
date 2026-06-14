import { describe, expect, it, vi } from 'vitest';
import { LSP_GET_SEMANTIC_CONTENT_TOOL_NAME } from '@octocodeai/octocode-tools-core';
import {
  BulkLspGetSemanticContentQuerySchema,
  LspGetSemanticContentQuerySchema,
} from '@octocodeai/octocode-tools-core';
import { registerLspGetSemanticContentTool } from '../../src/tools/lsp/semantic_content/register.js';
import { ALL_TOOLS } from '../../src/tools/toolConfig.js';

const removedLspToolNames = [
  `lsp${'Goto'}Definition`,
  `lsp${'Find'}References`,
  `lsp${'Call'}Hierarchy`,
];

describe('new public LSP tools', () => {
  it('advertises only lspGetSemanticContent without removed LSP tools', () => {
    const names = ALL_TOOLS.map(tool => tool.name);

    expect(names).toContain(LSP_GET_SEMANTIC_CONTENT_TOOL_NAME);
    for (const removedName of removedLspToolNames) {
      expect(names).not.toContain(removedName);
    }
    expect(names).toHaveLength(12);
  });

  it('registers the semantic tool with read-only annotations', () => {
    const server = { registerTool: vi.fn() };

    registerLspGetSemanticContentTool(server as never);

    expect(server.registerTool).toHaveBeenCalledWith(
      LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
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
  });
});
