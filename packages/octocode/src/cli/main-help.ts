// ── Help surface sync contract ────────────────────────────────────────────────
// ALL locations must be updated together when changing help text:
//   1. THIS FILE (main-help.ts)                    — top-level `--help`
//   2. packages/octocode/src/cli/commands/search.ts — renderEnvelope, per-command hints
//   3. packages/octocode-tools-core/src/oql/schemeText.ts — `--scheme` JSON output
//   4. octocode-mcp-host/…/resources/tools/oqlSearch.ts — MCP tool description (sibling repo resources)
//   5. octocode-mcp-host/…/resources/cli/search.ts  — CLICommandSpec (scheme[], whenToUse[])
//   6. octocode-mcp-host/…/resources/systemPrompt.ts — MCP + CLI system prompt
// ─────────────────────────────────────────────────────────────────────────────
import { c, bold, dim, underline } from '../utils/colors.js';
import { getAuthStatus } from '../features/github-oauth.js';
import {
  DIRECT_TOOL_CATEGORIES,
  DIRECT_TOOL_DEFINITIONS,
  getDirectToolCategory,
  getDirectToolDescription,
  loadToolContent,
  sortDirectToolNames,
} from '@octocodeai/octocode-tools-core/schema';
import { COMMAND_SPECS } from './commands/specs.js';

// Quick (read-first) commands get a rich arg hint; every other command is
// derived from COMMAND_SPECS below so the list never drifts or misses one.
const QUICK_COMMAND_NAMES = new Set(['search', 'unzip', 'clone', 'cache']);

/**
 * Essence of the agent protocol; `context` (or `context --full`) has the full
 * prompt. Pre-wrapped to ~78 visible columns as plain dim lines so it renders
 * cleanly in a normal terminal instead of reflowing into a wall of text.
 */
function buildAgentInstructionsBlock(): string[] {
  const body = [
    'search = read-only research. Pick a SOURCE (local path · owner/repo[/path]',
    '· npm name · --query <oql>), then a LANE: text · --tree · --search path',
    '· --op (LSP) · --target repositories|packages|pullRequests|commits|artifacts|diff.',
    'Loop: orient cheap (tree/discovery) → narrow → read exact',
    '(--content-view exact) → prove. Snippets are discovery, not proof;',
    'status:empty is a real run, not absence — follow next.* continuations.',
    'Before any raw `tools` call read `tools <name> --scheme` (never guess',
    'fields). Full protocol + playbook: `context`.',
  ];
  return [
    `  ${dim('<AGENT_INSTRUCTIONS>')}`,
    ...body.map(line => `  ${dim(line)}`),
    `  ${dim('</AGENT_INSTRUCTIONS>')}`,
  ];
}

const DESCRIPTION_PREFIXES = new Set([
  'github',
  'local',
  'npm',
  'package',
  'search',
  'other',
]);

function truncateDescription(desc: string, maxLen: number): string {
  if (desc.length <= maxLen) return desc;
  const cut = desc.lastIndexOf(' ', maxLen - 1);
  return cut > maxLen * 0.6
    ? desc.slice(0, cut) + '…'
    : desc.slice(0, maxLen - 1) + '…';
}

function extractShortDescription(fullDescription: string): string {
  return fullDescription
    .split('\n')[0]
    .trim()
    .replace(/^##\s*/, '');
}

async function getOptionalToolMetadata(): Promise<Awaited<
  ReturnType<typeof loadToolContent>
> | null> {
  try {
    return await loadToolContent();
  } catch {
    return null;
  }
}

function formatConciseToolDescription(
  toolName: string,
  metadata: Awaited<ReturnType<typeof loadToolContent>> | null
): string {
  const raw = extractShortDescription(
    getDirectToolDescription(toolName, metadata)
  );
  const parts = raw
    .split(/\s+\|\s+/)
    .map(part => part.trim())
    .filter(Boolean);
  const concise =
    parts.find(part => !DESCRIPTION_PREFIXES.has(part.toLowerCase())) ??
    raw.replace(/^(?:github|local|npm|package|search|other)\s*\|\s*/i, '');

  return truncateDescription(concise.replace(/\s+/g, ' ').trim(), 82);
}

function buildToolBlock(
  metadata: Awaited<ReturnType<typeof loadToolContent>> | null
): string[] {
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
      lines.push(
        `      ${c('cyan', namePad)} ${dim(formatConciseToolDescription(name, metadata))}`
      );
    }
  }

  return lines;
}

