// Repository-owned executable validation contracts. Tool descriptions and
// schemas derive from the same files under toolContract/resources/tools.
import type { z } from 'zod';

import type { ToolNames } from './types.js';

import { GitHubCodeSearchQuerySchema } from './resources/tools/ghSearchCode.js';
import { FileContentQuerySchema } from './resources/tools/ghGetFileContent.js';
import { GitHubViewRepoStructureQuerySchema } from './resources/tools/ghViewRepoStructure.js';
import { GitHubReposSearchSingleQuerySchema } from './resources/tools/ghSearchRepos.js';
import { GitHubPullRequestSearchQuerySchema } from './resources/tools/githubHistorySchema.js';
import { SearchPullRequestsQuerySchema } from './resources/tools/ghSearchPullRequests.js';
import { SearchIssuesQuerySchema } from './resources/tools/ghSearchIssues.js';
import { SearchCommitsQuerySchema } from './resources/tools/ghSearchCommits.js';
import { ListReleasesQuerySchema } from './resources/tools/ghListReleases.js';
import { SearchDiscussionsQuerySchema } from './resources/tools/ghSearchDiscussions.js';
import { NpmPackageQuerySchema } from './resources/tools/npmSearch.js';
import { CloneRepoQuerySchema } from './resources/tools/ghCloneRepo.js';
import { RipgrepQuerySchema } from './resources/tools/localSearchCode.js';
import { ViewStructureQuerySchema } from './resources/tools/localViewStructure.js';
import { FindFilesQuerySchema } from './resources/tools/localFindFiles.js';
import { LocalAnalyzeGraphQuerySchema } from '../tools/local_analyze_graph/scheme.js';
import { FetchContentQuerySchema } from './resources/tools/localGetFileContent.js';
import { LspGetSemanticsQuerySchema } from './resources/tools/lspGetSemantics.js';

// ---------------------------------------------------------------------------
// Shared meta schema carried by every tool query (id / mainResearchGoal /
// researchGoal / reasoning) and its derived prose map.
// ---------------------------------------------------------------------------
export {
  baseSchema,
  baseSchemaDescriptions,
  toolNames,
} from './resources/global.js';
export type { BaseSchemaShape } from './resources/global.js';

// ---------------------------------------------------------------------------
// Numeric bounds & defaults — the same constants the schemas enforce, exposed
// so consumers can reference the contract limits without hard-coding them.
// ---------------------------------------------------------------------------
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
  MAX_BINARY_ENTRIES,
  DEFAULT_BINARY_ENTRIES,
  MAX_BINARY_ENTRIES_PER_PAGE,
  MAX_BINARY_CONTEXT_LINES,
  DEFAULT_BINARY_CONTEXT_LINES,
  MAX_STRINGS_MIN_LENGTH,
  DEFAULT_STRINGS_MIN_LENGTH,
} from './resources/tools/_toolkit.js';

// ---------------------------------------------------------------------------
// Per-tool Zod query schemas. One primary schema per tool, plus the extra
// per-operation variants some tools expose (LSP operations, bulk clone).
// ---------------------------------------------------------------------------
export {
  GitHubCodeSearchQuerySchema,
  FileContentQuerySchema,
  GitHubViewRepoStructureQuerySchema,
  GitHubReposSearchSingleQuerySchema,
  GitHubPullRequestSearchQuerySchema,
  SearchPullRequestsQuerySchema,
  SearchIssuesQuerySchema,
  SearchCommitsQuerySchema,
  ListReleasesQuerySchema,
  SearchDiscussionsQuerySchema,
  NpmPackageQuerySchema,
  CloneRepoQuerySchema,
  RipgrepQuerySchema,
  ViewStructureQuerySchema,
  FindFilesQuerySchema,
  LocalAnalyzeGraphQuerySchema,
  FetchContentQuerySchema,
  LspGetSemanticsQuerySchema,
};

// ---------------------------------------------------------------------------
// Registry keyed by canonical agent-facing tool name (matches `toolNames`
// values and the keys of the `tools` map exposed from /mcp). One primary
// schema per tool — look up the validator for a given tool name in one step.
// ---------------------------------------------------------------------------
export const toolSchemas = {
  ghSearchCode: GitHubCodeSearchQuerySchema,
  ghGetFileContent: FileContentQuerySchema,
  ghViewRepoStructure: GitHubViewRepoStructureQuerySchema,
  ghSearchRepos: GitHubReposSearchSingleQuerySchema,
  ghSearchPullRequests: SearchPullRequestsQuerySchema,
  ghSearchIssues: SearchIssuesQuerySchema,
  ghSearchCommits: SearchCommitsQuerySchema,
  ghListReleases: ListReleasesQuerySchema,
  ghSearchDiscussions: SearchDiscussionsQuerySchema,
  npmSearch: NpmPackageQuerySchema,
  ghCloneRepo: CloneRepoQuerySchema,
  localSearchCode: RipgrepQuerySchema,
  localViewStructure: ViewStructureQuerySchema,
  localFindFiles: FindFilesQuerySchema,
  localAnalyzeGraph: LocalAnalyzeGraphQuerySchema,
  localGetFileContent: FetchContentQuerySchema,
  lspGetSemantics: LspGetSemanticsQuerySchema,
} as const satisfies Record<ToolNames[keyof ToolNames], z.ZodTypeAny>;

/** Look up the Zod query schema for a canonical tool name. */
export function findToolSchema(name: string): z.ZodTypeAny | undefined {
  return (toolSchemas as Record<string, z.ZodTypeAny>)[name];
}

// ---------------------------------------------------------------------------
// Inferred TypeScript types for every query schema — the parsed (output)
// shape, with defaults applied. Use `z.input<typeof Schema>` if you need the
// pre-parse shape instead.
// ---------------------------------------------------------------------------
export type GitHubCodeSearchQuery = z.infer<typeof GitHubCodeSearchQuerySchema>;
export type FileContentQuery = z.infer<typeof FileContentQuerySchema>;
export type GitHubViewRepoStructureQuery = z.infer<
  typeof GitHubViewRepoStructureQuerySchema
>;
export type GitHubReposSearchSingleQuery = z.infer<
  typeof GitHubReposSearchSingleQuerySchema
>;
export type GitHubPullRequestSearchQuery = z.infer<
  typeof GitHubPullRequestSearchQuerySchema
>;
export type SearchPullRequestsQuery = z.infer<
  typeof SearchPullRequestsQuerySchema
>;
export type SearchIssuesQuery = z.infer<typeof SearchIssuesQuerySchema>;
export type SearchCommitsQuery = z.infer<typeof SearchCommitsQuerySchema>;
export type ListReleasesQuery = z.infer<typeof ListReleasesQuerySchema>;
export type SearchDiscussionsQuery = z.infer<
  typeof SearchDiscussionsQuerySchema
>;
export type NpmPackageQuery = z.infer<typeof NpmPackageQuerySchema>;
export type CloneRepoQuery = z.infer<typeof CloneRepoQuerySchema>;
export type RipgrepQuery = z.infer<typeof RipgrepQuerySchema>;
export type ViewStructureQuery = z.infer<typeof ViewStructureQuerySchema>;
export type FindFilesQuery = z.infer<typeof FindFilesQuerySchema>;
export type LocalAnalyzeGraphQuery = z.infer<
  typeof LocalAnalyzeGraphQuerySchema
>;
export type FetchContentQuery = z.infer<typeof FetchContentQuerySchema>;
export type LspGetSemanticsQuery = z.infer<typeof LspGetSemanticsQuerySchema>;
