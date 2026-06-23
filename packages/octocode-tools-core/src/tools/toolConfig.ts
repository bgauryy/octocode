import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { ToolNames } from '@octocodeai/octocode-core/types';
import { type z } from 'zod';
import {
  CloneRepoQueryLocalSchema,
  BulkCloneRepoLocalSchema,
} from './github_clone_repo/scheme.js';
import {
  FileContentQueryLocalSchema,
  FileContentBulkQueryLocalSchema,
} from './github_fetch_content/scheme.js';
import {
  GitHubCodeSearchQueryLocalSchema,
  GitHubCodeSearchBulkQueryLocalSchema,
} from './github_search_code/scheme.js';
import {
  GitHubPullRequestSearchQueryLocalSchema,
  GitHubPullRequestSearchBulkQueryLocalSchema,
} from './github_search_pull_requests/scheme.js';
import {
  GitHubReposSearchSingleQueryLocalSchema,
  GitHubReposSearchBulkQueryLocalSchema,
} from './github_search_repos/scheme.js';
import {
  GitHubViewRepoStructureQueryLocalSchema,
  GitHubViewRepoStructureBulkQueryLocalSchema,
} from './github_view_repo_structure/scheme.js';
import {
  NpmSearchQueryLocalSchema,
  NpmSearchBulkQueryLocalSchema,
} from './package_search/scheme.js';
import {
  LocalFetchContentQuerySchema,
  LocalFetchContentBulkQuerySchema,
} from './local_fetch_content/scheme.js';
import {
  LocalFindFilesQuerySchema,
  LocalFindFilesBulkQuerySchema,
} from './local_find_files/scheme.js';
import {
  LocalRipgrepQuerySchema,
  LocalRipgrepBulkQuerySchema,
} from './local_ripgrep/scheme.js';
import {
  LocalViewStructureQuerySchema,
  LocalViewStructureBulkQuerySchema,
} from './local_view_structure/scheme.js';
import {
  BulkLspGetSemanticsQuerySchema,
  LspGetSemanticsQueryDisplaySchema,
} from './lsp/semantic_content/scheme.js';
import {
  LocalBinaryInspectQuerySchema,
  LocalBinaryInspectBulkQuerySchema,
} from './local_binary_inspect/scheme.js';
import {
  OqlDisplayQuerySchema as OqlSearchQuerySchema,
  OqlSearchInputSchema,
} from '../oql/schema.js';
import { executeInspectBinary } from './local_binary_inspect/execution.js';
import { executeCloneRepo } from './github_clone_repo/execution.js';
import { fetchMultipleGitHubFileContents } from './github_fetch_content/execution.js';
import { searchMultipleGitHubCode } from './github_search_code/execution.js';
import { searchMultipleGitHubPullRequests } from './github_search_pull_requests/execution.js';
import { searchMultipleGitHubRepos } from './github_search_repos/execution.js';
import { exploreMultipleRepositoryStructures } from './github_view_repo_structure/execution.js';
import { searchPackages } from './package_search/execution.js';
import { executeFetchContent } from './local_fetch_content/execution.js';
import { executeFindFiles } from './local_find_files/execution.js';
import { executeRipgrepSearch } from './local_ripgrep/execution.js';
import { executeViewStructure } from './local_view_structure/execution.js';
import { executeLspGetSemantics } from './lsp/semantic_content/execution.js';
import { executeOqlSearchTool } from './oql_search/execution.js';
import { LSP_GET_SEMANTIC_CONTENT_TOOL_NAME } from './lsp/shared/semanticTypes.js';
import { OQL_SEARCH_TOOL_NAME } from './toolNames.js';
import {
  DEFAULT_TOOL_METADATA_GATEWAY,
  type ToolMetadataGateway,
} from './toolMetadata/gateway.js';

export type { ToolMetadataGateway };
export { DEFAULT_TOOL_METADATA_GATEWAY };
export type { ToolInvocationCallback } from '../types/toolResults.js';

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
  isBinary?: boolean;
  type: 'search' | 'content' | 'history' | 'debug';

  skipMetadataCheck?: boolean;
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
  LSP_GET_SEMANTIC_CONTENT: ToolConfig;
  LOCAL_BINARY_INSPECT: ToolConfig;
  OQL_SEARCH: ToolConfig;
  ALL_TOOLS: ToolConfig[];
}

