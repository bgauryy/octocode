/**
 * octocode-tools-core public API
 *
 * Core tool implementations: execution functions, schemas, metadata,
 * server config, session, providers, and utilities.
 * No McpServer dependency — fully portable.
 */
export * from './security/bridge.js';
export * from './commands/BaseCommandBuilder.js';
export * from './commands/FindCommandBuilder.js';
export * from './commands/LsCommandBuilder.js';
export * from './commands/RipgrepCommandBuilder.js';
export * from './config.js';
export * from './errors/domainErrors.js';
export * from './errors/errorFactories.js';
export * from './errors/localToolErrors.js';
export * from './errors/pathUtils.js';
export * from './errors/ToolError.js';
export * from './github/client.js';
export * from './github/codeSearch.js';
export * from './github/directoryFetch.js';
export * from './github/errorConstants.js';
export * from './github/errors.js';
export * from './github/fileContent.js';
export * from './github/fileContentProcess.js';
export * from './github/fileContentRaw.js';
export * from './github/prByNumber.js';
export * from './github/prContentFetcher.js';
export * from './github/prTransformation.js';
export * from './github/pullRequestSearch.js';
export * from './github/queryBuilders.js';
export * from './github/repoSearch.js';
export * from './github/repoStructure.js';
export * from './github/repoStructurePagination.js';
export * from './github/repoStructureRecursive.js';
export * from './github/responseHeaders.js';
export * from './hints/types.js';
export * from './providers/capabilities.js';
export * from './providers/factory.js';
export * from './providers/github/githubContent.js';
export * from './providers/github/GitHubProvider.js';
export * from './providers/github/githubPullRequests.js';
export * from './providers/github/githubSearch.js';
export * from './providers/github/githubStructure.js';
export * from './providers/github/utils.js';
export * from './providers/providerQueries.js';
export * from './responses.js';
export * from './scheme/fields.js';
export * from './scheme/fileContentModeValidation.js';
export * from './scheme/responseEnvelope.js';
export * from './serverConfig.js';
export * from './session.js';
export * from './tools/evidence.js';
export * from './tools/executionGuard.js';
export * from './tools/github_clone_repo/cache.js';
export * from './tools/github_clone_repo/cloneRepo.js';
export * from './tools/github_clone_repo/execution.js';
export * from './tools/github_clone_repo/scheme.js';
export * from './tools/github_clone_repo/types.js';
export * from './tools/github_fetch_content/execution.js';
export * from './tools/github_fetch_content/finalizer.js';
export * from './tools/github_fetch_content/scheme.js';
export * from './tools/github_fetch_content/types.js';
export * from './tools/github_search_code/execution.js';
export * from './tools/github_search_code/finalizer.js';
export * from './tools/github_search_code/scheme.js';
export * from './tools/github_search_pull_requests/contentRequest.js';
export * from './tools/github_search_pull_requests/contentResponse.js';
export * from './tools/github_search_pull_requests/execution.js';
export * from './tools/github_search_pull_requests/scheme.js';
export * from './tools/github_search_pull_requests/types.js';
export * from './tools/github_search_repos/execution.js';
export * from './tools/github_search_repos/scheme.js';
export * from './tools/github_view_repo_structure/constants.js';
export * from './tools/github_view_repo_structure/execution.js';
export * from './tools/github_view_repo_structure/scheme.js';
export * from './tools/github_view_repo_structure/types.js';
export * from './tools/local_fetch_content/contentExtractor.js';
export * from './tools/local_fetch_content/execution.js';
export * from './tools/local_fetch_content/fetchContent.js';
export * from './tools/local_fetch_content/scheme.js';
export * from './tools/local_find_files/execution.js';
export * from './tools/local_find_files/findFiles.js';
export * from './tools/local_find_files/scheme.js';
export * from './tools/local_ripgrep/execution.js';
export * from './tools/local_ripgrep/grepFallbackExecutor.js';
export * from './tools/local_ripgrep/patternValidation.js';
export * from './tools/local_ripgrep/ripgrepExecutor.js';
export * from './tools/local_ripgrep/ripgrepParser.js';
export * from './tools/local_ripgrep/ripgrepResultBuilder.js';
export * from './tools/local_ripgrep/scheme.js';
export * from './tools/local_ripgrep/searchContentRipgrep.js';
export * from './tools/local_view_structure/execution.js';
export * from './tools/local_view_structure/local_view_structure.js';
export * from './tools/local_view_structure/scheme.js';
export * from './tools/local_view_structure/structureFilters.js';
export * from './tools/local_view_structure/structureParser.js';
export * from './tools/local_view_structure/structureWalker.js';
export * from './tools/lsp/semantic_content/execution.js';
export * from './tools/lsp/semantic_content/index.js';
export * from './tools/lsp/semantic_content/outputSchema.js';
export * from './tools/lsp/semantic_content/scheme.js';
export * from './tools/lsp/shared/callHierarchyTraversal.js';
export * from './tools/lsp/shared/resolveSymbolAnchor.js';
export * from './tools/lsp/shared/semanticTypes.js';
export * from './tools/package_search/execution.js';
export * from './tools/package_search/scheme.js';
export * from './tools/providerExecution.js';
export * from './tools/providerMappers.js';
export * from './tools/toolConfig.js';
export * from './tools/toolMetadata/baseSchema.js';
export * from './tools/toolMetadata/descriptions.js';
export * from './tools/toolMetadata/gateway.js';
export * from './tools/toolMetadata/metadataPresence.js';
export * from './tools/toolMetadata/names.js';
export * from './tools/toolMetadata/proxies.js';
export * from './tools/toolMetadata/state.js';
export * from './tools/toolMetadata/types.js';
export * from './tools/directToolCatalog.js';
export * from './tools/toolNames.js';
export * from './tools/utils.js';
export * from './types/bulk.js';
export * from './types/execution.js';
export * from './types/metadata.js';
export * from './types/promise.js';
export * from './types/responseTypes.js';
export * from './types/server.js';
export * from './types/session.js';
export * from './types/toolResults.js';
export * from './utils/core/bestEffort.js';
export * from './utils/core/compare.js';
export * from './utils/core/constants.js';
export * from './utils/core/promise.js';
export * from './utils/core/safeRegex.js';
export * from './utils/environment/environmentDetection.js';
export * from './utils/exec/commandAvailability.js';
export * from './utils/exec/npm.js';
export * from './utils/exec/ripgrepBinary.js';
export * from './utils/exec/safe.js';
export * from './utils/exec/spawn.js';
export * from './utils/file/byteOffset.js';
export * from './utils/file/filters.js';
export * from './utils/file/size.js';
export * from './utils/file/toolHelpers.js';
export * from './utils/http/cache.js';
export * from './utils/http/circuitBreaker.js';
export * from './utils/http/fetch.js';
export * from './utils/package/common.js';
export * from './utils/package/npm.js';
export * from './utils/package/schemas.js';
export * from './utils/package/types.js';
export * from './utils/pagination/boundary.js';
export * from './utils/pagination/charLimit.js';
export * from './utils/pagination/core.js';
export * from './utils/pagination/hints.js';
export * from './utils/pagination/outputSizeLimit.js';
export * from './utils/pagination/types.js';
export * from './utils/parsers/diff.js';
export * from './utils/parsers/ripgrep.js';
export * from './utils/parsers/schemas.js';
export * from './utils/response/bulk.js';
export * from './utils/response/callToolResult.js';
export * from './utils/response/charSavings.js';
export * from './utils/response/error.js';
export * from './utils/response/groupedFinalizer.js';
export * from './utils/response/pathRelativize.js';
export * from './utils/response/structuredPagination.js';

