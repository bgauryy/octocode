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
} from '@octocodeai/octocode-tools-core/schema';
import type { formatCallToolResultForOutput } from '@octocodeai/octocode-tools-core/direct';
import {
  TOOL_DEFINITIONS,
  findToolDefinition,
  getOptionalToolMetadata,
} from './registry.js';
import { getInputText, validateRawToolFootguns } from './input.js';
import {
  printJsonPayload,
  printToolCatalogJson,
  printToolSchemaJson,
} from './catalog-json.js';
import { showAvailableTools } from './list-view.js';
import { showMultipleToolSchemas, showToolHelp } from './help.js';

type ToolResult = Parameters<typeof formatCallToolResultForOutput>[0];

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

function printToolResult(
  result: ToolResult,
  outputMode: OutputMode,
  formatResult: typeof formatCallToolResultForOutput
): void {
  if (outputMode === 'compact') {
    const structured = (result as { structuredContent?: unknown })
      .structuredContent;
    console.log(JSON.stringify(structured ?? result));
    return;
  }
  console.log(formatResult(result, outputMode === 'json' ? 'json' : 'text'));
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
  if (args.options.json === true || args.options.compact === true) {
    printJsonPayload(
      {
        kind: 'octocode.toolError',
        version: 1,
        ...(toolName ? { tool: toolName } : {}),
        error: message,
        ...(details.length > 0 ? { details } : {}),
      },
      args.options.compact === true
    );
    return;
  }

  printToolError(message, details);
}

function getErrorDetails(error: unknown): string[] {
  return error instanceof DirectToolInputError ? error.details : [];
}

export async function executeToolCommand(args: ParsedArgs): Promise<boolean> {
  const maybeToolName = args.args[0];
  const toolName =
    typeof maybeToolName === 'string' ? maybeToolName : undefined;

  if (!toolName || toolName === 'list' || args.options.list === true) {
    if (args.options.json === true) {
      await printToolCatalogJson({
        full: args.options.full === true,
        compact: args.options.compact === true,
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
    await showMultipleToolSchemas(args.args);
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
    if (args.options.json === true) {
      await printToolSchemaJson(tool.name, {
        compact: args.options.compact === true,
      });
      return true;
    }
    await showToolHelp(tool.name);
    return true;
  }

  let inputText: string | undefined;
  try {
    inputText = getInputText(tool.name, args);
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
    const result = await executeDirectTool(tool.name, input);
    printToolResult(result, getOutputMode(args), formatCallToolResultForOutput);
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
