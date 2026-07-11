// The human-facing `tools` (no args) listing: tools grouped by category with
// concise descriptions, plus the schema/run/json quick-reference footer.
import { c, bold, dim } from '../../utils/colors.js';
import {
  getDirectToolCategory,
  sortDirectToolNames,
} from '@octocodeai/octocode-tools-core/schema';
import {
  TOOL_CATEGORIES,
  TOOL_DEFINITIONS,
  getOptionalToolMetadata,
} from './registry.js';
import { formatConciseToolDescription } from './formatting.js';

export async function showAvailableTools(): Promise<void> {
  const metadata = await getOptionalToolMetadata();
  const toolNames = sortDirectToolNames(
    TOOL_DEFINITIONS.map(tool => tool.name)
  );

  console.log();
  console.log(
    `  ${c('magenta', bold(`Octocode Tools (${toolNames.length})`))}  ${dim('name + concise description')}`
  );
  console.log();

  for (const category of TOOL_CATEGORIES) {
    const toolsInCategory = toolNames.filter(
      toolName => getDirectToolCategory(toolName) === category
    );
    if (toolsInCategory.length === 0) {
      continue;
    }

    console.log(`  ${bold(category)}`);
    for (const toolName of toolsInCategory) {
      const namePadded = toolName.padEnd(26);
      console.log(
        `    ${c('cyan', namePadded)} ${dim(formatConciseToolDescription(toolName, metadata))}`
      );
    }
    console.log();
  }

  console.log(
    `  ${bold('SCHEMA')}  ${c('yellow', 'tools <name> --scheme')}  ${dim('# required before raw calls')}`
  );
  console.log(
    `  ${bold('RUN')}     ${c('yellow', "tools <name> --queries '<json>' --compact")}  ${dim('# lean tool output')}`
  );
  console.log(
    `  ${bold('JSON')}    ${c('yellow', 'tools --json --compact')}  ${dim('# lean machine catalog')}`
  );
  console.log();
  console.log(`  ${dim('Full protocol: context  |  Help: tools <name>')}`);
  console.log();
}