// github/githubAPI — explicit exports excluding PullRequestItem (conflicts with providers/providerResults.ts)
export type { GitHubPullRequestItem, Repository } from './github/githubAPI.js';
export {
  isGitHubAPIError,
  isGitHubAPISuccess,
  isRepository,
} from './github/githubAPI.js';

// providers/types — explicit exports excluding DirectoryEntry/PullRequestItem (conflicts with structureFilters.ts)
export type {
  ProviderType,
  ProviderConfig,
  ProviderCapabilities,
  ProviderResponse,
  ICodeHostProvider,
} from './providers/types.js';
export type {
  CodeSearchQuery,
  FileContentQuery,
  RepoSearchQuery,
  PullRequestQuery,
  RepoStructureQuery,
} from './providers/types.js';
export type {
  UnifiedRepository,
  CodeSearchItem,
  CodeSearchResult,
  FileContentResult,
  RepoSearchResult,
  PullRequestSearchResult,
  RepoStructureResult,
} from './providers/types.js';
export { isProviderSuccess, isProviderError } from './providers/types.js';

// github/githubAPI — remaining types not in the explicit block above
export type {
  GitHubAPIError,
  GitHubAPISuccess,
  OptimizedCodeSearchResult,
  DiffEntry,
  PullRequestSimple,
  GitHubPullRequestsSearchParams,
  ContentDirectoryEntry,
  CodeSearchResultItem,
  RepoSearchResultItem,
  IssueSearchResultItem,
  IssueComment,
  PRReviewInfo,
  CommitFileInfo,
  CommitInfo,
  PRCommentItem,
  GetContentParameters,
  SearchCodeParameters,
  SearchCodeResponse,
  SearchReposParameters,
  GitHubAPIResponse,
} from './github/githubAPI.js';

// hints/dynamic — HINTS constant (getDynamicHints is re-exported via proxies.ts)
export { HINTS, hasDynamicHints } from './hints/dynamic.js';

// structureResponse — all exports (DirectoryEntry is already canonical from structureFilters.ts)
export {
  summarizeEntries,
  paginateEntries,
  buildEntryPaginationHints,
  buildWalkWarnings,
} from './tools/local_view_structure/structureResponse.js';

// utils/core/types — ExecResult (PaginationInfo excluded to avoid conflict with types/toolResults.ts)
export type { ExecResult } from './utils/core/types.js';

// hints/index — getHints utility (hasDynamicHints is internal)
export { getHints } from './hints/index.js';

// Per-tool hints objects with unique export names (avoids 'hints' name collision between tools)
export { hints as githubSearchCodeHints } from './tools/github_search_code/hints.js';
export { hints as githubFetchContentHints } from './tools/github_fetch_content/hints.js';
export { hints as githubViewRepoStructureHints } from './tools/github_view_repo_structure/hints.js';
export { hints as githubSearchReposHints } from './tools/github_search_repos/hints.js';
export { hints as githubSearchPullRequestsHints } from './tools/github_search_pull_requests/hints.js';
export { hints as githubCloneRepoHints } from './tools/github_clone_repo/hints.js';
export { hints as localRipgrepHints } from './tools/local_ripgrep/hints.js';
export { hints as localViewStructureHints } from './tools/local_view_structure/hints.js';
export { hints as localFindFilesHints } from './tools/local_find_files/hints.js';
export { hints as localFetchContentHints } from './tools/local_fetch_content/hints.js';
export { hints as lspSemanticContentHints } from './tools/lsp/semantic_content/hints.js';
export { hints as packageSearchHints } from './tools/package_search/hints.js';

// hints/dynamic — 3-arg version distinct from proxies.ts getDynamicHints(toolName, key)
export { getDynamicHints as getDynamicToolHints } from './hints/dynamic.js';
