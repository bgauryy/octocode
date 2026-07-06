import { completeMetadata } from '@octocodeai/octocode-core';

export const STATIC_TOOL_NAMES = completeMetadata.toolNames;

// Derived from core — single source of truth, drift-proof.
export const LSP_GET_SEMANTICS_TOOL_NAME =
  STATIC_TOOL_NAMES.LSP_GET_SEMANTIC_CONTENT;

// OQL is currently a tools-core search surface; keep its name in one place
// until it graduates into octocode-core tool metadata.
export const OQL_SEARCH_TOOL_NAME = 'oqlSearch';

export function isOqlEnabled(env: NodeJS.ProcessEnv = process.env): boolean {
  const raw = env.ENABLE_OQL;
  if (raw === undefined) return false;
  return ['1', 'true', 'yes', 'on'].includes(raw.trim().toLowerCase());
}

const LOCAL_TOOL_NAMES_SET = new Set<string>([
  STATIC_TOOL_NAMES.LOCAL_RIPGREP,
  STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
  STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
  STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
  STATIC_TOOL_NAMES.LOCAL_BINARY_INSPECT,
  LSP_GET_SEMANTICS_TOOL_NAME,
]);

export function isLocalTool(toolName: string): boolean {
  return LOCAL_TOOL_NAMES_SET.has(toolName);
}
