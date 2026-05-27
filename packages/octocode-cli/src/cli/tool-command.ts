import { z } from 'zod/v4';
import type { CLICommand, ParsedArgs } from './types.js';
import { c, bold, dim } from '../utils/colors.js';
import {
  initialize as initializeMcp,
  initializeProviders,
  loadToolContent,
  fetchMultipleGitHubFileContents,
  searchMultipleGitHubCode,
  searchMultipleGitHubPullRequests,
  searchMultipleGitHubRepos,
  exploreMultipleRepositoryStructures,
  executeCloneRepo,
  executeFetchContent,
  executeFindFiles,
  executeRipgrepSearch,
  executeViewStructure,
  executeGotoDefinition,
  executeFindReferences,
  executeCallHierarchy,
  searchPackages,
  formatCallToolResultForOutput,
  DEFAULT_TOOL_RESPONSE_FORMAT,
  GitHubCodeSearchQuerySchema,
  GitHubViewRepoStructureQuerySchema,
  GitHubReposSearchSingleQuerySchema,
  GitHubPullRequestSearchQuerySchema,
  FileContentQuerySchema,
  RipgrepQuerySchema,
  FetchContentQuerySchema,
  FindFilesQuerySchema,
  ViewStructureQuerySchema,
  LSPGotoDefinitionQuerySchema,
  LSPFindReferencesQuerySchema,
  LSPCallHierarchyQuerySchema,
  PackageSearchQuerySchema,
  CloneRepoQuerySchema,
} from 'octocode-mcp/public';

type ToolResult = {
  content?: Array<{ type?: string; text?: string }>;
  structuredContent?: unknown;
  isError?: boolean;
};

type ToolExecutor = (input: unknown) => Promise<ToolResult>;

export interface ToolDefinition {
  name: string;
  schema: z.ZodType;
  execute: ToolExecutor;
  requiresServerRuntime?: boolean;
  requiresProviders?: boolean;
}

interface JsonSchemaObject extends Record<string, unknown> {
  type?: string | string[];
  description?: string;
  enum?: unknown[];
  required?: string[];
  properties?: Record<string, unknown>;
  items?: unknown;
}

export const AUTO_FILLED_FIELDS = new Set([
  'id',
  'mainResearchGoal',
  'researchGoal',
  'reasoning',
]);

const TOOL_RUNTIME_OPTION_KEYS = new Set([
  'tool',
  'queries',
  'output',
  'o',
  'json',
  'help',
  'h',
  'version',
  'v',
  'list',
  'schema',
  'tools-context',
]);

const CANONICAL_TOOL_USAGE =
  "octocode-cli --tool <toolName> --queries '<json-stringified-input>'";

/** Wraps a typed executor into an untyped ToolExecutor.
 * SAFETY: callers MUST validate via tool.schema.safeParse() before invoking. */
function wrapExecutor<TInput>(
  fn: (input: TInput) => Promise<ToolResult>
): ToolExecutor {
  return async (input: unknown) => fn(input as TInput);
}

