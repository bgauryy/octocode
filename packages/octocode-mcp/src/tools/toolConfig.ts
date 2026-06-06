import {
  McpServer,
  RegisteredTool,
} from '@modelcontextprotocol/sdk/server/mcp.js';
import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { ToolNames } from '@octocodeai/octocode-core/types';
import { type z } from 'zod';
import type { ToolInvocationCallback } from '../types/toolResults.js';
import {
  CloneRepoQueryLocalSchema,
  BulkCloneRepoLocalSchema,
  FileContentQueryLocalSchema,
  FileContentBulkQueryLocalSchema,
  GitHubCodeSearchQueryLocalSchema,
  GitHubCodeSearchBulkQueryLocalSchema,
  GitHubPullRequestSearchQueryLocalSchema,
  GitHubPullRequestSearchBulkQueryLocalSchema,
  GitHubReposSearchSingleQueryLocalSchema,
  GitHubReposSearchBulkQueryLocalSchema,
  GitHubViewRepoStructureQueryLocalSchema,
  GitHubViewRepoStructureBulkQueryLocalSchema,
  PackageSearchQueryLocalSchema,
  PackageSearchBulkQueryLocalSchema,
} from '../scheme/remoteSchemaOverlay.js';
import {
  FetchContentQuerySchema,
  BulkFetchContentQuerySchema,
  FindFilesQuerySchema,
  BulkFindFilesSchema,
  RipgrepQuerySchema,
  BulkRipgrepQuerySchema,
  ViewStructureQuerySchema,
  BulkViewStructureSchema,
} from '../scheme/localSchemaOverlay.js';
import {
  LSPCallHierarchyQuerySchema,
  BulkLSPCallHierarchyQuerySchema,
  LSPFindReferencesQuerySchema,
  BulkLSPFindReferencesQuerySchema,
  LSPGotoDefinitionQuerySchema,
  BulkLSPGotoDefinitionQuerySchema,
} from '../scheme/lspSchemaOverlay.js';
import { executeCloneRepo } from './github_clone_repo/execution.js';
import { registerGitHubSearchCodeTool } from './github_search_code/github_search_code.js';
import { fetchMultipleGitHubFileContents } from './github_fetch_content/execution.js';
import { registerFetchGitHubFileContentTool } from './github_fetch_content/github_fetch_content.js';
import { searchMultipleGitHubCode } from './github_search_code/execution.js';
import { searchMultipleGitHubPullRequests } from './github_search_pull_requests/execution.js';
import { registerSearchGitHubReposTool } from './github_search_repos/github_search_repos.js';
import { registerSearchGitHubPullRequestsTool } from './github_search_pull_requests/github_search_pull_requests.js';
import { searchMultipleGitHubRepos } from './github_search_repos/execution.js';
import { exploreMultipleRepositoryStructures } from './github_view_repo_structure/execution.js';
import { registerViewGitHubRepoStructureTool } from './github_view_repo_structure/github_view_repo_structure.js';
import { searchPackages } from './package_search/execution.js';
import { registerPackageSearchTool } from './package_search/package_search.js';
import { registerGitHubCloneRepoTool } from './github_clone_repo/register.js';
import { executeFetchContent } from './local_fetch_content/execution.js';
import { executeFindFiles } from './local_find_files/execution.js';
import { executeRipgrepSearch } from './local_ripgrep/execution.js';
import { executeViewStructure } from './local_view_structure/execution.js';
import { registerLocalRipgrepTool } from './local_ripgrep/register.js';
import { registerLocalViewStructureTool } from './local_view_structure/register.js';
import { registerLocalFindFilesTool } from './local_find_files/register.js';
import { registerLocalFetchContentTool } from './local_fetch_content/register.js';
import { executeCallHierarchy } from './lsp_call_hierarchy/execution.js';
import { executeFindReferences } from './lsp_find_references/execution.js';
import { executeGotoDefinition } from './lsp_goto_definition/execution.js';
import { registerLSPGotoDefinitionTool } from './lsp_goto_definition/lsp_goto_definition.js';
import { registerLSPFindReferencesTool } from './lsp_find_references/register.js';
import { registerLSPCallHierarchyTool } from './lsp_call_hierarchy/register.js';
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
  fn: (
    server: McpServer,
    callback?: ToolInvocationCallback
  ) => RegisteredTool | Promise<RegisteredTool | null>;
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
    direct: {
      schema: GitHubCodeSearchQueryLocalSchema,
      inputSchema: GitHubCodeSearchBulkQueryLocalSchema,
      executionFn: searchMultipleGitHubCode,
      security: 'remote',
      requiresServerRuntime: true,
      requiresProviders: true,
    },
  });

  const GITHUB_FETCH_CONTENT = createTool(gateway, 'GITHUB_FETCH_CONTENT', {
    isDefault: true,
    isLocal: false,
    type: 'content',
    fn: registerFetchGitHubFileContentTool,
    direct: {
      schema: FileContentQueryLocalSchema,
      inputSchema: FileContentBulkQueryLocalSchema,
      executionFn: fetchMultipleGitHubFileContents,
      security: 'remote',
      requiresServerRuntime: true,
      requiresProviders: true,
    },
  });

  const GITHUB_VIEW_REPO_STRUCTURE = createTool(
    gateway,
    'GITHUB_VIEW_REPO_STRUCTURE',
    {
      isDefault: true,
      isLocal: false,
      type: 'content',
      fn: registerViewGitHubRepoStructureTool,
      direct: {
        schema: GitHubViewRepoStructureQueryLocalSchema,
        inputSchema: GitHubViewRepoStructureBulkQueryLocalSchema,
        executionFn: exploreMultipleRepositoryStructures,
        security: 'remote',
        requiresServerRuntime: true,
        requiresProviders: true,
      },
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
      direct: {
        schema: GitHubReposSearchSingleQueryLocalSchema,
        inputSchema: GitHubReposSearchBulkQueryLocalSchema,
        executionFn: searchMultipleGitHubRepos,
        security: 'remote',
        requiresServerRuntime: true,
        requiresProviders: true,
      },
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
      direct: {
        schema: GitHubPullRequestSearchQueryLocalSchema,
        inputSchema: GitHubPullRequestSearchBulkQueryLocalSchema,
        executionFn: searchMultipleGitHubPullRequests,
        security: 'remote',
        requiresServerRuntime: true,
        requiresProviders: true,
      },
    }
  );

  const PACKAGE_SEARCH = createTool(gateway, 'PACKAGE_SEARCH', {
    isDefault: true,
    isLocal: false,
    type: 'search',
    fn: registerPackageSearchTool,
    direct: {
      schema: PackageSearchQueryLocalSchema,
      inputSchema: PackageSearchBulkQueryLocalSchema,
      executionFn: searchPackages,
      security: 'remote',
      requiresServerRuntime: true,
    },
  });

  const GITHUB_CLONE_REPO = createTool(gateway, 'GITHUB_CLONE_REPO', {
    isDefault: true,
    isLocal: true,
    isClone: true,
    type: 'content',
    skipMetadataCheck: true,
    fn: registerGitHubCloneRepoTool,
    direct: {
      schema: CloneRepoQueryLocalSchema,
      inputSchema: BulkCloneRepoLocalSchema,
      executionFn: executeCloneRepo,
      security: 'remote',
      requiresServerRuntime: true,
      requiresProviders: true,
    },
  });

  const LOCAL_RIPGREP = createTool(gateway, 'LOCAL_RIPGREP', {
    isDefault: true,
    isLocal: true,
    type: 'search',
    fn: registerLocalRipgrepTool,
    direct: {
      schema: RipgrepQuerySchema,
      inputSchema: BulkRipgrepQuerySchema,
      executionFn: executeRipgrepSearch,
      security: 'basic',
    },
  });

  const LOCAL_VIEW_STRUCTURE = createTool(gateway, 'LOCAL_VIEW_STRUCTURE', {
    isDefault: true,
    isLocal: true,
    type: 'content',
    fn: registerLocalViewStructureTool,
    direct: {
      schema: ViewStructureQuerySchema,
      inputSchema: BulkViewStructureSchema,
      executionFn: executeViewStructure,
      security: 'basic',
    },
  });

  const LOCAL_FIND_FILES = createTool(gateway, 'LOCAL_FIND_FILES', {
    isDefault: true,
    isLocal: true,
    type: 'search',
    fn: registerLocalFindFilesTool,
    direct: {
      schema: FindFilesQuerySchema,
      inputSchema: BulkFindFilesSchema,
      executionFn: executeFindFiles,
      security: 'basic',
    },
  });

  const LOCAL_FETCH_CONTENT = createTool(gateway, 'LOCAL_FETCH_CONTENT', {
    isDefault: true,
    isLocal: true,
    type: 'content',
    fn: registerLocalFetchContentTool,
    direct: {
      schema: FetchContentQuerySchema,
      inputSchema: BulkFetchContentQuerySchema,
      executionFn: executeFetchContent,
      security: 'basic',
    },
  });

  const LSP_GOTO_DEFINITION = createTool(gateway, 'LSP_GOTO_DEFINITION', {
    isDefault: true,
    isLocal: true,
    type: 'content',
    fn: registerLSPGotoDefinitionTool,
    direct: {
      schema: LSPGotoDefinitionQuerySchema,
      inputSchema: BulkLSPGotoDefinitionQuerySchema,
      executionFn: executeGotoDefinition,
      security: 'basic',
      requiresServerRuntime: true,
    },
  });

  const LSP_FIND_REFERENCES = createTool(gateway, 'LSP_FIND_REFERENCES', {
    isDefault: true,
    isLocal: true,
    type: 'search',
    fn: registerLSPFindReferencesTool,
    direct: {
      schema: LSPFindReferencesQuerySchema,
      inputSchema: BulkLSPFindReferencesQuerySchema,
      executionFn: executeFindReferences,
      security: 'basic',
      requiresServerRuntime: true,
    },
  });

  const LSP_CALL_HIERARCHY = createTool(gateway, 'LSP_CALL_HIERARCHY', {
    isDefault: true,
    isLocal: true,
    type: 'content',
    fn: registerLSPCallHierarchyTool,
    direct: {
      schema: LSPCallHierarchyQuerySchema,
      inputSchema: BulkLSPCallHierarchyQuerySchema,
      executionFn: executeCallHierarchy,
      security: 'basic',
      requiresServerRuntime: true,
    },
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
