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
export { initialize, getGitHubToken, getTokenSource } from './serverConfig.js';
export { initializeProviders } from './providers/factory.js';
export type { TokenSourceType } from './types/server.js';

// Tool execution functions and security
import type { z } from 'zod/v4';
import type {
  FileContentQuerySchema,
  FetchContentQuerySchema,
  FindFilesQuerySchema,
  GitHubCodeSearchQuerySchema,
  GitHubPullRequestSearchQuerySchema,
  GitHubReposSearchSingleQuerySchema,
  GitHubViewRepoStructureQuerySchema,
  RipgrepQuerySchema,
  ViewStructureQuerySchema,
  LSPCallHierarchyQuerySchema,
  LSPFindReferencesQuerySchema,
  LSPGotoDefinitionQuerySchema,
  NpmPackageQuerySchema,
} from '@octocodeai/octocode-core/schemas';

export type FileContentQuery = z.infer<typeof FileContentQuerySchema>;
export type FetchContentQuery = z.infer<typeof FetchContentQuerySchema>;
export type FindFilesQuery = z.infer<typeof FindFilesQuerySchema>;
export type GitHubCodeSearchQuery = z.infer<typeof GitHubCodeSearchQuerySchema>;
export type GitHubPullRequestSearchQuery = z.infer<
  typeof GitHubPullRequestSearchQuerySchema
>;
export type GitHubReposSearchQuery = z.infer<
  typeof GitHubReposSearchSingleQuerySchema
>;
export type GitHubViewRepoStructureQuery = z.infer<
  typeof GitHubViewRepoStructureQuerySchema
>;
export type RipgrepSearchQuery = z.infer<typeof RipgrepQuerySchema>;
export type ViewStructureQuery = z.infer<typeof ViewStructureQuerySchema>;
export type LSPCallHierarchyQuery = z.infer<typeof LSPCallHierarchyQuerySchema>;
export type LSPFindReferencesQuery = z.infer<
  typeof LSPFindReferencesQuerySchema
>;
export type LSPGotoDefinitionQuery = z.infer<
  typeof LSPGotoDefinitionQuerySchema
>;
export type PackageSearchQuery = z.infer<typeof NpmPackageQuerySchema>;

export type {
  GitHubFileContentData as ContentResultData,
  GitHubPullRequestItem as PullRequestInfo,
  GitHubSearchCodeData as SearchResult,
  GitHubSearchPullRequestsData as PullRequestSearchResultData,
  PaginationInfo as PRSearchPagination,
  GitHubViewRepoStructureData as RepoStructureResultData,
  CharPagination as FetchContentPagination,
  LocalFindFilesEntry as FoundFile,
  PaginationInfo as FindFilesPagination,
  LocalSearchCodeMatch as RipgrepMatch,
  LocalSearchCodeMatchPagination as RipgrepMatchPagination,
  LocalSearchCodeFile as RipgrepFileMatches,
  PaginationInfo as SearchContentPagination,
  PaginationInfo as ViewStructurePagination,
  LspLocation as ReferenceLocation,
  PaginationInfo as LSPPaginationInfo,
  LspRange as LSPRange,
  PackageItem as PackageResultWithRepo,
  PackageSearchData as PackageSearchResult,
} from '@octocodeai/octocode-core/types';

export type {
  GitHubFetchContentToolResult as ContentResult,
  GitHubSearchPullRequestsToolResult as PullRequestSearchResult,
  GitHubRepositoryOutput as SimplifiedRepository,
  GitHubSearchRepositoriesData as RepoSearchResult,
  GitHubRepoStructureDirectoryEntry as DirectoryEntry,
  GitHubViewRepoStructureToolResult as RepoStructureResult,
  LocalGetFileContentToolResult as FetchContentResult,
  LocalFindFilesToolResult as FindFilesResult,
  LocalSearchCodeToolResult as SearchContentResult,
  LocalViewStructureToolResult as ViewStructureResult,
  LspCallHierarchyItem as CallHierarchyItem,
  LspIncomingCall as IncomingCall,
  LspOutgoingCall as OutgoingCall,
  LspCallHierarchyToolResult as CallHierarchyResult,
  LspFindReferencesToolResult as FindReferencesResult,
  LspGotoDefinitionToolResult as GotoDefinitionResult,
  LspExactPosition as ExactPosition,
} from '@octocodeai/octocode-core/extra-types';

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
export {
  buildDirectToolExampleQuery,
  DIRECT_TOOL_CATEGORIES,
  DIRECT_TOOL_DEFINITIONS,
  DirectToolInputError,
  executeDirectTool,
  findDirectToolDefinition,
  formatDirectToolOutputSchemaText,
  formatDirectToolMetadataSchemaText,
  formatDirectToolSchemaText,
  getDirectToolAutoFilledFields,
  getDirectToolCategory,
  getDirectToolDescription,
  getDirectToolDisplayFields,
  getDirectToolOutputFields,
  prepareDirectToolInputFromJsonText,
  sortDirectToolNames,
  type DirectToolCategory,
  type DirectToolDefinition,
  type DirectToolDisplayField,
  type DirectToolInput,
  type DirectToolMetadata,
  type DirectToolOutputField,
  type PrepareDirectToolInputOptions,
} from './tools/directToolCatalog.js';

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
  NpmPackageQuerySchema as PackageSearchQuerySchema,
  CloneRepoQuerySchema,
} from '@octocodeai/octocode-core/schemas';

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
