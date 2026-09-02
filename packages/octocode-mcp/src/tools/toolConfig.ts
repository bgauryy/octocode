import {
  McpServer,
  RegisteredTool,
  type CallToolResult,
} from '@modelcontextprotocol/server';
import type {
  ToolConfig,
  ToolExecutionArgs,
  ToolInvocationCallback,
} from '@octocodeai/octocode-tools-core';
import {
  ALL_TOOLS as CORE_ALL_TOOLS,
  STATIC_TOOL_NAMES,
} from '@octocodeai/octocode-tools-core';

import { createToolRegistration } from './registerTool.js';

export type {
  ToolConfig,
  ToolDirectExecutionConfig,
  ToolDirectSecurity,
} from '@octocodeai/octocode-tools-core';
export interface McpToolConfig extends ToolConfig {
  fn: (server: McpServer, callback?: ToolInvocationCallback) => RegisteredTool;
}

function createCoreToolRegistration(tool: ToolConfig): McpToolConfig['fn'] {
  const mayMaterializeLocalCache =
    tool.isClone || tool.name === STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT;
  const common = {
    name: tool.name,
    title: tool.title,
    description: tool.description,
    inputSchema: tool.direct.inputSchema,
    executionFn: tool.direct.executionFn as unknown as (
      args: ToolExecutionArgs<unknown>
    ) => Promise<CallToolResult>,
    annotations: mayMaterializeLocalCache ? { readOnlyHint: false } : undefined,
    security: tool.direct.security,
    timeoutMs: tool.direct.timeoutMs,
  };
  return createToolRegistration(common);
}

export const ALL_TOOLS: McpToolConfig[] = CORE_ALL_TOOLS.map(tool => {
  return { ...tool, fn: createCoreToolRegistration(tool) };
});
