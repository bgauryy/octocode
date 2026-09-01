import { McpServer } from '@modelcontextprotocol/server';
import type { ToolInvocationCallback } from '@octocodeai/octocode-tools-core';
import type { McpToolConfig } from './toolConfig.js';

type ToolRegistrationOutcome =
  { status: 'success' } | { status: 'failed'; toolName: string; error: string };

export async function registerSingleTool(
  tool: McpToolConfig,
  server: McpServer,
  callback: ToolInvocationCallback | undefined
): Promise<ToolRegistrationOutcome> {
  try {
    await tool.fn(server, callback);
    return { status: 'success' };
  } catch (error) {
    return {
      status: 'failed',
      toolName: tool.name,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

export async function registerToolsBatch(
  tools: McpToolConfig[],
  server: McpServer,
  callback: ToolInvocationCallback | undefined
): Promise<ToolRegistrationOutcome[]> {
  return Promise.all(
    tools.map(tool => registerSingleTool(tool, server, callback))
  );
}

export function summarizeOutcomes(outcomes: ToolRegistrationOutcome[]): {
  successCount: number;
  failedTools: string[];
  failedToolErrors?: Record<string, string>;
} {
  const successCount = outcomes.filter(
    outcome => outcome.status === 'success'
  ).length;

  const failures = outcomes.filter(
    (
      outcome
    ): outcome is { status: 'failed'; toolName: string; error: string } =>
      outcome.status === 'failed'
  );
  const failedTools = failures.map(outcome => outcome.toolName);
  const failedToolErrors = Object.fromEntries(
    failures.map(outcome => [outcome.toolName, outcome.error])
  );

  return failedTools.length > 0
    ? { successCount, failedTools, failedToolErrors }
    : { successCount, failedTools };
}
