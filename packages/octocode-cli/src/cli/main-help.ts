import { c, bold, dim } from '../utils/colors.js';
import {
  TOOL_CATEGORIES,
  TOOL_DEFINITIONS,
  getToolCategory,
  formatRequiredFields,
} from './tool-command.js';

const LSP_TYPES =
  'definition | references | callers | callees | callHierarchy | hover | documentSymbols | typeDefinition | implementation';

function buildToolBlock(): string[] {
  const lines: string[] = [];

  for (const category of TOOL_CATEGORIES) {
    const tools = TOOL_DEFINITIONS.filter(
      t => getToolCategory(t.name) === category
    );
    if (tools.length === 0) continue;

    lines.push(`    ${dim(category)}`);
    for (const tool of tools) {
      const fields = formatRequiredFields(tool.name);
      const namePad = tool.name.padEnd(28);
      lines.push(`      ${c('cyan', namePad)} ${dim(fields)}`);
      if (tool.name === 'lspGetSemanticContent') {
        const indent = ''.padEnd(28 + 6);
        lines.push(`      ${dim(indent)} ${dim('type: ' + LSP_TYPES)}`);
        lines.push(
          `      ${dim(indent)} ${dim('! run localSearchCode first → get uri + lineHint')}`
        );
      }
    }
  }

  return lines;
}

export function showHelp(): void {
  const toolCount = TOOL_DEFINITIONS.length;
  const toolLines = buildToolBlock();

  const lines = [
    '',
    `  ${c('magenta', bold('🔍🐙 Octocode'))}  ${dim('Code research CLI — GitHub · Local · LSP · Package')}`,
    '',

    // ── How to use (agent protocol inline) ────────────────────────────────
    `  ${bold('HOW TO USE')}`,
    `    ${c('red', bold('1.'))} Check schema BEFORE any tool call   ${c('yellow', 'octocode tools <name>')}`,
    `    ${c('red', bold('2.'))} Run a tool                          ${c('yellow', "octocode tools <name> --queries '<json>'")}`,
    `    ${c('cyan', '3.')} For file/search/PR use smart cmds   ${dim('(no schema needed — see below)')}`,
    `    ${dim('4.')} Full context + all schemas           ${c('yellow', 'octocode instructions')}`,
    `    ${dim('5.')} All schemas as inline JSON           ${c('yellow', 'octocode instructions --full')}`,
    '',

    // ── Smart commands — preferred ─────────────────────────────────────────
    `  ${bold('SMART COMMANDS')}  ${dim('— prefer over raw tool calls for file / search / PR')}`,
    `    ${dim('Auto-route local ↔ GitHub — no schema or owner/repo wiring needed')}`,
    `    ${c('cyan', 'octocode get')}    ${dim('<path | owner/repo/file>')}    ${dim('fetch + minify  [--mode none|standard|symbols]')}`,
    `    ${c('cyan', 'octocode tree')}   ${dim('<path | owner/repo>')}         ${dim('directory tree  [--depth N]')}`,
    `    ${c('cyan', 'octocode search')} ${dim('<pattern> <path | repo>')}     ${dim('code search     [--type, --limit, --page]')}`,
    `    ${c('cyan', 'octocode pr')}     ${dim('<owner/repo[#N] | PR-URL>')}   ${dim('PR info         [--patches, --comments, --deep]')}`,
    '',

    // ── All tools ─────────────────────────────────────────────────────────
    `  ${bold(`TOOLS (${toolCount})`)}  ${dim('* = required   ? = optional   |  octocode tools <name> → full schema + examples')}`,
    ...toolLines,
    '',

    // ── Output contract ───────────────────────────────────────────────────
    `  ${bold('OUTPUT CONTRACT')}  ${dim('(add --json to get the full envelope)')}`,
    `    ${dim('Default output:')}  clean YAML — read directly`,
    `    ${dim('--compact:    ')}   leanest YAML (fewer tokens)`,
    `    ${dim('--json envelope:')}`,
    `      ${c('cyan', 'isError')}                           ${dim('true = tool failed')}`,
    `      ${c('cyan', 'content[].text')}                    ${dim('YAML string (same as default output)')}`,
    `      ${c('cyan', 'structuredContent.results[]')}       ${dim('tool result objects  (id + data)')}`,
    `      ${c('cyan', 'structuredContent.base')}            ${dim('cwd / workspace root used for the query')}`,
    `      ${c('cyan', 'structuredContent.hints[]')}         ${dim('next-step suggestions — follow them')}`,
    `      ${c('cyan', 'structuredContent.evidence')}        ${dim('{ answerReady, complete, kind }')}`,
    `      ${dim('Trust evidence.answerReady — true = answer is complete, stop calling')}`,
    '',

    // ── Workflows ──────────────────────────────────────────────────────────
    `  ${bold('WORKFLOWS')}`,
    `    ${dim('local  →')}  localViewStructure ${dim('→')} localSearchCode ${dim('→')} localGetFileContent ${dim('→')} lspGetSemanticContent`,
    `    ${dim('github →')}  githubSearchRepositories ${dim('→')} githubViewRepoStructure ${dim('→')} githubGetFileContent`,
    `    ${dim('lsp    →')}  localSearchCode ${dim('(uri+lineHint)')} ${dim('→')} lspGetSemanticContent${dim('(uri, symbolName, lineHint, type)')}`,
    `    ${dim('pkg    →')}  packageSearch ${dim('→')} githubGetFileContent${dim('(owner/repo from result)')}`,
    '',

    // ── Output flags ───────────────────────────────────────────────────────
    `  ${bold('FLAGS')}`,
    `    ${c('cyan', '--json')}         raw JSON envelope   ${c('cyan', '--compact')}   leanest output   ${c('cyan', '--no-color')}  no ANSI`,
    '',

    // ── Management ─────────────────────────────────────────────────────────
    `  ${bold('MANAGEMENT')}`,
    `    ${c('cyan', 'install')} ${dim('--ide <cursor|claude-desktop|windsurf|vscode-cline|...>')}  ${dim('configure IDE')}`,
    `    ${c('cyan', 'auth')} ${dim('/ login / logout / status / token')}    ${dim('GitHub authentication')}`,
    `    ${c('cyan', 'skills')} ${dim('search|install|remove|list')}         ${dim('agent skills marketplace')}`,
    `    ${c('cyan', 'mcp')}    ${dim('list|install|remove|status')}         ${dim('MCP server registry (70+ servers)')}`,
    `    ${c('cyan', 'cache')} ${dim('status|clean')}                        ${dim('cache management')}`,
    '',

    // ── Exit codes ─────────────────────────────────────────────────────────
    `  ${bold('EXIT CODES')}`,
    `    ${c('cyan', '0')} ok   ${c('cyan', '2')} bad-input   ${c('cyan', '3')} not-found   ${c('cyan', '4')} auth-error   ${c('cyan', '5')} tool-error   ${c('cyan', '7')} rate-limited`,
    '',

    c('magenta', `  ─── 🔍🐙 ${bold('https://octocode.ai')} ───`),
    '',
  ];

  process.stdout.write(`${lines.join('\n')}\n`);
}
