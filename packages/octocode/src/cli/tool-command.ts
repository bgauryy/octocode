import type { CLICommand, ParsedArgs } from './types.js';
import './cjs-shim.js';
import { EXIT, classifyToolErrorText } from './exit-codes.js';
import { c, bold, dim } from '../utils/colors.js';
import {
  buildDirectToolExampleQuery,
  DIRECT_TOOL_CATEGORIES,
  DIRECT_TOOL_DEFINITIONS,
  DirectToolInputError,
  executeDirectTool,
  findDirectToolDefinition,
  formatCallToolResultForOutput,
  formatDirectToolSchemaText,
  getDirectToolAutoFilledFields,
  getDirectToolCategory,
  getDirectToolDescription,
  getDirectToolDisplayFields,
  loadToolContent,
  prepareDirectToolInputFromJsonText,
  sortDirectToolNames,
  type DirectToolDefinition,
  type DirectToolDisplayField,
} from '@octocodeai/octocode-tools-core/direct';

type ToolResult = Parameters<typeof formatCallToolResultForOutput>[0];

export type ToolDefinition = DirectToolDefinition;
export const TOOL_CATEGORIES = DIRECT_TOOL_CATEGORIES;

const TOOL_RUNTIME_OPTION_KEYS = new Set([
  'queries',
  'json',
  'help',
  'version',
  'list',
  'scheme',
  'compact',
  'format',
  'full',
  'no-color',
]);

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
  return `tools ${toolName} --queries '${exampleInput}'`;
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

function getPayloadQueries(rawPayload: unknown): unknown[] {
  if (Array.isArray(rawPayload)) return rawPayload;
  if (rawPayload && typeof rawPayload === 'object') {
    const queries = (rawPayload as { readonly queries?: unknown }).queries;
    if (Array.isArray(queries)) return queries;
    return [rawPayload];
  }
  return [];
}

function validateRawToolFootguns(toolName: string, inputText: string): void {
  if (toolName !== 'localSearchCode') return;

  let rawPayload: unknown;
  try {
    rawPayload = JSON.parse(inputText) as unknown;
  } catch {
    return;
  }

  const badIndex = getPayloadQueries(rawPayload).findIndex(
    query =>
      query &&
      typeof query === 'object' &&
      Array.isArray((query as { readonly keywords?: unknown }).keywords)
  );
  if (badIndex === -1) return;

  throw new DirectToolInputError(
    'localSearchCode.keywords must be a string, not an array.',
    [
      'Use {"path":".","keywords":"runCLI"} for localSearchCode.',
      'GitHub ghSearchCode uses keywords as an array; localSearchCode does not.',
      `Run tools ${toolName} --scheme before raw calls.`,
    ]
  );
}

export function truncateDescription(desc: string, maxLen: number): string {
  if (desc.length <= maxLen) return desc;
  const cut = desc.lastIndexOf(' ', maxLen - 1);
  return cut > maxLen * 0.6
    ? desc.slice(0, cut) + '…'
    : desc.slice(0, maxLen - 1) + '…';
}

export function formatRequiredFields(toolName: string): string {
  if (toolName === LSP_TOOL_NAME) {
    return '[uri*, type, symbolName?, lineHint?]';
  }

  const tool = findToolDefinition(toolName);
  if (!tool) return '';
  // Top-level fields only — filter out nested dotted paths (e.g. content.patches.ranges.file)
  const fields = getDirectToolDisplayFields(tool.name).filter(
    f => !f.name.includes('.')
  );
  const required = fields.filter(f => f.required).map(f => `${f.name}*`);
  const optional = fields.filter(f => !f.required);
  if (required.length > 0) {
    const optHint = optional.slice(0, 2).map(f => `${f.name}?`);
    const parts = optHint.length > 0 ? [...required, ...optHint] : required;
    return `[${parts.join(', ')}]`;
  }
  return `[${optional
    .slice(0, 3)
    .map(f => `${f.name}?`)
    .join(', ')}]`;
}

