import type { CLICommand, ParsedArgs } from './types.js';
import './cjs-shim.js';
import { c, bold, dim } from '../utils/colors.js';
import {
  buildDirectToolExampleQuery,
  DIRECT_TOOL_CATEGORIES,
  DIRECT_TOOL_DEFINITIONS,
  DirectToolInputError,
  executeDirectTool,
  findDirectToolDefinition,
  formatCallToolResultForOutput,
  formatDirectToolOutputSchemaText,
  formatDirectToolMetadataSchemaText,
  formatDirectToolSchemaText,
  getDirectToolAutoFilledFields,
  getDirectToolCategory,
  getDirectToolDescription,
  getDirectToolDisplayFields,
  getDirectToolOutputFields,
  loadToolContent,
  prepareDirectToolInputFromJsonText,
  sortDirectToolNames,
  type DirectToolDefinition,
  type DirectToolDisplayField,
} from 'octocode-mcp/public';

type ToolResult = Parameters<typeof formatCallToolResultForOutput>[0];

export type ToolDefinition = DirectToolDefinition;
export const TOOL_CATEGORIES = DIRECT_TOOL_CATEGORIES;

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

const CANONICAL_TOOL_USAGE = [
  'octocode tools                                   # list all tools',
  'octocode tools <name>                            # show input schema',
  'octocode tools <n1> <n2> ...                     # batch input schemas',
  "octocode tools <name> --queries '<json>'         # run a tool",
  "octocode tools <name> --queries '<json>' --json  # run, raw JSON output",
  'octocode instructions                            # MCP instructions + all schemas',
].join('\n');

export const TOOL_DEFINITIONS: ToolDefinition[] = DIRECT_TOOL_DEFINITIONS;
let toolMetadataPromise: Promise<
  Awaited<ReturnType<typeof loadToolContent>>
> | null = null;

export function findToolDefinition(name: string): ToolDefinition | undefined {
  return findDirectToolDefinition(name);
}

export function getToolCategory(
  toolName: string
): ReturnType<typeof getDirectToolCategory> {
  return getDirectToolCategory(toolName);
}

