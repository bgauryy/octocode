// ── Help surface sync contract ────────────────────────────────────────────────
// ALL locations must be updated together when changing help text:
//   1. THIS FILE (main-help.ts)                    — top-level `--help`
//   2. packages/octocode/src/cli/commands/search.ts — renderEnvelope, per-command hints
//   3. packages/octocode-tools-core/src/oql/schemeText.ts — `--scheme` JSON output
//   4. octocode-mcp-host/…/resources/tools/oqlSearch.ts — MCP tool description (sibling repo resources)
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
  sortDirectToolNames,
} from '@octocodeai/octocode-tools-core/schema';
import { paths } from '@octocodeai/octocode-tools-core/paths';

const LSP_TOOL = 'lspGetSemantics';

/** Canonical octocode-engineer skill — the agent playbook for these flows. */
const ENGINEER_SKILL_URL =
  'https://github.com/bgauryy/octocode/tree/main/skills/octocode-engineer';
const UNZIP_DESTINATION_PATTERN = join(paths.unzip, '<name>-<timestamp>');

/** Render a concise agent playbook; use `context --full` for the full MCP prompt. */
function buildAgentInstructionsBlock(): string[] {
  const lines: string[] = [`  ${dim('<AGENT_INSTRUCTIONS>')}`];

  lines.push(
    `  ${dim('Use')} ${c('cyan', 'search')} ${dim('for read-only research. Pick the source first, then the target/lane.')}`,
    `    ${c('cyan', 'Local')}  ${dim('path input → code, files, content, tree, artifacts, diff, and')} ${c('cyan', 'search --op')} ${dim('for LSP semantics.')}`,
    `    ${c('cyan', 'GitHub')} ${dim('owner/repo[/path] → code, tree, content, repos, PRs, commits. Use')} ${c('cyan', '--repo owner/repo --materialize required')} ${dim('when the GitHub index misses or AST/LSP proof is needed.')}`,
    `    ${c('cyan', 'npm')}    ${dim('package names →')} ${c('cyan', '--target packages')} ${dim('to resolve metadata/source repo, then continue with GitHub or local proof.')}`,
    `    ${c('cyan', 'OQL')}    ${dim('run')} ${c('cyan', 'search --scheme')} ${dim('before JSON; use')} ${c('cyan', 'search --explain --query ...')} ${dim('when routing is unclear.')}`,
    `  ${dim('Best practice: orient cheap → narrow → read exact → prove. Use discovery/tree/symbols first; fetch exact slices with --content-view exact only when quoting, diffing, or deciding.')}`,
    `  ${dim('Follow')} ${c('cyan', 'next.*')} ${dim('continuations for pages, exact reads, materialization, or LSP proof. Snippets are discovery, not proof.')}`,
    `  ${dim('Minimal OQL:')} ${c('yellow', '{"target":"code","from":{"kind":"local","path":"./src"},"where":{"kind":"text","value":"term"}}')}`,
    `  ${dim('Semantics flow:')} ${c('cyan', 'search file --op documentSymbols')} ${dim('→ use returned line with')} ${c('cyan', '--op references|callers|hover --symbol X --line N')}${dim('.')}`,
    `  ${dim('Trust evidence/status: empty is a real run with no rows, not proof of absence until scope, spelling, branch, and source were checked.')}`,
    '',
    `  ${dim('Tools:')} ${c('yellow', 'tools <name> --scheme')} ${dim('to read a schema (never guess fields), then')} ${c('yellow', "tools <name> --queries '<json>'")} ${dim('to run it. QUICK COMMANDS below cover the common path.')}`,
    `  ${dim('JSON shape:')} ${c('cyan', 'tools --json')} ${dim('returns a CallToolResult;')} ${c('cyan', 'search --query --json')} ${dim('returns the native OQL envelope with domain rows and next.* continuations.')}`,
    `  ${dim('Skill reference — read the')} ${c('cyan', 'octocode-engineer')} ${dim('flows to understand the research loop and leverage every tool fully:')}`,
    `    ${underline(ENGINEER_SKILL_URL)}`,
    `  ${dim('Auth: humans run')} ${c('yellow', 'login')}${dim('; agents run')} ${c('yellow', 'auth status --json')} ${dim('for token state; pass GITHUB_TOKEN / OCTOCODE_TOKEN / GH_TOKEN via env. Deeper protocol:')} ${c('cyan', 'context')}${dim('.')}`,
    `  ${dim('</AGENT_INSTRUCTIONS>')}`
  );

  return lines;
}

