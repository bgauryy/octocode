import { c, bold, dim } from '../utils/colors.js';
import {
  DIRECT_TOOL_CATEGORIES,
  DIRECT_TOOL_DEFINITIONS,
  getDirectToolCategory,
  getDirectToolDisplayFields,
  sortDirectToolNames,
} from '@octocodeai/octocode-tools-core/direct';

const LSP_TOOL = 'lspGetSemantics';

/** Brief [required*, optional?] summary for the --help tool list (top-level fields only). */
function formatBriefFields(toolName: string): string {
  if (toolName === LSP_TOOL) return '[uri*, type, symbolName?, lineHint?]';
  const fields = getDirectToolDisplayFields(toolName).filter(
    f => !f.name.includes('.')
  );
  const required = fields.filter(f => f.required).map(f => `${f.name}*`);
  const optional = fields.filter(f => !f.required);
  if (required.length > 0) {
    const optHint = optional.slice(0, 2).map(f => `${f.name}?`);
    return `[${[...required, ...optHint].join(', ')}]`;
  }
  return `[${optional
    .slice(0, 3)
    .map(f => `${f.name}?`)
    .join(', ')}]`;
}

function buildToolBlock(): string[] {
  const lines: string[] = [];
  const allNames = sortDirectToolNames(
    DIRECT_TOOL_DEFINITIONS.map(t => t.name)
  );

  for (const category of DIRECT_TOOL_CATEGORIES) {
    const names = allNames.filter(n => getDirectToolCategory(n) === category);
    if (names.length === 0) continue;

    lines.push(`    ${dim(category)}`);
    for (const name of names) {
      const namePad = name.padEnd(28);
      lines.push(`      ${c('cyan', namePad)} ${dim(formatBriefFields(name))}`);
      if (name === LSP_TOOL) {
        const indent = ''.padEnd(34);
        lines.push(
          `      ${dim(indent)} ${dim('type: definition | references | callers | callees | callHierarchy | hover | documentSymbols | typeDefinition | implementation')}`
        );
        lines.push(
          `      ${dim(indent)} ${dim('! run localSearchCode first → get uri + lineHint')}`
        );
      }
    }
  }

  return lines;
}

export function showHelp(): void {
  const toolCount = DIRECT_TOOL_DEFINITIONS.length;
  const toolLines = buildToolBlock();

  const lines = [
    '',
    `  ${c('magenta', bold('🔍🐙 Octocode'))}  ${dim('Code research CLI — GitHub · Local · LSP · Package')}`,
    '',

    // ── Agent rule — first thing an agent sees ──────────────────────────────
    `  ${c('red', bold('AGENTS — read schema before every raw tool call. Never guess fields.'))}`,
    `    ${c('yellow', 'octocode tools <name>')}           ${dim('# required fields, types, example call')}`,
    `    ${c('yellow', 'octocode tools <n1> <n2> ...')}    ${dim('# batch schema reads')}`,
    `    ${c('yellow', 'octocode context')}                ${dim('# full protocol + system prompt + all schemas')}`,
    '',

    // ── Live tool list ──────────────────────────────────────────────────────
    `  ${bold(`TOOLS (${toolCount})`)}  ${dim('* = required   ? = optional   |  octocode tools <name> → full schema + examples')}`,
    ...toolLines,
    '',

    // ── Smart commands (no schema needed) ──────────────────────────────────
    `  ${bold('SMART COMMANDS')}  ${dim('— auto-route local ↔ GitHub, no schema needed')}`,
    `    ${c('cyan', 'octocode get')}     ${dim('<path | owner/repo/file>')}   ${dim('fetch + minify  [--mode none|standard|symbols]')}`,
    `    ${c('cyan', 'octocode tree')}    ${dim('<path | owner/repo>')}        ${dim('directory tree  [--depth N]')}`,
    `    ${c('cyan', 'octocode files')}   ${dim('<query> [path | repo]')}      ${dim('file discovery  [--search path|content|both]')}`,
    `    ${c('cyan', 'octocode search')}  ${dim('<pattern> <path | repo>')}    ${dim('code search     [--type, --branch, --page]')}`,
    `    ${c('cyan', 'octocode pr')}      ${dim('<owner/repo[#N] | URL>')}     ${dim('PR info         [--patches, --comments, --deep]')}`,
    `    ${c('cyan', 'octocode repo')}    ${dim('<keywords...>')}              ${dim('repo discovery  [--topic, --language, --stars]')}`,
    `    ${c('cyan', 'octocode pkg')}     ${dim('<package>')}                  ${dim('npm metadata + source repo')}`,
    `    ${c('cyan', 'octocode symbols')} ${dim('<file | path>')}              ${dim('semantic outline before LSP')}`,
    `    ${c('cyan', 'octocode lsp')}     ${dim('<file> --type <type>')}       ${dim('semantic nav    [--symbol X --line N]')}`,
    '',

    // ── Management (users) ─────────────────────────────────────────────────
    `  ${bold('MANAGEMENT')}`,
    `    ${c('cyan', 'install')} ${dim('--ide <cursor|claude-desktop|windsurf|...>')}  ${dim('configure IDE')}`,
    `    ${c('cyan', 'auth')}    ${dim('<login|logout|status|token>')}                 ${dim('GitHub authentication')}`,
    `    ${c('cyan', 'skills')}  ${dim('<install|remove|list|sync>')}                  ${dim('skills marketplace')}`,
    `    ${c('cyan', 'status')}  ${dim('[--sync]')}                                    ${dim('auth + cache status')}`,
    '',

    // ── Flags + exit codes (one line each) ─────────────────────────────────
    `  ${bold('FLAGS')}  ${c('cyan', '--json')} raw envelope  ${c('cyan', '--compact')} leanest  ${c('cyan', '--no-color')} no ANSI`,
    `  ${bold('EXIT')}   0=ok  2=bad-input  3=not-found  4=auth  5=tool-error  7=rate-limited`,
    '',

    c('magenta', `  ─── 🔍🐙 ${bold('https://octocode.ai')} ───`),
    '',
  ];

  process.stdout.write(`${lines.join('\n')}\n`);
}
