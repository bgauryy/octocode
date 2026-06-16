import { c, bold, dim } from '../utils/colors.js';
import {
  HELP_TOOL_CATEGORIES,
  HELP_TOOL_DEFINITIONS,
} from './tool-help-data.js';

const LSP_TYPES =
  'definition | references | callers | callees | callHierarchy | hover | documentSymbols | typeDefinition | implementation';

function buildToolBlock(): string[] {
  const lines: string[] = [];

  for (const category of HELP_TOOL_CATEGORIES) {
    const tools = HELP_TOOL_DEFINITIONS.filter(t => t.category === category);
    if (tools.length === 0) continue;

    lines.push(`    ${dim(category)}`);
    for (const tool of tools) {
      const namePad = tool.name.padEnd(28);
      lines.push(`      ${c('cyan', namePad)} ${dim(tool.fields)}`);
      if (tool.name === 'lspGetSemantics') {
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
  const toolCount = HELP_TOOL_DEFINITIONS.length;
  const toolLines = buildToolBlock();

  const lines = [
    '',
    `  ${c('magenta', bold('🔍🐙 Octocode'))}  ${dim('Code research CLI — GitHub · Local · LSP · Package')}`,
    '',

    `  ${bold('HOW TO USE')}`,
    `    ${c('cyan', '1.')} Smart research                 ${dim('tree/files/search/get/pr/repo/pkg/symbols/lsp')}`,
    `    ${c('red', bold('2.'))} Raw schema first              ${c('yellow', 'octocode tools <name> --scheme')}`,
    `    ${c('red', bold('3.'))} Run raw MCP tool              ${c('yellow', "octocode tools <name> --queries '<json>'")}`,
    `    ${dim('4.')} System prompt + protocol        ${c('yellow', 'octocode context')} ${dim('(or --context)')}`,
    `    ${dim('5.')} Full tool descriptions/schemas  ${c('yellow', 'octocode context --full')} ${dim('(or --context --full)')}`,
    `    ${dim('6.')} Command help                    ${c('yellow', 'octocode <command> --help')}`,
    '',

    `  ${bold('SMART COMMANDS')}  ${dim('— preferred for normal research')}`,
    `    ${dim('Common flows without raw schemas; file/search commands auto-route local ↔ GitHub')}`,
    `    ${c('cyan', 'octocode get')}    ${dim('<path | owner/repo/file>')}    ${dim('fetch + minify  [--match-string, --mode]')}`,
    `    ${c('cyan', 'octocode tree')}   ${dim('<path | owner/repo>')}         ${dim('directory tree  [--depth N]')}`,
    `    ${c('cyan', 'octocode files')}  ${dim('<query> [path | repo]')}       ${dim('file discovery [--search path|content|both]')}`,
    `    ${c('cyan', 'octocode search')} ${dim('<pattern> <path | repo>')}     ${dim('code search     [--type, --branch, --page]')}`,
    `    ${c('cyan', 'octocode pr')}     ${dim('<owner/repo[#N] | PR-URL>')}   ${dim('PR info         [--patches, --comments, --deep]')}`,
    `    ${c('cyan', 'octocode repo')}   ${dim('<keywords...>')}               ${dim('repo discovery  [--topic, --language, --stars]')}`,
    `    ${c('cyan', 'octocode pkg')}    ${dim('<package>')}                   ${dim('npm metadata + source repo')}`,
    `    ${c('cyan', 'octocode symbols')} ${dim('<file | path>')}               ${dim('semantic outline before LSP')}`,
    `    ${c('cyan', 'octocode lsp')}    ${dim('<file> --type <type>')}        ${dim('semantic nav after symbol+line')}`,
    '',

    `  ${bold(`TOOLS (${toolCount})`)}  ${dim('* = required   ? = optional   |  octocode tools <name> → full schema + examples')}`,
    ...toolLines,
    '',

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

    `  ${bold('RESEARCH LOOP')}`,
    `    ${dim('1 orient')}  tree / repo / pkg / pr`,
    `    ${dim('2 search')}  files / search`,
    `    ${dim('3 read  ')}  get exact slices; choose --mode standard|symbols|none`,
    `    ${dim('4 prove ')}  symbols/lsp or PR content; stop when evidence.answerReady is true`,
    '',
    `  ${bold('COMMON FLOWS')}`,
    `    ${dim('local  →')}  octocode tree ${dim('→')} octocode files/search ${dim('→')} octocode get ${dim('→')} octocode symbols/lsp`,
    `    ${dim('github →')}  octocode repo ${dim('→')} octocode tree ${dim('→')} octocode search/files ${dim('→')} octocode get`,
    `    ${dim('pr     →')}  octocode pr ${dim('owner/repo')} ${dim('→')} octocode pr ${dim('owner/repo#123 --deep --patches')}`,
    `    ${dim('pkg    →')}  octocode pkg ${dim('<package>')} ${dim('→')} octocode repo/tree/get ${dim('(source repo from result)')}`,
    `    ${dim('lsp    →')}  octocode symbols ${dim('<file|path>')} ${dim('→')} octocode lsp ${dim('--type references --symbol X --line N')}`,
    '',

    `  ${bold('FLAGS')}`,
    `    ${c('cyan', '--json')}         raw JSON envelope   ${c('cyan', '--compact')}   leanest output   ${c('cyan', '--no-color')}  no ANSI`,
    '',

    `  ${bold('MANAGEMENT')}`,
    `    ${c('cyan', 'install')} ${dim('--ide <cursor|claude-desktop|windsurf|vscode-cline|...>')}  ${dim('configure IDE')}`,
    `    ${c('cyan', 'auth')} ${dim('<login|logout|status|token|refresh>')} ${dim('GitHub authentication')}`,
    `    ${c('cyan', 'skills')} ${dim('<search|read|install|remove|list|sync>')} ${dim('skills marketplace')}`,
    `    ${c('cyan', 'status')} ${dim('[--sync]')}                           ${dim('auth, MCP clients, cache')}`,
    '',

    `  ${bold('EXIT CODES')}`,
    `    ${c('cyan', '0')} ok   ${c('cyan', '2')} bad-input   ${c('cyan', '3')} not-found   ${c('cyan', '4')} auth-error   ${c('cyan', '5')} tool-error   ${c('cyan', '7')} rate-limited`,
    '',

    c('magenta', `  ─── 🔍🐙 ${bold('https://octocode.ai')} ───`),
    '',
  ];

  process.stdout.write(`${lines.join('\n')}\n`);
}
