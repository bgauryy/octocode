import { c, bold, dim } from '../utils/colors.js';
import {
  HELP_TOOL_CATEGORIES,
  HELP_TOOL_DEFINITIONS,
} from './tool-help-data.js';

function findHelpTool(toolName: string) {
  return HELP_TOOL_DEFINITIONS.find(tool => tool.name === toolName);
}

export function showLightAvailableTools(): void {
  console.log();
  console.log(
    `  ${c('magenta', bold('Octocode Tools'))}  ${dim('runtime-light schema list')}`
  );
  console.log();
  console.log(
    `  ${dim('For full schemas and execution, the Octocode runtime must load successfully.')}`
  );

  for (const category of HELP_TOOL_CATEGORIES) {
    const tools = HELP_TOOL_DEFINITIONS.filter(
      tool => tool.category === category
    );
    if (tools.length === 0) continue;

    console.log();
    console.log(`  ${bold(category)}`);
    for (const tool of tools) {
      console.log(`    ${c('cyan', tool.name.padEnd(28))} ${dim(tool.fields)}`);
    }
  }

  console.log();
}

export function showLightToolHelp(toolName: string): boolean {
  const tool = findHelpTool(toolName);
  if (!tool) {
    return false;
  }

  console.log();
  console.log(`  ${c('magenta', bold(tool.name))}  ${dim(tool.category)}`);
  console.log();
  console.log(`  ${bold('Input Fields')}`);
  console.log(`    ${dim(tool.fields)}`);
  console.log();
  console.log(`  ${bold('Example')}`);
  console.log(
    `    ${c('yellow', `octocode tools ${tool.name} --queries '<json>'`)}`
  );
  console.log();
  console.log(
    `  ${dim('Full schema unavailable because the Octocode runtime did not load.')}`
  );
  console.log();
  return true;
}

export function printLightInstructions(options: { full?: boolean } = {}): void {
  console.log('Octocode CLI — Agent Protocol');
  console.log();
  console.log(
    '1. Use smart commands first for common research: get, tree, search, pr, pkg, symbols, lsp.'
  );
  console.log('2. For raw tools, inspect the schema before calling:');
  console.log('   octocode tools <name>');
  console.log("   octocode tools <name> --queries '<json>'");
  console.log(
    '3. Read default YAML directly; use --json only when you need the envelope.'
  );
  console.log();
  console.log('Smart commands:');
  console.log('  octocode get <path|owner/repo/file>');
  console.log('  octocode tree <path|owner/repo>');
  console.log('  octocode search <pattern> <path|repo>');
  console.log('  octocode pr <owner/repo[#N] | PR-URL>');
  console.log('  octocode pkg <package>');
  console.log('  octocode symbols <file|path>');
  console.log('  octocode lsp <file> --type <type>');
  console.log();
  showLightAvailableTools();
  if (options.full) {
    console.log(
      dim(
        'Full JSON schemas unavailable because the Octocode runtime did not load.'
      )
    );
  }
}

export function printToolRuntimeUnavailable(): void {
  console.log();
  console.log(`  ${c('red', 'x')} Octocode tool runtime failed to load.`);
  console.log(
    `  ${dim('Schema summaries are available, but tool execution requires the packaged runtime.')}`
  );
  console.log();
}
