export * from './security/bridge.js';
export * from './config.js';
export * from './errors/domainErrors.js';
export * from './errors/errorFactories.js';
export * from './errors/localToolErrors.js';
export { redactPath } from '@octocodeai/octocode-engine/pathUtils';
export * from './errors/ToolError.js';
export * from './github/client.js';
export * from './github/codeSearch.js';
export {
  MAX_DIRECTORY_FILES,
  MAX_TOTAL_SIZE,
} from './github/directoryFetch/helpers.js';
export { fetchDirectoryContents } from './github/directoryFetch/fetchDirectoryContents.js';
export { fetchFileContentToDisk } from './github/directoryFetch/fetchFileContentToDisk.js';
export * from './github/errorConstants.js';
export * from './github/errors.js';
export * from './github/fileContent.js';
export * from './github/fileContentProcess.js';
export {
  applyContentPagination,
  fetchFileTimestamp,
} from './github/fileContentPagination.js';
export type {
  RawContentResult,
  RawContentFetchOptions,
  RawContentFetchResponse,
} from './github/fileContentRaw/fetch.js';
export { fetchRawGitHubFileContent } from './github/fileContentRaw/fetch.js';
export * from './github/prByNumber.js';
export { shouldEnrichPullRequestFromSearch } from './github/prContentFetcher/flags.js';
export {
  transformPullRequestItemFromSearch,
  transformPullRequestItemFromREST,
} from './github/prContentFetcher/transform.js';
export * from './github/prTransformation.js';
export * from './github/pullRequestSearch.js';
export * from './github/history.js';
export { getOwnerQualifier } from './github/queryBuilders/base.js';
export {
  buildCodeSearchQuery,
  buildRepoSearchQuery,
} from './github/queryBuilders/codeAndRepo.js';
export {
  buildPullRequestSearchQuery,
  shouldUseSearchForPRs,
} from './github/queryBuilders/pullRequests.js';
export {
  buildIssueSearchQuery,
  shouldUseSearchForIssues,
  type IssueSearchParams,
} from './github/queryBuilders/issues.js';
export * from './github/repoSearch.js';
export { viewGitHubRepositoryStructureAPI } from './github/repoStructure/fetchOrchestration.js';
export * from './github/repoStructurePagination.js';
export * from './github/repoStructureRecursive.js';
export * from './github/responseHeaders.js';
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
export * from './serverConfig.js';
export * from './cacheMaintenance.js';
export * from './session.js';
export * from './tools/executionGuard.js';
export * from './tools/github_clone_repo/cache.js';
export {
  cleanupStaleMaterializationArtifacts,
  tryRecoverStaleCloneLock,
  writeCloneLockMeta,
} from './tools/github_clone_repo/cacheArtifacts.js';
export {
  getCloneBaseDir,
  getTreeBaseDir,
  getCloneDir,
  getTreeDir,
} from './tools/github_clone_repo/cachePaths.js';
export * from './tools/github_clone_repo/cloneRepo.js';
export * from './tools/github_clone_repo/execution.js';
export * from './tools/github_clone_repo/scheme.js';
export * from './tools/github_clone_repo/types.js';
export * from './tools/github_fetch_content/execution.js';
export * from './tools/github_fetch_content/finalizer.js';
export * from './tools/github_fetch_content/scheme.js';
export * from './tools/github_fetch_content/types.js';
export * from './tools/github_search/execution.js';
export * from './tools/github_search/scheme.js';
export * from './tools/github_search_pull_requests/contentRequest.js';
export * from './tools/github_search_pull_requests/contentResponse.js';
export * from './tools/github_search_pull_requests/execution.js';
export * from './tools/github_search_pull_requests/scheme.js';
export * from './tools/github_search_pull_requests/splitSchemes.js';
export * from './tools/github_search_pull_requests/splitExecutions.js';
export * from './tools/github_search_pull_requests/types.js';
export * from './tools/local_fetch_content/contentExtractor.js';
export * from './tools/local_fetch_content/execution.js';
export * from './tools/local_fetch_content/fetchContent.js';
export * from './tools/local_fetch_content/scheme.js';
export * from './tools/local_analyze_graph/execution.js';
export * from './tools/local_analyze_graph/scheme.js';
export * from './tools/local_search/execution.js';
export * from './tools/local_search/scheme.js';
export * from './tools/lsp/semantic_content/execution.js';
export * from './tools/lsp/semantic_content/scheme.js';
export * from './tools/lsp/shared/callHierarchyTraversal.js';
export * from './tools/lsp/shared/resolveSymbolAnchor.js';
export * from './tools/lsp/shared/semanticTypes.js';
export * from './tools/package_search/execution.js';
export {
  foldKeywords,
  isPackageNotFoundError,
} from './tools/package_search/queryHelpers.js';
export { buildPackagePagination } from './tools/package_search/pagination.js';
export * from './tools/package_search/scheme.js';
export * from './tools/providerExecution.js';
export * from './tools/providerMappers/codeSearch.js';
export * from './tools/providerMappers/repoSearch.js';
export * from './tools/providerMappers/pullRequests.js';
export * from './tools/providerMappers/fileContent.js';
export * from './tools/providerMappers/repoStructure.js';
export * from './tools/toolConfig.js';
export type {
  ToolConfig,
  ToolDirectExecutionConfig,
  ToolDirectSecurity,
} from './tools/toolCatalogFactory.js';
export { baseSchemaDescriptions } from './toolContract/input/resources/global.js';
export { PUBLIC_TOOL_DESCRIPTIONS } from './toolContract/descriptions.js';
export * from './tools/toolMetadata/metadataPresence.js';
export * from './tools/toolMetadata/names.js';
export * from './tools/toolMetadata/state.js';
export * from './tools/toolMetadata/types.js';
export * from './tools/directToolCatalog/toolCatalogDefinitions.js';
export * from './tools/directToolCatalog/toolCatalogFormatters.js';
export * from './tools/directToolCatalog/toolSchemaIntrospection.js';
export * from './tools/directToolCatalog/toolCommandPatterns.js';
export * from './tools/directToolCatalog/toolInputPreparation.js';
export * from './tools/directToolCatalog/toolSchemaRelations.js';
export { executeDirectTool } from './tools/directToolCatalog.exec.js';
export * from './tools/toolNames.js';
export * from './tools/utils.js';
export * from './types/bulk.js';
export * from './types/execution.js';
export * from './types/promise.js';
export * from './types/responseTypes.js';
export * from './types/server.js';
export * from './types/session.js';
export * from './types/toolResults.js';
export * from './utils/core/bestEffort.js';
export * from './utils/core/compare.js';
export * from './utils/core/constants.js';
export * from './utils/core/lines.js';
export * from './utils/core/promise.js';
export * from './utils/core/safeRegex.js';
export * from './utils/environment/environmentDetection.js';
export * from './utils/exec/npm.js';
export * from './utils/exec/safe.js';
export * from './utils/exec/spawn/env.js';
export * from './utils/exec/spawn/wrappers.js';
export * from './utils/file/byteOffset.js';
export {
  DISCOVERY_IGNORED_FILE_EXTENSIONS,
  DISCOVERY_IGNORED_FILE_NAMES,
  DISCOVERY_IGNORED_FOLDER_NAMES,
  getDiscoveryExtension,
  shouldIgnoreDiscoveryDir,
  shouldIgnoreDiscoveryFile,
} from '@octocodeai/octocode-engine/security';
export type { DiscoveryExtensionOptions } from '@octocodeai/octocode-engine/security';
export * from './utils/file/size.js';
export * from './utils/file/toolHelpers.js';
export * from './utils/http/cache/key.js';
export * from './utils/http/cache/dataCache.js';
export * from './utils/http/cache/conditional.js';
export * from './utils/http/cache/management.js';
export * from './utils/http/cache/diskStore.js';
export * from './utils/http/circuitBreaker.js';
export * from './utils/http/fetch.js';
export * from './utils/package/common.js';
export {
  getNpmRegistryUrl,
  checkNpmRegistryReachable,
} from './utils/package/npm/npmRegistry.js';
export {
  isExactPackageName,
  searchNpmPackage,
  checkNpmDeprecation,
} from './utils/package/npm/npmDeprecation.js';
export * from './utils/package/schemas.js';
export * from './utils/package/types.js';
export * from './utils/pagination/boundary.js';
export * from './utils/pagination/charLimit.js';
export * from './utils/pagination/core.js';
export * from './utils/pagination/hints.js';
export * from './utils/pagination/types.js';
export * from './utils/parsers/diff.js';
export * from './utils/parsers/schemas.js';
export { computeQueryTimeout } from './utils/response/bulk/queries.js';
export { executeBulkOperation } from './utils/response/bulk/response.js';
export * from './utils/response/callToolResult.js';
export * from './utils/response/normalizedError.js';
export * from './utils/response/charSavings.js';
export * from './utils/response/error.js';
export * from './utils/response/groupedFinalizer.js';
export * from './utils/response/pathRelativize.js';

