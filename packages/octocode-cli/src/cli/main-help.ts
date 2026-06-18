import { c, bold, dim } from '../utils/colors.js';
import { getAuthStatus } from '../features/github-oauth.js';
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

  let isAuthenticated = false;
  try {
    isAuthenticated = getAuthStatus().authenticated;
  } catch {
    // ignore — treat as unauthenticated
  }

  const authBanner: string[] = isAuthenticated
    ? []
    : [
        `  ${c('red', '─'.repeat(62))}`,
        `  ${c('red', bold('  ⚠  NOT AUTHENTICATED'))}  ${c('red', 'GitHub token required for tool calls.')}`,
        `  ${c('red', '     Run: ')}${c('yellow', bold('octocode login'))}`,
        `  ${c('red', '─'.repeat(62))}`,
        '',
      ];

  const lines = [
    '',
    ...authBanner,
    `  ${c('magenta', bold('🔍🐙 Octocode'))}  ${dim('Code research CLI — GitHub · Local · LSP · AST · Package')}`,
    '',

    // ── Agent rule — first thing an agent sees ──────────────────────────────
    `  ${c('red', bold('AGENTS — read schema before every raw tool call. Never guess fields.'))}`,
    `    ${c('yellow', 'octocode tools <name>')}           ${dim('# required fields, types, example call')}`,
    `    ${c('yellow', 'octocode tools <n1> <n2> ...')}    ${dim('# batch schema reads')}`,
    `    ${c('yellow', 'octocode context')}                ${dim('# protocol + system prompt + tool descriptions')}`,
    `    ${c('yellow', 'octocode auth login')}             ${dim('# authenticate — GitHub token required for all GitHub tools')}`,
    `    ${c('yellow', 'octocode skills list')}            ${dim('# browse agent skills (install with: octocode skills install --skill <name>)')}`,
    `    ${c('yellow', 'octocode status')}                 ${dim('# health check: auth + cache + MCP')}`,
    '',

    // ── Smart-usage playbook (distilled from the system prompt) ─────────────
    `  ${c('green', bold('PLAYBOOK'))}  ${dim('locate → map → search → read → prove — cheapest tool that proves/disproves, smallest slice, stop when evidence.answerReady')}`,
    `    ${c('cyan', 'orient cheap')}    ${dim('concise:true (string list) · localSearchCode mode:discovery (paths) · localViewStructure maxDepth:1 then drill')}`,
    `    ${c('cyan', 'minify by goal')}  ${dim('symbols=skeleton (orient unknown) · standard=read (default) · none=exact quote/diff')}`,
    `    ${c('cyan', 'batch')}           ${dim('up to 5 sub-queries/call (N paths/PRs/pkgs in one); serialize only search→read→LSP')}`,
    `    ${c('cyan', 'prove')}           ${dim('snippets are discovery, not proof — re-read exact text · search→lineHint→LSP · npmSearch→owner/repo')}`,
    '',

    // ── Live tool list ──────────────────────────────────────────────────────
    `  ${bold(`TOOLS (${toolCount})`)}  ${dim('* = required   ? = optional   |  octocode tools <name> → full schema + examples')}`,
    ...toolLines,
    '',

    // Smart commands temporarily unhooked — will be re-added in a future release.
    // octocode get / tree / files / search / pr / repo / pkg / symbols / lsp

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
