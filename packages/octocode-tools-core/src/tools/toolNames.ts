import { toolNames } from '../toolContract/resources/global.js';
export { LOCAL_ANALYZE_GRAPH_TOOL_NAME } from '../toolContract/resources/tools/localAnalyzeGraph.js';
import { LOCAL_ANALYZE_GRAPH_TOOL_NAME } from '../toolContract/resources/tools/localAnalyzeGraph.js';

export const STATIC_TOOL_NAMES = toolNames;

// Derived from the repository-owned contract — single source of truth.
export const LSP_GET_SEMANTICS_TOOL_NAME =
  STATIC_TOOL_NAMES.LSP_GET_SEMANTIC_CONTENT;

// ghListReleases is a niche surface (release history) — opt-in only, so the
// default toolset stays lean. Enable with ENABLE_RELEASES=1.
export function isReleasesEnabled(
  env: NodeJS.ProcessEnv = process.env
): boolean {
  const raw = env.ENABLE_RELEASES;
  if (raw === undefined) return false;
  return ['1', 'true', 'yes', 'on'].includes(raw.trim().toLowerCase());
}

// ghSearchDiscussions is GraphQL-only (GitHub Discussions have no REST list
// endpoint) and a niche surface — opt-in only. Enable with ENABLE_DISCUSSIONS=1.
export function isDiscussionsEnabled(
  env: NodeJS.ProcessEnv = process.env
): boolean {
  const raw = env.ENABLE_DISCUSSIONS;
  if (raw === undefined) return false;
  return ['1', 'true', 'yes', 'on'].includes(raw.trim().toLowerCase());
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