export const TOOL_DEFINITIONS: ToolDefinition[] = [
  {
    name: 'githubSearchCode',
    schema: GitHubCodeSearchQuerySchema,
    execute: wrapExecutor(searchMultipleGitHubCode),
    requiresServerRuntime: true,
    requiresProviders: true,
  },
  {
    name: 'githubGetFileContent',
    schema: FileContentQuerySchema,
    execute: wrapExecutor(fetchMultipleGitHubFileContents),
    requiresServerRuntime: true,
    requiresProviders: true,
  },
  {
    name: 'githubViewRepoStructure',
    schema: GitHubViewRepoStructureQuerySchema,
    execute: wrapExecutor(exploreMultipleRepositoryStructures),
    requiresServerRuntime: true,
    requiresProviders: true,
  },
  {
    name: 'githubSearchRepositories',
    schema: GitHubReposSearchSingleQuerySchema,
    execute: wrapExecutor(searchMultipleGitHubRepos),
    requiresServerRuntime: true,
    requiresProviders: true,
  },
  {
    name: 'githubSearchPullRequests',
    schema: GitHubPullRequestSearchQuerySchema,
    execute: wrapExecutor(searchMultipleGitHubPullRequests),
    requiresServerRuntime: true,
    requiresProviders: true,
  },
  {
    name: 'githubCloneRepo',
    schema: CloneRepoQuerySchema,
    execute: wrapExecutor(executeCloneRepo),
    requiresServerRuntime: true,
    requiresProviders: true,
  },
  {
    name: 'packageSearch',
    schema: PackageSearchQuerySchema,
    execute: wrapExecutor(searchPackages),
    requiresServerRuntime: true,
  },
  {
    name: 'localSearchCode',
    schema: RipgrepQuerySchema,
    execute: wrapExecutor(executeRipgrepSearch),
  },
  {
    name: 'localGetFileContent',
    schema: FetchContentQuerySchema,
    execute: wrapExecutor(executeFetchContent),
  },
  {
    name: 'localFindFiles',
    schema: FindFilesQuerySchema,
    execute: wrapExecutor(executeFindFiles),
  },
  {
    name: 'localViewStructure',
    schema: ViewStructureQuerySchema,
    execute: wrapExecutor(executeViewStructure),
  },
  {
    name: 'lspGotoDefinition',
    schema: LSPGotoDefinitionQuerySchema,
    execute: wrapExecutor(executeGotoDefinition),
    requiresServerRuntime: true,
  },
  {
    name: 'lspFindReferences',
    schema: LSPFindReferencesQuerySchema,
    execute: wrapExecutor(executeFindReferences),
    requiresServerRuntime: true,
  },
  {
    name: 'lspCallHierarchy',
    schema: LSPCallHierarchyQuerySchema,
    execute: wrapExecutor(executeCallHierarchy),
    requiresServerRuntime: true,
  },
];

let serverRuntimeInitPromise: Promise<void> | null = null;
let providerRuntimeInitPromise: Promise<void> | null = null;
let toolMetadataPromise: Promise<
  Awaited<ReturnType<typeof loadToolContent>>
> | null = null;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isJsonSchemaObject(value: unknown): value is JsonSchemaObject {
  return isRecord(value);
}

export function findToolDefinition(name: string): ToolDefinition | undefined {
  return TOOL_DEFINITIONS.find(tool => tool.name === name);
}

export function getToolCategory(
  toolName: string
): 'GitHub' | 'Local' | 'LSP' | 'Package' | 'Other' {
  if (toolName.startsWith('github')) {
    return 'GitHub';
  }

  if (toolName.startsWith('local')) {
    return 'Local';
  }

  if (toolName.startsWith('lsp')) {
    return 'LSP';
  }

  if (toolName === 'packageSearch') {
    return 'Package';
  }

  return 'Other';
}

function sortToolNames(toolNames: string[]): string[] {
  const categoryOrder = ['GitHub', 'Local', 'LSP', 'Package', 'Other'];

  return [...toolNames].sort((left, right) => {
    const leftCategory = categoryOrder.indexOf(getToolCategory(left));
    const rightCategory = categoryOrder.indexOf(getToolCategory(right));

    if (leftCategory !== rightCategory) {
      return leftCategory - rightCategory;
    }

    return 0;
  });
}

async function loadToolMetadata(): Promise<
  Awaited<ReturnType<typeof loadToolContent>>
> {
  if (!toolMetadataPromise) {
    toolMetadataPromise = loadToolContent();
  }

  return toolMetadataPromise;
}

async function getOptionalToolMetadata(): Promise<Awaited<
  ReturnType<typeof loadToolContent>
> | null> {
  try {
    return await loadToolMetadata();
  } catch {
    return null;
  }
}

function getToolDescription(
  toolName: string,
  metadata?: Awaited<ReturnType<typeof loadToolContent>> | null
): string {
  return metadata?.tools[toolName]?.description ?? toolName;
}

function formatSchemaText(toolName: string): string {
  const tool = findToolDefinition(toolName);
  if (!tool) {
    return '{}';
  }

  return JSON.stringify(z.toJSONSchema(tool.schema), null, 2);
}

function formatMetadataSchemaText(
  schema: Record<string, string> | undefined
): string {
  return JSON.stringify(schema ?? {}, null, 2);
}

function normalizeKey(key: string): string {
  return key.replace(/[-_]+([a-zA-Z0-9])/g, (_, char: string) =>
    char.toUpperCase()
  );
}

function buildDefaultGoal(toolName: string): string {
  return `Execute ${toolName} via octocode-cli`;
}