export type { GitHubPullRequestItem, Repository } from './github/githubAPI.js';
export {
  isGitHubAPIError,
  isGitHubAPISuccess,
  isRepository,
} from './github/githubAPI.js';

export type {
  ProviderType,
  ProviderConfig,
  ProviderCapabilities,
  ProviderResponse,
  ICodeHostProvider,
} from './providers/types.js';
export type {
  UnifiedRepository,
  CodeSearchItem,
  CodeSearchResult,
  FileContentResult,
  RepoSearchResult,
  PullRequestSearchResult,
  RepoStructureResult,
} from './providers/providerResults.js';
export { isProviderSuccess, isProviderError } from './providers/types.js';

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

export {
  summarizeEntries,
  paginateEntries,
  buildEntryPaginationHints,
  buildWalkWarnings,
} from './tools/local_view_structure/structureResponse.js';

export type { ExecResult } from './utils/core/types.js';

export {
  securityRegistry,
  ContentSanitizer,
} from '@octocodeai/octocode-engine/security';
export { maskSensitiveData } from '@octocodeai/octocode-engine/mask';
export { configureSecurity } from '@octocodeai/octocode-engine/withSecurityValidation';

export type {
  OAuthToken,
  StoredCredentials,
  StoreResult,
  DeleteResult,
  CredentialsStore,
  TokenSource,
  GetCredentialsOptions,
  ResolvedToken,
  TokenWithRefreshResult,
  ResolvedTokenWithRefresh,
  RefreshResult,
  FullTokenResolution,
  GhCliTokenGetter,
} from './shared/credentials/index.js';
export {
  storeCredentials,
  getCredentials,
  getCredentialsSync,
  deleteCredentials,
  updateToken,
  invalidateCredentialsCache,
  getToken,
  getTokenSync,
  resolveToken,
  getTokenWithRefresh,
  resolveTokenWithRefresh,
  refreshAuthToken,
  resolveTokenFull,
  resetTokenResolution,
  listStoredHosts,
  listStoredHostsSync,
  hasCredentials,
  hasCredentialsSync,
  isTokenExpired,
  isRefreshTokenExpired,
  getCredentialsFilePath,
  readCredentialsStore,
  encrypt,
  decrypt,
  ensureOctocodeDir,
  OCTOCODE_DIR,
  CREDENTIALS_FILE,
  KEY_FILE,
  getGhCliToken,
} from './shared/credentials/index.js';
// Env-token helpers are single-sourced in @octocodeai/config — import them from there directly.
export {
  isWindows,
  isMac,
  isLinux,
  HOME,
  getAppDataPath,
  getLocalAppDataPath,
  getPlatformName,
  getArchitecture,
} from './shared/platform/index.js';
export type {
  ToolCharSavingsStats,
  GitHubCacheHitStats,
  StatsCounterMap,
  SessionTotalUsageStats,
  SessionStats,
  PersistedSession,
  PersistedStats,
  SessionUpdateResult,
  SessionOptions,
} from './shared/session/index.js';
export {
  SESSION_FILE,
  STATS_FILE,
  getSessionId,
  getOrCreateSession,
  updateSessionStats,
  resetSessionStats,
  flushSession,
  flushSessionSync,
  deleteSession,
  incrementToolCalls,
  incrementErrors,
  incrementRateLimits,
  incrementRateLimitByProvider,
  incrementGitHubCacheHits,
  incrementGitHubCacheRateLimits,
  incrementPackageRegistryFailures,
  incrementToolCharSavings,
  _resetSessionState,
} from './shared/session/index.js';
// Config types, defaults, resolver, and validation are single-sourced in
// @octocodeai/config — import them from there directly, not through this barrel.
export { OctocodeConfigSchema } from './shared/config/schemas.js';
export {
  OCTOCODE_HOME,
  getDefaultOctocodeHome,
  getOctocodeDir,
  paths,
  ensureHome,
  ensureTmp,
  ensureClone,
  ensureRepos,
  ensureTree,
} from './shared/paths.js';
export { getDirectorySizeBytes, formatBytes } from './shared/fs-utils.js';

export { localCompleteMetadata as completeMetadata } from './toolContract/metadata.js';
export type { LocalCompleteMetadata as CompleteMetadata } from './toolContract/types.js';

export { z } from 'zod';
