import { c, bold, dim } from '../utils/colors.js';

/**
 * Light fallback shown when the Octocode tool runtime fails to load.
 * Never lists tool names or fields statically — those come from the live runtime.
 */
export function showLightAvailableTools(): void {
  console.log();
  console.log(
    `  ${c('magenta', bold('Octocode Tools'))}  ${dim('runtime unavailable')}`
  );
  console.log();
  console.log(
    `  ${dim('The tool runtime did not load. Tool names and schemas are only available when the runtime starts.')}`
  );
  console.log();
  console.log(`  ${bold('When the runtime loads, use:')}`);
  console.log(
    `    ${c('yellow', 'octocode tools')}                                   ${dim('# list all tools with live schema')}`
  );
  console.log(
    `    ${c('yellow', 'octocode tools <name>')}                            ${dim('# show full input schema for one tool')}`
  );
  console.log(
    `    ${c('yellow', 'octocode tools <name> --scheme')}                   ${dim('# schema only, never runs')}`
  );
  console.log(
    `    ${c('yellow', "octocode tools <name> --queries '<json>'")}         ${dim('# run a tool')}`
  );
  console.log();
  console.log(`  ${bold('AGENT CONTEXT')}`);
  console.log(
    `    ${c('yellow', 'octocode context')}                                 ${dim('# protocol + system prompt + compact tool schemas')}`
  );
  console.log(
    `    ${c('yellow', 'octocode context --full')}                          ${dim('# full schemas when runtime loads')}`
  );
  console.log();
}

/**
 * Returns false so the caller falls back to showLightAvailableTools().
 * Per-tool help requires the live runtime — no static fallback to avoid stale data.
 */
export function showLightToolHelp(_toolName: string): boolean {
  return false;
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