function applyDefaultQueryFields(
  toolName: string,
  index: number,
  query: Record<string, unknown>
): Record<string, unknown> {
  const nextQuery = { ...query };
  const category = getToolCategory(toolName);

  if (typeof nextQuery.id !== 'string' || nextQuery.id.trim().length === 0) {
    nextQuery.id = `${toolName}-${index + 1}`;
  }

  if (category === 'GitHub' || category === 'Package') {
    if (
      typeof nextQuery.mainResearchGoal !== 'string' ||
      nextQuery.mainResearchGoal.trim().length === 0
    ) {
      nextQuery.mainResearchGoal = buildDefaultGoal(toolName);
    }
  }

  if (
    typeof nextQuery.researchGoal !== 'string' ||
    nextQuery.researchGoal.trim().length === 0
  ) {
    nextQuery.researchGoal = buildDefaultGoal(toolName);
  }

  if (
    typeof nextQuery.reasoning !== 'string' ||
    nextQuery.reasoning.trim().length === 0
  ) {
    nextQuery.reasoning = 'Executed via octocode-cli tool command';
  }

  return nextQuery;
}

function normalizeQueryObject(query: unknown): Record<string, unknown> {
  if (!isRecord(query)) {
    throw new Error('Tool input must be a JSON object or an array of objects.');
  }

  const normalized: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(query)) {
    normalized[normalizeKey(key)] = value;
  }

  return normalized;
}

function formatToolExampleCommand(toolName: string): string {
  const tool = findToolDefinition(toolName);
  const exampleInput = tool
    ? JSON.stringify(buildExampleQuery(tool))
    : '{"path":".","pattern":"needle"}';

  return `octocode-cli --tool ${toolName} --queries '${exampleInput}'`;
}

function getUnexpectedToolOptionKeys(args: ParsedArgs): string[] {
  return Object.keys(args.options).filter(
    key => key !== 'input' && !TOOL_RUNTIME_OPTION_KEYS.has(key)
  );
}

function buildToolPayload(
  toolName: string,
  args: ParsedArgs
): {
  queries: Array<Record<string, unknown>>;
  responseCharLength?: number;
  responseCharOffset?: number;
} | null {
  if (args.options.input !== undefined) {
    throw new Error(
      `Legacy --input is not supported. Use ${formatToolExampleCommand(toolName)}.`
    );
  }

  const unexpectedOptionKeys = getUnexpectedToolOptionKeys(args);
  if (unexpectedOptionKeys.length > 0) {
    const formattedKeys = unexpectedOptionKeys
      .map(key => `--${key}`)
      .join(', ');

    throw new Error(
      `Unsupported tool flags: ${formattedKeys}. Use ${formatToolExampleCommand(toolName)}.`
    );
  }

  if (args.args.length > 2) {
    throw new Error(
      `Pass tool input as one quoted JSON string. Use ${formatToolExampleCommand(toolName)}.`
    );
  }

  const inputText =
    typeof args.options.queries === 'string'
      ? args.options.queries
      : args.args[1];
  if (typeof inputText !== 'string') {
    return null;
  }

  let rawPayload: unknown;

  try {
    rawPayload = JSON.parse(inputText) as unknown;
  } catch {
    throw new Error(
      `Tool input must be valid JSON. Use ${formatToolExampleCommand(toolName)}.`
    );
  }

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
    throw new Error(
      'Tool input must be a JSON object, an array of query objects, or { "queries": [...] }.'
    );
  }

  if (queriesInput.length === 0) {
    throw new Error('At least one query is required.');
  }

  return {
    queries: queriesInput.map((query, index) =>
      applyDefaultQueryFields(toolName, index, normalizeQueryObject(query))
    ),
    responseCharLength,
    responseCharOffset,
  };
}