function extractShortDescription(fullDescription: string): string {
  return fullDescription
    .split('\n')[0]
    .trim()
    .replace(/^##\s*/, '');
}

function formatFullDescription(fullDescription: string): string {
  const short = extractShortDescription(fullDescription);
  const rest = fullDescription.slice(short.length).trim();
  if (!rest) return '';

  return rest
    .replace(/<\/?[a-z][a-z0-9]*>/gi, '')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

const LSP_TOOL_NAME = 'lspGetSemantics';

const LSP_TYPE_EXAMPLES: Array<[string, Record<string, unknown>]> = [
  [
    'definition — jump to declaration',
    {
      uri: '/path/to/file.ts',
      type: 'definition',
      symbolName: 'myFunction',
      lineHint: 42,
    },
  ],
  [
    'references — all usages',
    {
      uri: '/path/to/file.ts',
      type: 'references',
      symbolName: 'MyClass',
      lineHint: 10,
    },
  ],
  [
    'callers — who calls this function',
    {
      uri: '/path/to/file.ts',
      type: 'callers',
      symbolName: 'handleRequest',
      lineHint: 55,
    },
  ],
  [
    'callees — what this function calls',
    {
      uri: '/path/to/file.ts',
      type: 'callees',
      symbolName: 'handleRequest',
      lineHint: 55,
    },
  ],
  [
    'hover — type signature + docs',
    {
      uri: '/path/to/file.ts',
      type: 'hover',
      symbolName: 'myVar',
      lineHint: 20,
    },
  ],
  [
    'documentSymbols — file outline (no symbolName/lineHint needed)',
    { uri: '/path/to/file.ts', type: 'documentSymbols' },
  ],
  [
    'typeDefinition — where the type was declared',
    {
      uri: '/path/to/file.ts',
      type: 'typeDefinition',
      symbolName: 'myVar',
      lineHint: 20,
    },
  ],
  [
    'implementation — concrete impl of interface member',
    {
      uri: '/path/to/file.ts',
      type: 'implementation',
      symbolName: 'render',
      lineHint: 88,
    },
  ],
];

export async function showAvailableTools(): Promise<void> {
  const metadata = await getOptionalToolMetadata();

  console.log();
  console.log(
    `  ${c('red', bold('SCHEMA REQUIRED — never call a tool without reading its schema:'))}`
  );
  console.log(
    `    ${c('yellow', 'tools <name>')}                                     ${dim('# required fields, types, example call')}`
  );
  console.log(
    `    ${c('yellow', 'tools <name> --scheme')}                            ${dim('# schema only, never runs')}`
  );
  console.log(
    `    ${c('yellow', 'tools <n1> <n2> ...')}                              ${dim('# batch schema reads')}`
  );
  console.log(
    `    ${c('yellow', 'context --full')}                                   ${dim('# full protocol + complete tool descriptions')}`
  );
  console.log(
    `    ${c('yellow', 'context --json')}                                   ${dim('# machine-readable context wrapper')}`
  );
  console.log();

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

    console.log(`  ${bold(category)}`);
    for (const toolName of toolsInCategory) {
      const shortDesc = truncateDescription(
        extractShortDescription(getDirectToolDescription(toolName, metadata)),
        60
      );
      const fields = formatRequiredFields(toolName);
      const namePadded = toolName.padEnd(26);
      const fieldsPadded = fields.padEnd(32);
      console.log(
        `    ${c('cyan', namePadded)} ${dim(fieldsPadded)} ${dim(shortDesc)}`
      );
      if (toolName === LSP_TOOL_NAME) {
        const indent = ''.padEnd(30);
        console.log(
          `    ${dim(indent)} ${dim('type: definition|references|callers|callees|callHierarchy')}`
        );
        console.log(
          `    ${dim(indent)} ${dim('      hover|documentSymbols|typeDefinition|implementation')}`
        );
      }
    }
    console.log();
  }

  console.log(
    `  ${bold('TO CALL')}  tools <name> --queries '<json>'  ${dim('# YAML (default)')}`
  );
  console.log(
    `           tools <name> --queries '<json>' --json  ${dim('# raw envelope')}`
  );
  console.log(
    `           tools <name> --queries '<json>' --compact  ${dim('# leanest')}`
  );
  console.log();
  // Smart commands temporarily unhooked — will be re-added in a future release.
  console.log(`  ${dim('Full protocol: context  |  All commands: --help')}`);
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
  const fullDescription = getDirectToolDescription(tool.name, metadata);
  const shortDesc = extractShortDescription(fullDescription);
  const extendedDesc = formatFullDescription(fullDescription);

  console.log();
  console.log(`  ${c('magenta', bold(tool.name))}  ${dim(shortDesc)}`);
  console.log(
    `  ${dim('Runtime: same Octocode MCP tool implementation under the hood.')}`
  );
  console.log();

  if (extendedDesc) {
    console.log(`  ${bold('Description')}`);
    for (const line of extendedDesc.split('\n')) {
      console.log(`  ${dim(line)}`);
    }
    console.log();
  }

  console.log(`  ${bold('Input Schema')}`);
  for (const field of fields) {
    const reqTag = field.required ? c('red', ' [required]') : '';
    const meta = field.constraints ? `, ${field.constraints}` : '';
    console.log(
      `    ${c('cyan', field.name)} (${field.type}${meta})${reqTag}${field.description ? dim(` - ${field.description}`) : ''}`
    );
  }
  console.log();

  console.log(`  ${dim('Auto-filled')}: ${autoFilledFields.join(', ')}`);
  console.log();

  console.log(`  ${bold('Output Schema')}`);
  console.log(`    ${dim('Default (YAML):')}`);
  console.log(
    `      ${dim('Clean YAML — read directly. Next steps come from typed fields: pagination, next, location, warnings, error.')}`
  );
  console.log(`    ${dim('--json envelope:')}`);
  console.log(
    `      ${c('cyan', 'isError')}                          ${dim('true = tool failed')}`
  );
  console.log(
    `      ${c('cyan', 'content[].text')}                   ${dim('YAML string (same as default output)')}`
  );
  console.log(
    `      ${c('cyan', 'structuredContent.results[]')}      ${dim('tool result objects  (id + data)')}`
  );
  console.log(
    `      ${c('cyan', 'structuredContent.base')}           ${dim('cwd / workspace root used for the query')}`
  );
  console.log(
    `      ${c('cyan', 'structuredContent.pagination')}     ${dim('nextPage / nextCharOffset — page only when present')}`
  );
  console.log(
    `      ${c('cyan', 'structuredContent.next')}           ${dim('typed follow-up params for the next call')}`
  );
  console.log(
    `      ${c('cyan', 'structuredContent.location')}       ${dim('where remote content was saved (kind, localPath, repoRoot, ...)')}`
  );
  console.log(
    `      ${c('cyan', 'structuredContent.warnings[]')}     ${dim('non-fatal issues to account for')}`
  );
  console.log(
    `      ${c('cyan', 'structuredContent.error')}          ${dim('failure detail when isError is true')}`
  );
  console.log();

  console.log(`  ${bold('Flags')}`);
  console.log(
    `    ${c('cyan', '--json')}     ${dim('raw JSON envelope (structuredContent + content + isError)')}`
  );
  console.log(
    `    ${c('cyan', '--compact')}  ${dim('leanest output — fewer tokens')}`
  );

  console.log();

  if (tool.name === LSP_TOOL_NAME) {
    console.log(`  ${bold('Examples by type')}`);
    console.log(
      `  ${dim('Run localSearchCode first to get the exact uri + lineHint, then:')}`
    );
    console.log();
    for (const [label, query] of LSP_TYPE_EXAMPLES) {
      console.log(`    ${dim('#')} ${label}`);
      console.log(
        `    ${c('yellow', `tools ${LSP_TOOL_NAME} --queries '${JSON.stringify(query)}'`)}`
      );
      console.log();
    }
  } else {
    console.log(`  ${bold('Example')}`);
    console.log(`    ${c('yellow', formatToolExampleCommand(tool.name))}`);
    console.log(
      `    ${c('yellow', formatToolExampleCommand(tool.name) + ' --json')}`
    );
    console.log();
  }

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
      const meta = field.constraints ? `, ${field.constraints}` : '';
      console.log(
        `    ${c('cyan', field.name)} (${field.type}${meta})${reqTag}${field.description ? dim(` - ${field.description}`) : ''}`
      );
    }
    console.log(`  ${dim('Auto-filled')}: ${autoFilledFields.join(', ')}`);
    console.log(
      `  ${bold('Example')}  ${c('yellow', formatToolExampleCommand(tool.name))}`
    );
  }

  console.log();
}

