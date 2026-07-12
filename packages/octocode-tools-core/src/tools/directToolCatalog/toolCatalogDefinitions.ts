/**
 * Engine-free direct-tool catalog: registry of tool definitions (name +
 * display/bulk zod schemas) plus category/sort/output-field helpers. Split out
 * of `directToolCatalog.meta.ts` (still the public barrel) — see that file's
 * header comment for the full P3 engine-free rationale.
 */
import { z } from 'zod';
import {
  isOqlEnabled,
  OQL_SEARCH_TOOL_NAME,
  STATIC_TOOL_NAMES,
} from '../toolNames.js';
import { LSP_GET_SEMANTICS_TOOL_NAME } from '../lsp/shared/semanticTypes.js';
import {
  CloneRepoQueryLocalSchema,
  BulkCloneRepoLocalSchema,
  FileContentQueryLocalSchema,
  FileContentBulkQueryLocalSchema,
  GitHubCodeSearchQueryLocalSchema,
  GitHubCodeSearchBulkQueryLocalSchema,
  GitHubPullRequestSearchQueryLocalSchema,
  GitHubPullRequestSearchBulkQueryLocalSchema,
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
  LocalRipgrepQuerySchema,
  LocalRipgrepBulkQuerySchema,
  LocalViewStructureQuerySchema,
  LocalViewStructureBulkQuerySchema,
  BulkLspGetSemanticsQuerySchema,
  LspGetSemanticsQueryDisplaySchema,
  OqlSearchQuerySchema,
  OqlSearchInputSchema,
} from '../toolSchemaImports.js';

export type DirectToolInput = Record<string, unknown> & {
  queries: unknown[];
};

export interface DirectToolDefinition {
  name: string;

  schema: z.ZodType;

  inputSchema: z.ZodType;
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
    STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
    STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
    STATIC_TOOL_NAMES.GITHUB_CLONE_REPO,
    STATIC_TOOL_NAMES.LOCAL_RIPGREP,
    STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
    STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
    STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
    LSP_GET_SEMANTICS_TOOL_NAME,
    STATIC_TOOL_NAMES.PACKAGE_SEARCH,
    ...(isOqlEnabled() ? [OQL_SEARCH_TOOL_NAME] : []),
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

export interface DirectToolOutputField {
  name: string;
  type: string;
  optional?: boolean;
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

const DIRECT_TOOL_BASE_AUTO_FILLED_FIELDS: readonly DirectToolAutoFilledField[] =
  ['id', 'researchGoal', 'reasoning'];

const DIRECT_TOOL_OUTPUT_FIELDS: readonly DirectToolOutputField[] = [
  {
    name: 'content',
    type: 'Array<{ type: string; text: string }>',
  },
  {
    name: 'structuredContent',
    type: 'object',
    optional: true,
  },
  {
    name: 'isError',
    type: 'boolean',
    optional: true,
  },
];

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
    name: STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    schema: GitHubPullRequestSearchQueryLocalSchema,
    inputSchema: GitHubPullRequestSearchBulkQueryLocalSchema,
  },
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
    name: STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
    schema: LocalFetchContentQuerySchema,
    inputSchema: LocalFetchContentBulkQuerySchema,
  },
  {
    name: LSP_GET_SEMANTICS_TOOL_NAME,
    schema: LspGetSemanticsQueryDisplaySchema,
    inputSchema: BulkLspGetSemanticsQuerySchema,
  },
  ...(isOqlEnabled()
    ? [
        {
          name: OQL_SEARCH_TOOL_NAME,
          schema: OqlSearchQuerySchema,
          inputSchema: OqlSearchInputSchema,
        },
      ]
    : []),
];

export function findDirectToolDefinition(
  name: string
): DirectToolDefinition | undefined {
  return DIRECT_TOOL_DEFINITIONS.find(tool => tool.name === name);
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

export function formatDirectToolSchemaText(toolName: string): string {
  const tool = findDirectToolDefinition(toolName);
  if (!tool) {
    return '{}';
  }

  try {
    return JSON.stringify(
      z.toJSONSchema(tool.inputSchema, { io: 'input' }),
      null,
      2
    );
  } catch {
    return JSON.stringify(
      z.toJSONSchema(tool.schema, { io: 'input' }),
      null,
      2
    );
  }
}

export function formatDirectToolMetadataSchemaText(
  schema: Record<string, string> | undefined
): string {
  return JSON.stringify(schema ?? {}, null, 2);
}

export function getDirectToolAutoFilledFields(toolName: string): string[] {
  const category = getDirectToolCategory(toolName);
  const fields = [...DIRECT_TOOL_BASE_AUTO_FILLED_FIELDS];

  if (category === 'GitHub' || category === 'Package') {
    fields.splice(1, 0, 'mainResearchGoal');
  }

  return fields;
}

export function getDirectToolOutputFields(): DirectToolOutputField[] {
  return DIRECT_TOOL_OUTPUT_FIELDS.map(field => ({ ...field }));
}

export function formatDirectToolOutputSchemaText(): string {
  return JSON.stringify(
    Object.fromEntries(
      DIRECT_TOOL_OUTPUT_FIELDS.map(field => [
        field.name,
        field.optional ? `${field.type} (optional)` : field.type,
      ])
    ),
    null,
    2
  );
}

export function getDirectToolDescription(
  toolName: string,
  metadata?: DirectToolMetadata | null
): string {
  return metadata?.tools?.[toolName]?.description ?? toolName;
}
