// ── Help surface sync contract ────────────────────────────────────────────────
// ALL locations must be updated together when changing help text:
//   1. THIS FILE (main-help.ts)                    — top-level `--help`
//   2. packages/octocode/src/cli/tool-command/context.ts — agent-context dump
//   3. octocode-mcp-host/…/resources/systemPrompt.ts — MCP + CLI system prompt
// ─────────────────────────────────────────────────────────────────────────────
import { c, bold, dim, underline } from '../utils/colors.js';
import { getAuthStatus } from '../features/github-oauth.js';
import {
  DIRECT_TOOL_CATEGORIES,
  getDirectToolCategory,
  getDirectToolDescription,
  loadToolContent,
  sortDirectToolNames,
} from '@octocodeai/octocode-tools-core/schema';
import { COMMAND_SPECS } from './commands/specs.js';
import { REGISTERED_COMMAND_NAMES } from './commands/index.js';
import {
  AGENT_TOOL_COMMANDS,
  CONTINUATION_GUIDANCE,
} from './tool-command/agent-contract.js';
import { TOOL_DEFINITIONS } from './tool-command/registry.js';

// Quick (read-first) commands get a rich arg hint; every other command is
// derived from COMMAND_SPECS below so the list never drifts or misses one.
const QUICK_COMMAND_NAMES = new Set(['cache']);
const REGISTERED_COMMAND_NAME_SET = new Set(REGISTERED_COMMAND_NAMES);

/**
 * Agent instructions block: explains how to drive the CLI (list tools, read a
 * schema, and call a tool. The canonical system prompt and per-tool guidance
 * stay behind `context --full` so top-level help remains a cheap router.
 */
function buildAgentInstructionsBlock(): string[] {
  const usage = [
    'AGENT: inspect schema before unfamiliar calls; never guess fields.',
    '  tools --json --compact                    lean catalog',
    '  tools <name> --scheme --json --compact    lean machine schema',
    '  tools <name> --scheme                     full descriptions + relations',
    "  tools <name> --queries '<json>' --compact  lean typed result (default output is YAML)",
    '  Batch with queries[]; results[] preserve zero-based index and isolate errors.',
    `  ${CONTINUATION_GUIDANCE}`,
    '  Full MCP protocol + tool guidance: context --full; cheapest index: context --minimal.',
  ];
  return [
    `  ${dim('<AGENT_INSTRUCTIONS>')}`,
    ...usage.map(line => `  ${dim(line)}`),
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
    TOOL_DEFINITIONS.filter(tool => !tool.disabled).map(tool => tool.name)
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
 * `context [--full|--minimal] [--json]` form (a contract checked by cli:check).
 */
const COMMAND_INDEX: Record<string, { label?: string; desc: string }> = {
  skill: { desc: 'manage bundled Octocode skills' },
  context: {
    label: 'context [--full|--minimal] [--json]',
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
  const toolCount = TOOL_DEFINITIONS.filter(tool => !tool.disabled).length;
  const catalogCount = TOOL_DEFINITIONS.length;
  const metadata = await getOptionalToolMetadata();
  const toolLines = buildToolBlock(metadata);
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
    `  ${c('green', bold('QUICK COMMANDS'))}  ${dim('read-only materialization')}`,
    quick(
      'cache',
      'fetch <owner/repo> [path]',
      'materialize remote content locally'
    ),
    '',

    // ── Raw execution — every tool, schema-exact ───────────────────────────
    `  ${bold(`TOOLS (${toolCount} enabled / ${catalogCount} cataloged)`)}  ${dim('name + concise description')}`,
    `    ${c('yellow', 'tools'.padEnd(31))} ${dim('list public catalog + availability')}`,
    `    ${c('yellow', AGENT_TOOL_COMMANDS.schema.padEnd(31))} ${dim('lean schema + relations')}`,
    `    ${c('yellow', "tools <name> --queries '<json>' --compact".padEnd(31))} ${dim('lean run')}`,
    ...toolLines,
    '',

    // ── Every other command — an INDEX (short summary), full usage in --help ─
    `  ${bold('MORE COMMANDS')}  ${dim('· full usage:')} ${c('cyan', '<command> --help')}`,
    // `context` is dispatched in cli/index.ts (not a command loader) but must
    // appear in MORE COMMANDS — cli:check asserts the context usage label.
    ...COMMAND_SPECS.filter(
      s =>
        !QUICK_COMMAND_NAMES.has(s.name) &&
        (REGISTERED_COMMAND_NAME_SET.has(s.name) || s.name === 'context')
    ).map(s => commandIndexLine(s.name)),
    '',

    // ── Flags · exit codes · docs (compact, no repetition) ─────────────────
    `  ${bold('FLAGS')}  ${c('cyan', '--json')} ${dim('envelope ·')} ${c('cyan', '--compact')} ${dim('lean ·')} ${c('cyan', '--pretty')} ${dim('readable JSON ·')} ${c('cyan', '--raw')} ${dim('bare file ·')} ${c('cyan', '--no-color')}`,
    `  ${bold('EXIT')}   ${dim('0 ok · 2 input · 3 not-found · 4 auth · 5 tool · 7 rate-limit')}`,
    `  ${bold('DOCS')}   ${underline('https://github.com/bgauryy/octocode/tree/main/docs')}`,
    '',

    // ── Agent protocol — last, so humans reach quick commands first ─────────
    ...agentInstructions,
    '',
  ];

  process.stdout.write(`${lines.join('\n')}\n`);
}
