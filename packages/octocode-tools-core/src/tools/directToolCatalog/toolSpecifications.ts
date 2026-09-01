/**
 * Engine-free source of truth for public direct-tool names and schemas.
 * Runtime execution metadata is attached separately by `toolConfig.ts`.
 */
import type { z } from 'zod';
import { PUBLIC_TOOL_DESCRIPTIONS } from '../../toolContract/descriptions.js';
import { LSP_GET_SEMANTICS_TOOL_NAME } from '../lsp/shared/semanticTypes.js';
import {
  GITHUB_SEARCH_TOOL_NAME,
  GITHUB_SEARCH_HISTORY_TOOL_NAME,
  GITHUB_GET_HISTORY_ITEM_TOOL_NAME,
  isDiscussionsEnabled,
  isReleasesEnabled,
  LOCAL_ANALYZE_GRAPH_TOOL_NAME,
  LOCAL_SEARCH_TOOL_NAME,
  STATIC_TOOL_NAMES,
} from '../toolNames.js';
import {
  BulkCloneRepoLocalSchema,
  BulkLspGetSemanticsQuerySchema,
  CloneRepoQueryLocalSchema,
  FileContentBulkQueryLocalSchema,
  FileContentQueryLocalSchema,
  GitHubSearchBulkQuerySchema,
  GitHubSearchQuerySchema,
  ListReleasesBulkLocalSchema,
  ListReleasesLocalSchema,
  LocalAnalyzeGraphBulkQuerySchema,
  LocalAnalyzeGraphQuerySchema,
  LocalFetchContentBulkQuerySchema,
  LocalFetchContentQuerySchema,
  LocalSearchBulkQuerySchema,
  LocalSearchQuerySchema,
  LspGetSemanticsQueryDisplaySchema,
  NpmSearchBulkQueryLocalSchema,
  NpmSearchQueryLocalSchema,
  GitHubGetHistoryItemBulkQueryLocalSchema,
  GitHubGetHistoryItemQueryLocalSchema,
  GitHubSearchHistoryBulkQueryLocalSchema,
  GitHubSearchHistoryQueryLocalSchema,
  SearchDiscussionsBulkLocalSchema,
  SearchDiscussionsLocalSchema,
} from '../toolSchemaImports.js';

interface DirectToolAvailability {
  envVar: 'ENABLE_RELEASES' | 'ENABLE_DISCUSSIONS';
  enabled: () => boolean;
}

export interface DirectToolSpecification {
  name: string;
  title: string;
  description: string;
  schema: z.ZodType;
  inputSchema: z.ZodType;
  availability?: DirectToolAvailability;
}

export const DIRECT_TOOL_SPECIFICATIONS: readonly DirectToolSpecification[] = [
  {
    name: GITHUB_SEARCH_TOOL_NAME,
    title: 'GitHub Search',
    description: PUBLIC_TOOL_DESCRIPTIONS.ghSearch,
    schema: GitHubSearchQuerySchema,
    inputSchema: GitHubSearchBulkQuerySchema,
  },
  {
    name: STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
    title: 'GitHub File Content Fetch',
    description: PUBLIC_TOOL_DESCRIPTIONS.ghGetFileContent,
    schema: FileContentQueryLocalSchema,
    inputSchema: FileContentBulkQueryLocalSchema,
  },
  {
    name: GITHUB_SEARCH_HISTORY_TOOL_NAME,
    title: 'GitHub History Search',
    description: PUBLIC_TOOL_DESCRIPTIONS.ghSearchHistory,
    schema: GitHubSearchHistoryQueryLocalSchema,
    inputSchema: GitHubSearchHistoryBulkQueryLocalSchema,
  },
  {
    name: GITHUB_GET_HISTORY_ITEM_TOOL_NAME,
    title: 'GitHub History Item',
    description: PUBLIC_TOOL_DESCRIPTIONS.ghGetHistoryItem,
    schema: GitHubGetHistoryItemQueryLocalSchema,
    inputSchema: GitHubGetHistoryItemBulkQueryLocalSchema,
  },
  {
    name: STATIC_TOOL_NAMES.GITHUB_RELEASES,
    title: 'GitHub Releases',
    description: PUBLIC_TOOL_DESCRIPTIONS.ghListReleases,
    schema: ListReleasesLocalSchema,
    inputSchema: ListReleasesBulkLocalSchema,
    availability: {
      envVar: 'ENABLE_RELEASES',
      enabled: isReleasesEnabled,
    },
  },
  {
    name: STATIC_TOOL_NAMES.GITHUB_DISCUSSIONS,
    title: 'GitHub Discussions Search',
    description: PUBLIC_TOOL_DESCRIPTIONS.ghSearchDiscussions,
    schema: SearchDiscussionsLocalSchema,
    inputSchema: SearchDiscussionsBulkLocalSchema,
    availability: {
      envVar: 'ENABLE_DISCUSSIONS',
      enabled: isDiscussionsEnabled,
    },
  },
  {
    name: STATIC_TOOL_NAMES.PACKAGE_SEARCH,
    title: 'Package Search',
    description: PUBLIC_TOOL_DESCRIPTIONS.npmSearch,
    schema: NpmSearchQueryLocalSchema,
    inputSchema: NpmSearchBulkQueryLocalSchema,
  },
  {
    name: STATIC_TOOL_NAMES.GITHUB_CLONE_REPO,
    title: 'Clone / Fetch GitHub Repository Locally',
    description: PUBLIC_TOOL_DESCRIPTIONS.ghCloneRepo,
    schema: CloneRepoQueryLocalSchema,
    inputSchema: BulkCloneRepoLocalSchema,
  },
  {
    name: LOCAL_SEARCH_TOOL_NAME,
    title: 'Local Search',
    description: PUBLIC_TOOL_DESCRIPTIONS.localSearch,
    schema: LocalSearchQuerySchema,
    inputSchema: LocalSearchBulkQuerySchema,
  },
  {
    name: LOCAL_ANALYZE_GRAPH_TOOL_NAME,
    title: 'Local Analyze Graph',
    description: PUBLIC_TOOL_DESCRIPTIONS.localAnalyzeGraph,
    schema: LocalAnalyzeGraphQuerySchema,
    inputSchema: LocalAnalyzeGraphBulkQuerySchema,
  },
  {
    name: STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
    title: 'Local Fetch Content',
    description: PUBLIC_TOOL_DESCRIPTIONS.localGetFileContent,
    schema: LocalFetchContentQuerySchema,
    inputSchema: LocalFetchContentBulkQuerySchema,
  },
  {
    name: LSP_GET_SEMANTICS_TOOL_NAME,
    title: 'Get Semantic Content',
    description: PUBLIC_TOOL_DESCRIPTIONS.lspGetSemantics,
    schema: LspGetSemanticsQueryDisplaySchema,
    inputSchema: BulkLspGetSemanticsQuerySchema,
  },
];

export function isDirectToolSpecificationEnabled(
  specification: DirectToolSpecification
): boolean {
  return specification.availability?.enabled() ?? true;
}
