import type {
  AuthInfo,
  CallToolResult,
  McpServer,
  RegisteredTool,
} from '@modelcontextprotocol/server';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  ALL_TOOLS as CORE_ALL_TOOLS,
  GITHUB_SEARCH_TOOL_NAME,
  STATIC_TOOL_NAMES,
} from '@octocodeai/octocode-tools-core';
import type { ToolExecutionArgs } from '@octocodeai/octocode-tools-core';
import { createMockMcpServer } from '../fixtures/mcp-fixtures.js';
import {
  registerToolsBatch,
  summarizeOutcomes,
} from '../../src/tools/registrationExecutor.js';
import type { McpToolConfig } from '../../src/tools/toolConfig.js';

describe('core-driven MCP tool registration', () => {
  afterEach(() => {
    vi.resetModules();
  });

  it('registers ghSearch with remote annotations and forwards callback plus request context', async () => {
    const coreTool = CORE_ALL_TOOLS.find(
      tool => tool.name === GITHUB_SEARCH_TOOL_NAME
    );
    expect(coreTool).toBeDefined();

    const originalExecutionFn = coreTool!.direct.executionFn;
    let receivedArgs: ToolExecutionArgs<unknown> | undefined;
    coreTool!.direct.executionFn = async args => {
      receivedArgs = args;
      return {
        content: [{ type: 'text', text: 'ok' }],
        structuredContent: { results: [] },
      } satisfies CallToolResult;
    };

    try {
      const { ALL_TOOLS } = await import('../../src/tools/toolConfig.js');
      const githubSearch = ALL_TOOLS.find(
        tool => tool.name === GITHUB_SEARCH_TOOL_NAME
      );
      expect(githubSearch).toBeDefined();

      const mcp = createMockMcpServer();
      const callback = vi.fn();
      githubSearch!.fn(mcp.server, callback);

      expect(mcp.registrations[0]).toMatchObject({
        name: GITHUB_SEARCH_TOOL_NAME,
        options: {
          title: coreTool!.title,
          description: coreTool!.description,
          annotations: {
            openWorldHint: true,
          },
        },
      });

      const authInfo = { token: 'test-token' } as AuthInfo;
      const controller = new AbortController();
      const queries = [{ operation: 'code', keywords: ['needle'] }];
      await mcp.callTool(
        GITHUB_SEARCH_TOOL_NAME,
        { queries, responseCharOffset: 7, responseCharLength: 101 },
        { authInfo, sessionId: 'session-123', signal: controller.signal }
      );

      expect(callback).toHaveBeenCalledWith(GITHUB_SEARCH_TOOL_NAME, queries);
      expect(receivedArgs).toMatchObject({
        queries,
        responseCharOffset: 7,
        responseCharLength: 101,
        authInfo,
        sessionId: 'session-123',
      });
      expect(receivedArgs?.signal).toBeInstanceOf(AbortSignal);
      expect(receivedArgs?.signal?.aborted).toBe(false);
      controller.abort();
      expect(receivedArgs?.signal?.aborted).toBe(true);
    } finally {
      coreTool!.direct.executionFn = originalExecutionFn;
    }
  });

  it('invokes callbacks and forwards pagination plus cancellation for basic tools', async () => {
    vi.resetModules();
    const { ALL_TOOLS: liveCoreTools } =
      await import('@octocodeai/octocode-tools-core');
    const coreTool = liveCoreTools.find(
      tool => tool.direct.security === 'basic'
    );
    expect(coreTool).toBeDefined();
    const originalExecutionFn = coreTool!.direct.executionFn;
    let receivedArgs: ToolExecutionArgs<unknown> | undefined;
    coreTool!.direct.executionFn = async args => {
      receivedArgs = args;
      return {
        content: [{ type: 'text', text: 'ok' }],
        structuredContent: { results: [] },
      } satisfies CallToolResult;
    };

    try {
      const { ALL_TOOLS } = await import('../../src/tools/toolConfig.js');
      const basicTool = ALL_TOOLS.find(tool => tool.name === coreTool!.name)!;
      const mcp = createMockMcpServer();
      const callback = vi.fn();
      basicTool.fn(mcp.server, callback);
      const controller = new AbortController();
      const queries = [{}];

      await mcp.callTool(
        basicTool.name,
        { queries, responseCharOffset: 3, responseCharLength: 50 },
        { signal: controller.signal }
      );

      expect(callback).toHaveBeenCalledWith(basicTool.name, queries);
      expect(receivedArgs).toMatchObject({
        queries,
        responseCharOffset: 3,
        responseCharLength: 50,
      });
      expect(receivedArgs?.signal).toBeInstanceOf(AbortSignal);
      expect(receivedArgs?.signal?.aborted).toBe(false);
      controller.abort();
      expect(receivedArgs?.signal?.aborted).toBe(true);
    } finally {
      coreTool!.direct.executionFn = originalExecutionFn;
    }
  });

  it.each([
    STATIC_TOOL_NAMES.GITHUB_CLONE_REPO,
    STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
  ])('marks cache-materializing %s as not read-only', async toolName => {
    vi.resetModules();
    const { ALL_TOOLS } = await import('../../src/tools/toolConfig.js');
    const tool = ALL_TOOLS.find(item => item.name === toolName);
    expect(tool).toBeDefined();

    const mcp = createMockMcpServer();
    tool!.fn(mcp.server);

    expect(mcp.registrations[0]?.options.annotations).toMatchObject({
      readOnlyHint: false,
    });
  });

  it('batches isolated registration failures and summarizes their messages', async () => {
    const template = CORE_ALL_TOOLS[0]!;
    const ok = {
      ...template,
      name: 'ok',
      fn: () => ({}) as RegisteredTool,
    } as McpToolConfig;
    const failed = {
      ...template,
      name: 'failed',
      fn: () => {
        throw 'registration failed';
      },
    } as McpToolConfig;

    const outcomes = await registerToolsBatch(
      [ok, failed],
      {} as McpServer,
      undefined
    );

    expect(summarizeOutcomes(outcomes)).toEqual({
      successCount: 1,
      failedTools: ['failed'],
      failedToolErrors: { failed: 'registration failed' },
    });
  });
});
