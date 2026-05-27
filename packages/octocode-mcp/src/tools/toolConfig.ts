import {
  McpServer,
  RegisteredTool,
} from '@modelcontextprotocol/sdk/server/mcp.js';
import type { ToolNames } from '@octocodeai/octocode-core';
import type { ToolInvocationCallback } from '../types/toolResults.js';
import { registerGitHubSearchCodeTool } from './github_search_code/github_search_code.js';
import { registerFetchGitHubFileContentTool } from './github_fetch_content/github_fetch_content.js';
import { registerSearchGitHubReposTool } from './github_search_repos/github_search_repos.js';
import { registerSearchGitHubPullRequestsTool } from './github_search_pull_requests/github_search_pull_requests.js';
import { registerViewGitHubRepoStructureTool } from './github_view_repo_structure/github_view_repo_structure.js';
import { registerPackageSearchTool } from './package_search/package_search.js';
import { registerGitHubCloneRepoTool } from './github_clone_repo/register.js';
import { registerLocalRipgrepTool } from './local_ripgrep/register.js';
import { registerLocalViewStructureTool } from './local_view_structure/register.js';
import { registerLocalFindFilesTool } from './local_find_files/register.js';
import { registerLocalFetchContentTool } from './local_fetch_content/register.js';
import { registerLSPGotoDefinitionTool } from './lsp_goto_definition/lsp_goto_definition.js';
import { registerLSPFindReferencesTool } from './lsp_find_references/register.js';
import { registerLSPCallHierarchyTool } from './lsp_call_hierarchy/register.js';
import {
  DEFAULT_TOOL_METADATA_GATEWAY,
  type ToolMetadataGateway,
} from './toolMetadata/gateway.js';

export interface ToolConfig {
  name: string;
  description: string;
  isDefault: boolean;
  isLocal: boolean;
  /**
   * When true, the tool requires ENABLE_CLONE (in addition to ENABLE_LOCAL).
   * Used for the clone/fetch repository tool.
   */
  isClone?: boolean;
  type: 'search' | 'content' | 'history' | 'debug';
  /**
   * When true, skip the remote metadata check during registration.
   * Use for new tools not yet published in the metadata API.
   */
  skipMetadataCheck?: boolean;
  fn: (
    server: McpServer,
    callback?: ToolInvocationCallback
  ) => RegisteredTool | Promise<RegisteredTool | null>;
}

/** Exported for branch coverage testing: fallback when tool not in DESCRIPTIONS */
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

