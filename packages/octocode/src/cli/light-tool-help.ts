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
  console.log(
    `  ${dim('Common cause: the native engine addon (.node) was rejected by a sandboxed/app-embedded Node')}`
  );
  console.log(
    `  ${dim('(e.g. an editor runtime). Re-run with system Node — check `which node` — for local dogfood.')}`
  );
  console.log();
  console.log(`  ${bold('When the runtime loads, use:')}`);
  console.log(
    `    ${c('yellow', 'tools')}                                            ${dim('# list all tools with live schema')}`
  );
  console.log(
    `    ${c('yellow', 'tools <name>')}                                     ${dim('# show full input schema for one tool')}`
  );
  console.log(
    `    ${c('yellow', 'tools <name> --scheme')}                            ${dim('# schema only, never runs')}`
  );
  console.log(
    `    ${c('yellow', "tools <name> --queries '<json>'")}                  ${dim('# run a tool')}`
  );
  console.log();
  console.log(`  ${bold('AGENT CONTEXT')}`);
  console.log(
    `    ${c('yellow', 'context')}                                          ${dim('# protocol + system prompt + compact tool schemas')}`
  );
  console.log(
    `    ${c('yellow', 'context --full')}                                   ${dim('# full schemas when runtime loads')}`
  );
  console.log(
    `    ${c('yellow', 'context --json')}                                   ${dim('# machine-readable context wrapper')}`
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
    'This fallback output shows the CLI protocol. Full MCP metadata needs the packaged runtime.'
  );
  console.log();
  console.log('Protocol:');
  console.log('1. Authenticate for private GitHub repos and higher limits:');
  console.log('   login');
  console.log('   auth status --json  # auth-only token state');
  console.log('2. Inspect a tool schema before calling (required):');
  console.log('   tools <name>    # schema: fields, types, example');
  console.log('   tools <n1> <n2> ...  # batch schema reads');
  console.log("   tools <name> --queries '<json>'");
  console.log('3. Read the agent protocol and tool descriptions:');
  console.log(
    '   context         # protocol + system prompt + tool descriptions'
  );
  console.log('   context --full  # full tool descriptions');
  console.log('   context --json  # machine-readable context wrapper');
  console.log('4. Use auth status for read-only token/auth state:');
  console.log('   auth status --json');
  console.log(
    '5. Read YAML output directly; use --json only when you need the envelope.'
  );
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
