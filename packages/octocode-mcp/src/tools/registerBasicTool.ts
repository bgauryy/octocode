import {
  McpServer,
  RegisteredTool,
} from '@modelcontextprotocol/sdk/server/mcp.js';
import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { toMCPSchema } from '../types/toolTypes.js';
import {
  DESCRIPTIONS,
  withBasicSecurityValidation,
} from '@octocodeai/octocode-tools-core';

interface BasicToolConfig<TInput extends object> {
  name: string;
  title: string;
  inputSchema: object;
  executionFn: (args: TInput) => Promise<CallToolResult>;
  annotations?: {
    readOnlyHint?: boolean;
    destructiveHint?: boolean;
    idempotentHint?: boolean;
    openWorldHint?: boolean;
  };
}

export function createBasicToolRegistration<TInput extends object>({
  name,
  title,
  inputSchema,
  executionFn,
  annotations,
}: BasicToolConfig<TInput>): (server: McpServer) => RegisteredTool {
  return (server: McpServer) =>
    server.registerTool(
      name,
      {
        description: DESCRIPTIONS[name],
        inputSchema: toMCPSchema(inputSchema),
        annotations: {
          title,
          readOnlyHint: annotations?.readOnlyHint ?? true,
          destructiveHint: annotations?.destructiveHint ?? false,
          idempotentHint: annotations?.idempotentHint ?? true,
          openWorldHint: annotations?.openWorldHint ?? false,
        },
      },
      withBasicSecurityValidation(executionFn, name)
    );
}
