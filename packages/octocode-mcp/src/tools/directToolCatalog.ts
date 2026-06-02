import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { z } from 'zod/v4';
import { initialize } from '../serverConfig.js';
import { initializeProviders } from '../providers/factory.js';
import {
  buildToolErrorResult,
  sanitizeCallToolResult,
} from '../utils/secureServer.js';
import {
  withBasicSecurityValidation,
  withSecurityValidation,
} from '../utils/securityBridge.js';
import { STATIC_TOOL_NAMES } from './toolNames.js';
import { ALL_TOOLS, type ToolConfig } from './toolConfig.js';

export type DirectToolInput = Record<string, unknown> & {
  queries: unknown[];
  responseCharLength?: number;
  responseCharOffset?: number;
};

export interface DirectToolDefinition {
  name: string;
  /** Per-query schema for help text and examples. */
  schema: z.ZodType;
  /** Canonical MCP bulk input schema used before direct execution. */
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
    STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
    STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
    STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
    STATIC_TOOL_NAMES.PACKAGE_SEARCH,
  ].map((name, index) => [name, index])
);

export interface DirectToolDisplayField {
  name: string;
  required: boolean;
  type: string;
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

const DIRECT_TOOL_AUTO_FILLED_FIELDS: ReadonlySet<string> = new Set(
  DIRECT_TOOL_AUTO_FILLED_FIELD_NAMES
);

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
    // SAFETY: every catalog entry pairs this executor with the same MCP-owned
    // bulk schema used by server registration for that tool.
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
  if (toolName.startsWith('github')) {
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

  const requiredFields = new Set(
    Array.isArray(jsonSchema.required)
      ? jsonSchema.required.filter(
          name => !DIRECT_TOOL_AUTO_FILLED_FIELDS.has(name)
        )
      : []
  );

  const properties = isRecord(jsonSchema.properties)
    ? jsonSchema.properties
    : {};

  return Object.entries(properties)
    .filter(([name]) => !DIRECT_TOOL_AUTO_FILLED_FIELDS.has(name))
    .map(([name, value]) => {
      const schema = isJsonSchemaObject(value) ? value : {};
      return {
        name,
        required: requiredFields.has(name),
        type: describeSchemaType(schema),
        description:
          typeof schema.description === 'string'
            ? schema.description
            : undefined,
      };
    });
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

  // SAFETY: Direct tool input schemas are all MCP bulk schemas containing the
  // ToolExecutionArgs envelope fields consumed by the execution functions.
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
  let responseCharLength: number | undefined;
  let responseCharOffset: number | undefined;

  if (Array.isArray(rawPayload)) {
    queriesInput = rawPayload;
  } else if (isRecord(rawPayload) && Array.isArray(rawPayload.queries)) {
    queriesInput = rawPayload.queries;
    if (typeof rawPayload.responseCharLength === 'number') {
      responseCharLength = rawPayload.responseCharLength;
    }
    if (typeof rawPayload.responseCharOffset === 'number') {
      responseCharOffset = rawPayload.responseCharOffset;
    }
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

  return {
    queries: queriesInput.map((query, index) =>
      applyDefaultQueryFields(
        toolName,
        index,
        normalizeQueryObject(toolName, query),
        {
          sourceLabel: options.sourceLabel,
        }
      )
    ),
    responseCharLength,
    responseCharOffset,
  };
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
  query: unknown
): Record<string, unknown> {
  if (!isRecord(query)) {
    throw new DirectToolInputError(
      'Tool input must be a JSON object or an array of objects.'
    );
  }

  const schemaFields = new Set([
    ...getDirectToolDisplayFields(toolName).map(field => field.name),
    ...DIRECT_TOOL_AUTO_FILLED_FIELDS,
  ]);
  const normalized: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(query)) {
    if (schemaFields.has(key)) {
      normalized[key] = value;
      continue;
    }
    const normalizedKey = normalizeKey(key);
    normalized[schemaFields.has(normalizedKey) ? normalizedKey : key] = value;
  }

  return normalized;
}

function normalizeKey(key: string): string {
  return key.replace(/[-_]+([a-zA-Z0-9])/g, (_, char: string) =>
    char.toUpperCase()
  );
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
      return 'octocode-mcp';
    case 'keywordsToSearch':
      return ['toolName'];
    case 'ecosystem':
      return 'npm';
    case 'name':
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

export async function executeDirectTool(
  name: string,
  input: unknown
): Promise<CallToolResult> {
  const tool = findDirectToolRuntimeDefinition(name);
  if (!tool) {
    throw new Error(`Unknown tool: ${name}`);
  }

  const parsedInput = parseDirectToolInput(tool, input);
  await ensureDirectToolRuntimeReady(tool);
  return runDirectTool(tool, parsedInput);
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

  // SAFETY: Direct tool input schemas are all MCP bulk schemas containing the
  // ToolExecutionArgs envelope fields consumed by the execution functions.
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