/** Brief [required*, optional?] summary for the --help tool list (top-level fields only). */
function formatBriefFields(toolName: string): string {
  // `type` is the only always-required field; `uri` is required for every type
  // except workspaceSymbol, so it stays optional here (see tool-command.ts).
  if (toolName === LSP_TOOL) return '[type, uri?, symbolName?, lineHint?]';
  if (toolName === 'ghHistoryResearch') {
    return '[type: prs|commits, since?, until?]';
  }
  if (toolName === 'localBinaryInspect') {
    return '[path*, mode: inspect|list|extract|decompress|strings|unpack]';
  }
  if (toolName === 'ghSearchCode') {
    return '[keywords[]?, owner?, repo?]';
  }
  if (toolName === 'localSearchCode') {
    return '[path*, keywords:string?, mode?]';
  }
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
          `      ${dim(indent)} ${dim('type: definition | references | callers | callees | callHierarchy | hover | documentSymbols | typeDefinition | implementation | workspaceSymbol | supertypes | subtypes | diagnostic')}`
        );
        lines.push(
          `      ${dim(indent)} ${dim('! run search first → get uri + lineHint; workspaceSymbol can start from workspaceRoot + symbolName')}`
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
  const agentInstructions = buildAgentInstructionsBlock();

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
    `  ${c('green', bold('QUICK COMMANDS'))}  ${dim('search-first read-only surface plus materialization workflows. Add --json for raw output.')}`,
    quick(
      'search',
      '"<text>" <path|owner/repo> | owner/repo#N | PR-URL | --query <oql-json> | --scheme',
      'read-only OQL across local, GitHub, npm, semantics, artifacts, PR/history, diff — run --scheme first'
    ),
    quick(
      'unzip',
      '<archive>',
      `unpack archive → ${UNZIP_DESTINATION_PATTERN}, then search it`
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

    // ── Search lanes — the agent's source map ──────────────────────────────
    `  ${c('green', bold('SEARCH LANES'))}  ${dim('choose source + lane before adding filters')}`,
    `    ${c('cyan', 'Local')}    ${c('yellow', 'search "term" ./src')} ${dim('·')} ${c('yellow', 'search ./src --tree')} ${dim('·')} ${c('yellow', 'search file.ts --op documentSymbols')}`,
    `    ${c('cyan', 'GitHub')}   ${c('yellow', 'search "term" owner/repo')} ${dim('·')} ${c('yellow', 'search owner/repo --tree')} ${dim('·')} ${c('yellow', 'search owner/repo#123 --target pullRequests')}`,
    `    ${c('cyan', 'npm')}      ${c('yellow', 'search zod --target packages')} ${dim('→ package metadata and source repo')}`,
    `    ${c('cyan', 'OQL JSON')} ${c('yellow', 'search --scheme')} ${dim('→ copy target/from/where/params shape;')} ${c('yellow', 'search --explain --query ...')} ${dim('checks routing')}`,
    '',

    // ── Remote-as-local bridge ──────────────────────────────────────────────
    `  ${c('green', bold('REMOTE AS LOCAL'))}  ${dim('use --repo to analyse GitHub content with local tools')}`,
    `    ${dim('Add')} ${c('cyan', '--repo <owner/repo[@branch]>')} ${dim('to search when you want GitHub content materialized before local proof.')}`,
    `    ${dim('search is the canonical read-only remote-as-local path. The first materialized call fetches; subsequent calls use the disk cache (24 h).')}`,
    `    ${dim('Decision tree:')}`,
    `      ${c('cyan', 'search "<kw>" --repo owner/repo')}      ${dim('→ text search across a remote repo')}`,
    `      ${c('cyan', 'search src --repo owner/repo --search path --materialize auto')} ${dim('→ exact remote filename/path discovery')}`,
    `      ${c('cyan', 'search owner/repo --tree')}             ${dim('→ remote directory tree')}`,
    `      ${c('cyan', 'search path/to/file --repo owner/repo')} ${dim('→ read a single remote file')}`,
    `      ${c('cyan', 'clone owner/repo')}                     ${dim('→ git clone (use for structural + semantic analysis)')}`,
    `      ${c('cyan', 'cache fetch owner/repo [path]')}        ${dim('→ explicit tree-fetch + returns location.{localPath,complete,verified}')}`,
    `    ${dim('After materialization the')} ${c('cyan', 'location')} ${dim('block in every result carries localPath, cached, complete, and verified.')}`,
    `    ${dim('verified:false = served from disk cache (completeness unconfirmed). Use --force-refresh or clone to get verified:true.')}`,
    '',

    // ── Raw execution — every tool, including ones without a quick command ──
    `  ${bold(`TOOLS (${toolCount})`)}  ${dim('raw execution — schema-exact, incl. clone, artifact inspect, structural + semantic code intelligence')}`,
    `    ${c('yellow', 'tools'.padEnd(28))} ${dim('list all tools')}`,
    `    ${c('yellow', 'tools <name> --scheme'.padEnd(28))} ${dim('read schema (never guess fields)')}`,
    `    ${c('yellow', "tools <name> --queries '<json>'".padEnd(28))} ${dim('run a tool (1 object or array of ≤5)')}`,
    `    ${c('yellow', 'context [--full] [--json]'.padEnd(28))} ${dim('optional — protocol + system prompt + descriptions (deeper research)')}`,
    ...toolLines,
    '',

    // ── Playbook (distilled from the system prompt) ─────────────────────────
    `  ${c('green', bold('PLAYBOOK'))}  ${dim('cheapest tool first · smallest slice · narrow before paging')}`,
    `    ${c('cyan', 'orient cheap')}    ${dim('search --tree · search --search path · concise:true / discovery views')}`,
    `    ${c('cyan', 'minify by goal')}  ${dim('symbols=skeleton (orient unknown) · standard=read (default) · none=exact quote/diff')}`,
    `    ${c('cyan', 'prove')}           ${dim('snippets are discovery, not proof — re-read exact text · structural/text search→lineHint→search --op (semantics) · search --target packages→source repo')}`,
    '',

    // ── Management (users) ─────────────────────────────────────────────────
    `  ${bold('MANAGEMENT')}`,
    `    ${c('cyan', 'install')} ${dim('--ide <cursor|claude-desktop|windsurf|...>')}  ${dim('configure IDE')}`,
    `    ${c('cyan', 'skill')}   ${dim('--name <skill> | --add <github-folder>')}        ${dim('install Agent Skill folder')}`,
    `    ${c('cyan', 'auth')}    ${dim('[login|logout|refresh|status] [--json]')}       ${dim('auth menu + script-safe status')}`,
    `    ${c('cyan', 'login')}   ${dim('[--hostname <host>]')}                         ${dim('interactive auth picker')}`,
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
