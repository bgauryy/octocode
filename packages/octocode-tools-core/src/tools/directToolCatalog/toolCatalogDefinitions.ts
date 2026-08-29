/**
 * Engine-free direct-tool catalog: registry of tool definitions (name +
 * display/bulk zod schemas) plus category/sort/output-field helpers. Split out
 * of `directToolCatalog.meta.ts` (still the public barrel) — see that file's
 * header comment for the full P3 engine-free rationale.
 */
import { z } from 'zod';
import {
  isReleasesEnabled,
  isDiscussionsEnabled,
  STATIC_TOOL_NAMES,
  LOCAL_ANALYZE_GRAPH_TOOL_NAME,
} from '../toolNames.js';
import { LSP_GET_SEMANTICS_TOOL_NAME } from '../lsp/shared/semanticTypes.js';
import {
  CloneRepoQueryLocalSchema,
  BulkCloneRepoLocalSchema,
  FileContentQueryLocalSchema,
  FileContentBulkQueryLocalSchema,
  GitHubCodeSearchQueryLocalSchema,
  GitHubCodeSearchBulkQueryLocalSchema,
  SearchPullRequestsLocalSchema,
  SearchPullRequestsBulkLocalSchema,
  SearchIssuesLocalSchema,
  SearchIssuesBulkLocalSchema,
  SearchCommitsLocalSchema,
  SearchCommitsBulkLocalSchema,
  ListReleasesLocalSchema,
  ListReleasesBulkLocalSchema,
  SearchDiscussionsLocalSchema,
  SearchDiscussionsBulkLocalSchema,
  GitHubReposSearchSingleQueryLocalSchema,
  GitHubReposSearchBulkQueryLocalSchema,
  GitHubViewRepoStructureQueryLocalSchema,
  GitHubViewRepoStructureBulkQueryLocalSchema,
  NpmSearchQueryLocalSchema,
  NpmSearchBulkQueryLocalSchema,
  LocalFetchContentQuerySchema,
  LocalFetchContentBulkQuerySchema,
  LocalFindFilesQuerySchema,
  LocalFindFilesBulkQuerySchema,
  LocalAnalyzeGraphQuerySchema,
  LocalAnalyzeGraphBulkQuerySchema,
  LocalRipgrepQuerySchema,
  LocalRipgrepBulkQuerySchema,
  LocalViewStructureQuerySchema,
  LocalViewStructureBulkQuerySchema,
  BulkLspGetSemanticsQuerySchema,
  LspGetSemanticsQueryDisplaySchema,
} from '../toolSchemaImports.js';

export type DirectToolInput = Record<string, unknown> & {
  queries: unknown[];
};

export interface DirectToolDefinition {
  name: string;

  schema: z.ZodType;

  inputSchema: z.ZodType;

  /**
   * Present only for opt-in tools (ghListReleases/ghSearchDiscussions) that
   * are NOT currently enabled. Their entry still appears here (schema/help
   * stay discoverable via `tools <name> --scheme` without already knowing the
   * env var) — the runtime execution registry (`ALL_TOOLS` in toolConfig.ts)
   * is the actual enforcement point and still omits them until enabled.
   */
  disabled?: { envVar: string };
}

export type DirectToolCategory = 'GitHub' | 'Local Code' | 'Package' | 'Other';

export const DIRECT_TOOL_CATEGORIES: readonly DirectToolCategory[] = [
  'GitHub',
  'Local Code',
  'Package',
  'Other',
];
const DIRECT_TOOL_RELEVANCE_ORDER = new Map<string, number>(
  [
    STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
    STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
    STATIC_TOOL_NAMES.GITHUB_PULL_REQUESTS,
    STATIC_TOOL_NAMES.GITHUB_ISSUES,
    STATIC_TOOL_NAMES.GITHUB_COMMITS,
    STATIC_TOOL_NAMES.GITHUB_RELEASES,
    STATIC_TOOL_NAMES.GITHUB_DISCUSSIONS,
    STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
    STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
    STATIC_TOOL_NAMES.GITHUB_CLONE_REPO,
    STATIC_TOOL_NAMES.LOCAL_RIPGREP,
    STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
    LOCAL_ANALYZE_GRAPH_TOOL_NAME,
    STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
    STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
    LSP_GET_SEMANTICS_TOOL_NAME,
    STATIC_TOOL_NAMES.PACKAGE_SEARCH,
  ].map((name, index) => [name, index])
);
export interface DirectToolDisplayField {
  name: string;
  required: boolean;
  type: string;
  /** Numeric bounds and default, e.g. "1-100, default 30" — surfaced inline so
   * agents see the full constraint without fetching the raw JSON schema. */
  constraints?: string;
  description?: string;
}