/**
 * Short index summaries for non-quick commands. The full multi-flag usage lives
 * in `<command> --help`; the top-level help only needs a scannable one-liner
 * that fits a normal terminal. `context`'s label keeps the exact
 * `context [--full] [--json]` form (a contract checked by cli:check).
 */
const COMMAND_INDEX: Record<string, { label?: string; desc: string }> = {
  skill: { desc: 'install / list agent skills' },
  context: {
    label: 'context [--full] [--json]',
    desc: 'agent protocol + tools',
  },
  install: { desc: 'add Octocode to an IDE / MCP client' },
  auth: { desc: 'GitHub auth (login · logout · refresh · status)' },
  login: { desc: 'authenticate with GitHub' },
  logout: { desc: 'sign out of GitHub' },
  status: { desc: 'auth + cache + MCP-client health' },
  'lsp-server': { desc: 'language servers (list · install · status)' },
};

/** One scannable `name  short-summary` index line for a non-quick command. */
function commandIndexLine(name: string): string {
  const entry = COMMAND_INDEX[name];
  const label = entry?.label ?? name;
  const desc = entry?.desc ?? '';
  return `    ${c('cyan', label.padEnd(26))} ${dim(desc)}`;
}

/** One aligned `name <args>  description` line for the QUICK COMMANDS block. */
function quick(name: string, argHint: string, description: string): string {
  return `    ${c('cyan', name.padEnd(8))} ${dim(argHint.padEnd(28))}  ${dim(description)}`;
}

export async function showHelp(): Promise<void> {
  const toolCount = DIRECT_TOOL_DEFINITIONS.length;
  const toolLines = buildToolBlock(await getOptionalToolMetadata());
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
        `  ${c('red', bold('⚠ not authenticated'))} ${dim('— public calls run anonymously; run')} ${c('yellow', bold('login'))} ${dim('for private repos + limits')}`,
        '',
      ];

  const lines = [
    '',
    ...authBanner,
    `  ${c('magenta', bold('🔍🐙 Octocode'))}`,
    '',

    // ── Quick commands FIRST — the friendly, human-first surface ────────────
    `  ${c('green', bold('QUICK COMMANDS'))}  ${dim('read-only research + materialization')}`,
    quick(
      'search',
      '<text> <path|repo> · --scheme',
      'read-only research; --scheme first'
    ),
    quick('unzip', '<archive>', 'unpack an archive, then search it'),
    quick(
      'clone',
      '<owner/repo[/path][@branch]>',
      'clone a repo/subtree locally'
    ),
    quick(
      'cache',
      'fetch <owner/repo> [path]',
      'materialize remote content locally'
    ),
    '',

    // ── Raw execution — every tool, schema-exact ───────────────────────────
    `  ${bold(`TOOLS (${toolCount})`)}  ${dim('name + concise description')}`,
    `    ${c('yellow', 'tools'.padEnd(31))} ${dim('list all tools')}`,
    `    ${c('yellow', 'tools <name> --scheme'.padEnd(31))} ${dim('read schema (never guess)')}`,
    `    ${c('yellow', "tools <name> --queries '<json>' --compact".padEnd(31))} ${dim('lean run')}`,
    ...toolLines,
    '',

    // ── Every other command — an INDEX (short summary), full usage in --help ─
    `  ${bold('MORE COMMANDS')}  ${dim('· full usage:')} ${c('cyan', '<command> --help')}`,
    ...COMMAND_SPECS.filter(s => !QUICK_COMMAND_NAMES.has(s.name)).map(s =>
      commandIndexLine(s.name)
    ),
    '',

    // ── Flags · exit codes · docs (compact, no repetition) ─────────────────
    `  ${bold('FLAGS')}  ${c('cyan', '--json')} ${dim('envelope ·')} ${c('cyan', '--compact')} ${dim('lean ·')} ${c('cyan', '--raw')} ${dim('bare file ·')} ${c('cyan', '--no-color')}`,
    `  ${bold('EXIT')}   ${dim('0 ok · 2 input · 3 not-found · 4 auth · 5 tool · 7 rate-limit')}`,
    `  ${bold('DOCS')}   ${underline('https://github.com/bgauryy/octocode/tree/main/docs')}`,
    '',

    // ── Agent protocol — last, so humans reach quick commands first ─────────
    ...agentInstructions,
    '',
  ];

  process.stdout.write(`${lines.join('\n')}\n`);
}
