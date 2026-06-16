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
  formatDirectToolMetadataSchemaText,
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

const CANONICAL_TOOL_USAGE = [
  'octocode tools                                   # list all tools',
  'octocode tools <name>                            # show input schema',
  'octocode tools <name> --scheme                   # show input/output schema explicitly',
  'octocode tools <n1> <n2> ...                     # batch input schemas',
  "octocode tools <name> --queries '<json>'         # run a tool",
  "octocode tools <name> --queries '<json>' --json  # run, raw JSON output",
  'octocode context                                 # agent protocol + MCP system prompt + compact tool schemas',
  'octocode context --full                          # same, plus full JSON schemas',
  'octocode --context                               # same as octocode context',
  'octocode --context --full                        # same as octocode context --full',
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
  const fields = getDirectToolDisplayFields(tool.name);
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
    `  ${c('magenta', bold('Octocode Tools'))}  ${dim('(* = required field)')}`
  );
  console.log();
  console.log(
    `  ${c('red', bold('REQUIRED BEFORE CALLING ANY RAW MCP TOOL:'))} read its schema first`
  );
  console.log();
  console.log(`  ${bold('AGENT CONTEXT')}`);
  console.log(
    `    ${c('yellow', 'octocode context')}                                 ${dim('# protocol + system prompt + compact tool schemas')}`
  );
  console.log(
    `    ${c('yellow', 'octocode context --full')}                          ${dim('# all tool descriptions + full JSON schemas')}`
  );
  console.log();
  console.log(`  ${bold('RAW TOOL CALLS')}`);
  console.log(
    `    ${c('yellow', 'octocode tools')}                                   ${dim('# list all raw MCP tools')}`
  );
  console.log(
    `    ${c('yellow', 'octocode tools <name>')}                            ${dim('# show input schema/help for one tool')}`
  );
  console.log(
    `    ${c('yellow', 'octocode tools <name> --scheme')}                   ${dim('# show input/output schema, never runs')}`
  );
  console.log(
    `    ${c('yellow', "octocode tools <name> --queries '<json>'")}         ${dim('# run one tool')}`
  );
  console.log(
    `    ${c('yellow', "octocode tools <name> --queries '<json>' --json")}  ${dim('# run with raw JSON envelope')}`
  );
  console.log(
    `    ${c('yellow', 'octocode tools <n1> <n2> ...')}                     ${dim('# batch-read schemas')}`
  );
  console.log();
  console.log(
    `  ${bold('TIP')}  ${dim('For common research use smart commands — no schema needed:')}`
  );
  console.log(
    `    ${c('cyan', 'octocode get')}    ${dim('<path | owner/repo/file>')}    ${dim('fetch + minify  [--mode none|standard|symbols]')}`
  );
  console.log(
    `    ${c('cyan', 'octocode tree')}   ${dim('<path | owner/repo>')}         ${dim('directory tree  [--depth N]')}`
  );
  console.log(
    `    ${c('cyan', 'octocode files')}  ${dim('<query> [path|repo]')}         ${dim('file discovery [--search path|content|both]')}`
  );
  console.log(
    `    ${c('cyan', 'octocode search')} ${dim('<pattern> <path|repo>')}       ${dim('code search     [--type, --limit]')}`
  );
  console.log(
    `    ${c('cyan', 'octocode pr')}     ${dim('<owner/repo[#N] | URL>')}      ${dim('PR info         [--patches, --deep]')}`
  );
  console.log(
    `    ${c('cyan', 'octocode repo')}   ${dim('<keywords...>')}               ${dim('repo discovery  [--topic, --language, --stars]')}`
  );
  console.log(
    `    ${c('cyan', 'octocode pkg')}    ${dim('<package>')}                   ${dim('npm metadata + source repo')}`
  );
  console.log(
    `    ${c('cyan', 'octocode symbols')} ${dim('<file|path>')}                 ${dim('semantic outline before LSP')}`
  );
  console.log(
    `    ${c('cyan', 'octocode lsp')}    ${dim('<file> --type <type>')}        ${dim('semantic nav after symbol+line')}`
  );
  console.log(
    `    ${dim('Full command scheme:')} ${c('yellow', 'octocode <command> --help')}`
  );
  console.log(
    `    ${dim('Research loop:')} ${c('cyan', 'tree/repo/pkg/pr')} ${dim('→')} ${c('cyan', 'files/search')} ${dim('→')} ${c('cyan', 'get')} ${dim('→')} ${c('cyan', 'symbols/lsp or PR content')}`
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

    console.log();
    console.log(`  ${bold(category)}`);
    for (const toolName of toolsInCategory) {
      const shortDesc = truncateDescription(
        extractShortDescription(getDirectToolDescription(toolName, metadata)),
        68
      );
      const fields = formatRequiredFields(toolName);
      const namePadded = toolName.padEnd(26);
      const fieldsPadded = fields.padEnd(28);
      console.log(
        `    ${c('cyan', namePadded)} ${dim(fieldsPadded)} ${dim(shortDesc)}`
      );
      if (toolName === LSP_TOOL_NAME) {
        const indent = ''.padEnd(26 + 4);
        console.log(
          `    ${dim(indent)} ${dim('type: definition|references|callers|callees|callHierarchy')}`
        );
        console.log(
          `    ${dim(indent)} ${dim('      hover|documentSymbols|typeDefinition|implementation')}`
        );
      }
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
    console.log(
      `    ${c('cyan', field.name)} (${field.type})${reqTag}${field.description ? dim(` - ${field.description}`) : ''}`
    );
  }
  console.log();

  console.log(`  ${dim('Auto-filled')}: ${autoFilledFields.join(', ')}`);
  console.log();

  console.log(`  ${bold('Output Schema')}`);
  console.log(`    ${dim('Default (YAML):')}`);
  console.log(
    `      ${dim('Clean YAML — read directly. Trust hints[] for next steps.')}`
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
    `      ${c('cyan', 'structuredContent.hints[]')}        ${dim('next-step suggestions — follow them')}`
  );
  console.log(
    `      ${c('cyan', 'structuredContent.evidence')}       ${dim('{ answerReady, complete, kind }')}`
  );
  console.log(
    `      ${dim('Trust evidence.answerReady — true = answer complete, stop calling')}`
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
        `    ${c('yellow', `octocode tools ${LSP_TOOL_NAME} --queries '${JSON.stringify(query)}'`)}`
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

function formatToolFieldsCompact(toolName: string): string {
  const fields = getDirectToolDisplayFields(toolName);
  if (fields.length === 0) {
    return '  (no input fields)';
  }
  return fields
    .map(field => {
      const req = field.required ? ' [required]' : '';
      return `  ${field.name} (${field.type})${req}`;
    })
    .join('\n');
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
        ? 'This is the full agent target: CLI protocol, MCP system prompt, tool descriptions, and full JSON schemas.'
        : 'This is the compact agent target: CLI protocol, MCP system prompt, tool descriptions, and schema summaries.',
      'Tool runtime: `octocode tools` runs the same Octocode MCP tool implementations under the hood.',
      full
        ? 'Use `octocode context` for the shorter version.'
        : 'Use `octocode context --full` when you need every JSON schema inline.',
      'Shortcut: `octocode --context` prints this same agent target.',
      'Follow this protocol:',
      '',
      '  *** SCHEMA CHECK — REQUIRED BEFORE EVERY RAW TOOL CALL ***',
      "  Always read a tool's schema before calling it:",
      '    octocode tools <name> --scheme           # schema: required fields, types, examples',
      '    octocode tools <name>                    # same schema/help shortcut',
      '    octocode tools <n1> <n2> ...             # batch: read multiple schemas at once',
      full
        ? '  Full JSON schemas are included in this output below.'
        : '  Compact schema summaries are included below; use --full for exact JSON schemas.',
      '',
      '  *** RESEARCH LOOP ***',
      '  1. Orient: tree / repo / pkg / pr.',
      '  2. Search: files / search.',
      '  3. Read: get exact slices; choose --mode standard|symbols|none.',
      '  4. Prove: symbols/lsp or PR content; stop when evidence.answerReady is true.',
      '',
      '  *** SMART COMMANDS — USE THESE FIRST for file / search / repo / PR / package / LSP ***',
      '  These cover common flows without raw schemas; file/search commands auto-route local ↔ GitHub:',
      '    octocode get <path|owner/repo/file>      — fetch + minify (--mode none|standard|symbols, --match-string, --start-line, --end-line, --full-content)',
      '    octocode tree <path|owner/repo>          — directory structure (--depth <n>)',
      '    octocode files <query> [path|repo]       — file path/content discovery (--search path|content|both, --source auto|local|github)',
      '    octocode search <pattern> <path|repo>    — code search (--type, --branch, --limit, --page)',
      '    octocode pr <owner/repo[#N] | PR-URL>    — PR list/search OR deep-dive (--patches, --comments, --commits, --deep)',
      '    octocode repo <keywords...>              — repository discovery (--topic, --language, --stars, --sort)',
      '    octocode pkg <package>                   — npm metadata + source repository',
      '    octocode symbols <file|path>              — semantic outline / documentSymbols before LSP navigation',
      '    octocode lsp <file> --type <type>         — semantic nav after search/symbols gives --symbol + --line',
      '  Full smart-command scheme: octocode <command> --help',
      '',
      '  *** TOOL CALLS ***',
      "  octocode tools <name> --queries '<json>'           # run tool, YAML output",
      "  octocode tools <name> --queries '<json>' --json    # run tool, raw JSON envelope",
      "  octocode tools <name> --queries '<json>' --compact # run tool, leanest output",
      '',
      '  Output: clean YAML by default; use --compact for leanest text, --json for the raw envelope.',
      '',
      '  Exit codes: 0=ok  2=bad-input  3=not-found  4=auth  5=tool-error  7=rate-limited',
      '',
      '  Tool list: `octocode tools`   All commands: `octocode --help`',
    ].join('\n'),
    '',
    'CLI Usage:',
    CANONICAL_TOOL_USAGE,
    '',
    'Agent System Prompt (Octocode MCP Instructions):',
    metadata.instructions.trim(),
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
      '    structuredContent.hints[]: string[]    next-step suggestions — follow them',
      '    structuredContent.evidence: object     { answerReady: boolean, complete: boolean, kind: string }',
      '  Trust evidence.answerReady — when true, the answer is complete; stop calling.',
    ].join('\n'),
    '',
    'Tools:',
  ];

  toolNames.forEach((toolName, index) => {
    const description = getDirectToolDescription(toolName, metadata);

    sections.push(`${index + 1}. ${toolName}`);
    sections.push(
      `Description: ${full ? description.trim() : extractShortDescription(description)}`
    );

    if (full) {
      const schemaText = findDirectToolDefinition(toolName)
        ? formatDirectToolSchemaText(toolName)
        : formatDirectToolMetadataSchemaText(metadata.tools[toolName]?.schema);
      sections.push('Input schema:');
      sections.push(schemaText);
    } else if (findDirectToolDefinition(toolName)) {
      sections.push('Input fields:');
      sections.push(formatToolFieldsCompact(toolName));
    } else {
      sections.push('Input schema:');
      sections.push(
        formatDirectToolMetadataSchemaText(metadata.tools[toolName]?.schema)
      );
    }
    sections.push('');
  });

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
    const input = prepareDirectToolInputFromJsonText(tool.name, inputText, {
      sourceLabel: 'octocode-cli',
      onUnknownFields: (unknownFields, queryIndex) => {
        console.error(
          `  ${c('yellow', '!')} Query ${queryIndex + 1}: unknown field(s): ${unknownFields.join(', ')} — run \`octocode tools ${tool.name}\` to see valid fields.`
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
  description:
    'Run an Octocode MCP tool directly using the same implementation under the hood',
  usage: `octocode tools <toolName> [--scheme] [--queries '<json-stringified-input>']`,
  options: [
    {
      name: 'queries',
      description: 'JSON-stringified tool input (query object or array).',
      hasValue: true,
    },
    {
      name: 'list',
      description: 'List available tools.',
    },
    {
      name: 'scheme',
      description:
        'Show the selected tool schema summary instead of running it.',
    },
  ],
  handler: async (args: ParsedArgs) => {
    const success = await executeToolCommand(args);
    if (!success && !process.exitCode) {
      process.exitCode = EXIT.GENERAL;
    }
  },
};