export interface DirectToolCommandPattern {
  label: string;
  query: Record<string, unknown>;
  command: string;
}

export interface DirectToolMetadata {
  tools?: Record<
    string,
    { description?: string; schema?: Record<string, string> }
  >;
}

export type DirectToolAutoFilledField =
  'id' | 'mainResearchGoal' | 'researchGoal' | 'reasoning';

export interface PrepareDirectToolInputOptions {
  sourceLabel?: string;
  rejectUnknownFields?: boolean;

  onUnknownFields?: (unknownFields: string[], queryIndex: number) => void;
}

export class DirectToolInputError extends Error {
  constructor(
    message: string,
    readonly details: string[] = []
  ) {
    super(message);
    this.name = 'DirectToolInputError';
  }
}

const DIRECT_TOOL_AUTO_FILLED_FIELD_NAMES: readonly DirectToolAutoFilledField[] =
  ['id', 'mainResearchGoal', 'researchGoal', 'reasoning'];

export const DIRECT_TOOL_AUTO_FILLED_FIELDS: ReadonlySet<string> = new Set([
  ...DIRECT_TOOL_AUTO_FILLED_FIELD_NAMES,
]);

/**
 * Engine-free tool definitions (name + display/bulk schema). Order mirrors
 * `ALL_TOOLS` in `toolConfig.ts`; each schema is the SAME object that
 * `toolConfig` attaches an executionFn to. Kept in lockstep by a drift test.
 */
export const DIRECT_TOOL_DEFINITIONS: DirectToolDefinition[] = [
  {
    name: STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
    schema: GitHubCodeSearchQueryLocalSchema,
    inputSchema: GitHubCodeSearchBulkQueryLocalSchema,
  },
  {
    name: STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
    schema: FileContentQueryLocalSchema,
    inputSchema: FileContentBulkQueryLocalSchema,
  },
  {
    name: STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
    schema: GitHubViewRepoStructureQueryLocalSchema,
    inputSchema: GitHubViewRepoStructureBulkQueryLocalSchema,
  },
  {
    name: STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
    schema: GitHubReposSearchSingleQueryLocalSchema,
    inputSchema: GitHubReposSearchBulkQueryLocalSchema,
  },
  {
    name: STATIC_TOOL_NAMES.GITHUB_PULL_REQUESTS,
    schema: SearchPullRequestsLocalSchema,
    inputSchema: SearchPullRequestsBulkLocalSchema,
  },
  {
    name: STATIC_TOOL_NAMES.GITHUB_ISSUES,
    schema: SearchIssuesLocalSchema,
    inputSchema: SearchIssuesBulkLocalSchema,
  },
  {
    name: STATIC_TOOL_NAMES.GITHUB_COMMITS,
    schema: SearchCommitsLocalSchema,
    inputSchema: SearchCommitsBulkLocalSchema,
  },
  // ghListReleases is opt-in (ENABLE_RELEASES=1) — gated to match ALL_TOOLS.
  // CLI discovery combines this runtime list with the disabled definitions
  // below and marks availability explicitly.
  ...(isReleasesEnabled()
    ? [
        {
          name: STATIC_TOOL_NAMES.GITHUB_RELEASES,
          schema: ListReleasesLocalSchema,
          inputSchema: ListReleasesBulkLocalSchema,
        },
      ]
    : []),
  // ghSearchDiscussions is opt-in (ENABLE_DISCUSSIONS=1) — gated to match ALL_TOOLS.
  ...(isDiscussionsEnabled()
    ? [
        {
          name: STATIC_TOOL_NAMES.GITHUB_DISCUSSIONS,
          schema: SearchDiscussionsLocalSchema,
          inputSchema: SearchDiscussionsBulkLocalSchema,
        },
      ]
    : []),
  {
    name: STATIC_TOOL_NAMES.PACKAGE_SEARCH,
    schema: NpmSearchQueryLocalSchema,
    inputSchema: NpmSearchBulkQueryLocalSchema,
  },
  {
    name: STATIC_TOOL_NAMES.GITHUB_CLONE_REPO,
    schema: CloneRepoQueryLocalSchema,
    inputSchema: BulkCloneRepoLocalSchema,
  },
  {
    name: STATIC_TOOL_NAMES.LOCAL_RIPGREP,
    schema: LocalRipgrepQuerySchema,
    inputSchema: LocalRipgrepBulkQuerySchema,
  },
  {
    name: STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
    schema: LocalViewStructureQuerySchema,
    inputSchema: LocalViewStructureBulkQuerySchema,
  },
  {
    name: STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
    schema: LocalFindFilesQuerySchema,
    inputSchema: LocalFindFilesBulkQuerySchema,
  },
  {
    name: LOCAL_ANALYZE_GRAPH_TOOL_NAME,
    schema: LocalAnalyzeGraphQuerySchema,
    inputSchema: LocalAnalyzeGraphBulkQuerySchema,
  },
  {
    name: STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
    schema: LocalFetchContentQuerySchema,
    inputSchema: LocalFetchContentBulkQuerySchema,
  },
  {
    name: LSP_GET_SEMANTICS_TOOL_NAME,
    schema: LspGetSemanticsQueryDisplaySchema,
    inputSchema: BulkLspGetSemanticsQuerySchema,
  },
];

