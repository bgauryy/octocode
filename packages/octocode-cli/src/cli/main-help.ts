import { c, bold, dim } from '../utils/colors.js';
import {
  TOOL_CATEGORIES,
  TOOL_DEFINITIONS,
  getToolCategory,
} from './tool-command.js';

function buildToolLines(): string[] {
  const lines: string[] = [];

  for (const category of TOOL_CATEGORIES) {
    const tools = TOOL_DEFINITIONS.filter(
      t => getToolCategory(t.name) === category
    );
    if (tools.length === 0) continue;

    lines.push(`    ${dim(category)}`);
    for (const tool of tools) {
      lines.push(`    ${c('cyan', tool.name)}`);
    }
  }

  return lines;
}

export function showHelp(): void {
  const toolLines = buildToolLines();
  const toolCount = TOOL_DEFINITIONS.length;

  const lines = [
    '',
    `  ${c('magenta', bold('🔍🐙 Octocode CLI'))}`,
    '',
    `  ${bold('INSTRUCTIONS FOR AGENTS')}  ${dim('(do this before making any tool request)')}`,
    `    ${dim('0.')} Load agent context (protocol + tools + fields)    ${c('yellow', 'octocode --agent')}`,
    `    ${dim('1.')} List all available tools                          ${c('yellow', 'octocode tools')}`,
    `    ${dim('2.')} Read a tool's input schema                        ${c('yellow', 'octocode tools <name>')}`,
    `    ${dim('3.')} Full context with every JSON schema inline        ${c('yellow', 'octocode --agent --full')}`,
    '',
    `  ${bold('USAGE')}`,
    `    ${c('magenta', 'octocode')} <command> [options]                    ${dim('manage Octocode')}`,
    `    ${c('magenta', 'octocode')} tools                                  ${dim('list all tools')}`,
    `    ${c('magenta', 'octocode')} tools <name>                           ${dim('show input schema')}`,
    `    ${c('magenta', 'octocode')} tools <n1> <n2> ...                    ${dim('batch input schemas')}`,
    `    ${c('magenta', 'octocode')} tools <name> --queries '<json>'        ${dim('run a tool')}`,
    `    ${c('magenta', 'octocode')} instructions                           ${dim('MCP instructions + all schemas')}`,
    '',
    `  ${bold('COMMANDS')}  ${dim('(manage Octocode configuration)')}`,
    `    ${c('magenta', 'install')}          Configure octocode-mcp for an IDE`,
    `    ${c('magenta', 'auth')}             Manage GitHub authentication`,
    `    ${c('magenta', 'login / logout')}   Sign in or out of GitHub`,
    `    ${c('magenta', 'status / token')}   Show auth status or print token`,
    `    ${c('magenta', 'skills')}           Search, install & manage Octocode skills`,
    `    ${c('magenta', 'mcp')}              Manage MCP marketplace`,
    `    ${c('magenta', 'sync')}             Sync MCP configs across IDEs`,
    `    ${c('magenta', 'cache')}            Inspect and clean Octocode cache`,
    '',
    `  ${bold('SKILLS')}  ${dim('(octocode skills <subcommand>)')}`,
    `    ${c('magenta', 'search')} ${dim('<query>')}    Find skills ${dim('(agent protocol; --direct for skills.sh results)')}`,
    `    ${c('magenta', 'read')} ${dim('<path|url>')}   Print a SKILL.md ${dim('(local path, owner/repo/path, or GitHub URL)')}`,
    `    ${c('magenta', 'list')}              List skills installed across all AI clients`,
    `    ${c('magenta', 'install')}           Install skills ${dim('(--skill <name>, --local <path>, --targets <list>)')}`,
    `    ${c('magenta', 'remove')}            Remove a skill ${dim('(--skill <name> or --local <path>)')}`,
    `    ${c('magenta', 'sync')} ${dim('<from> <to>')}  Copy skills from one client target to another`,
    '',
    `  ${bold('TOOLS')}  ${dim(`(${toolCount} tools — run directly from terminal)`)}`,
    ...toolLines,
    '',
    `  ${bold('OPTIONS')}`,
    `    ${c('cyan', '--json')}            Raw JSON (full MCP envelope) for tool runs`,
    `    ${c('cyan', '--compact')}         Leanest tool output (concise verbosity, fewer tokens)`,
    `    ${c('cyan', '--no-color')}        Disable ANSI colors (also via NO_COLOR=1)`,
    `    ${c('cyan', '-h, --help')}        Show this help`,
    `    ${c('cyan', '-v, --version')}     Show version`,
    '',
    `  ${bold('EXAMPLES')}`,
    `    ${c('yellow', 'octocode tools')}                                                          ${dim('# list')}`,
    `    ${c('yellow', 'octocode tools localSearchCode')}                                          ${dim('# schema')}`,
    `    ${c('yellow', 'octocode tools localSearchCode githubSearchCode')}                         ${dim('# batch schemas')}`,
    `    ${c('yellow', `octocode tools localSearchCode --queries '{"path":".","pattern":"fn"}'`)}  ${dim('# run')}`,
    `    ${c('yellow', 'octocode instructions')}                                                   ${dim('# full context')}`,
    '',
    `    ${c('yellow', 'octocode install --ide cursor')}`,
    `    ${c('yellow', 'octocode skills search "code review"')}                                     ${dim('# find skills')}`,
    `    ${c('yellow', 'octocode skills search "code review" --direct')}                            ${dim('# skills.sh results')}`,
    `    ${c('yellow', 'octocode skills install --targets claude-code,cursor')}`,
    '',
    c('magenta', `  ─── 🔍🐙 ${bold('https://octocode.ai')} ───`),
    '',
  ];

  process.stdout.write(`${lines.join('\n')}\n`);
}