export function getDisplayFields(
  tool: ToolDefinition
): DirectToolDisplayField[] {
  return getDirectToolDisplayFields(tool.name);
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

function formatToolExampleCommand(toolName: string): string {
  const exampleInput = JSON.stringify(buildDirectToolExampleQuery(toolName));
  return `octocode tools ${toolName} --queries '${exampleInput}'`;
}

function getUnexpectedToolOptionKeys(args: ParsedArgs): string[] {
  return Object.keys(args.options).filter(
    key => key !== 'input' && !TOOL_RUNTIME_OPTION_KEYS.has(key)
  );
}

function getInputText(toolName: string, args: ParsedArgs): string | undefined {
  if (args.options.input !== undefined) {
    throw new DirectToolInputError(
      `Legacy --input is not supported. Use ${formatToolExampleCommand(toolName)}.`
    );
  }

  const unexpectedOptionKeys = getUnexpectedToolOptionKeys(args);
  if (unexpectedOptionKeys.length > 0) {
    const formattedKeys = unexpectedOptionKeys
      .map(key => `--${key}`)
      .join(', ');

    throw new DirectToolInputError(
      `Unsupported tool flags: ${formattedKeys}. Use ${formatToolExampleCommand(toolName)}.`
    );
  }

  if (args.args.length > 2) {
    throw new DirectToolInputError(
      `Pass tool input as one quoted JSON string. Use ${formatToolExampleCommand(toolName)}.`
    );
  }

  return typeof args.options.queries === 'string'
    ? args.options.queries
    : args.args[1];
}

function extractShortDescription(fullDescription: string): string {
  return fullDescription
    .split('\n')[0]
    .trim()
    .replace(/^##\s*/, '');
}

export async function showAvailableTools(): Promise<void> {
  const metadata = await getOptionalToolMetadata();

  console.log();
  console.log(`  ${c('magenta', bold('Octocode Tools'))}`);
  console.log(
    `  ${dim('tools <name>')} ${dim('→ schema')}   ${dim("tools <name> --queries '<json>'")} ${dim('→ run')}`
  );

  const toolNames = sortDirectToolNames(
    TOOL_DEFINITIONS.map(tool => tool.name)
  );

  for (const category of TOOL_CATEGORIES) {
    const toolsInCategory = toolNames.filter(
      toolName => getDirectToolCategory(toolName) === category
    );
    if (toolsInCategory.length === 0) {
      continue;
    }

    console.log();
    console.log(`  ${bold(category)}`);
    for (const toolName of toolsInCategory) {
      const shortDesc = extractShortDescription(
        getDirectToolDescription(toolName, metadata)
      );
      const padded = toolName.padEnd(32);
      console.log(`    ${c('cyan', padded)} ${dim(shortDesc)}`);
    }
  }

  console.log();
}

export async function showToolHelp(toolName: string): Promise<boolean> {
  const tool = findToolDefinition(toolName);
  if (!tool) {
    return false;
  }

  const metadata = await getOptionalToolMetadata();
  const fields = getDirectToolDisplayFields(tool.name);
  const autoFilledFields = getDirectToolAutoFilledFields(tool.name);
  const shortDesc = extractShortDescription(
    getDirectToolDescription(tool.name, metadata)
  );

  console.log();
  console.log(`  ${c('magenta', bold(tool.name))}  ${dim(shortDesc)}`);
  console.log();

  console.log(`  ${bold('Input Schema')}`);
  for (const field of fields) {
    const reqTag = field.required ? c('red', ' [required]') : '';
    console.log(
      `    ${c('cyan', field.name)} (${field.type})${reqTag}${field.description ? dim(` - ${field.description}`) : ''}`
    );
  }
  console.log();

  console.log(`  ${dim('Auto-filled')}: ${autoFilledFields.join(', ')}`);
  console.log();

  console.log(`  ${bold('Output Schema')}`);
  for (const field of getDirectToolOutputFields()) {
    const optional = field.optional ? ' (optional)' : '';
    console.log(`    ${dim(field.name)}: ${field.type}${optional}`);
  }
  console.log();

  console.log(`  ${bold('Flags')}`);
  console.log(
    `    ${c('cyan', '--json')}   ${dim('Output raw JSON (structuredContent + content + isError)')}`
  );
  console.log();

  console.log(`  ${bold('Example')}`);
  console.log(`    ${c('yellow', formatToolExampleCommand(tool.name))}`);
  console.log(
    `    ${c('yellow', formatToolExampleCommand(tool.name) + ' --json')}`
  );
  console.log();

  return true;
}

export async function showMultipleToolSchemas(
  toolNames: string[]
): Promise<void> {
  const metadata = await getOptionalToolMetadata();

  for (const toolName of toolNames) {
    const tool = findToolDefinition(toolName);
    if (!tool) {
      console.log();
      console.log(`  ${c('red', 'x')} Unknown tool: ${toolName}`);
      continue;
    }

    const shortDesc = extractShortDescription(
      getDirectToolDescription(tool.name, metadata)
    );
    const fields = getDirectToolDisplayFields(tool.name);
    const autoFilledFields = getDirectToolAutoFilledFields(tool.name);

    console.log();
    console.log(`  ${c('magenta', bold(tool.name))}  ${dim(shortDesc)}`);
    console.log(`  ${bold('Input Schema')}`);
    for (const field of fields) {
      const reqTag = field.required ? c('red', ' [required]') : '';
      console.log(
        `    ${c('cyan', field.name)} (${field.type})${reqTag}${field.description ? dim(` - ${field.description}`) : ''}`
      );
    }
    console.log(`  ${dim('Auto-filled')}: ${autoFilledFields.join(', ')}`);
    console.log(
      `  ${bold('Example')}  ${c('yellow', formatToolExampleCommand(tool.name))}`
    );
  }

  console.log();
}

export async function getToolsContextString(): Promise<string> {
  const metadata = await loadToolMetadata();
  const toolNames = sortDirectToolNames(Object.keys(metadata.tools));

  const sections: string[] = [
    'CLI Usage:',
    CANONICAL_TOOL_USAGE,
    '',
    'Octocode MCP Instructions:',
    metadata.instructions.trim(),
    '',
    'Output schema (all tools):',
    formatDirectToolOutputSchemaText(),
    '',
    'Tools:',
  ];

  toolNames.forEach((toolName, index) => {
    const schemaText = findDirectToolDefinition(toolName)
      ? formatDirectToolSchemaText(toolName)
      : formatDirectToolMetadataSchemaText(metadata.tools[toolName]?.schema);

    const shortDesc = extractShortDescription(
      getDirectToolDescription(toolName, metadata)
    );

    sections.push(`${index + 1}. ${toolName}`);
    sections.push(`Description: ${shortDesc}`);
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
  console.log(`  ${c('red', 'x')} ${message}`);
  for (const detail of details) {
    console.log(`  ${dim('-')} ${detail}`);
  }
  console.log();
}

function getErrorDetails(error: unknown): string[] {
  return error instanceof DirectToolInputError ? error.details : [];
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

  // Batch schema mode: multiple positional args, no --queries
  if (
    args.args.length > 1 &&
    typeof args.options.queries !== 'string' &&
    args.args.every(n => findToolDefinition(n) !== undefined)
  ) {
    await showMultipleToolSchemas(args.args);
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

  let inputText: string | undefined;
  try {
    inputText = getInputText(tool.name, args);
  } catch (error) {
    printToolError(
      error instanceof Error ? error.message : 'Failed to parse tool input.',
      getErrorDetails(error)
    );
    return false;
  }

  if (!inputText) {
    await showToolHelp(tool.name);
    return true;
  }

  try {
    const input = prepareDirectToolInputFromJsonText(tool.name, inputText, {
      sourceLabel: 'octocode-cli',
    });
    if (!input) {
      await showToolHelp(tool.name);
      return true;
    }

    const result = await executeDirectTool(tool.name, input);
    printToolResult(result, getOutputMode(args));
    return !result.isError;
  } catch (error) {
    printToolError(
      error instanceof Error ? error.message : 'Tool execution failed.',
      getErrorDetails(error)
    );
    return false;
  }
}

export const toolCommand: CLICommand = {
  name: 'tool',
  description: 'Run an Octocode tool directly',
  usage: `octocode --tool <toolName> --queries '<json-stringified-input>'`,
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
