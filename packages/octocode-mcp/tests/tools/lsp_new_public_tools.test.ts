import { describe, expect, it } from 'vitest';
import { LSP_GET_SEMANTIC_CONTENT_TOOL_NAME } from '../../../octocode-tools-core/src/tools/lsp/shared/semanticTypes.js';
import {
  BulkLspGetSemanticsQuerySchema,
  LspGetSemanticsQuerySchema,
} from '../../../octocode-tools-core/src/tools/lsp/semantic_content/scheme.js';
import { registerLspGetSemanticsTool } from '../../src/tools/lsp/semantic_content/register.js';
import { ALL_TOOLS } from '../../src/tools/toolConfig.js';
import { createMockMcpServer } from '../fixtures/mcp-fixtures.js';

const removedLspToolNames = [
  `lsp${'Goto'}Definition`,
  `lsp${'Find'}References`,
  `lsp${'Call'}Hierarchy`,
];

describe('new public LSP tools', () => {
  it('advertises only lspGetSemantics without removed LSP tools', () => {
    const names = ALL_TOOLS.map(tool => tool.name);

    expect(names).toContain(LSP_GET_SEMANTIC_CONTENT_TOOL_NAME);
    for (const removedName of removedLspToolNames) {
      expect(names).not.toContain(removedName);
    }
    expect(names).toHaveLength(13);
  });

  it('registers the semantic tool with read-only annotations', () => {
    const server = createMockMcpServer();

    registerLspGetSemanticsTool(server.server);

    expect(server.registrations).toContainEqual(
      expect.objectContaining({
        name: LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
        options: expect.objectContaining({
          inputSchema: expect.any(Object),
          annotations: expect.objectContaining({ readOnlyHint: true }),
        }),
        handler: expect.any(Function),
      })
    );
  });

  it('enforces semantic type anchoring rules', () => {
    expect(
      LspGetSemanticsQuerySchema.safeParse({
        type: 'documentSymbols',
        uri: '/tmp/a.ts',
      }).success
    ).toBe(true);
    expect(
      LspGetSemanticsQuerySchema.safeParse({
        type: 'definition',
        uri: '/tmp/a.ts',
        symbolName: 'target',
        lineHint: 1,
      }).success
    ).toBe(true);
    expect(
      LspGetSemanticsQuerySchema.safeParse({
        type: 'definition',
        uri: '/tmp/a.ts',
      }).success
    ).toBe(false);
  });

  it('bulk schemas parse minimal valid requests', () => {
    expect(
      BulkLspGetSemanticsQuerySchema.safeParse({
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
