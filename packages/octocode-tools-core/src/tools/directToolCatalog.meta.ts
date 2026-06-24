/**
 * Engine-free direct-tool catalog metadata (P3).
 *
 * This module holds EVERYTHING the schema/help/`--scheme`/`context` paths need
 * — tool definitions (name + display/bulk zod schemas), schema-text formatters,
 * display-field extraction, example builders, and input preparation — WITHOUT
 * importing `@octocodeai/octocode-engine` (no native `.node` load at module
 * eval). The schemas are sourced from each tool's engine-free `scheme.ts`, the
 * same modules `toolConfig.ts` consumes, so the two cannot drift on shape (a
 * drift test asserts name/schema parity against the runtime `ALL_TOOLS`).
 *
 * The execution path (`executeDirectTool`) lives in `directToolCatalog.exec.ts`,
 * which DOES import the engine; it is only reached when a tool actually runs.
 * The `@octocodeai/octocode-tools-core/schema` subpath re-exports only this
 * module so engine-less runtimes (e.g. Codex.app Node) can read schemas.
 */
import { z } from 'zod';
import { OQL_SEARCH_TOOL_NAME, STATIC_TOOL_NAMES } from './toolNames.js';
import { LSP_GET_SEMANTIC_CONTENT_TOOL_NAME } from './lsp/shared/semanticTypes.js';
import {
  CloneRepoQueryLocalSchema,
  BulkCloneRepoLocalSchema,
} from './github_clone_repo/scheme.js';
import {
  FileContentQueryLocalSchema,
  FileContentBulkQueryLocalSchema,
} from './github_fetch_content/scheme.js';
import {
  GitHubCodeSearchQueryLocalSchema,
  GitHubCodeSearchBulkQueryLocalSchema,
} from './github_search_code/scheme.js';
import {
  GitHubPullRequestSearchQueryLocalSchema,
  GitHubPullRequestSearchBulkQueryLocalSchema,
} from './github_search_pull_requests/scheme.js';
import {
  GitHubReposSearchSingleQueryLocalSchema,
  GitHubReposSearchBulkQueryLocalSchema,
} from './github_search_repos/scheme.js';
import {
  GitHubViewRepoStructureQueryLocalSchema,
  GitHubViewRepoStructureBulkQueryLocalSchema,
} from './github_view_repo_structure/scheme.js';
import {
  NpmSearchQueryLocalSchema,
  NpmSearchBulkQueryLocalSchema,
} from './package_search/scheme.js';
import {
  LocalFetchContentQuerySchema,
  LocalFetchContentBulkQuerySchema,
} from './local_fetch_content/scheme.js';
import {
  LocalFindFilesQuerySchema,
  LocalFindFilesBulkQuerySchema,
} from './local_find_files/scheme.js';
import {
  LocalRipgrepQuerySchema,
  LocalRipgrepBulkQuerySchema,
} from './local_ripgrep/scheme.js';
import {
  LocalViewStructureQuerySchema,
  LocalViewStructureBulkQuerySchema,
} from './local_view_structure/scheme.js';
import {
  BulkLspGetSemanticsQuerySchema,
  LspGetSemanticsQueryDisplaySchema,
} from './lsp/semantic_content/scheme.js';
import {
  LocalBinaryInspectQuerySchema,
  LocalBinaryInspectBulkQuerySchema,
} from './local_binary_inspect/scheme.js';
import {
  OqlDisplayQuerySchema as OqlSearchQuerySchema,
  OqlSearchInputSchema,
} from '../oql/schema.js';

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
    LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
    STATIC_TOOL_NAMES.PACKAGE_SEARCH,
    OQL_SEARCH_TOOL_NAME,
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

type DirectToolAutoFilledField =
  | 'id'
  | 'mainResearchGoal'
  | 'researchGoal'
  | 'reasoning';

