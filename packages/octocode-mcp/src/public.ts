/**
 * Narrow public API for programmatic Octocode consumers.
 *
 * This package is the MCP interface, so this facade only exposes the pieces a
 * non-MCP caller needs to initialize metadata/config state and run the same
 * tool executors through core. MCP registration internals and direct-tool
 * catalog helpers intentionally stay out of this surface.
 */

export type { CompleteMetadata } from '@octocodeai/octocode-core/types';

export {
  CloneRepoQuerySchema,
  FetchContentQuerySchema,
  FileContentQuerySchema,
  FindFilesQuerySchema,
  GitHubCodeSearchQuerySchema,
  GitHubPullRequestSearchQuerySchema,
  GitHubReposSearchSingleQuerySchema,
  GitHubViewRepoStructureQuerySchema,
  NpmPackageQuerySchema as NpmSearchQuerySchema,
  RipgrepQuerySchema,
  ViewStructureQuerySchema,
} from '@octocodeai/octocode-core/schemas';

export {
  createRoleBasedResult,
  executeCloneRepo,
  executeFetchContent,
  executeFindFiles,
  executeLspGetSemantics,
  executeRipgrepSearch,
  executeViewStructure,
  exploreMultipleRepositoryStructures,
  fetchMultipleGitHubFileContents,
  initialize,
  initializeProviders,
  loadToolContent,
  LspGetSemanticsQuerySchema,
  QuickResult,
  searchMultipleGitHubCode,
  searchMultipleGitHubPullRequests,
  searchMultipleGitHubRepos,
  searchPackages,
  StatusEmoji,
} from '@octocodeai/octocode-tools-core';
