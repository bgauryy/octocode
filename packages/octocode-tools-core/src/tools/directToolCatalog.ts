import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { z } from 'zod';
import { initialize, isCloneEnabled } from '../serverConfig.js';
import { initializeProviders } from '../providers/factory.js';
import { STATIC_TOOL_NAMES } from './toolNames.js';
import { LSP_GET_SEMANTIC_CONTENT_TOOL_NAME } from './lsp/shared/semanticTypes.js';
import type { ToolConfig } from './toolConfig.js';
import { ALL_TOOLS } from './toolConfig.js';
import {
  buildToolErrorResult,
  sanitizeCallToolResult,
} from '../utils/response/callToolResult.js';
import {
  withBasicSecurityValidation,
  withSecurityValidation,
} from '../security/bridge.js';
import { releaseAllPooledClients } from 'octocode-lsp/manager';

export type DirectToolInput = Record<string, unknown> & {
  queries: unknown[];
};

export interface DirectToolDefinition {
  name: string;

  schema: z.ZodType;

  inputSchema: z.ZodType;
}

export type DirectToolCategory =
  | 'GitHub'
  | 'Local'
  | 'LSP'
  | 'Package'
  | 'Other';

export const DIRECT_TOOL_CATEGORIES: readonly DirectToolCategory[] = [
  'GitHub',
  'Local',
  'LSP',
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

type DirectToolRuntimeDefinition = DirectToolDefinition & {
  execute: (input: DirectToolInput) => Promise<CallToolResult>;
  security: ToolConfig['direct']['security'];
  requiresServerRuntime?: boolean;
  requiresProviders?: boolean;
};

let serverRuntimeInitPromise: Promise<void> | null = null;
let providerRuntimeInitPromise: Promise<void> | null = null;

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

function wrapExecution(
  fn: ToolConfig['direct']['executionFn']
): (input: DirectToolInput) => Promise<CallToolResult> {
  return async input => {
    return fn(input as never);
  };
}

function createDirectTool(tool: ToolConfig): DirectToolRuntimeDefinition {
  const { direct } = tool;
  return {
    name: tool.name,
    schema: direct.schema,
    inputSchema: direct.inputSchema,
    execute: wrapExecution(direct.executionFn),
    security: direct.security,
    requiresServerRuntime: direct.requiresServerRuntime,
    requiresProviders: direct.requiresProviders,
  };
}

const DIRECT_TOOL_RUNTIME_DEFINITIONS: DirectToolRuntimeDefinition[] =
  ALL_TOOLS.map(createDirectTool);

export const DIRECT_TOOL_DEFINITIONS: DirectToolDefinition[] =
  DIRECT_TOOL_RUNTIME_DEFINITIONS.map(toDirectToolDefinition);

export function findDirectToolDefinition(
  name: string
): DirectToolDefinition | undefined {
  const tool = findDirectToolRuntimeDefinition(name);
  return tool ? toDirectToolDefinition(tool) : undefined;
}

export function getDirectToolCategory(toolName: string): DirectToolCategory {
  if (toolName.startsWith('gh')) {
    return 'GitHub';
  }

  if (toolName.startsWith('local')) {
    return 'Local';
  }

  if (toolName.startsWith('lsp')) {
    return 'LSP';
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

export function buildDirectToolExampleQuery(
  toolName: string
): Record<string, unknown> {
  const fields = getDirectToolDisplayFields(toolName);
  const requiredFields = fields.filter(field => field.required);
  const sourceFields =
    requiredFields.length > 0 ? requiredFields : fields.slice(0, 4);
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
  const tool = findDirectToolRuntimeDefinition(toolName);
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

function describeSchemaConstraints(schema: JsonSchemaObject): string | undefined {
  const parts: string[] = [];
  const min = typeof schema.minimum === 'number' ? schema.minimum : undefined;
  const max = typeof schema.maximum === 'number' ? schema.maximum : undefined;
  if (min !== undefined && max !== undefined) parts.push(`${min}-${max}`);
  else if (min !== undefined) parts.push(`>=${min}`);
  else if (max !== undefined) parts.push(`<=${max}`);
  if ('default' in schema) parts.push(`default ${JSON.stringify(schema.default)}`);
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

  if (Array.isArray(schema.type)) {
    return schema.type.join(' | ');
  }

  if (typeof schema.type === 'string') {
    return schema.type;
  }

  return 'value';
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
    return [name];
  }

  if (type === 'integer' || type === 'number') {
    return 1;
  }

  if (type === 'boolean') {
    return true;
  }

  if (type.startsWith('enum(')) {
    const match = /^enum\(([^,)]+)/.exec(type);
    return match?.[1] ?? name;
  }

  switch (name) {
    case 'path':
      return '.';
    case 'owner':
      return 'bgauryy';
    case 'repo':
      return 'octocode';
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

export async function executeDirectTool(
  name: string,
  input: unknown
): Promise<CallToolResult> {
  const tool = findDirectToolRuntimeDefinition(name);
  if (!tool) {
    throw new Error(`Unknown tool: ${name}`);
  }

  const parsedInput = parseDirectToolInput(tool, input);
  try {
    await ensureDirectToolRuntimeReady(tool);
    if (name === STATIC_TOOL_NAMES.GITHUB_CLONE_REPO && !isCloneEnabled()) {
      const disabledResult: CallToolResult = {
        content: [
          {
            type: 'text',
            text: 'error: ghCloneRepo is disabled\nmessage: Set ENABLE_CLONE=true (and ENABLE_LOCAL=true) to enable repository cloning.\nhints:\n- To browse without cloning, use ghViewRepoStructure to list files or ghGetFileContent to read specific files.',
          },
        ],
        structuredContent: {
          status: 'error',
          tool: name,
          code: 'TOOL_DISABLED',
          error: {
            message:
              'ghCloneRepo is disabled — set ENABLE_CLONE=true (and ENABLE_LOCAL=true) to enable repository cloning.',
          },
          hints: [
            'To browse without cloning, use ghViewRepoStructure to list files or ghGetFileContent to read specific files.',
          ],
        },
        isError: true,
      };
      return sanitizeCallToolResult(disabledResult);
    }
    return await runDirectTool(tool, parsedInput);
  } finally {
    if (name === LSP_GET_SEMANTIC_CONTENT_TOOL_NAME) {
      await releaseAllPooledClients();
    }
  }
}

function toDirectToolDefinition(
  tool: DirectToolRuntimeDefinition
): DirectToolDefinition {
  return {
    name: tool.name,
    schema: tool.schema,
    inputSchema: tool.inputSchema,
  };
}

function findDirectToolRuntimeDefinition(
  name: string
): DirectToolRuntimeDefinition | undefined {
  return DIRECT_TOOL_RUNTIME_DEFINITIONS.find(tool => tool.name === name);
}

function parseDirectToolInput(
  tool: DirectToolRuntimeDefinition,
  input: unknown
): DirectToolInput {
  const result = tool.inputSchema.safeParse(input);
  if (!result.success) {
    throw result.error;
  }

  return result.data as DirectToolInput;
}

async function ensureDirectToolRuntimeReady(
  tool: DirectToolRuntimeDefinition
): Promise<void> {
  if (tool.requiresServerRuntime) {
    if (!serverRuntimeInitPromise) {
      serverRuntimeInitPromise = initialize();
    }
    await serverRuntimeInitPromise;
  }

  if (tool.requiresProviders) {
    if (!providerRuntimeInitPromise) {
      providerRuntimeInitPromise = initializeProviders().then(() => undefined);
    }
    await providerRuntimeInitPromise;
  }
}

async function runDirectTool(
  tool: DirectToolRuntimeDefinition,
  input: DirectToolInput
): Promise<CallToolResult> {
  try {
    const result =
      tool.security === 'remote'
        ? await runRemoteDirectTool(tool, input)
        : await runBasicDirectTool(tool, input);
    return sanitizeCallToolResult(result);
  } catch (error) {
    return buildToolErrorResult(tool.name, error);
  }
}

async function runRemoteDirectTool(
  tool: DirectToolRuntimeDefinition,
  input: DirectToolInput
): Promise<CallToolResult> {
  const handler = withSecurityValidation<DirectToolInput>(
    tool.name,
    async (sanitizedArgs, authInfo, sessionId) =>
      tool.execute({ ...sanitizedArgs, authInfo, sessionId })
  );

  return handler(input, {});
}

async function runBasicDirectTool(
  tool: DirectToolRuntimeDefinition,
  input: DirectToolInput
): Promise<CallToolResult> {
  const handler = withBasicSecurityValidation<DirectToolInput>(
    tool.execute,
    tool.name
  );

  return handler(input);
}