// Opt-in tools when NOT enabled, kept OUT of the executable-definition list
// but included in DIRECT_TOOL_DISCOVERY_DEFINITIONS with an explicit gate.
const DISABLED_TOOL_DEFINITIONS: DirectToolDefinition[] = [
  ...(isReleasesEnabled()
    ? []
    : [
        {
          name: STATIC_TOOL_NAMES.GITHUB_RELEASES,
          schema: ListReleasesLocalSchema,
          inputSchema: ListReleasesBulkLocalSchema,
          disabled: { envVar: 'ENABLE_RELEASES' },
        },
      ]),
  ...(isDiscussionsEnabled()
    ? []
    : [
        {
          name: STATIC_TOOL_NAMES.GITHUB_DISCUSSIONS,
          schema: SearchDiscussionsLocalSchema,
          inputSchema: SearchDiscussionsBulkLocalSchema,
          disabled: { envVar: 'ENABLE_DISCUSSIONS' },
        },
      ]),
];

/** Every public direct-tool schema, including opt-in tools marked disabled. */
export const DIRECT_TOOL_DISCOVERY_DEFINITIONS: DirectToolDefinition[] = [
  ...DIRECT_TOOL_DEFINITIONS,
  ...DISABLED_TOOL_DEFINITIONS,
];

export function findDirectToolDefinition(
  name: string
): DirectToolDefinition | undefined {
  return (
    DIRECT_TOOL_DEFINITIONS.find(tool => tool.name === name) ??
    DISABLED_TOOL_DEFINITIONS.find(tool => tool.name === name)
  );
}

export function getDirectToolCategory(toolName: string): DirectToolCategory {
  if (toolName.startsWith('gh')) {
    return 'GitHub';
  }

  if (toolName.startsWith('local') || toolName.startsWith('lsp')) {
    return 'Local Code';
  }

  if (toolName === STATIC_TOOL_NAMES.PACKAGE_SEARCH) {
    return 'Package';
  }

  return 'Other';
}

export function sortDirectToolNames(toolNames: string[]): string[] {
  return [...toolNames].sort((left, right) => {
    const leftCategory = DIRECT_TOOL_CATEGORIES.indexOf(
      getDirectToolCategory(left)
    );
    const rightCategory = DIRECT_TOOL_CATEGORIES.indexOf(
      getDirectToolCategory(right)
    );

    if (leftCategory !== rightCategory) {
      return leftCategory - rightCategory;
    }

    const leftRank =
      DIRECT_TOOL_RELEVANCE_ORDER.get(left) ?? Number.MAX_SAFE_INTEGER;
    const rightRank =
      DIRECT_TOOL_RELEVANCE_ORDER.get(right) ?? Number.MAX_SAFE_INTEGER;

    if (leftRank !== rightRank) {
      return leftRank - rightRank;
    }

    return left.localeCompare(right);
  });
}
