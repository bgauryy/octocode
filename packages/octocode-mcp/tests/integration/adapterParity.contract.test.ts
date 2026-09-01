import type { CallToolResult } from '@modelcontextprotocol/server';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  ADAPTER_PARITY_CASES,
  CANONICAL_ADAPTER_TOOL_NAMES,
  createAdapterParityErrorResult,
  createAdapterParityPageResult,
  getAdapterParityContinuation,
  type AdapterParityCase,
} from '../../../octocode-tools-core/tests/fixtures/adapterParityFixture.js';
import { createMockMcpServer } from '../fixtures/mcp-fixtures.js';

describe('MCP registration and dispatch adapter parity', () => {
  beforeEach(() => {
    vi.resetModules();
    process.env.ENABLE_LOCAL = 'true';
    process.env.ENABLE_CLONE = 'true';
    delete process.env.TOOLS_TO_RUN;
    delete process.env.DISABLE_TOOLS;
  });

  afterEach(() => {
    delete process.env.ENABLE_LOCAL;
    delete process.env.ENABLE_CLONE;
    delete process.env.TOOLS_TO_RUN;
    delete process.env.DISABLE_TOOLS;
    vi.restoreAllMocks();
  });

  async function createFixtureMcp() {
    const { ALL_TOOLS: coreTools } =
      await import('@octocodeai/octocode-tools-core');
    const invocationCount = new Map<string, number>();
    for (const coreTool of coreTools) {
      coreTool.direct.executionFn = async () => {
        const count = (invocationCount.get(coreTool.name) ?? 0) + 1;
        invocationCount.set(coreTool.name, count);
        const testCase = ADAPTER_PARITY_CASES.find(
          item => item.name === coreTool.name
        )!;
        if (count === 1) return createAdapterParityPageResult(testCase, 1);
        if (count === 2) return createAdapterParityPageResult(testCase, 2);
        return createAdapterParityErrorResult(testCase);
      };
    }

    const { ALL_TOOLS: mcpTools } =
      await import('../../src/tools/toolConfig.js');
    const mcp = createMockMcpServer();
    for (const tool of mcpTools) tool.fn(mcp.server);
    return { coreTools, mcpTools, mcp };
  }

  it('registers exactly the canonical ten names and schemas', async () => {
    const { coreTools, mcpTools, mcp } = await createFixtureMcp();
    expect(mcpTools.map(tool => tool.name)).toEqual(
      CANONICAL_ADAPTER_TOOL_NAMES
    );
    expect(mcp.registrations.map(registration => registration.name)).toEqual(
      CANONICAL_ADAPTER_TOOL_NAMES
    );

    for (const registration of mcp.registrations) {
      const coreTool = coreTools.find(tool => tool.name === registration.name)!;
      expect(registration.options).toMatchObject({
        title: coreTool.title,
        description: coreTool.description,
        inputSchema: coreTool.direct.inputSchema,
      });
    }
  });

  it.each(ADAPTER_PARITY_CASES)(
    '$name preserves schema, result, row error, outer error, and executable continuation contracts',
    async (testCase: AdapterParityCase) => {
      const { coreTools, mcp } = await createFixtureMcp();
      const coreTool = coreTools.find(tool => tool.name === testCase.name)!;
      expect(
        coreTool.direct.inputSchema.safeParse(testCase.input).success
      ).toBe(true);

      const first = await mcp.callTool(testCase.name, testCase.input);
      expect(first).toEqual(
        createAdapterParityPageResult(testCase, 1) satisfies CallToolResult
      );
      expect(first.structuredContent).toMatchObject({
        results: [
          { index: 0 },
          {
            index: 1,
            status: 'error',
            data: { error: { code: 'ADAPTER_FIXTURE_ROW_ERROR' } },
          },
        ],
      });

      const next = getAdapterParityContinuation(first);
      const continuationInput = { queries: [next.query] };
      expect(
        coreTool.direct.inputSchema.safeParse(continuationInput).success
      ).toBe(true);
      await expect(mcp.callTool(next.tool, continuationInput)).resolves.toEqual(
        createAdapterParityPageResult(testCase, 2)
      );

      await expect(
        mcp.callTool(testCase.name, testCase.input)
      ).resolves.toEqual(createAdapterParityErrorResult(testCase));
    }
  );
});
