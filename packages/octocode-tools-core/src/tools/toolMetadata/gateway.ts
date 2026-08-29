import { DESCRIPTIONS, isToolInMetadata, TOOL_NAMES } from './proxies.js';

export interface ToolMetadataGateway {
  hasTool(toolName: string): boolean;
  getDescription(toolName: string): string;
  getToolName(key: string): string;
}

export const DEFAULT_TOOL_METADATA_GATEWAY: ToolMetadataGateway = {
  hasTool(toolName: string): boolean {
    return isToolInMetadata(toolName);
  },
  getDescription(toolName: string): string {
    return DESCRIPTIONS[toolName] ?? '';
  },
  getToolName(key: string): string {
    const value = TOOL_NAMES[key as keyof typeof TOOL_NAMES];
    return value ?? key;
  },
};
