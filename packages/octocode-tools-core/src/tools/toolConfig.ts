import {
  CloneRepoQueryLocalSchema,
  BulkCloneRepoLocalSchema,
  FileContentQueryLocalSchema,
  FileContentBulkQueryLocalSchema,
  GitHubCodeSearchQueryLocalSchema,
  GitHubCodeSearchBulkQueryLocalSchema,
  GitHubReposSearchSingleQueryLocalSchema,
  GitHubReposSearchBulkQueryLocalSchema,
  GitHubViewRepoStructureQueryLocalSchema,
  GitHubViewRepoStructureBulkQueryLocalSchema,
  NpmSearchQueryLocalSchema,
  NpmSearchBulkQueryLocalSchema,
  LocalFetchContentQuerySchema,
  LocalFetchContentBulkQuerySchema,
  LocalFindFilesQuerySchema,
  LocalFindFilesBulkQuerySchema,
  LocalAnalyzeGraphQuerySchema,
  LocalAnalyzeGraphBulkQuerySchema,
  LocalRipgrepQuerySchema,
  LocalRipgrepBulkQuerySchema,
  LocalViewStructureQuerySchema,
  LocalViewStructureBulkQuerySchema,
  BulkLspGetSemanticsQuerySchema,
  LspGetSemanticsQueryDisplaySchema,
} from './toolSchemaImports.js';
import { executeCloneRepo } from './github_clone_repo/execution.js';
import { fetchMultipleGitHubFileContents } from './github_fetch_content/execution.js';
import { searchMultipleGitHubCode } from './github_search_code/execution.js';
import {
  SearchPullRequestsLocalSchema,
  SearchPullRequestsBulkLocalSchema,
  SearchIssuesLocalSchema,
  SearchIssuesBulkLocalSchema,
  SearchCommitsLocalSchema,
  SearchCommitsBulkLocalSchema,
  ListReleasesLocalSchema,
  ListReleasesBulkLocalSchema,
} from './github_search_pull_requests/splitSchemes.js';
import {
  SearchDiscussionsLocalSchema,
  SearchDiscussionsBulkLocalSchema,
} from './github_search_discussions/scheme.js';
import {
  searchMultipleGitHubPullRequestsSplit,
  searchMultipleGitHubIssues,
  searchMultipleGitHubCommits,
  listMultipleGitHubReleases,
} from './github_search_pull_requests/splitExecutions.js';
import { searchMultipleGitHubDiscussions } from './github_search_discussions/execution.js';
import { searchMultipleGitHubRepos } from './github_search_repos/execution.js';
import { exploreMultipleRepositoryStructures } from './github_view_repo_structure/execution.js';
import { searchPackages } from './package_search/execution.js';
import { executeFetchContent } from './local_fetch_content/execution.js';
import { executeFindFiles } from './local_find_files/execution.js';
import { executeAnalyzeGraph } from './local_analyze_graph/execution.js';
import { executeRipgrepSearch } from './local_ripgrep/execution.js';
import { executeViewStructure } from './local_view_structure/execution.js';
import { executeLspGetSemantics } from './lsp/semantic_content/execution.js';
import { LSP_GET_SEMANTICS_TOOL_NAME } from './lsp/shared/semanticTypes.js';
import { isReleasesEnabled, isDiscussionsEnabled } from './toolNames.js';
import {
  DEFAULT_TOOL_METADATA_GATEWAY,
  type ToolMetadataGateway,
} from './toolMetadata/gateway.js';
import {
  createTool,
  getDescription,
  type ToolConfig,
  type ToolDirectExecutionConfig,
  type ToolDirectSecurity,
} from './toolCatalogFactory.js';

export type { ToolMetadataGateway };
export { DEFAULT_TOOL_METADATA_GATEWAY };
export { getDescription };
export type { ToolConfig, ToolDirectExecutionConfig, ToolDirectSecurity };
export type { ToolInvocationCallback } from '../types/toolResults.js';

