import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import type { ToolInvocationCallback } from '../types/toolResults.js';
import type { ToolConfig } from './toolConfig.js';

type ToolRegistrationOutcome =
  | { status: 'success' }
  | { status: 'failed'; toolName: string }
  | { status: 'skipped' };

export async function registerSingleTool(
  tool: ToolConfig,
  server: McpServer,
  callback: ToolInvocationCallback | undefined,
  metadataValidator: (tool: ToolConfig) => boolean
): Promise<ToolRegistrationOutcome> {
  if (!metadataValidator(tool)) {
    return { status: 'skipped' };
  }

  try {
    const result = await tool.fn(server, callback);
    return result !== null ? { status: 'success' } : { status: 'skipped' };
  } catch {
    return { status: 'failed', toolName: tool.name };
  }
}

export async function registerToolsBatch(
  tools: ToolConfig[],
  server: McpServer,
  callback: ToolInvocationCallback | undefined,
  metadataValidator: (tool: ToolConfig) => boolean
): Promise<ToolRegistrationOutcome[]> {
  return Promise.all(
    tools.map(tool =>
      registerSingleTool(tool, server, callback, metadataValidator)
    )
  );
}

export function summarizeOutcomes(outcomes: ToolRegistrationOutcome[]): {
  successCount: number;
  failedTools: string[];
} {
  const successCount = outcomes.filter(
    outcome => outcome.status === 'success'
  ).length;

  const failedTools = outcomes
    .filter(
      (outcome): outcome is { status: 'failed'; toolName: string } =>
        outcome.status === 'failed'
    )
    .map(outcome => outcome.toolName);

  return { successCount, failedTools };
}
