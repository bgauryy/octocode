// ── Help surface sync contract ────────────────────────────────────────────────
// ALL locations must be updated together when changing help text:
//   1. THIS FILE (main-help.ts)                    — top-level `--help`
//   2. packages/octocode/src/cli/commands/search.ts — renderEnvelope, per-command hints
//   3. packages/octocode-tools-core/src/oql/schemeText.ts — `--scheme` JSON output
//   4. octocode-mcp-host/…/resources/tools/oqlSearch.ts — MCP tool description (now in sibling repo resources)
//   5. octocode-mcp-host/…/resources/cli/search.ts  — CLICommandSpec (scheme[], whenToUse[])
//   6. octocode-mcp-host/…/resources/systemPrompt.ts — MCP + CLI system prompt
// ─────────────────────────────────────────────────────────────────────────────
import { join } from 'node:path';
import { c, bold, dim, underline } from '../utils/colors.js';
import { getAuthStatus } from '../features/github-oauth.js';
import {
  DIRECT_TOOL_CATEGORIES,
  DIRECT_TOOL_DEFINITIONS,
  getDirectToolCategory,
  getDirectToolDisplayFields,
  loadToolContent,
  sortDirectToolNames,
} from '@octocodeai/octocode-tools-core/direct';
import { paths } from '@octocodeai/octocode-tools-core/paths';

const LSP_TOOL = 'lspGetSemantics';

/** Canonical octocode-engineer skill — the agent playbook for these flows. */
const ENGINEER_SKILL_URL =
  'https://github.com/bgauryy/octocode/tree/main/skills/octocode-engineer';
const UNZIP_DESTINATION_PATTERN = join(paths.unzip, '<name>-<timestamp>');

/**
 * The verbatim system prompt (Octocode MCP instructions) shown inside
 * <AGENT_INSTRUCTIONS>. Loaded from the shared tool metadata so the help
 * surface stays byte-identical to what the MCP server and `context` emit.
 * Falls back to null on any failure — the block degrades gracefully.
 */
async function loadAgentInstructions(): Promise<string | null> {
  try {
    const metadata = await loadToolContent();
    const systemPrompt = metadata.systemPrompt.trim();
    return systemPrompt ? systemPrompt : null;
  } catch {
    return null;
  }
}

/** Render the <AGENT_INSTRUCTIONS> block: system prompt + skill pointer. */
function buildAgentInstructionsBlock(instructions: string | null): string[] {
  const lines: string[] = [`  ${dim('<AGENT_INSTRUCTIONS>')}`];

  if (instructions) {
    // The system prompt itself — the canonical research strategy.
    for (const line of instructions.split('\n')) {
      lines.push(`  ${dim(line)}`);
    }
  } else {
    // Fallback if metadata can't be loaded — keep the essentials inline.
    lines.push(
      `  ${dim('One toolset for LOCAL files and EXTERNAL GitHub/npm research.')}`,
      `  ${dim('Flow: locate → search → read the smallest slice → prove. Cheapest tool first; orient before you read.')}`,
      `  ${dim('3.')} ${c('red', bold('Do NOT hallucinate'))} ${dim('paths, lines, or fields — verify with tools; snippets are discovery, not proof.')}`
    );
  }

  lines.push(
    '',
    `  ${dim('Tools:')} ${c('yellow', 'tools <name> --scheme')} ${dim('to read a schema (never guess fields), then')} ${c('yellow', "tools <name> --queries '<json>'")} ${dim('to run it. QUICK COMMANDS below cover the common path.')}`,
    `  ${dim('Skill reference — read the')} ${c('cyan', 'octocode-engineer')} ${dim('flows to understand the research loop and leverage every tool fully:')}`,
    `    ${underline(ENGINEER_SKILL_URL)}`,
    `  ${dim('Auth: humans run')} ${c('yellow', 'login')}${dim('; agents run')} ${c('yellow', 'auth status --json')} ${dim('for token state; pass GITHUB_TOKEN / OCTOCODE_TOKEN / GH_TOKEN via env. Deeper protocol:')} ${c('cyan', 'context')}${dim('.')}`,
    `  ${dim('</AGENT_INSTRUCTIONS>')}`
  );

  return lines;
}

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