export async function getToolsContextString(
  options: { full?: boolean } = {}
): Promise<string> {
  const full = options.full === true;
  const metadata = await loadToolMetadata();
  const toolNames = sortDirectToolNames(Object.keys(metadata.tools));

  const sections: string[] = [
    'Octocode CLI — Agent Context',
    [
      full
        ? 'Agent context: protocol, system prompt, full tool descriptions. Schemas are read separately, on demand.'
        : 'Agent context: protocol, system prompt, short tool descriptions. Use --full for complete descriptions; read schemas separately.',
      'Follow this protocol:',
      '',
      '  *** SCHEMA CHECK — REQUIRED BEFORE EVERY RAW TOOL CALL ***',
      '  This context lists what each tool is for. It does NOT include schemas —',
      "  read a tool's schema before calling it:",
      '    tools <name> --scheme           # schema: fields, types, bounds, defaults',
      '    tools <name>                    # same schema/help shortcut',
      '    tools <n1> <n2> ... --scheme    # batch: read multiple schemas at once',
      '',
      '  *** RESEARCH LOOP ***',
      '  1. Orient: localViewStructure / ghViewRepoStructure / npmSearch.',
      '  2. Search: localSearchCode / ghSearchCode.',
      '  3. Read: localGetFileContent / ghGetFileContent — smallest slice, choose minify standard|symbols|none.',
      '  4. Prove: lspGetSemantics or ghHistoryResearch; stop when evidence.answerReady is true.',
      '',
      '  *** ORIENT CHEAP — BEFORE READING ***',
      '  concise:true         flat string lists — ghSearchRepos→"owner/repo", ghSearchCode→"owner/repo:path", ghHistoryResearch list→"#number title"',
      '  mode:"discovery"     localSearchCode paths only, no snippets (~80% cheaper than paginated)',
      '  minify:"symbols"     skeleton+line-gutter — orient any unknown file first; never paginated',
      '  minify:"standard"    strips comments/blanks — default read mode',
      '  minify:"none"        exact raw text — for quotes, diffs, exact matching',
      '',
      '  *** PAGINATION ***',
      '  Read the typed fields — pagination (nextPage/nextCharOffset) and next carry the exact follow-up params.',
      '  Page only when pagination.hasMore or contentPagination.*.hasMore is true; narrow scope before paging.',
      '  responseCharLength/responseCharOffset (root params, siblings of queries) cap the whole envelope.',
      '',
      '  *** TOOL CALLS ***',
      "  tools <name> --queries '<json>'           # run tool, YAML output",
      "  tools <name> --queries '<json>' --json    # run tool, raw JSON envelope",
      "  tools <name> --queries '<json>' --compact # run tool, leanest output",
      '',
      '  Output: clean YAML by default; use --compact for leanest text, --json for the raw envelope.',
      '',
      '  Exit codes: 0=ok  2=bad-input  3=not-found  4=auth  5=tool-error  7=rate-limited',
      '',
      '  *** REFERENCES ***',
      '  Docs:  https://github.com/bgauryy/octocode/tree/main/docs',
      '  Research playbook: https://github.com/bgauryy/octocode/tree/main/skills/octocode-engineer',
      '  Quick commands (ls/cat/grep/find/repo/pr/pkg/…) are the fastest path; raw `tools` need a schema read first.',
      '  Do not hallucinate paths, lines, or fields — verify with the tools; snippets are discovery, not proof.',
      '',
    ].join('\n'),
    '',
    'Agent System Prompt (Octocode MCP Instructions):',
    metadata.systemPrompt.trim(),
    '',
    'Output contract (all tools):',
    [
      '  Default output: clean YAML — read it directly. No parsing needed.',
      '  Add --compact for leanest output. Add --json for the full envelope below.',
      '',
      '  --json envelope:',
      '    isError: boolean                       true = tool failed',
      '    content[].text: string                 YAML string (same as default output)',
      '    structuredContent.results[]: array     tool result objects (id + data)',
      '    structuredContent.base: string         cwd / workspace root used for the query',
      '    structuredContent.pagination: object   nextPage / nextCharOffset — page only when present',
      '    structuredContent.next: object         typed follow-up params for the next call',
      '    structuredContent.location: object     where remote content was saved (kind, localPath, repoRoot, ...)',
      '    structuredContent.warnings[]: string[] non-fatal issues to account for',
      '    structuredContent.error: object        failure detail when isError is true',
    ].join('\n'),
    '',
    'Tools (grouped by source):',
  ];

  const CATEGORY_ORDER: Array<{
    cat: ReturnType<typeof getDirectToolCategory>;
    label: string;
  }> = [
    { cat: 'GitHub', label: 'GitHub' },
    { cat: 'Local', label: 'Local' },
    { cat: 'LSP', label: 'LSP' },
    { cat: 'Package', label: 'npm' },
    { cat: 'Other', label: 'Other' },
  ];

  let toolIndex = 0;
  for (const { cat, label } of CATEGORY_ORDER) {
    const inCategory = toolNames.filter(
      toolName => getDirectToolCategory(toolName) === cat
    );
    if (inCategory.length === 0) continue;

    sections.push(`${label}:`);
    for (const toolName of inCategory) {
      toolIndex += 1;
      const description = getDirectToolDescription(toolName, metadata);
      if (full) {
        sections.push(`  ${toolIndex}. ${toolName}`);
        sections.push(description.trim());
      } else {
        sections.push(
          `  ${toolIndex}. ${toolName} — ${extractShortDescription(description)}`
        );
      }
    }
    sections.push('');
  }

  sections.push(
    'Schemas are not shown here — read them on demand (required before any call):'
  );
  sections.push(
    '  tools <name> --scheme            # one tool',
    '  tools <n1> <n2> ... --scheme     # several tools at once'
  );

  return sections.join('\n').trim();
}

