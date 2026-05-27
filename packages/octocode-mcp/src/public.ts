/**
 * Public API exports for octocode-mcp
 *
 * This module is the stable surface for programmatic consumers (e.g.
 * `octocode-cli`, `octocode-research`). Internal helpers — pattern-matching
 * fallbacks, individual `register*Tool` functions, the lower-level core
 * implementations that hide behind the bulk `execute*` runners — are NOT part
 * of the public API and are intentionally absent from this file.
 *
 * @example
 * ```typescript
 * import {
 *   STATIC_TOOL_NAMES,
 *   type CompleteMetadata,
 *   type ToolNames,
 *   type ToolMetadata
 * } from 'octocode-mcp/public';
 * ```
 */

// Server registration and configuration
export { registerTools } from './tools/toolsManager.js';
export { ALL_TOOLS, type ToolConfig } from './tools/toolConfig.js';
export {
  initialize,
  getGitHubToken,
  getToken,
  getTokenSource,
} from './serverConfig.js';
export { initializeProviders } from './providers/factory.js';
export type { TokenSourceType } from './types/server.js';

// Tool execution functions and security
export { DEFAULT_TOOL_RESPONSE_FORMAT } from './types/execution.js';

export type {
  FileContentQuery,
  FetchContentQuery,
  FindFilesQuery,
  GitHubCodeSearchQuery,
  GitHubFetchContentData as ContentResultData,
  GitHubFetchContentToolResult as ContentResult,
  GitHubPullRequestSearchQuery,
  GitHubPullRequestOutput as PullRequestInfo,
  GitHubReposSearchQuery,
  GitHubSearchCodeData as SearchResult,
  GitHubSearchPullRequestsData as PullRequestSearchResultData,
  GitHubSearchPullRequestsPagination as PRSearchPagination,
  GitHubSearchPullRequestsToolResult as PullRequestSearchResult,
  GitHubRepositoryOutput as SimplifiedRepository,
  GitHubSearchRepositoriesData as RepoSearchResult,
  GitHubViewRepoStructureQuery,
  GitHubRepoStructureDirectoryEntry as DirectoryEntry,
  GitHubViewRepoStructureData as RepoStructureResultData,
  GitHubViewRepoStructureToolResult as RepoStructureResult,
  LocalGetFileContentPagination as FetchContentPagination,
  LocalGetFileContentToolResult as FetchContentResult,
  LocalFindFilesEntry as FoundFile,
  LocalFindFilesPagination as FindFilesPagination,
  LocalFindFilesToolResult as FindFilesResult,
  RipgrepQuery as RipgrepSearchQuery,
  LocalSearchCodeMatch as RipgrepMatch,
  LocalSearchCodeMatchPagination as RipgrepMatchPagination,
  LocalSearchCodeFile as RipgrepFileMatches,
  LocalSearchCodePagination as SearchContentPagination,
  LocalSearchCodeToolResult as SearchContentResult,
  ViewStructureQuery,
  LocalViewStructurePagination as ViewStructurePagination,
  LocalViewStructureToolResult as ViewStructureResult,
  LSPCallHierarchyQuery,
  LspCallHierarchyItem as CallHierarchyItem,
  LspIncomingCall as IncomingCall,
  LspOutgoingCall as OutgoingCall,
  LspCallHierarchyToolResult as CallHierarchyResult,
  LSPFindReferencesQuery,
  LspReferenceLocation as ReferenceLocation,
  LspFindReferencesToolResult as FindReferencesResult,
  LspFindReferencesPagination as LSPPaginationInfo,
  LSPGotoDefinitionQuery,
  LspGotoDefinitionToolResult as GotoDefinitionResult,
  LspExactPosition as ExactPosition,
  LspRange as LSPRange,
  LspSymbolKind as SymbolKind,
  LspCodeSnippet as CodeSnippet,
  PackageSearchQuery,
  PackageSearchPackage as PackageResultWithRepo,
  PackageSearchData as PackageSearchResult,
} from '@octocodeai/octocode-core';

// Tool execution — canonical bulk entry points
export { fetchMultipleGitHubFileContents } from './tools/github_fetch_content/execution.js';
export { searchMultipleGitHubCode } from './tools/github_search_code/execution.js';
export { searchMultipleGitHubPullRequests } from './tools/github_search_pull_requests/execution.js';
export { searchMultipleGitHubRepos } from './tools/github_search_repos/execution.js';
export { exploreMultipleRepositoryStructures } from './tools/github_view_repo_structure/execution.js';
export { executeFetchContent } from './tools/local_fetch_content/execution.js';
export { executeFindFiles } from './tools/local_find_files/execution.js';
export { executeRipgrepSearch } from './tools/local_ripgrep/execution.js';
export { executeViewStructure } from './tools/local_view_structure/execution.js';
export { executeCallHierarchy } from './tools/lsp_call_hierarchy/execution.js';
export { executeFindReferences } from './tools/lsp_find_references/execution.js';
export { executeGotoDefinition } from './tools/lsp_goto_definition/execution.js';
export { searchPackages } from './tools/package_search/execution.js';
export { executeCloneRepo } from './tools/github_clone_repo/execution.js';

export { withBasicSecurityValidation } from './utils/securityBridge.js';

// Zod schemas (re-exported from @octocodeai/octocode-core for convenience)
export {
  GitHubCodeSearchQuerySchema,
  GitHubViewRepoStructureQuerySchema,
  GitHubReposSearchSingleQuerySchema,
  GitHubPullRequestSearchQuerySchema,
  FileContentQuerySchema,
  RipgrepQuerySchema,
  FetchContentQuerySchema,
  FindFilesQuerySchema,
  ViewStructureQuerySchema,
  LSPGotoDefinitionQuerySchema,
  LSPFindReferencesQuerySchema,
  LSPCallHierarchyQuerySchema,
  PackageSearchQuerySchema,
  CloneRepoQuerySchema,
} from '@octocodeai/octocode-core';

// Tool metadata
export { loadToolContent } from './tools/toolMetadata/state.js';

// Response formatting
export {
  createResult,
  createResponseFormat,
  createRoleBasedResult,
  formatCallToolResultForOutput,
  ContentBuilder,
  QuickResult,
  StatusEmoji,
  StatusEmojis,
} from './responses.js';
export type {
  ContentRole,
  RoleContentBlock,
  RoleBasedResultOptions,
  RoleAnnotations,
  CallToolResultOutputMode,
} from './responses.js';

// Session management
export {
  initializeSession,
  logSessionInit,
  logToolCall,
  logPromptCall,
  logSessionError,
  logRateLimit,
} from './session.js';
export type {
  SessionData,
  ToolCallData,
  ErrorData,
  RateLimitData,
} from './types/session.js';
