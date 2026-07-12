import {
  McpServer,
  RegisteredTool,
} from '@modelcontextprotocol/sdk/server/mcp.js';
import type {
  ToolConfig,
  ToolInvocationCallback,
  ToolMetadataGateway,
} from '@octocodeai/octocode-tools-core';
import {
  ALL_TOOLS as CORE_ALL_TOOLS,
  OQL_SEARCH as CORE_OQL_SEARCH,
  STATIC_TOOL_NAMES,
  LSP_GET_SEMANTICS_TOOL_NAME,
  OQL_SEARCH_TOOL_NAME,
  DEFAULT_TOOL_METADATA_GATEWAY,
  getDescription,
} from '@octocodeai/octocode-tools-core';

import { registerGitHubSearchCodeTool } from './github_search_code/github_search_code.js';
import { registerFetchGitHubFileContentTool } from './github_fetch_content/github_fetch_content.js';
import { registerViewGitHubRepoStructureTool } from './github_view_repo_structure/github_view_repo_structure.js';
import { registerSearchGitHubReposTool } from './github_search_repos/github_search_repos.js';
import { registerSearchGitHubPullRequestsTool } from './github_search_pull_requests/github_search_pull_requests.js';
import { registerNpmSearchTool } from './package_search/package_search.js';
import { registerGitHubCloneRepoTool } from './github_clone_repo/github_clone_repo.js';
import { registerLocalRipgrepTool } from './local_ripgrep/register.js';
import { registerLocalViewStructureTool } from './local_view_structure/register.js';
import { registerLocalFindFilesTool } from './local_find_files/register.js';
import { registerLocalFetchContentTool } from './local_fetch_content/register.js';
import { registerLspGetSemanticsTool } from './lsp/semantic_content/register.js';
import { registerOqlSearchTool } from './oql_search/register.js';

export type {
  ToolConfig,
  ToolDirectExecutionConfig,
  ToolDirectSecurity,
} from '@octocodeai/octocode-tools-core';
export { getDescription, DEFAULT_TOOL_METADATA_GATEWAY };
export type { ToolMetadataGateway };

export interface McpToolConfig extends ToolConfig {
  fn: (
    server: McpServer,
    callback?: ToolInvocationCallback
  ) => RegisteredTool | Promise<RegisteredTool | null>;
}

const MCP_FN_MAP: Record<string, McpToolConfig['fn']> = {
  [STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE]: registerGitHubSearchCodeTool,
  [STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT]: registerFetchGitHubFileContentTool,
  [STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE]:
    registerViewGitHubRepoStructureTool,
  [STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES]: registerSearchGitHubReposTool,
  [STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS]:
    registerSearchGitHubPullRequestsTool,
  [STATIC_TOOL_NAMES.PACKAGE_SEARCH]: registerNpmSearchTool,
  [STATIC_TOOL_NAMES.GITHUB_CLONE_REPO]: registerGitHubCloneRepoTool,
  [STATIC_TOOL_NAMES.LOCAL_RIPGREP]: registerLocalRipgrepTool,
  [STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE]: registerLocalViewStructureTool,
  [STATIC_TOOL_NAMES.LOCAL_FIND_FILES]: registerLocalFindFilesTool,
  [STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT]: registerLocalFetchContentTool,
  [LSP_GET_SEMANTICS_TOOL_NAME]: registerLspGetSemanticsTool,
  [OQL_SEARCH_TOOL_NAME]: registerOqlSearchTool,
};

export const ALL_TOOLS: McpToolConfig[] = CORE_ALL_TOOLS.map(tool => {
  const fn = MCP_FN_MAP[tool.name];
  if (!fn) {
    throw new Error(`No MCP fn registered for tool: ${tool.name}`);
  }
  return { ...tool, fn };
});

function requireTool(name: string): McpToolConfig {
  const tool = ALL_TOOLS.find(t => t.name === name);
  if (!tool) throw new Error(`Tool not found in MCP registry: "${name}"`);
  return tool;
}

export const GITHUB_SEARCH_CODE = requireTool(
  STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE
);
export const GITHUB_FETCH_CONTENT = requireTool(
  STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT
);
export const GITHUB_VIEW_REPO_STRUCTURE = requireTool(
  STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE
);
export const GITHUB_SEARCH_REPOSITORIES = requireTool(
  STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
);
export const GITHUB_SEARCH_PULL_REQUESTS = requireTool(
  STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS
);
export const PACKAGE_SEARCH = requireTool(STATIC_TOOL_NAMES.PACKAGE_SEARCH);
export const GITHUB_CLONE_REPO = requireTool(
  STATIC_TOOL_NAMES.GITHUB_CLONE_REPO
);
export const LOCAL_RIPGREP = requireTool(STATIC_TOOL_NAMES.LOCAL_RIPGREP);
export const LOCAL_VIEW_STRUCTURE = requireTool(
  STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE
);
export const LOCAL_FIND_FILES = requireTool(STATIC_TOOL_NAMES.LOCAL_FIND_FILES);
export const LOCAL_FETCH_CONTENT = requireTool(
  STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT
);
export const LSP_GET_SEMANTIC_CONTENT = requireTool(
  LSP_GET_SEMANTICS_TOOL_NAME
);
export const OQL_SEARCH: McpToolConfig = {
  ...CORE_OQL_SEARCH,
  fn: registerOqlSearchTool,
};
