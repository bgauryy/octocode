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
 *   type CompleteMetadata,
 *   type ToolNames,
 *   type ToolMetadata
 * } from 'octocode-mcp/public';
 * ```
 */

export { registerTools } from './tools/toolsManager.js';
export { ALL_TOOLS, type ToolConfig } from './tools/toolConfig.js';
export {
  initialize,
  getGitHubToken,
  getTokenSource,
  initializeProviders,
} from '@octocodeai/octocode-tools-core';
export type { TokenSourceType } from '@octocodeai/octocode-tools-core';

import type { z } from 'zod';
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
export type NpmSearchQuery = z.infer<typeof NpmPackageQuerySchema>;

export type {
  LspGetSemanticsQuery,
  LspSemanticEnvelope,
  ResolvedSymbol,
  SemanticContentType,
} from '@octocodeai/octocode-tools-core';

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
  NpmSearchData as NpmSearchResult,
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
  LspExactPosition as ExactPosition,
} from '@octocodeai/octocode-core/extra-types';

export {
  fetchMultipleGitHubFileContents,
  searchMultipleGitHubCode,
  searchMultipleGitHubPullRequests,
  searchMultipleGitHubRepos,
  exploreMultipleRepositoryStructures,
  executeFetchContent,
  executeFindFiles,
  executeRipgrepSearch,
  executeViewStructure,
  executeLspGetSemantics,
  searchPackages,
  executeCloneRepo,
} from '@octocodeai/octocode-tools-core';
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
  withBasicSecurityValidation,
  type DirectToolCategory,
  type DirectToolDefinition,
  type DirectToolDisplayField,
  type DirectToolInput,
  type DirectToolMetadata,
  type DirectToolOutputField,
  type PrepareDirectToolInputOptions,
} from '@octocodeai/octocode-tools-core';

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
  NpmPackageQuerySchema as NpmSearchQuerySchema,
  CloneRepoQuerySchema,
} from '@octocodeai/octocode-core/schemas';

export {
  BulkLspGetSemanticsQuerySchema,
  LspGetSemanticsQuerySchema,
  LspGetSemanticsOutputSchema,
  loadToolContent,
  createResult,
  createResponseFormat,
  createRoleBasedResult,
  formatCallToolResultForOutput,
  ContentBuilder,
  QuickResult,
  StatusEmoji,
  StatusEmojis,
  initializeSession,
  logSessionInit,
  logToolCall,
  logPromptCall,
  logSessionError,
  logRateLimit,
} from '@octocodeai/octocode-tools-core';
export type {
  ContentRole,
  RoleContentBlock,
  RoleBasedResultOptions,
  RoleAnnotations,
  CallToolResultOutputMode,
  SessionData,
  ToolCallData,
  ErrorData,
  RateLimitData,
} from '@octocodeai/octocode-tools-core';
