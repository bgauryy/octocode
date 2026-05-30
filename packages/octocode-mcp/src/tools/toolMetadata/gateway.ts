import type { ToolNames } from '@octocodeai/octocode-core/types';
import { DESCRIPTIONS, isToolInMetadata, TOOL_NAMES } from './proxies.js';

export interface ToolMetadataGateway {
  hasTool(toolName: string): boolean;
  getDescription(toolName: string): string;
  getToolName<TKey extends keyof ToolNames>(key: TKey): ToolNames[TKey];
}

export const DEFAULT_TOOL_METADATA_GATEWAY: ToolMetadataGateway = {
  hasTool(toolName: string): boolean {
    return isToolInMetadata(toolName);
  },
  getDescription(toolName: string): string {
    return DESCRIPTIONS[toolName] ?? '';
  },
  getToolName<TKey extends keyof ToolNames>(key: TKey): ToolNames[TKey] {
    const value = TOOL_NAMES[key];
    return (value ?? String(key)) as ToolNames[TKey];
  },
};
