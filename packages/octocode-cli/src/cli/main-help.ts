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
    `  ${bold('TOOL RUNTIME')}  ${dim('`octocode tools` runs the same Octocode MCP tool implementations under the hood')}`,
    '',
    `  ${bold('RESEARCH COMMANDS')}  ${dim('(smart-route local ↔ GitHub automatically)')}`,
    `    ${c('magenta', 'get')} ${dim('<path|owner/repo/file>')}    Fetch + minify file content ${dim('(--mode, --match-string, --start-line, --end-line, --page)')}`,
    `    ${c('magenta', 'tree')} ${dim('<path|owner/repo>')}        Directory structure ${dim('(--depth, default 2 for GitHub)')}`,
    `    ${c('magenta', 'search')} ${dim('<pattern> <path|repo>')}  Code search ${dim('(--type, --limit, --page)')}`,
    `    ${c('magenta', 'pr')} ${dim('<owner/repo[#N]|URL>')}       PR list/search or deep-dive ${dim('(--state, --patches, --comments, --commits, --deep)')}`,
    '',
    `  ${bold('MANAGEMENT COMMANDS')}`,
    `    ${c('magenta', 'install')}          Configure octocode-mcp for an IDE`,
    `    ${c('magenta', 'auth')}             Manage GitHub authentication`,
    `    ${c('magenta', 'login / logout')}   Sign in or out of GitHub`,
    `    ${c('magenta', 'status / token')}   Show auth status or print token`,
    `    ${c('magenta', 'skills')}           Search, install & manage Octocode skills`,
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
    `    ${c('yellow', 'octocode get bgauryy/octocode-mcp/src/index.ts')}                            ${dim('# fetch file')}`,
    `    ${c('yellow', 'octocode tree bgauryy/octocode-mcp --depth 2')}                             ${dim('# structure')}`,
    `    ${c('yellow', 'octocode search "executeDirectTool" bgauryy/octocode-mcp')}                 ${dim('# code search')}`,
    `    ${c('yellow', 'octocode pr bgauryy/octocode-mcp --state merged --limit 5')}                ${dim('# PR list')}`,
    `    ${c('yellow', 'octocode pr bgauryy/octocode-mcp#142 --patches')}                           ${dim('# PR diff')}`,
    `    ${c('yellow', 'octocode pr bgauryy/octocode-mcp#142 --deep')}                              ${dim('# full PR')}`,
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
