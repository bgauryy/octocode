import { toolNames } from '@octocodeai/octocode-core/schemas';
import { parseBooleanEnv } from '@octocodeai/config';

export const LOCAL_ANALYZE_GRAPH_TOOL_NAME = toolNames.LOCAL_ANALYZE_GRAPH;

export const STATIC_TOOL_NAMES = toolNames;

// Derived from the shared core contract — single source of truth.
export const LSP_GET_SEMANTICS_TOOL_NAME =
  STATIC_TOOL_NAMES.LSP_GET_SEMANTIC_CONTENT;

/** Whether the opt-in GitHub releases tool is enabled. */
export function isReleasesEnabled(
  env: NodeJS.ProcessEnv = process.env
): boolean {
  return parseBooleanEnv(env.ENABLE_RELEASES) ?? false;
}

/** Whether the opt-in GitHub Discussions tool is enabled. */
export function isDiscussionsEnabled(
  env: NodeJS.ProcessEnv = process.env
): boolean {
  return parseBooleanEnv(env.ENABLE_DISCUSSIONS) ?? false;
}

const LOCAL_TOOL_NAMES_SET = new Set<string>([
  STATIC_TOOL_NAMES.LOCAL_RIPGREP,
  STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
  STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
  STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
  LOCAL_ANALYZE_GRAPH_TOOL_NAME,
  LSP_GET_SEMANTICS_TOOL_NAME,
]);

export function isLocalTool(toolName: string): boolean {
  return LOCAL_TOOL_NAMES_SET.has(toolName);
}
