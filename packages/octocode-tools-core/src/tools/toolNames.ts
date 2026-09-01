import { toolNames } from '@octocodeai/octocode-core/schemas';
import { parseBooleanEnv } from '@octocodeai/config';

const {
  GITHUB_PULL_REQUESTS: _retiredPullRequests,
  GITHUB_ISSUES: _retiredIssues,
  GITHUB_COMMITS: _retiredCommits,
  ...retainedToolNames
} = toolNames;

export const LOCAL_ANALYZE_GRAPH_TOOL_NAME = toolNames.LOCAL_ANALYZE_GRAPH;
export const LOCAL_SEARCH_TOOL_NAME = 'localSearch';
export const GITHUB_SEARCH_TOOL_NAME = toolNames.GITHUB_SEARCH;
export const GITHUB_SEARCH_HISTORY_TOOL_NAME = 'ghSearchHistory';
export const GITHUB_GET_HISTORY_ITEM_TOOL_NAME = 'ghGetHistoryItem';

export const STATIC_TOOL_NAMES = {
  ...retainedToolNames,
  GITHUB_SEARCH_HISTORY: GITHUB_SEARCH_HISTORY_TOOL_NAME,
  GITHUB_GET_HISTORY_ITEM: GITHUB_GET_HISTORY_ITEM_TOOL_NAME,
} as const;

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
  LOCAL_SEARCH_TOOL_NAME,
  STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
  LOCAL_ANALYZE_GRAPH_TOOL_NAME,
  LSP_GET_SEMANTICS_TOOL_NAME,
]);

export function isLocalTool(toolName: string): boolean {
  return LOCAL_TOOL_NAMES_SET.has(toolName);
}
