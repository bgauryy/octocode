import { completeMetadata } from '@octocodeai/octocode-core';
import { LSP_GET_SEMANTIC_CONTENT_TOOL_NAME } from './lsp/shared/semanticTypes.js';

export const STATIC_TOOL_NAMES = completeMetadata.toolNames;

const LOCAL_TOOL_NAMES_SET = new Set<string>([
  STATIC_TOOL_NAMES.LOCAL_RIPGREP,
  STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
  STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
  STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
  STATIC_TOOL_NAMES.LOCAL_BINARY_INSPECT,
  LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
]);

export function isLocalTool(toolName: string): boolean {
  return LOCAL_TOOL_NAMES_SET.has(toolName);
}
