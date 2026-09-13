// The main `tools <name> ...` dispatcher: routes to list/schema/help views,
// or parses input and actually runs the tool (loading the engine-bearing
// `/direct` module only at that point — see the P3 note below).
import type { ParsedArgs } from '../types.js';
import { EXIT, classifyToolErrorText } from '../exit-codes.js';
import { c, dim } from '../../utils/colors.js';
import {
  DirectToolInputError,
  formatDirectToolSchemaText,
  getDirectToolDescription,
  prepareDirectToolInputFromJsonText,
} from '@octocodeai/octocode-core/schema';
import type { formatCallToolResultForOutput } from '@octocodeai/octocode-tools-core/direct';
import {
  TOOL_DEFINITIONS,
  findToolDefinition,
  getToolEnableInstruction,
} from './registry.js';
import { getInputText, validateRawToolFootguns } from './input.js';
import {
  buildQueryFromFlags,
  extractToolArgvTail,
  hasToolFlagInput,
} from './flags-to-query.js';
import {
  printJsonPayload,
  printMultipleToolSchemasJson,
  printToolCatalogJson,
  printToolSchemaJson,
} from './catalog-json.js';
import { showAvailableTools } from './list-view.js';
import {
  showMultipleToolSchemas,
  showToolHelp,
  showToolHelpBrief,
} from './help.js';

type ToolResult = Parameters<typeof formatCallToolResultForOutput>[0];

type OutputMode = 'text' | 'json' | 'compact';

// Minified structured JSON is the default: it is the cheapest representation
// for agents (fewest tokens) and identical to what MCP serves. Humans opt into
// the readable YAML view with --yaml (alias --text); --json keeps the pretty
// structured form.
function getOutputMode(args: ParsedArgs): OutputMode {
  if (args.options.yaml === true || args.options.text === true) {
    return 'text';
  }
  if (args.options.json === true && args.options.compact !== true) {
    return 'json';
  }

  return 'compact';
}

function printToolResult(
  result: ToolResult,
  args: ParsedArgs,
  outputMode: OutputMode,
  formatResult: typeof formatCallToolResultForOutput
): void {
  if (outputMode !== 'text') {
    const structured = (result as { structuredContent?: unknown })
      .structuredContent;
    console.log(
      JSON.stringify(
        structured ?? result,
        null,
        outputMode === 'json' || args.options.pretty === true ? 2 : 0
      )
    );
    return;
  }
  console.log(formatResult(result, 'text'));
}

function printToolError(message: string, details: string[] = []): void {
  console.log();
  console.log(`  ${c('red', 'x')} ${message}`);
  for (const detail of details) {
    console.log(`  ${dim('-')} ${detail}`);
  }
  console.log();
}

function printToolCommandError(
  args: ParsedArgs,
  toolName: string | undefined,
  message: string,
  details: string[] = []
): void {
  if (getOutputMode(args) !== 'text') {
    printJsonPayload(
      {
        kind: 'octocode.toolError',
        version: 1,
        ...(toolName ? { tool: toolName } : {}),
        error: message,
        ...(details.length > 0 ? { details } : {}),
      },
      getOutputMode(args) === 'compact',
      args.options.pretty === true
    );
    return;
  }

  printToolError(message, details);
}

function getErrorDetails(error: unknown): string[] {
  return error instanceof DirectToolInputError
    ? error.details.map(detail =>
        detail.replace(/(Run tools \S+ --scheme)(?! --brief)/, '$1 --brief')
      )
    : [];
}

export async function executeToolCommand(args: ParsedArgs): Promise<boolean> {
  const maybeToolName = args.args[0];
  const toolName =
    typeof maybeToolName === 'string' ? maybeToolName : undefined;

  if (args.options.list !== undefined) {
    printToolCommandError(
      args,
      toolName,
      'Unsupported tools option: --list. Run `tools` to list the catalog.'
    );
    process.exitCode = EXIT.USAGE;
    return false;
  }

  if (!toolName) {
    if (args.options.json === true) {
      await printToolCatalogJson({
        full: args.options.full === true,
        compact: args.options.compact === true,
        pretty: args.options.pretty === true,
      });
      return true;
    }
    await showAvailableTools();
    return true;
  }

  if (
    args.args.length > 1 &&
    typeof args.options.queries !== 'string' &&
    args.args.every(n => findToolDefinition(n) !== undefined)
  ) {
    if (args.options.json === true) {
      await printMultipleToolSchemasJson(args.args, {
        compact: args.options.compact === true,
        pretty: args.options.pretty === true,
      });
    } else {
      await showMultipleToolSchemas(args.args);
    }
    return true;
  }

  const tool = findToolDefinition(toolName);
  if (!tool) {
    printToolCommandError(args, toolName, `Unknown tool: ${toolName}`, [
      `Available tools: ${TOOL_DEFINITIONS.map(item => item.name).join(', ')}`,
    ]);
    process.exitCode = EXIT.NOT_FOUND;
    return false;
  }

  if (args.options.format === 'tool') {
    const inputSchema = JSON.parse(formatDirectToolSchemaText(tool.name));
    console.log(
      JSON.stringify(
        {
          name: tool.name,
          description: getDirectToolDescription(tool.name),
          inputSchema,
        },
        null,
        2
      )
    );
    return true;
  }

  if (args.options.scheme === true) {
    if (args.options.brief === true) {
      await showToolHelpBrief(tool.name);
      return true;
    }
    if (args.options.json === true) {
      await printToolSchemaJson(tool.name, {
        compact: args.options.compact === true,
        pretty: args.options.pretty === true,
      });
      return true;
    }
    await showToolHelp(tool.name);
    return true;
  }

  if (tool.disabled) {
    const instruction = getToolEnableInstruction(tool.name);
    printToolCommandError(
      args,
      tool.name,
      `Tool '${tool.name}' is disabled${instruction ? ` — ${instruction}.` : '.'}`
    );
    process.exitCode = EXIT.NOT_FOUND;
    return false;
  }

  let inputText: string | undefined;
  try {
    if (typeof args.options.queries === 'string') {
      inputText = getInputText(tool.name, args);
    } else {
      // Schema-flag input: `tools astSearch tree --path src --max-depth 2`.
      // Flags compile into the same single-query JSON that --queries takes,
      // so validation and execution below are identical for both paths.
      const tail = extractToolArgvTail(args.raw, tool.name, args);
      if (hasToolFlagInput(tail)) {
        inputText = JSON.stringify(buildQueryFromFlags(tool.name, tail));
      }
    }
  } catch (error) {
    printToolCommandError(
      args,
      tool.name,
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
      rejectUnknownFields: true,
    });
    if (!input) {
      await showToolHelp(tool.name);
      return true;
    }

    // Engine-bearing modules are loaded only now, when a tool actually runs —
    // keeping the schema/help paths above engine-free (P3).
    const { executeDirectTool, formatCallToolResultForOutput } =
      await import('@octocodeai/octocode-tools-core/direct');
    const outputMode = getOutputMode(args);
    const result = await (outputMode !== 'text'
      ? executeDirectTool(tool.name, input, { resultProjection: 'structured' })
      : executeDirectTool(tool.name, input));
    printToolResult(result, args, outputMode, formatCallToolResultForOutput);
    if (result.isError) {
      process.exitCode = classifyToolErrorText(JSON.stringify(result));
      return false;
    }
    return true;
  } catch (error) {
    printToolCommandError(
      args,
      tool.name,
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