export function getDisplayFields(tool: ToolDefinition): Array<{
  name: string;
  required: boolean;
  type: string;
  description?: string;
}> {
  const jsonSchema = z.toJSONSchema(tool.schema);
  if (!isJsonSchemaObject(jsonSchema)) {
    return [];
  }

  const requiredFields = new Set(
    Array.isArray(jsonSchema.required)
      ? jsonSchema.required.filter(name => !AUTO_FILLED_FIELDS.has(name))
      : []
  );

  const properties = isRecord(jsonSchema.properties)
    ? jsonSchema.properties
    : {};

  return Object.entries(properties)
    .filter(([name]) => !AUTO_FILLED_FIELDS.has(name))
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

function buildExampleQuery(tool: ToolDefinition): Record<string, unknown> {
  const fields = getDisplayFields(tool);
  const requiredFields = fields.filter(field => field.required);
  const sourceFields =
    requiredFields.length > 0 ? requiredFields : fields.slice(0, 4);
  const example: Record<string, unknown> = {};

  for (const field of sourceFields) {
    example[field.name] = buildExampleValue(field.name, field.type);
  }

  return example;
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

export async function showAvailableTools(): Promise<void> {
  const metadata = await getOptionalToolMetadata();

  console.log();
  console.log(`  ${c('magenta', bold('Octocode Tools'))}`);

  const categories = ['GitHub', 'Local', 'LSP', 'Package'] as const;

  const toolNames = sortToolNames(TOOL_DEFINITIONS.map(tool => tool.name));

  for (const category of categories) {
    const toolsInCategory = toolNames.filter(
      toolName => getToolCategory(toolName) === category
    );
    if (toolsInCategory.length === 0) {
      continue;
    }

    console.log();
    console.log(`  ${bold(category)}`);
    for (const toolName of toolsInCategory) {
      console.log(
        `    ${c('cyan', toolName)} ${dim('-')} ${getToolDescription(toolName, metadata)}`
      );
    }
  }

  console.log();
  console.log(
    `  ${dim('Tip:')} ${c('yellow', 'octocode-cli --tool localSearchCode --queries \'{"path":".","pattern":"runCLI"}\'')}`
  );
  console.log();
}

export async function showToolHelp(toolName: string): Promise<boolean> {
  const tool = findToolDefinition(toolName);
  if (!tool) {
    return false;
  }

  const metadata = await getOptionalToolMetadata();
  const fields = getDisplayFields(tool);

  console.log();
  console.log(`  ${c('magenta', bold(tool.name))}`);
  console.log(`  ${getToolDescription(tool.name, metadata)}`);
  console.log();

  console.log(`  ${bold('Input Schema')}`);
  for (const field of fields) {
    const reqTag = field.required ? c('red', ' [required]') : '';
    console.log(
      `    ${c('cyan', field.name)} (${field.type})${reqTag}${field.description ? dim(` — ${field.description}`) : ''}`
    );
  }
  console.log();

  console.log(
    `  ${dim('Auto-filled')}: id, researchGoal, reasoning${
      getToolCategory(tool.name) === 'GitHub' ||
      getToolCategory(tool.name) === 'Package'
        ? ', mainResearchGoal'
        : ''
    }`
  );
  console.log();

  console.log(`  ${bold('Output Schema')}`);
  console.log(`    ${dim('content')}: Array<{ type: string; text: string }>`);
  console.log(`    ${dim('structuredContent')}: object (optional)`);
  console.log(`    ${dim('isError')}: boolean (optional)`);
  console.log();

  const exampleQuery = buildExampleQuery(tool);
  console.log(`  ${bold('Example')}`);
  console.log(
    `    ${c('yellow', `octocode-cli --tool ${tool.name} --queries '${JSON.stringify(exampleQuery)}'`)}`
  );
  console.log();

  return true;
}

export async function getToolsContextString(): Promise<string> {
  const metadata = await loadToolMetadata();
  const toolNames = sortToolNames(Object.keys(metadata.tools));

  const sections: string[] = [
    'CLI Contract:',
    `- ${CANONICAL_TOOL_USAGE}`,
    '- octocode-cli --tools-context',
    '',
    'Octocode MCP Instructions:',
    metadata.instructions.trim(),
    '',
    'Output schema (all tools):',
    JSON.stringify(
      {
        content: 'Array<{ type: string; text: string }>',
        structuredContent: 'object (optional)',
        isError: 'boolean (optional)',
      },
      null,
      2
    ),
    '',
    'Tools:',
  ];

  toolNames.forEach((toolName, index) => {
    const schemaText = findToolDefinition(toolName)
      ? formatSchemaText(toolName)
      : formatMetadataSchemaText(metadata.tools[toolName]?.schema);

    sections.push(`${index + 1}. ${toolName}`);
    sections.push(`Description: ${getToolDescription(toolName, metadata)}`);
    sections.push('Input schema:');
    sections.push(schemaText);
    sections.push('');
  });

  return sections.join('\n').trim();
}

export async function printToolsContext(): Promise<void> {
  console.log(await getToolsContextString());
}

function getOutputMode(args: ParsedArgs): 'text' | 'json' {
  if (args.options.json === true) {
    return 'json';
  }

  const output = args.options.output ?? args.options.o;
  if (typeof output === 'string' && output.toLowerCase() === 'json') {
    return 'json';
  }

  return 'text';
}

function printToolResult(
  result: ToolResult,
  outputMode: 'text' | 'json'
): void {
  console.log(formatCallToolResultForOutput(result, outputMode));
}

function printToolError(message: string, details: string[] = []): void {
  console.log();
  console.log(`  ${c('red', '✗')} ${message}`);
  for (const detail of details) {
    console.log(`  ${dim('-')} ${detail}`);
  }
  console.log();
}

function formatValidationIssues(error: z.ZodError): string[] {
  return error.issues.map(issue => {
    const path = issue.path.length > 0 ? issue.path.join('.') : 'input';
    return `${path}: ${issue.message}`;
  });
}

async function ensureServerRuntimeReady(): Promise<void> {
  if (!serverRuntimeInitPromise) {
    serverRuntimeInitPromise = initializeMcp();
  }

  await serverRuntimeInitPromise;
}

async function ensureProvidersReady(): Promise<void> {
  if (!providerRuntimeInitPromise) {
    providerRuntimeInitPromise = initializeProviders().then(() => undefined);
  }

  await providerRuntimeInitPromise;
}

async function ensureToolRuntimeReady(tool: ToolDefinition): Promise<void> {
  if (tool.requiresServerRuntime) {
    await ensureServerRuntimeReady();
  }

  if (tool.requiresProviders) {
    await ensureProvidersReady();
  }
}

export async function executeToolCommand(args: ParsedArgs): Promise<boolean> {
  const maybeToolName = args.args[0];
  const toolName =
    typeof maybeToolName === 'string'
      ? maybeToolName
      : typeof args.options.tool === 'string'
        ? args.options.tool
        : undefined;

  if (!toolName || toolName === 'list' || args.options.list === true) {
    await showAvailableTools();
    return true;
  }

  const tool = findToolDefinition(toolName);
  if (!tool) {
    printToolError(`Unknown tool: ${toolName}`, [
      `Available tools: ${TOOL_DEFINITIONS.map(item => item.name).join(', ')}`,
    ]);
    return false;
  }

  if (args.options.schema === true) {
    await showToolHelp(tool.name);
    return true;
  }

  let payload;
  try {
    payload = buildToolPayload(tool.name, args);
  } catch (error) {
    printToolError(
      error instanceof Error ? error.message : 'Failed to parse tool input.'
    );
    return false;
  }

  if (!payload) {
    await showToolHelp(tool.name);
    return true;
  }

  const validationResults = payload.queries.map(query =>
    tool.schema.safeParse(query)
  );
  const validationFailure = validationResults.find(result => !result.success);
  if (validationFailure && !validationFailure.success) {
    printToolError('Tool input does not match the expected schema.', [
      ...formatValidationIssues(validationFailure.error),
    ]);
    return false;
  }

  const queries = validationResults
    .filter(
      (result): result is z.ZodSafeParseSuccess<Record<string, unknown>> =>
        result.success
    )
    .map(result => result.data);

  try {
    await ensureToolRuntimeReady(tool);
    const result = await tool.execute({
      queries,
      responseCharLength: payload.responseCharLength,
      responseCharOffset: payload.responseCharOffset,
      format: DEFAULT_TOOL_RESPONSE_FORMAT,
    });
    printToolResult(result, getOutputMode(args));
    return !result.isError;
  } catch (error) {
    printToolError(
      error instanceof Error ? error.message : 'Tool execution failed.'
    );
    return false;
  }
}

export const toolCommand: CLICommand = {
  name: 'tool',
  description: 'Run an Octocode tool directly',
  usage: `octocode-cli --tool <toolName> --queries '<json-stringified-input>'`,
  options: [
    {
      name: 'tool',
      description: 'Tool name to execute.',
      hasValue: true,
    },
    {
      name: 'queries',
      description: 'JSON-stringified tool input (query object or array).',
      hasValue: true,
    },
    {
      name: 'output',
      short: 'o',
      description: 'Output format: text (default) or json.',
      hasValue: true,
      default: 'text',
    },
    {
      name: 'list',
      description: 'List available tools.',
    },
    {
      name: 'schema',
      description:
        'Show the selected tool schema summary instead of running it.',
    },
  ],
  handler: async (args: ParsedArgs) => {
    const success = await executeToolCommand(args);
    if (!success) {
      process.exitCode = 1;
    }
  },
};