function createTool(
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

interface ToolCatalog {
  GITHUB_SEARCH_CODE: ToolConfig;
  GITHUB_FETCH_CONTENT: ToolConfig;
  GITHUB_VIEW_REPO_STRUCTURE: ToolConfig;
  GITHUB_SEARCH_REPOSITORIES: ToolConfig;
  GITHUB_SEARCH_PULL_REQUESTS: ToolConfig;
  PACKAGE_SEARCH: ToolConfig;
  GITHUB_CLONE_REPO: ToolConfig;
  LOCAL_RIPGREP: ToolConfig;
  LOCAL_VIEW_STRUCTURE: ToolConfig;
  LOCAL_FIND_FILES: ToolConfig;
  LOCAL_FETCH_CONTENT: ToolConfig;
  LSP_GOTO_DEFINITION: ToolConfig;
  LSP_FIND_REFERENCES: ToolConfig;
  LSP_CALL_HIERARCHY: ToolConfig;
  ALL_TOOLS: ToolConfig[];
}

function createToolCatalog(
  gateway: ToolMetadataGateway = DEFAULT_TOOL_METADATA_GATEWAY
): ToolCatalog {
  const GITHUB_SEARCH_CODE = createTool(gateway, 'GITHUB_SEARCH_CODE', {
    isDefault: true,
    isLocal: false,
    type: 'search',
    fn: registerGitHubSearchCodeTool,
  });

  const GITHUB_FETCH_CONTENT = createTool(gateway, 'GITHUB_FETCH_CONTENT', {
    isDefault: true,
    isLocal: false,
    type: 'content',
    fn: registerFetchGitHubFileContentTool,
  });

  const GITHUB_VIEW_REPO_STRUCTURE = createTool(
    gateway,
    'GITHUB_VIEW_REPO_STRUCTURE',
    {
      isDefault: true,
      isLocal: false,
      type: 'content',
      fn: registerViewGitHubRepoStructureTool,
    }
  );

  const GITHUB_SEARCH_REPOSITORIES = createTool(
    gateway,
    'GITHUB_SEARCH_REPOSITORIES',
    {
      isDefault: true,
      isLocal: false,
      type: 'search',
      fn: registerSearchGitHubReposTool,
    }
  );

  const GITHUB_SEARCH_PULL_REQUESTS = createTool(
    gateway,
    'GITHUB_SEARCH_PULL_REQUESTS',
    {
      isDefault: true,
      isLocal: false,
      type: 'history',
      fn: registerSearchGitHubPullRequestsTool,
    }
  );

  const PACKAGE_SEARCH = createTool(gateway, 'PACKAGE_SEARCH', {
    isDefault: true,
    isLocal: false,
    type: 'search',
    fn: registerPackageSearchTool,
  });

  const GITHUB_CLONE_REPO = createTool(gateway, 'GITHUB_CLONE_REPO', {
    isDefault: true,
    isLocal: true,
    isClone: true,
    type: 'content',
    skipMetadataCheck: true,
    fn: registerGitHubCloneRepoTool,
  });

  const LOCAL_RIPGREP = createTool(gateway, 'LOCAL_RIPGREP', {
    isDefault: true,
    isLocal: true,
    type: 'search',
    fn: registerLocalRipgrepTool,
  });

  const LOCAL_VIEW_STRUCTURE = createTool(gateway, 'LOCAL_VIEW_STRUCTURE', {
    isDefault: true,
    isLocal: true,
    type: 'content',
    fn: registerLocalViewStructureTool,
  });

  const LOCAL_FIND_FILES = createTool(gateway, 'LOCAL_FIND_FILES', {
    isDefault: true,
    isLocal: true,
    type: 'search',
    fn: registerLocalFindFilesTool,
  });

  const LOCAL_FETCH_CONTENT = createTool(gateway, 'LOCAL_FETCH_CONTENT', {
    isDefault: true,
    isLocal: true,
    type: 'content',
    fn: registerLocalFetchContentTool,
  });

  const LSP_GOTO_DEFINITION = createTool(gateway, 'LSP_GOTO_DEFINITION', {
    isDefault: true,
    isLocal: true,
    type: 'content',
    fn: registerLSPGotoDefinitionTool,
  });

  const LSP_FIND_REFERENCES = createTool(gateway, 'LSP_FIND_REFERENCES', {
    isDefault: true,
    isLocal: true,
    type: 'search',
    fn: registerLSPFindReferencesTool,
  });

  const LSP_CALL_HIERARCHY = createTool(gateway, 'LSP_CALL_HIERARCHY', {
    isDefault: true,
    isLocal: true,
    type: 'content',
    fn: registerLSPCallHierarchyTool,
  });

  const ALL_TOOLS: ToolConfig[] = [
    GITHUB_SEARCH_CODE,
    GITHUB_FETCH_CONTENT,
    GITHUB_VIEW_REPO_STRUCTURE,
    GITHUB_SEARCH_REPOSITORIES,
    GITHUB_SEARCH_PULL_REQUESTS,
    PACKAGE_SEARCH,
    GITHUB_CLONE_REPO,
    LOCAL_RIPGREP,
    LOCAL_VIEW_STRUCTURE,
    LOCAL_FIND_FILES,
    LOCAL_FETCH_CONTENT,
    LSP_GOTO_DEFINITION,
    LSP_FIND_REFERENCES,
    LSP_CALL_HIERARCHY,
  ];

  return {
    GITHUB_SEARCH_CODE,
    GITHUB_FETCH_CONTENT,
    GITHUB_VIEW_REPO_STRUCTURE,
    GITHUB_SEARCH_REPOSITORIES,
    GITHUB_SEARCH_PULL_REQUESTS,
    PACKAGE_SEARCH,
    GITHUB_CLONE_REPO,
    LOCAL_RIPGREP,
    LOCAL_VIEW_STRUCTURE,
    LOCAL_FIND_FILES,
    LOCAL_FETCH_CONTENT,
    LSP_GOTO_DEFINITION,
    LSP_FIND_REFERENCES,
    LSP_CALL_HIERARCHY,
    ALL_TOOLS,
  };
}

const DEFAULT_TOOL_CATALOG = createToolCatalog();

export const GITHUB_SEARCH_CODE = DEFAULT_TOOL_CATALOG.GITHUB_SEARCH_CODE;
export const GITHUB_FETCH_CONTENT = DEFAULT_TOOL_CATALOG.GITHUB_FETCH_CONTENT;
export const GITHUB_VIEW_REPO_STRUCTURE =
  DEFAULT_TOOL_CATALOG.GITHUB_VIEW_REPO_STRUCTURE;
export const GITHUB_SEARCH_REPOSITORIES =
  DEFAULT_TOOL_CATALOG.GITHUB_SEARCH_REPOSITORIES;
export const GITHUB_SEARCH_PULL_REQUESTS =
  DEFAULT_TOOL_CATALOG.GITHUB_SEARCH_PULL_REQUESTS;
export const PACKAGE_SEARCH = DEFAULT_TOOL_CATALOG.PACKAGE_SEARCH;
export const GITHUB_CLONE_REPO = DEFAULT_TOOL_CATALOG.GITHUB_CLONE_REPO;
export const LOCAL_RIPGREP = DEFAULT_TOOL_CATALOG.LOCAL_RIPGREP;
export const LOCAL_VIEW_STRUCTURE = DEFAULT_TOOL_CATALOG.LOCAL_VIEW_STRUCTURE;
export const LOCAL_FIND_FILES = DEFAULT_TOOL_CATALOG.LOCAL_FIND_FILES;
export const LOCAL_FETCH_CONTENT = DEFAULT_TOOL_CATALOG.LOCAL_FETCH_CONTENT;
export const ALL_TOOLS = DEFAULT_TOOL_CATALOG.ALL_TOOLS;