export interface PrepareDirectToolInputOptions {
  sourceLabel?: string;

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

interface JsonSchemaObject extends Record<string, unknown> {
  type?: string | string[];
  description?: string;
  enum?: unknown[];
  required?: string[];
  properties?: Record<string, unknown>;
  items?: unknown;
}

const DIRECT_TOOL_AUTO_FILLED_FIELD_NAMES: readonly DirectToolAutoFilledField[] =
  ['id', 'mainResearchGoal', 'researchGoal', 'reasoning'];

const DIRECT_TOOL_AUTO_FILLED_FIELDS: ReadonlySet<string> = new Set([
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
    name: LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
    schema: LspGetSemanticsQueryDisplaySchema,
    inputSchema: BulkLspGetSemanticsQuerySchema,
  },
  {
    name: STATIC_TOOL_NAMES.LOCAL_BINARY_INSPECT,
    schema: LocalBinaryInspectQuerySchema,
    inputSchema: LocalBinaryInspectBulkQuerySchema,
  },
  {
    name: OQL_SEARCH_TOOL_NAME,
    schema: OqlSearchQuerySchema,
    inputSchema: OqlSearchInputSchema,
  },
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
    return JSON.stringify(z.toJSONSchema(tool.inputSchema), null, 2);
  } catch {
    return JSON.stringify(z.toJSONSchema(tool.schema), null, 2);
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

export function getDirectToolDisplayFields(
  toolName: string
): DirectToolDisplayField[] {
  const tool = findDirectToolDefinition(toolName);
  if (!tool) {
    return [];
  }

  const jsonSchema = z.toJSONSchema(tool.schema);
  if (!isJsonSchemaObject(jsonSchema)) {
    return [];
  }

  const properties = isRecord(jsonSchema.properties)
    ? jsonSchema.properties
    : {};

  const requiredFields = new Set(
    Array.isArray(jsonSchema.required)
      ? jsonSchema.required.filter(
          name =>
            !DIRECT_TOOL_AUTO_FILLED_FIELDS.has(name) &&
            !hasSchemaDefault(properties[name])
        )
      : []
  );

  return collectDisplayFields(properties, requiredFields);
}

export function formatDirectToolCommandPattern(
  toolName: string,
  query: Record<string, unknown>
): string {
  return `tools ${toolName} --queries '${JSON.stringify(query)}'`;
}

export function buildDirectToolCommandPatterns(
  toolName: string
): DirectToolCommandPattern[] {
  const knownPatterns = buildKnownDirectToolCommandPatternQueries(toolName);
  if (knownPatterns.length > 0) {
    return knownPatterns.map(pattern => ({
      ...pattern,
      command: formatDirectToolCommandPattern(toolName, pattern.query),
    }));
  }

  const query = buildSchemaDerivedExampleQuery(toolName);
  if (Object.keys(query).length === 0) {
    return [];
  }

  return [
    {
      label: 'schema-derived',
      query,
      command: formatDirectToolCommandPattern(toolName, query),
    },
  ];
}

export function buildDirectToolExampleQuery(
  toolName: string
): Record<string, unknown> {
  return buildDirectToolCommandPatterns(toolName)[0]?.query ?? {};
}

function buildSchemaDerivedExampleQuery(
  toolName: string
): Record<string, unknown> {
  if (!findDirectToolDefinition(toolName)) {
    return {};
  }

  const fields = getDirectToolDisplayFields(toolName);
  const topLevelFields = fields.filter(field => !field.name.includes('.'));
  const sourceFields = selectCommandPatternFields(topLevelFields);
  const example: Record<string, unknown> = {};

  for (const field of sourceFields) {
    example[field.name] = buildExampleValue(field.name, field.type);
  }

  if (
    toolName.startsWith('lsp') &&
    fields.some(field => field.name === 'uri')
  ) {
    example.uri ??= 'uri';
  }

  if (toolName === LSP_GET_SEMANTIC_CONTENT_TOOL_NAME) {
    example.type ??= 'definition';
    example.symbolName ??= 'symbolName';
    example.lineHint ??= 1;
  }

  return example;
}

function buildKnownDirectToolCommandPatternQueries(
  toolName: string
): Array<{ label: string; query: Record<string, unknown> }> {
  if (toolName === OQL_SEARCH_TOOL_NAME) {
    return [
      {
        label: 'local code query',
        query: {
          schema: 'oql',
          target: 'code',
          from: { kind: 'local', path: '.' },
          where: { kind: 'text', value: 'executeDirectTool' },
          view: 'discovery',
          limit: 5,
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS) {
    return [
      {
        label: 'PR search',
        query: {
          type: 'prs',
          owner: 'facebook',
          repo: 'react',
          keywordsToSearch: ['useState'],
          concise: true,
          limit: 5,
        },
      },
      {
        label: 'commit history',
        query: {
          type: 'commits',
          owner: 'facebook',
          repo: 'react',
          path: 'packages/react/src',
          since: '2024-01-01T00:00:00Z',
          perPage: 5,
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE) {
    return [
      {
        label: 'path search',
        query: {
          keywords: ['package.json'],
          owner: 'facebook',
          repo: 'react',
          match: 'path',
          concise: true,
          limit: 5,
        },
      },
      {
        label: 'content search',
        query: {
          keywords: ['useState'],
          owner: 'facebook',
          repo: 'react',
          extension: 'js',
          limit: 5,
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES) {
    return [
      {
        label: 'repository search',
        query: {
          keywords: ['react'],
          language: 'TypeScript',
          stars: '>1000',
          concise: true,
          limit: 5,
        },
      },
      {
        label: 'owner repositories',
        query: {
          owner: 'facebook',
          concise: true,
          limit: 5,
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE) {
    return [
      {
        label: 'repo tree',
        query: {
          owner: 'facebook',
          repo: 'react',
          path: 'packages',
          maxDepth: 2,
          itemsPerPage: 50,
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.GITHUB_CLONE_REPO) {
    return [
      {
        label: 'full repo clone',
        query: {
          owner: 'bgauryy',
          repo: 'octocode',
        },
      },
      {
        label: 'subtree clone',
        query: {
          owner: 'facebook',
          repo: 'react',
          sparsePath: 'packages/react',
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.LOCAL_RIPGREP) {
    return [
      {
        label: 'text search',
        query: {
          path: '.',
          keywords: 'runCLI',
        },
      },
      {
        label: 'structural code search',
        query: {
          path: 'src',
          mode: 'structural',
          pattern: 'eval($X)',
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT) {
    return [
      {
        label: 'exact line range',
        query: {
          path: 'src/index.ts',
          startLine: 1,
          endLine: 40,
          minify: 'none',
        },
      },
      {
        label: 'matched slice',
        query: {
          path: 'src/index.ts',
          matchString: 'registerTool',
          contextLines: 8,
          minify: 'standard',
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.LOCAL_FIND_FILES) {
    return [
      {
        label: 'basename globs',
        query: {
          path: '.',
          names: ['*.ts', 'package.json'],
          entryType: 'f',
          itemsPerPage: 20,
        },
      },
      {
        label: 'monorepo path glob',
        query: {
          path: '.',
          pathPattern: 'packages/*/src/**',
          entryType: 'f',
          itemsPerPage: 20,
        },
      },
    ];
  }

  if (toolName === STATIC_TOOL_NAMES.LOCAL_BINARY_INSPECT) {
    return [
      {
        label: 'archive listing',
        query: {
          path: 'archive.zip',
          mode: 'list',
          entriesPerPage: 50,
        },
      },
      {
        label: 'binary strings',
        query: {
          path: 'dist/app.node',
          mode: 'strings',
          minLength: 8,
          charLength: 2000,
        },
      },
    ];
  }

  if (toolName === LSP_GET_SEMANTIC_CONTENT_TOOL_NAME) {
    return [
      {
        label: 'semantic definition',
        query: {
          uri: '/path/to/file.ts',
          type: 'definition',
          symbolName: 'myFunction',
          lineHint: 42,
        },
      },
      {
        label: 'symbol outline',
        query: {
          uri: '/path/to/file.ts',
          type: 'documentSymbols',
        },
      },
    ];
  }

  return [];
}

export function prepareDirectToolInputFromJsonText(
  toolName: string,
  inputText: string | undefined,
  options: PrepareDirectToolInputOptions = {}
): DirectToolInput | null {
  if (typeof inputText !== 'string') {
    return null;
  }

  let rawPayload: unknown;
  try {
    rawPayload = JSON.parse(inputText) as unknown;
  } catch {
    throw new DirectToolInputError('Tool input must be valid JSON.');
  }

  return prepareDirectToolInput(toolName, rawPayload, options);
}

export function prepareDirectToolInput(
  toolName: string,
  rawPayload: unknown,
  options: PrepareDirectToolInputOptions = {}
): DirectToolInput {
  const payload = buildDirectToolPayload(toolName, rawPayload, options);
  const tool = findDirectToolDefinition(toolName);
  if (!tool) {
    throw new DirectToolInputError(`Unknown tool: ${toolName}`);
  }

  const result = tool.inputSchema.safeParse(payload);
  if (!result.success) {
    throw new DirectToolInputError(
      'Tool input does not match the expected schema.',
      formatDirectToolValidationIssues(result.error)
    );
  }

  return result.data as DirectToolInput;
}

export function formatDirectToolValidationIssues(error: z.ZodError): string[] {
  return error.issues.map(issue => {
    const path = issue.path.length > 0 ? issue.path.join('.') : 'input';
    return `${path}: ${issue.message}`;
  });
}

function buildDirectToolPayload(
  toolName: string,
  rawPayload: unknown,
  options: PrepareDirectToolInputOptions
): DirectToolInput {
  let queriesInput: unknown[] = [];

  if (Array.isArray(rawPayload)) {
    queriesInput = rawPayload;
  } else if (isRecord(rawPayload) && Array.isArray(rawPayload.queries)) {
    queriesInput = rawPayload.queries;
  } else if (isRecord(rawPayload)) {
    queriesInput = [rawPayload];
  } else {
    throw new DirectToolInputError(
      'Tool input must be a JSON object, an array of query objects, or { "queries": [...] }.'
    );
  }

  if (queriesInput.length === 0) {
    throw new DirectToolInputError('At least one query is required.');
  }

  const envelopeFields =
    isRecord(rawPayload) && Array.isArray(rawPayload.queries)
      ? Object.fromEntries(
          Object.entries(rawPayload).filter(([key]) => key !== 'queries')
        )
      : {};

  let hadUnknownFields = false;
  const wrappedOptions: PrepareDirectToolInputOptions = {
    ...options,
    onUnknownFields: (fields, index) => {
      hadUnknownFields = true;
      options.onUnknownFields?.(fields, index);
    },
  };
  const processedQueries = queriesInput.map((query, index) =>
    applyDefaultQueryFields(
      toolName,
      index,
      normalizeQueryObject(toolName, query, index, wrappedOptions),
      { sourceLabel: options.sourceLabel }
    )
  );
  if (hadUnknownFields && options.onUnknownFields !== undefined) {
    throw new DirectToolInputError(
      'Tool input contains unknown fields. See warnings above for details.'
    );
  }
  return { ...envelopeFields, queries: processedQueries };
}

function applyDefaultQueryFields(
  toolName: string,
  index: number,
  query: Record<string, unknown>,
  options: Pick<PrepareDirectToolInputOptions, 'sourceLabel'>
): Record<string, unknown> {
  const nextQuery = { ...query };
  const category = getDirectToolCategory(toolName);
  const sourceLabel = options.sourceLabel ?? 'direct tool execution';
  const defaultGoal = buildDefaultGoal(toolName, sourceLabel);

  if (typeof nextQuery.id !== 'string' || nextQuery.id.trim().length === 0) {
    nextQuery.id = `${toolName}-${index + 1}`;
  }

  if (category === 'GitHub' || category === 'Package') {
    if (
      typeof nextQuery.mainResearchGoal !== 'string' ||
      nextQuery.mainResearchGoal.trim().length === 0
    ) {
      nextQuery.mainResearchGoal = defaultGoal;
    }
  }

  if (
    typeof nextQuery.researchGoal !== 'string' ||
    nextQuery.researchGoal.trim().length === 0
  ) {
    nextQuery.researchGoal = defaultGoal;
  }

  if (
    typeof nextQuery.reasoning !== 'string' ||
    nextQuery.reasoning.trim().length === 0
  ) {
    nextQuery.reasoning = `Executed via ${sourceLabel} tool command`;
  }

  return nextQuery;
}

function buildDefaultGoal(toolName: string, sourceLabel: string): string {
  return `Execute ${toolName} via ${sourceLabel}`;
}

function normalizeQueryObject(
  toolName: string,
  query: unknown,
  queryIndex: number,
  options: Pick<PrepareDirectToolInputOptions, 'onUnknownFields'> = {}
): Record<string, unknown> {
  if (!isRecord(query)) {
    throw new DirectToolInputError(
      'Tool input must be a JSON object or an array of objects.'
    );
  }

  const schemaFields = new Set([
    ...getDirectToolDisplayFields(toolName)
      .filter(field => !field.name.includes('.'))
      .map(field => field.name),
    ...DIRECT_TOOL_AUTO_FILLED_FIELDS,
  ]);
  const exactQuery: Record<string, unknown> = {};
  const unknownFields: string[] = [];
  for (const [key, value] of Object.entries(query)) {
    if (schemaFields.has(key)) {
      exactQuery[key] = value;
      continue;
    }
    // Drop unknown fields (legacy/removed/typo) so the schema never hard-fails
    // the call; the agent is still warned via onUnknownFields below.
    unknownFields.push(key);
  }

  if (unknownFields.length > 0 && schemaFields.size > 0) {
    options.onUnknownFields?.(unknownFields, queryIndex);
  }

  return exactQuery;
}

function describeSchemaConstraints(
  schema: JsonSchemaObject
): string | undefined {
  const parts: string[] = [];
  const min = typeof schema.minimum === 'number' ? schema.minimum : undefined;
  const max = typeof schema.maximum === 'number' ? schema.maximum : undefined;
  if (min !== undefined && max !== undefined) parts.push(`${min}-${max}`);
  else if (min !== undefined) parts.push(`>=${min}`);
  else if (max !== undefined) parts.push(`<=${max}`);
  if ('default' in schema)
    parts.push(`default ${JSON.stringify(schema.default)}`);
  return parts.length > 0 ? parts.join(', ') : undefined;
}

function describeSchemaType(schema: JsonSchemaObject): string {
  if (Array.isArray(schema.enum) && schema.enum.length > 0) {
    return `enum(${schema.enum.map(String).join(', ')})`;
  }

  if (schema.type === 'array') {
    const items = isJsonSchemaObject(schema.items) ? schema.items : undefined;
    return `array<${items ? describeSchemaType(items) : 'value'}>`;
  }

  // Unions (z.union → anyOf, z.discriminatedUnion → oneOf) carry no top-level
  // `type`, which would otherwise fall through to the opaque "value". Render the
  // member types instead, e.g. `string | array<string>`.
  const union = Array.isArray(schema.anyOf)
    ? schema.anyOf
    : Array.isArray(schema.oneOf)
      ? schema.oneOf
      : undefined;
  if (union) {
    const members = union
      .filter(isJsonSchemaObject)
      .map(describeSchemaType)
      .filter(t => t !== 'value');
    if (members.length > 0) return [...new Set(members)].join(' | ');
  }

  if (Array.isArray(schema.type)) {
    return schema.type.join(' | ');
  }

  if (typeof schema.type === 'string') {
    return schema.type;
  }

  return 'value';
}

const COMMAND_PATTERN_MAX_OPTIONAL_FIELDS = 4;

const COMMAND_PATTERN_FIELD_PRIORITY: ReadonlyMap<string, number> = new Map([
  ['keywords', 10],
  ['keywordsToSearch', 11],
  ['query', 12],
  ['text', 13],
  ['packageName', 14],
  ['name', 15],
  ['uri', 20],
  ['type', 21],
  ['owner', 30],
  ['repo', 31],
  ['extension', 32],
  ['filename', 33],
  ['language', 34],
  ['path', 40],
  ['target', 50],
  ['from', 51],
  ['scope', 52],
  ['pattern', 60],
  ['rule', 61],
  ['op', 80],
  ['operation', 81],
  ['minify', 90],
]);

const LOW_SIGNAL_COMMAND_PATTERN_FIELDS: ReadonlySet<string> = new Set([
  'page',
  'itemsPerPage',
  'limit',
  'matchPage',
  'maxFiles',
  'maxMatchesPerFile',
  'matchContentLength',
  'responseCharLength',
  'responseCharOffset',
]);

function selectCommandPatternFields(
  fields: readonly DirectToolDisplayField[]
): DirectToolDisplayField[] {
  const requiredFields = fields.filter(field => field.required);
  const selected = new Map<string, DirectToolDisplayField>();

  for (const field of requiredFields) {
    selected.set(field.name, field);
  }

  const optionalCandidates = fields
    .filter(field => !field.required && isUsefulCommandPatternField(field))
    .sort(compareCommandPatternFields);
  const optionalLimit = Math.max(
    COMMAND_PATTERN_MAX_OPTIONAL_FIELDS,
    requiredFields.length
  );

  for (const field of optionalCandidates) {
    if (selected.size >= optionalLimit) {
      break;
    }
    selected.set(field.name, field);
  }

  if (selected.size > 0) {
    return [...selected.values()];
  }

  return fields
    .filter(field => !LOW_SIGNAL_COMMAND_PATTERN_FIELDS.has(field.name))
    .filter(field => field.type !== 'boolean')
    .slice(0, COMMAND_PATTERN_MAX_OPTIONAL_FIELDS);
}

function isUsefulCommandPatternField(field: DirectToolDisplayField): boolean {
  if (LOW_SIGNAL_COMMAND_PATTERN_FIELDS.has(field.name)) {
    return false;
  }

  if (field.type === 'boolean') {
    return false;
  }

  if (hasDisplayFieldDefault(field)) {
    return COMMAND_PATTERN_FIELD_PRIORITY.has(field.name);
  }

  return true;
}

function compareCommandPatternFields(
  left: DirectToolDisplayField,
  right: DirectToolDisplayField
): number {
  const leftPriority =
    COMMAND_PATTERN_FIELD_PRIORITY.get(left.name) ?? Number.MAX_SAFE_INTEGER;
  const rightPriority =
    COMMAND_PATTERN_FIELD_PRIORITY.get(right.name) ?? Number.MAX_SAFE_INTEGER;

  if (leftPriority !== rightPriority) {
    return leftPriority - rightPriority;
  }

  return left.name.localeCompare(right.name);
}

function hasDisplayFieldDefault(field: DirectToolDisplayField): boolean {
  return field.constraints?.includes('default ') === true;
}

function collectDisplayFields(
  properties: Record<string, unknown>,
  requiredFields: ReadonlySet<string>,
  prefix = ''
): DirectToolDisplayField[] {
  const fields: DirectToolDisplayField[] = [];

  for (const [name, value] of Object.entries(properties)) {
    if (!prefix && DIRECT_TOOL_AUTO_FILLED_FIELDS.has(name)) {
      continue;
    }

    const schema = isJsonSchemaObject(value) ? value : {};
    const fieldName = prefix ? `${prefix}.${name}` : name;
    fields.push({
      name: fieldName,
      required: requiredFields.has(name),
      type: describeSchemaType(schema),
      constraints: describeSchemaConstraints(schema),
      description:
        typeof schema.description === 'string' ? schema.description : undefined,
    });

    if (isRecord(schema.properties)) {
      const nestedRequired = new Set(
        Array.isArray(schema.required)
          ? schema.required.filter(nestedName =>
              typeof nestedName === 'string'
                ? !hasSchemaDefault(schema.properties?.[nestedName])
                : false
            )
          : []
      );
      fields.push(
        ...collectDisplayFields(schema.properties, nestedRequired, fieldName)
      );
    }

    const itemSchema =
      schema.type === 'array' && isJsonSchemaObject(schema.items)
        ? schema.items
        : undefined;
    if (itemSchema && isRecord(itemSchema.properties)) {
      const nestedRequired = new Set(
        Array.isArray(itemSchema.required)
          ? itemSchema.required.filter(nestedName =>
              typeof nestedName === 'string'
                ? !hasSchemaDefault(itemSchema.properties?.[nestedName])
                : false
            )
          : []
      );
      fields.push(
        ...collectDisplayFields(
          itemSchema.properties,
          nestedRequired,
          fieldName
        )
      );
    }
  }

  return fields;
}

function buildExampleValue(name: string, type: string): unknown {
  if (type.startsWith('array<')) {
    const innerType = type.slice('array<'.length, -1);
    return [buildScalarExampleValue(name, innerType)];
  }

  return buildScalarExampleValue(name, type);
}

function buildScalarExampleValue(name: string, type: string): unknown {
  if (type.startsWith('enum(')) {
    const match = /^enum\(([^,)]+)/.exec(type);
    return match?.[1] ?? name;
  }

  if (type === 'integer' || type === 'number') {
    return name === 'lineHint' ? 42 : 5;
  }

  if (type === 'boolean') {
    return true;
  }

  switch (name) {
    case 'keywords':
    case 'keywordsToSearch':
    case 'query':
    case 'text':
      return 'runCLI';
    case 'path':
      return '.';
    case 'uri':
      return '/path/to/file.ts';
    case 'owner':
      return 'facebook';
    case 'repo':
      return 'react';
    case 'extension':
      return 'ts';
    case 'filename':
      return 'package.json';
    case 'language':
      return 'TypeScript';
    case 'symbolName':
      return 'myFunction';
    case 'name':
    case 'packageName':
      return 'react';
    default:
      return name;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isJsonSchemaObject(value: unknown): value is JsonSchemaObject {
  return isRecord(value);
}

function hasSchemaDefault(value: unknown): boolean {
  return isJsonSchemaObject(value) && 'default' in value;
}