function createToolCatalog(
  gateway: ToolMetadataGateway = DEFAULT_TOOL_METADATA_GATEWAY
): ToolCatalog {
  const GITHUB_SEARCH_CODE = createTool(gateway, 'GITHUB_SEARCH_CODE', {
    isDefault: true,
    isLocal: false,
    type: 'search',
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
    direct: {
      schema: NpmSearchQueryLocalSchema,
      inputSchema: NpmSearchBulkQueryLocalSchema,
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
    direct: {
      schema: LocalRipgrepQuerySchema,
      inputSchema: LocalRipgrepBulkQuerySchema,
      executionFn: executeRipgrepSearch,
      security: 'basic',
    },
  });

  const LOCAL_VIEW_STRUCTURE = createTool(gateway, 'LOCAL_VIEW_STRUCTURE', {
    isDefault: true,
    isLocal: true,
    type: 'content',
    direct: {
      schema: LocalViewStructureQuerySchema,
      inputSchema: LocalViewStructureBulkQuerySchema,
      executionFn: executeViewStructure,
      security: 'basic',
    },
  });

  const LOCAL_FIND_FILES = createTool(gateway, 'LOCAL_FIND_FILES', {
    isDefault: true,
    isLocal: true,
    type: 'search',
    direct: {
      schema: LocalFindFilesQuerySchema,
      inputSchema: LocalFindFilesBulkQuerySchema,
      executionFn: executeFindFiles,
      security: 'basic',
    },
  });

  const LOCAL_FETCH_CONTENT = createTool(gateway, 'LOCAL_FETCH_CONTENT', {
    isDefault: true,
    isLocal: true,
    type: 'content',
    direct: {
      schema: LocalFetchContentQuerySchema,
      inputSchema: LocalFetchContentBulkQuerySchema,
      executionFn: executeFetchContent,
      security: 'basic',
    },
  });

  const LSP_GET_SEMANTIC_CONTENT: ToolConfig = {
    name: LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
    description: getDescription(LSP_GET_SEMANTIC_CONTENT_TOOL_NAME, gateway),
    isDefault: true,
    isLocal: true,
    skipMetadataCheck: true,
    type: 'content',
    direct: {
      schema: LspGetSemanticsQueryDisplaySchema,
      inputSchema: BulkLspGetSemanticsQuerySchema,
      executionFn: executeLspGetSemantics,
      security: 'basic',
      requiresServerRuntime: true,
    },
  };

  const LOCAL_BINARY_INSPECT = createTool(gateway, 'LOCAL_BINARY_INSPECT', {
    isDefault: true,
    isLocal: true,
    isBinary: true,
    type: 'content',
    direct: {
      schema: LocalBinaryInspectQuerySchema,
      inputSchema: LocalBinaryInspectBulkQuerySchema,
      executionFn: executeInspectBinary,
      security: 'basic',
    },
  });

  const OQL_SEARCH: ToolConfig = {
    name: OQL_SEARCH_TOOL_NAME,
    description: getDescription(OQL_SEARCH_TOOL_NAME, gateway),
    isDefault: true,
    isLocal: false,
    type: 'search',
    direct: {
      schema: OqlSearchQuerySchema,
      inputSchema: OqlSearchInputSchema,
      executionFn: executeOqlSearchTool,
      security: 'remote',
      requiresServerRuntime: true,
      requiresProviders: true,
    },
  };

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
    LSP_GET_SEMANTIC_CONTENT,
    LOCAL_BINARY_INSPECT,
    OQL_SEARCH,
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
    LSP_GET_SEMANTIC_CONTENT,
    LOCAL_BINARY_INSPECT,
    OQL_SEARCH,
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
export const LSP_GET_SEMANTIC_CONTENT =
  DEFAULT_TOOL_CATALOG.LSP_GET_SEMANTIC_CONTENT;
export const LOCAL_BINARY_INSPECT = DEFAULT_TOOL_CATALOG.LOCAL_BINARY_INSPECT;
export const OQL_SEARCH = DEFAULT_TOOL_CATALOG.OQL_SEARCH;
export const ALL_TOOLS = DEFAULT_TOOL_CATALOG.ALL_TOOLS;
