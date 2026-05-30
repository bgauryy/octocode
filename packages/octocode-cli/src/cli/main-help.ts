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
      const padded = tool.name.padEnd(28);
      lines.push(`    ${c('cyan', padded)} ${dim('--tool')} ${tool.name}`);
    }
  }

  return lines;
}

export function showHelp(): void {
  const toolLines = buildToolLines();

  const lines = [
    '',
    `  ${c('magenta', bold('🔍🐙 Octocode CLI'))}`,
    '',
    `  ${bold('Two things in one binary:')}`,
    `    ${c('magenta', '1.')} ${bold('Manage')} — install, auth, skills, MCP marketplace, sync, cache`,
    `    ${c('magenta', '2.')} ${bold('Run tools')} — call any Octocode tool directly from terminal`,
    '',
    `  ${bold('USAGE')}`,
    `    ${c('magenta', 'octocode-cli')} <command> [options]              ${dim('manage Octocode')}`,
    `    ${c('magenta', 'octocode-cli')} --tool <name> --queries '<json>'  ${dim('run a tool')}`,
    '',
    `  ${bold('COMMANDS')}  ${dim('(manage Octocode configuration)')}`,
    `    ${c('magenta', 'install')}          Configure octocode-mcp for an IDE`,
    `    ${c('magenta', 'auth')}             Manage GitHub authentication`,
    `    ${c('magenta', 'login / logout')}   Sign in or out of GitHub`,
    `    ${c('magenta', 'status / token')}   Show auth status or print token`,
    `    ${c('magenta', 'skills')}           Install/remove bundled Octocode skills`,
    `    ${c('magenta', 'mcp')}              Manage MCP marketplace`,
    `    ${c('magenta', 'sync')}             Sync MCP configs across IDEs`,
    `    ${c('magenta', 'cache')}            Inspect and clean Octocode cache`,
    '',
    `  ${bold('TOOLS')}  ${dim('(run Octocode tools directly — for agents and humans)')}`,
    ...toolLines,
    '',
    `  ${bold('OPTIONS')}`,
    `    ${c('cyan', "--tool <name> --queries '<json>'")}  Run a tool`,
    `    ${c('cyan', '--tool <name> --help')}              Show tool input/output schema`,
    `    ${c('cyan', '--tools-context')}                   Full MCP instructions + all schemas`,
    `    ${c('cyan', '--json')}                            Raw JSON output`,
    `    ${c('cyan', '-h, --help')}                        Show this help`,
    `    ${c('cyan', '-v, --version')}                     Show version`,
    '',
    `  ${bold('EXAMPLES')}`,
    `    ${dim('# Run a tool')}`,
    `    ${c('yellow', `octocode-cli --tool localSearchCode --queries '{"path":".","pattern":"runCLI"}'`)}`,
    `    ${c('yellow', `octocode-cli --tool githubSearchCode --queries '{"keywordsToSearch":["hook"],"owner":"facebook","repo":"react"}'`)}`,
    '',
    `    ${dim('# Discover tools')}`,
    `    ${c('yellow', 'octocode-cli --tool githubSearchCode --help')}`,
    `    ${c('yellow', 'octocode-cli --tools-context')}`,
    '',
    `    ${dim('# Manage Octocode')}`,
    `    ${c('yellow', 'octocode-cli install --ide cursor')}`,
    `    ${c('yellow', 'octocode-cli skills install --targets claude-code,cursor')}`,
    '',
    c('magenta', `  ─── 🔍🐙 ${bold('https://octocode.ai')} ───`),
    '',
  ];

  process.stdout.write(`${lines.join('\n')}\n`);
}