/** One aligned `name <args>  description` line for the QUICK COMMANDS block. */
function quick(name: string, argHint: string, description: string): string {
  return `    ${c('cyan', name.padEnd(8))} ${dim(argHint.padEnd(28))}  ${dim(description)}`;
}

export async function showHelp(): Promise<void> {
  const toolCount = DIRECT_TOOL_DEFINITIONS.length;
  const toolLines = buildToolBlock();
  const agentInstructions = buildAgentInstructionsBlock(
    await loadAgentInstructions()
  );

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
        `  ${c('red', bold('  ⚠  NOT AUTHENTICATED'))}  ${c('red', 'No GitHub token configured.')}`,
        `  ${c('red', '     Public GitHub calls may run anonymously; login enables private repos and higher limits.')}`,
        `  ${c('red', '     Run: ')}${c('yellow', bold('login'))}`,
        `  ${c('red', '─'.repeat(62))}`,
        '',
      ];

  const lines = [
    '',
    ...authBanner,
    `  ${c('magenta', bold('🔍🐙 Octocode'))}  ${dim('Code research CLI — for humans and agents')}`,
    '',

    // ── Quick commands FIRST — the friendly, human-first surface ────────────
    `  ${c('green', bold('QUICK COMMANDS'))}  ${dim('smart shortcuts — auto-route local path vs owner/repo. Add --json for raw output.')}`,
    quick(
      'ls',
      '<path|owner/repo>',
      'directory tree; a file (or --symbols) shows a symbol outline'
    ),
    quick(
      'cat',
      '<path|owner/repo/path>',
      'read + minify a file (--mode none|standard|symbols)'
    ),
    quick(
      'grep',
      '<keywords> <path|owner/repo>',
      'text/regex search → file + line; --pattern/--rule for AST shape (local)'
    ),
    quick(
      'search',
      '--query <oql-json> | --scheme',
      'OQL: typed research across code/content/files/LSP/repos/packages/history — run --scheme first to see the full typed schema'
    ),
    quick(
      'find',
      '<query> [path|owner/repo]',
      'find files by name/path/content'
    ),
    quick('diff', '<left> <right>', 'compare two files (local or GitHub refs)'),
    quick(
      'lsp',
      '<file> --type <type> --symbol <s>',
      'identity: defs, refs, callers, hover; --line optional'
    ),
    quick('repo', '<keywords...>', 'discover GitHub repositories'),
    quick('pr', '<owner/repo[#N]|PR-URL>', 'list PRs or deep-read one PR'),
    quick(
      'history',
      '<owner/repo[/path]>',
      'commit history (who/when) → #PR deep-read'
    ),
    quick('pkg', '<package>', 'npm package + source repo'),
    quick(
      'binary',
      '<file>',
      'list, decompress, or strings (archives & binaries)'
    ),
    quick(
      'unzip',
      '<archive>',
      `unpack archive → ${UNZIP_DESTINATION_PATTERN}, then grep/ls/cat it`
    ),
    quick(
      'clone',
      '<owner/repo[/path][@branch]>',
      `clone a repo or subtree → ${paths.clone}`
    ),
    quick(
      'cache',
      'fetch <owner/repo> [path]',
      'save remote content locally + return structured location data'
    ),
    '',

    // ── Remote-as-local bridge ──────────────────────────────────────────────
    `  ${c('green', bold('REMOTE AS LOCAL'))}  ${dim('use --repo to analyse GitHub content with local tools')}`,
    `    ${dim('Add')} ${c('cyan', '--repo <owner/repo[@branch]>')} ${dim('to any local command to transparently materialize remote content.')}`,
    `    ${dim('grep │ ls │ cat │ find │ lsp all accept --repo. The first call fetches; subsequent calls use the disk cache (24 h).')}`,
    `    ${dim('Decision tree:')}`,
    `      ${c('cyan', 'grep <kw> --repo owner/repo')}          ${dim('→ text search across a remote repo (tree-fetch, fast)')}`,
    `      ${c('cyan', 'ls --repo owner/repo')}                 ${dim('→ remote directory tree')}`,
    `      ${c('cyan', 'cat --repo owner/repo/path/to/file')}   ${dim('→ read a single remote file (does NOT need clone)')}`,
    `      ${c('cyan', 'clone owner/repo')}                     ${dim('→ git clone (use for full repo AST/LSP/dead-code analysis)')}`,
    `      ${c('cyan', 'cache fetch owner/repo [path]')}        ${dim('→ explicit tree-fetch + returns location.{localPath,complete,verified}')}`,
    `    ${dim('After materialization the')} ${c('cyan', 'location')} ${dim('block in every result carries localPath, cached, complete, and verified.')}`,
    `    ${dim('verified:false = served from disk cache (completeness unconfirmed). Use --force-refresh or clone to get verified:true.')}`,
    '',

    // ── Raw execution — every tool, including ones without a quick command ──
    `  ${bold(`TOOLS (${toolCount})`)}  ${dim('raw execution — schema-exact, all tools incl. clone, binary inspect, AST')}`,
    `    ${c('yellow', 'tools'.padEnd(28))} ${dim('list all tools')}`,
    `    ${c('yellow', 'tools <name> --scheme'.padEnd(28))} ${dim('read schema (never guess fields)')}`,
    `    ${c('yellow', "tools <name> --queries '<json>'".padEnd(28))} ${dim('run a tool (1 object or array of ≤5)')}`,
    `    ${c('yellow', 'context [--full] [--json]'.padEnd(28))} ${dim('optional — protocol + system prompt + descriptions (deeper research)')}`,
    ...toolLines,
    '',

    // ── Playbook (distilled from the system prompt) ─────────────────────────
    `  ${c('green', bold('PLAYBOOK'))}  ${dim('cheapest tool first · smallest slice · narrow before paging')}`,
    `    ${c('cyan', 'orient cheap')}    ${dim('concise:true (string list) · localSearchCode mode:discovery (paths) · ls then drill')}`,
    `    ${c('cyan', 'minify by goal')}  ${dim('symbols=skeleton (orient unknown) · standard=read (default) · none=exact quote/diff')}`,
    `    ${c('cyan', 'prove')}           ${dim('snippets are discovery, not proof — re-read exact text · search→lineHint→lsp · pkg→owner/repo')}`,
    '',

    // ── Management (users) ─────────────────────────────────────────────────
    `  ${bold('MANAGEMENT')}`,
    `    ${c('cyan', 'install')} ${dim('--ide <cursor|claude-desktop|windsurf|...>')}  ${dim('configure IDE')}`,
    `    ${c('cyan', 'login')}   ${dim('[--hostname <host>]')}                         ${dim('GitHub authentication')}`,
    `    ${c('cyan', 'logout')}  ${dim('[--hostname <host>]')}                         ${dim('clear stored credentials')}`,
    `    ${c('cyan', 'status')}  ${dim('[--sync]')}                                    ${dim('token/auth + cache status')}`,
    '',

    // ── Flags + exit codes (one line each) ─────────────────────────────────
    `  ${bold('FLAGS')}  ${c('cyan', '--json')} raw envelope  ${c('cyan', '--compact')} leanest  ${c('cyan', '--no-color')} no ANSI`,
    `  ${bold('EXIT')}   0=ok  2=bad-input  3=not-found  4=auth  5=tool-error  7=rate-limited`,
    `  ${bold('DOCS')}   ${underline('https://github.com/bgauryy/octocode/tree/main/docs')}  ${dim('· per-command:')} ${c('cyan', '<command> --help')}`,
    '',

    c('magenta', `  ─── 🔍🐙 ${bold('https://octocode.ai')} ───`),
    '',

    // ── Agent instructions — system prompt + skill pointer (end of file so
    //    humans reach the quick commands before the agent protocol block) ────
    ...agentInstructions,
    '',
  ];

  process.stdout.write(`${lines.join('\n')}\n`);
}
