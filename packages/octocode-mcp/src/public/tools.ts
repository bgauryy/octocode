import type {
  FileContentQuery,
  FetchContentQuery,
  FindFilesQuery,
  GitHubCodeSearchQuery,
  GitHubFetchContentData,
  GitHubFetchContentToolResult,
  GitHubPullRequestOutput,
  GitHubPullRequestSearchQuery,
  GitHubRepoStructureDirectoryEntry,
  GitHubRepositoryOutput,
  GitHubReposSearchQuery,
  GitHubSearchCodeData,
  GitHubSearchPullRequestsData,
  GitHubSearchPullRequestsPagination,
  GitHubSearchPullRequestsToolResult,
  GitHubSearchRepositoriesData,
  GitHubViewRepoStructureData,
  GitHubViewRepoStructureQuery,
  GitHubViewRepoStructureToolResult,
  LocalFindFilesEntry,
  LocalFindFilesPagination,
  LocalFindFilesToolResult,
  LocalGetFileContentPagination,
  LocalGetFileContentToolResult,
  LocalSearchCodeFile,
  LocalSearchCodeMatch,
  LocalSearchCodeMatchPagination,
  LocalSearchCodePagination,
  LocalSearchCodeToolResult,
  LocalViewStructurePagination,
  LocalViewStructureToolResult,
  LSPCallHierarchyQuery,
  LSPFindReferencesQuery,
  LSPGotoDefinitionQuery,
  LspCallHierarchyItem,
  LspCallHierarchyToolResult,
  LspCodeSnippet,
  LspExactPosition,
  LspFindReferencesPagination,
  LspFindReferencesToolResult,
  LspGotoDefinitionToolResult,
  LspIncomingCall,
  LspOutgoingCall,
  LspRange,
  LspReferenceLocation,
  LspSymbolKind,
  PackageSearchData,
  PackageSearchPackage,
  PackageSearchQuery,
  RipgrepQuery,
  ViewStructureQuery,
} from '@octocodeai/octocode-core';

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
};

export { fetchMultipleGitHubFileContents } from '../tools/github_fetch_content/execution.js';

export { searchMultipleGitHubCode } from '../tools/github_search_code/execution.js';

export { searchMultipleGitHubPullRequests } from '../tools/github_search_pull_requests/execution.js';

export { searchMultipleGitHubRepos } from '../tools/github_search_repos/execution.js';

export { exploreMultipleRepositoryStructures } from '../tools/github_view_repo_structure/execution.js';

// --- Local Tools ---
export { registerLocalFetchContentTool } from '../tools/local_fetch_content/register.js';
export { fetchContent } from '../tools/local_fetch_content/fetchContent.js';
export { executeFetchContent } from '../tools/local_fetch_content/execution.js';

export { registerLocalFindFilesTool } from '../tools/local_find_files/register.js';
export { findFiles } from '../tools/local_find_files/findFiles.js';
export { executeFindFiles } from '../tools/local_find_files/execution.js';

export { registerLocalRipgrepTool } from '../tools/local_ripgrep/register.js';
export { searchContentRipgrep } from '../tools/local_ripgrep/searchContentRipgrep.js';
export { executeRipgrepSearch } from '../tools/local_ripgrep/execution.js';
export type { SearchStats } from '../tools/local_ripgrep/types.js';

export { registerLocalViewStructureTool } from '../tools/local_view_structure/register.js';
export { viewStructure } from '../tools/local_view_structure/local_view_structure.js';
export { executeViewStructure } from '../tools/local_view_structure/execution.js';

export { registerLSPCallHierarchyTool } from '../tools/lsp_call_hierarchy/register.js';
export { executeCallHierarchy } from '../tools/lsp_call_hierarchy/execution.js';
export {
  processCallHierarchy,
  parseRipgrepJsonOutput,
  extractFunctionBody,
  inferSymbolKind,
  createRange,
  escapeRegex,
} from '../tools/lsp_call_hierarchy/callHierarchy.js';

export { registerLSPFindReferencesTool } from '../tools/lsp_find_references/register.js';
export { executeFindReferences } from '../tools/lsp_find_references/execution.js';
export {
  findReferences,
  findReferencesWithLSP,
  findReferencesWithPatternMatching,
} from '../tools/lsp_find_references/lsp_find_references.js';

export { executeGotoDefinition } from '../tools/lsp_goto_definition/execution.js';
export type { LSPErrorType } from '../tools/lsp_goto_definition/types.js';

export { searchPackages } from '../tools/package_search/execution.js';
export type {
  NpmPackageSearchQuery,
  PythonPackageSearchQuery,
  MinimalPackageResult,
  NpmPackageResult,
  PythonPackageResult,
  PackageResult,
  DeprecationInfo,
  PackageSearchAPIResult,
  PackageSearchError,
} from '../tools/package_search/types.js';

export { withBasicSecurityValidation } from '../utils/securityBridge.js';

export { registerGitHubCloneRepoTool } from '../tools/github_clone_repo/register.js';
