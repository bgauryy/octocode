// Human-facing per-tool help: `tools <name>` (single) and `tools <n1> <n2>
// ...` (batch schema-only) views.
import { c, bold, dim } from '../../utils/colors.js';
import {
  buildDirectToolCommandPatterns,
  getDirectToolAutoFilledFields,
  getDirectToolDescription,
  getDirectToolDisplayFields,
} from '@octocodeai/octocode-tools-core/schema';
import { findToolDefinition, getOptionalToolMetadata } from './registry.js';
import {
  LSP_TOOL_NAME,
  extractShortDescription,
  formatFullDescription,
  formatToolExampleCommand,
  getToolSchemaGuidance,
} from './formatting.js';
import { LSP_TYPE_EXAMPLES } from './lsp-examples.js';

// `tools <name> --scheme --brief`: params + one example, nothing else — the
// cheapest schema read for agents that just need callable field names.
export async function showToolHelpBrief(toolName: string): Promise<boolean> {
  const tool = findToolDefinition(toolName);
  if (!tool) {
    return false;
  }

  const metadata = await getOptionalToolMetadata();
  const fields = getDirectToolDisplayFields(tool.name);
  const shortDesc = extractShortDescription(
    getDirectToolDescription(tool.name, metadata)
  );

  console.log();
  console.log(`  ${c('magenta', bold(tool.name))}  ${dim(shortDesc)}`);
  console.log();
  for (const field of fields) {
    const reqTag = field.required ? c('red', ' [required]') : '';
    const meta = field.constraints ? `, ${field.constraints}` : '';
    console.log(
      `    ${c('cyan', field.name)} (${field.type}${meta})${reqTag}${field.description ? dim(` - ${field.description}`) : ''}`
    );
  }
  console.log();
  if (tool.name === LSP_TOOL_NAME) {
    const [label, query] = LSP_TYPE_EXAMPLES[0]!;
    console.log(`    ${dim('#')} ${label}`);
    console.log(
      `    ${c('yellow', `tools ${LSP_TOOL_NAME} --queries '${JSON.stringify(query)}'`)}`
    );
  } else {
    console.log(`    ${c('yellow', formatToolExampleCommand(tool.name))}`);
  }
  console.log();
  return true;
}

export async function showToolHelp(toolName: string): Promise<boolean> {
  const tool = findToolDefinition(toolName);
  if (!tool) {
    return false;
  }

  const metadata = await getOptionalToolMetadata();
  const fields = getDirectToolDisplayFields(tool.name);
  const autoFilledFields = getDirectToolAutoFilledFields(tool.name);
  const commandPatterns = buildDirectToolCommandPatterns(tool.name);
  const fullDescription = getDirectToolDescription(tool.name, metadata);
  const shortDesc = extractShortDescription(fullDescription);
  const extendedDesc = formatFullDescription(fullDescription);
  const guidance = getToolSchemaGuidance(tool.name);

  console.log();
  console.log(`  ${c('magenta', bold(tool.name))}  ${dim(shortDesc)}`);
  console.log(
    `  ${dim('Runtime: same Octocode MCP tool implementation under the hood.')}`
  );
  for (const line of guidance) {
    console.log(`  ${dim(line)}`);
  }
  console.log();

  if (extendedDesc) {
    console.log(`  ${bold('Description')}`);
    for (const line of extendedDesc.split('\n')) {
      console.log(`  ${dim(line)}`);
    }
    console.log();
  }

  if (commandPatterns.length > 0 && tool.name !== LSP_TOOL_NAME) {
    console.log(
      `  ${bold(commandPatterns.length === 1 ? 'Command Pattern' : 'Command Patterns')}`
    );
    for (const pattern of commandPatterns) {
      console.log(`    ${dim('#')} ${pattern.label}`);
      console.log(`    ${c('yellow', pattern.command)}`);
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

  console.log(`  ${bold('Flags')}`);
  console.log(
    `    ${c('cyan', '--json')}     ${dim('raw JSON envelope (structuredContent + content + isError)')}`
  );
  console.log(
    `    ${c('cyan', '--compact')}  ${dim('lean structuredContent JSON')}`
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
    const exampleCommand = formatToolExampleCommand(tool.name);
    console.log(`    ${c('yellow', exampleCommand)}`);
    console.log(`    ${c('yellow', exampleCommand + ' --json')}`);
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
    const commandPatterns = buildDirectToolCommandPatterns(tool.name);
    const guidance = getToolSchemaGuidance(tool.name);

    console.log();
    console.log(`  ${c('magenta', bold(tool.name))}  ${dim(shortDesc)}`);
    for (const line of guidance) {
      console.log(`  ${dim(line)}`);
    }
    console.log(`  ${bold('Input Schema')}`);
    for (const field of fields) {
      const reqTag = field.required ? c('red', ' [required]') : '';
      const meta = field.constraints ? `, ${field.constraints}` : '';
      console.log(
        `    ${c('cyan', field.name)} (${field.type}${meta})${reqTag}${field.description ? dim(` - ${field.description}`) : ''}`
      );
    }
    console.log(`  ${dim('Auto-filled')}: ${autoFilledFields.join(', ')}`);
    const exampleCommand =
      commandPatterns[0]?.command ?? formatToolExampleCommand(tool.name);
    console.log(`  ${bold('Example')}  ${c('yellow', exampleCommand)}`);
  }

  console.log();
}
