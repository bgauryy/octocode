/**
 * Internal input-schema primitives owned by tools-core.
 *
 * Public canonical schemas are exported from each direct tool's scheme and
 * assembled by the direct-tool catalog. This barrel intentionally has no
 * second tool-name registry.
 */
export {
  baseSchema,
  baseSchemaDescriptions,
  toolNames,
} from './input/resources/global.js';
export type { BaseSchemaShape } from './input/resources/global.js';
export { getToolSchemaRelations } from './input/resources/toolRelations.js';
export { getToolSchemaVariants } from './input/resources/toolVariants.js';
export type { ToolSchemaVariant } from './input/resources/toolVariants.js';

export {
  MAX_CONTEXT_LINES,
  MAX_PAGE_NUMBER,
  MAX_CHAR_OFFSET,
  MAX_CHAR_LENGTH,
  MAX_LINE_NUMBER,
  MAX_ORDER_HINT,
  MAX_LOCAL_DEPTH,
  MAX_FIND_DEPTH,
  MAX_LOCAL_LIMIT,
  MAX_LOCAL_ITEMS_PER_PAGE,
  MAX_GITHUB_SEARCH_LIMIT,
  DEFAULT_GITHUB_SEARCH_LIMIT,
  MAX_GITHUB_STRUCTURE_ITEMS_PER_PAGE,
  DEFAULT_GITHUB_STRUCTURE_ITEMS_PER_PAGE,
  MAX_PR_ITEMS_PER_PAGE,
  DEFAULT_PR_ITEMS_PER_PAGE,
  MAX_MATCH_CONTENT_LENGTH,
  DEFAULT_MATCH_CONTENT_LENGTH,
  MAX_MATCH_COUNT,
  MAX_SEARCH_ITEMS_PER_PAGE,
  MAX_LSP_DEPTH,
  MAX_LSP_ITEMS_PER_PAGE,
  MAX_GITHUB_STRUCTURE_DEPTH,
} from './input/resources/tools/_toolkit.js';

export { GitHubSearchQuerySchema } from './input/resources/tools/ghSearch.js';
export { GitHubCodeSearchQuerySchema } from './input/resources/tools/githubCodeOperation.js';
export { FileContentQuerySchema } from './input/resources/tools/ghGetFileContent.js';
export { GitHubViewRepoStructureQuerySchema } from './input/resources/tools/githubTreeOperation.js';
export { GitHubReposSearchSingleQuerySchema } from './input/resources/tools/githubRepositoriesOperation.js';
export { GitHubPullRequestSearchQuerySchema } from './input/resources/tools/githubHistorySchema.js';
export { SearchPullRequestsQuerySchema } from './input/resources/tools/historyPullRequestInput.js';
export { SearchIssuesQuerySchema } from './input/resources/tools/historyIssueInput.js';
export { SearchCommitsQuerySchema } from './input/resources/tools/historyCommitInput.js';
export { NpmPackageQuerySchema } from './input/resources/tools/npmSearch.js';
export { CloneRepoQuerySchema } from './input/resources/tools/ghCloneRepo.js';
export { LocalSearchQuerySchema } from './input/resources/tools/localSearch.js';
export { RipgrepQuerySchema } from './input/resources/tools/localTextOperation.js';
export { ViewStructureQuerySchema } from './input/resources/tools/localTreeOperation.js';
export { FindFilesQuerySchema } from './input/resources/tools/localFilesOperation.js';
export { LocalAnalyzeGraphQuerySchema } from './input/resources/tools/localAnalyzeGraph.js';
export { FetchContentQuerySchema } from './input/resources/tools/localGetFileContent.js';
export { LspGetSemanticsQuerySchema } from './input/resources/tools/lspGetSemantics.js';