export async function printToolsContext(
  options: { full?: boolean } = {}
): Promise<void> {
  console.log(await getToolsContextString(options));
}

type OutputMode = 'text' | 'json' | 'compact';

function getOutputMode(args: ParsedArgs): OutputMode {
  if (args.options.compact === true) {
    return 'compact';
  }
  if (args.options.json === true) {
    return 'json';
  }

  return 'text';
}

function printToolResult(result: ToolResult, outputMode: OutputMode): void {
  if (outputMode === 'compact') {
    const structured = (result as { structuredContent?: unknown })
      .structuredContent;
    console.log(JSON.stringify(structured ?? result));
    return;
  }
  console.log(
    formatCallToolResultForOutput(
      result,
      outputMode === 'json' ? 'json' : 'text'
    )
  );
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
    typeof maybeToolName === 'string' ? maybeToolName : undefined;

  if (!toolName || toolName === 'list' || args.options.list === true) {
    await showAvailableTools();
    return true;
  }

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
    process.exitCode = EXIT.NOT_FOUND;
    return false;
  }

  if (args.options.format === 'tool') {
    const metadata = await getOptionalToolMetadata();
    const inputSchema = JSON.parse(formatDirectToolSchemaText(tool.name));
    console.log(
      JSON.stringify(
        {
          name: tool.name,
          description: getDirectToolDescription(tool.name, metadata),
          inputSchema,
        },
        null,
        2
      )
    );
    return true;
  }

  if (args.options.scheme === true) {
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
    process.exitCode = EXIT.USAGE;
    return false;
  }

  if (!inputText) {
    await showToolHelp(tool.name);
    return true;
  }

  try {
    validateRawToolFootguns(tool.name, inputText);
    const input = prepareDirectToolInputFromJsonText(tool.name, inputText, {
      sourceLabel: 'octocode',
      onUnknownFields: (unknownFields, queryIndex) => {
        console.error(
          `  ${c('yellow', '!')} Query ${queryIndex + 1}: unknown field(s): ${unknownFields.join(', ')} — run \`tools ${tool.name}\` to see valid fields.`
        );
      },
    });
    if (!input) {
      await showToolHelp(tool.name);
      return true;
    }

    const result = await executeDirectTool(tool.name, input);
    printToolResult(result, getOutputMode(args));
    if (result.isError) {
      process.exitCode = classifyToolErrorText(JSON.stringify(result));
      return false;
    }
    return true;
  } catch (error) {
    printToolError(
      error instanceof Error ? error.message : 'Tool execution failed.',
      getErrorDetails(error)
    );
    process.exitCode =
      error instanceof DirectToolInputError
        ? EXIT.USAGE
        : classifyToolErrorText(
            error instanceof Error ? error.message : String(error)
          );
    return false;
  }
}

export const toolCommand: CLICommand = {
  name: 'tools',
  options: [
    { name: 'queries', hasValue: true },
    { name: 'list' },
    { name: 'scheme' },
  ],
  handler: async (args: ParsedArgs) => {
    const success = await executeToolCommand(args);
    if (!success && !process.exitCode) {
      process.exitCode = EXIT.GENERAL;
    }
  },
};