interface ToolCatalog {
  GITHUB_SEARCH_CODE: ToolConfig;
  GITHUB_FETCH_CONTENT: ToolConfig;
  GITHUB_VIEW_REPO_STRUCTURE: ToolConfig;
  GITHUB_SEARCH_REPOSITORIES: ToolConfig;
  GITHUB_PULL_REQUESTS: ToolConfig;
  GITHUB_ISSUES: ToolConfig;
  GITHUB_COMMITS: ToolConfig;
  GITHUB_RELEASES: ToolConfig;
  GITHUB_DISCUSSIONS: ToolConfig;
  PACKAGE_SEARCH: ToolConfig;
  GITHUB_CLONE_REPO: ToolConfig;
  LOCAL_RIPGREP: ToolConfig;
  LOCAL_VIEW_STRUCTURE: ToolConfig;
  LOCAL_FIND_FILES: ToolConfig;
  LOCAL_ANALYZE_GRAPH: ToolConfig;
  LOCAL_FETCH_CONTENT: ToolConfig;
  LSP_GET_SEMANTIC_CONTENT: ToolConfig;
  ALL_TOOLS: ToolConfig[];
}

const REMOTE_DIRECT = {
  security: 'remote',
  requiresServerRuntime: true,
  requiresProviders: true,
} as const;

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
      ...REMOTE_DIRECT,
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
      ...REMOTE_DIRECT,
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
        ...REMOTE_DIRECT,
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
        ...REMOTE_DIRECT,
      },
    }
  );

  const GITHUB_PULL_REQUESTS = createTool(gateway, 'GITHUB_PULL_REQUESTS', {
    isDefault: true,
    isLocal: false,
    type: 'history',
    direct: {
      schema: SearchPullRequestsLocalSchema,
      inputSchema: SearchPullRequestsBulkLocalSchema,
      executionFn: searchMultipleGitHubPullRequestsSplit,
      ...REMOTE_DIRECT,
    },
  });

  const GITHUB_ISSUES = createTool(gateway, 'GITHUB_ISSUES', {
    isDefault: true,
    isLocal: false,
    type: 'history',
    direct: {
      schema: SearchIssuesLocalSchema,
      inputSchema: SearchIssuesBulkLocalSchema,
      executionFn: searchMultipleGitHubIssues,
      ...REMOTE_DIRECT,
    },
  });

  const GITHUB_COMMITS = createTool(gateway, 'GITHUB_COMMITS', {
    isDefault: true,
    isLocal: false,
    type: 'history',
    direct: {
      schema: SearchCommitsLocalSchema,
      inputSchema: SearchCommitsBulkLocalSchema,
      executionFn: searchMultipleGitHubCommits,
      ...REMOTE_DIRECT,
    },
  });

  const GITHUB_RELEASES = createTool(gateway, 'GITHUB_RELEASES', {
    isDefault: false,
    isLocal: false,
    type: 'history',
    direct: {
      schema: ListReleasesLocalSchema,
      inputSchema: ListReleasesBulkLocalSchema,
      executionFn: listMultipleGitHubReleases,
      ...REMOTE_DIRECT,
    },
  });

  const GITHUB_DISCUSSIONS = createTool(gateway, 'GITHUB_DISCUSSIONS', {
    isDefault: false,
    isLocal: false,
    type: 'history',
    direct: {
      schema: SearchDiscussionsLocalSchema,
      inputSchema: SearchDiscussionsBulkLocalSchema,
      executionFn: searchMultipleGitHubDiscussions,
      ...REMOTE_DIRECT,
    },
  });

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
      timeoutMs: 150_000,
      ...REMOTE_DIRECT,
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

  const LOCAL_ANALYZE_GRAPH = createTool(gateway, 'LOCAL_ANALYZE_GRAPH', {
    isDefault: true,
    isLocal: true,
    type: 'search',
    direct: {
      schema: LocalAnalyzeGraphQuerySchema,
      inputSchema: LocalAnalyzeGraphBulkQuerySchema,
      executionFn: executeAnalyzeGraph,
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
    name: LSP_GET_SEMANTICS_TOOL_NAME,
    description: getDescription(LSP_GET_SEMANTICS_TOOL_NAME, gateway),
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

  const ALL_TOOLS: ToolConfig[] = [
    GITHUB_SEARCH_CODE,
    GITHUB_FETCH_CONTENT,
    GITHUB_VIEW_REPO_STRUCTURE,
    GITHUB_SEARCH_REPOSITORIES,
    GITHUB_PULL_REQUESTS,
    GITHUB_ISSUES,
    GITHUB_COMMITS,
    ...(isReleasesEnabled() ? [GITHUB_RELEASES] : []), // ENABLE_RELEASES=1
    ...(isDiscussionsEnabled() ? [GITHUB_DISCUSSIONS] : []), // ENABLE_DISCUSSIONS=1 (GraphQL)
    PACKAGE_SEARCH,
    GITHUB_CLONE_REPO,
    LOCAL_RIPGREP,
    LOCAL_VIEW_STRUCTURE,
    LOCAL_FIND_FILES,
    LOCAL_ANALYZE_GRAPH,
    LOCAL_FETCH_CONTENT,
    LSP_GET_SEMANTIC_CONTENT,
  ];

  return {
    GITHUB_SEARCH_CODE,
    GITHUB_FETCH_CONTENT,
    GITHUB_VIEW_REPO_STRUCTURE,
    GITHUB_SEARCH_REPOSITORIES,
    GITHUB_PULL_REQUESTS,
    GITHUB_ISSUES,
    GITHUB_COMMITS,
    GITHUB_RELEASES,
    GITHUB_DISCUSSIONS,
    PACKAGE_SEARCH,
    GITHUB_CLONE_REPO,
    LOCAL_RIPGREP,
    LOCAL_VIEW_STRUCTURE,
    LOCAL_FIND_FILES,
    LOCAL_ANALYZE_GRAPH,
    LOCAL_FETCH_CONTENT,
    LSP_GET_SEMANTIC_CONTENT,
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
export const GITHUB_PULL_REQUESTS = DEFAULT_TOOL_CATALOG.GITHUB_PULL_REQUESTS;
export const GITHUB_ISSUES = DEFAULT_TOOL_CATALOG.GITHUB_ISSUES;
export const GITHUB_COMMITS = DEFAULT_TOOL_CATALOG.GITHUB_COMMITS;
export const GITHUB_RELEASES = DEFAULT_TOOL_CATALOG.GITHUB_RELEASES;
export const GITHUB_DISCUSSIONS = DEFAULT_TOOL_CATALOG.GITHUB_DISCUSSIONS;
export const PACKAGE_SEARCH = DEFAULT_TOOL_CATALOG.PACKAGE_SEARCH;
export const GITHUB_CLONE_REPO = DEFAULT_TOOL_CATALOG.GITHUB_CLONE_REPO;
export const LOCAL_RIPGREP = DEFAULT_TOOL_CATALOG.LOCAL_RIPGREP;
export const LOCAL_VIEW_STRUCTURE = DEFAULT_TOOL_CATALOG.LOCAL_VIEW_STRUCTURE;
export const LOCAL_FIND_FILES = DEFAULT_TOOL_CATALOG.LOCAL_FIND_FILES;
export const LOCAL_ANALYZE_GRAPH = DEFAULT_TOOL_CATALOG.LOCAL_ANALYZE_GRAPH;
export const LOCAL_FETCH_CONTENT = DEFAULT_TOOL_CATALOG.LOCAL_FETCH_CONTENT;
export const LSP_GET_SEMANTIC_CONTENT =
  DEFAULT_TOOL_CATALOG.LSP_GET_SEMANTIC_CONTENT;
export const ALL_TOOLS = DEFAULT_TOOL_CATALOG.ALL_TOOLS;
