import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { ToolNames } from '@octocodeai/octocode-core/types';
import { type z } from 'zod';
import {
  DEFAULT_TOOL_METADATA_GATEWAY,
  type ToolMetadataGateway,
} from './toolMetadata/gateway.js';

export type ToolDirectSecurity = 'basic' | 'remote';

export interface ToolDirectExecutionConfig {
  schema: z.ZodType;

  inputSchema: z.ZodType;
  executionFn: (input: never) => Promise<CallToolResult>;
  security: ToolDirectSecurity;
  requiresServerRuntime?: boolean;
  requiresProviders?: boolean;
}

export interface ToolConfig {
  name: string;
  description: string;
  isDefault: boolean;
  isLocal: boolean;

  isClone?: boolean;
  type: 'search' | 'content' | 'history' | 'debug';

  skipMetadataCheck?: boolean;
  direct: ToolDirectExecutionConfig;
}

export const getDescription = (
  toolName: string,
  gateway: Pick<
    ToolMetadataGateway,
    'getDescription'
  > = DEFAULT_TOOL_METADATA_GATEWAY
): string => {
  return gateway.getDescription(toolName);
};

function getToolName<TKey extends keyof ToolNames>(
  key: TKey,
  gateway: Pick<ToolMetadataGateway, 'getToolName'>
): ToolNames[TKey] {
  return gateway.getToolName(key);
}

export function createTool(
  gateway: ToolMetadataGateway,
  nameKey: keyof ToolNames,
  config: Omit<ToolConfig, 'name' | 'description'>
): ToolConfig {
  const name = getToolName(nameKey, gateway);
  return {
    ...config,
    name,
    description: getDescription(name, gateway),
  };
}
