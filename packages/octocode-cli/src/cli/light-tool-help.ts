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
  console.log();
  console.log(`  ${bold('AGENT CONTEXT')}`);
  console.log(
    `    ${c('yellow', 'octocode context')}                                 ${dim('# protocol + system prompt + compact tool schemas')}`
  );
  console.log(
    `    ${c('yellow', 'octocode --context')}                               ${dim('# same agent context shortcut')}`
  );
  console.log(
    `    ${c('yellow', 'octocode context --full')}                          ${dim('# full schemas when runtime loads')}`
  );
  console.log();
  console.log(`  ${bold('RAW TOOL CALLS')}`);
  console.log(
    `    ${c('yellow', 'octocode tools')}                                   ${dim('# list raw MCP tools')}`
  );
  console.log(
    `    ${c('yellow', 'octocode tools <name>')}                            ${dim('# show input fields for one tool')}`
  );
  console.log(
    `    ${c('yellow', 'octocode tools <name> --scheme')}                   ${dim('# show schema/help only')}`
  );
  console.log(
    `    ${c('yellow', "octocode tools <name> --queries '<json>'")}         ${dim('# run one tool when runtime loads')}`
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
  console.log('Octocode CLI — Agent Context');
  console.log();
  console.log(
    'This fallback output shows the CLI protocol and compact tool summaries. Full MCP metadata needs the packaged runtime.'
  );
  console.log();
  console.log(
    'Smart commands: get, tree, files, search, pr, repo, pkg, symbols, lsp.'
  );
  console.log(
    '1. Start research with smart commands: tree/repo/pkg/pr -> files/search -> get -> symbols/lsp or PR content.'
  );
  console.log('2. Show smart-command help with:');
  console.log('   octocode <command> --help');
  console.log('3. For raw MCP tools, inspect the schema before calling:');
  console.log('   octocode tools <name> --scheme');
  console.log('   octocode tools <name>');
  console.log("   octocode tools <name> --queries '<json>'");
  console.log(
    '4. Use `octocode context` or `octocode --context` for the agent protocol and system prompt; add --full for full schemas when runtime loads.'
  );
  console.log(
    '5. Read default YAML directly; use --json only when you need the envelope.'
  );
  console.log();
  console.log('Smart commands:');
  console.log('  octocode get <path|owner/repo/file>');
  console.log('  octocode tree <path|owner/repo>');
  console.log('  octocode files <query> [path|repo]');
  console.log('  octocode search <pattern> <path|repo>');
  console.log('  octocode pr <owner/repo[#N] | PR-URL>');
  console.log('  octocode repo <keywords...>');
  console.log('  octocode pkg <package>');
  console.log('  octocode symbols <file|path>       # outline first');
  console.log('  octocode lsp <file> --type <type>  # nav after symbol+line');
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
