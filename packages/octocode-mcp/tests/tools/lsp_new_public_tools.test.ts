import { describe, expect, it } from 'vitest';
import { LSP_GET_SEMANTIC_CONTENT_TOOL_NAME } from '../../../octocode-tools-core/src/tools/lsp/shared/semanticTypes.js';
import {
  BulkLspGetSemanticContentQuerySchema,
  LspGetSemanticContentQuerySchema,
} from '../../../octocode-tools-core/src/tools/lsp/semantic_content/scheme.js';
import { registerLspGetSemanticContentTool } from '../../src/tools/lsp/semantic_content/register.js';
import { ALL_TOOLS } from '../../src/tools/toolConfig.js';
import { createMockMcpServer } from '../fixtures/mcp-fixtures.js';

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
    const server = createMockMcpServer();

    registerLspGetSemanticContentTool(server.server);

    expect(server.registrations).toContainEqual(
      expect.objectContaining({
        name: LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
        options: expect.objectContaining({
          inputSchema: expect.any(Object),
          outputSchema: expect.any(Object),
          annotations: expect.objectContaining({ readOnlyHint: true }),
        }),
        handler: expect.any(Function),
      })
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
