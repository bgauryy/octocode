import { c, bold, dim, underline } from '../utils/colors.js';
import {
  DIRECT_TOOL_CATEGORIES,
  getDirectToolCategory,
  formatConciseToolDescription,
  sortDirectToolNames,
} from '@octocodeai/octocode-core/schema';
import { COMMAND_SPECS } from './commands/specs.js';
import {
  AGENT_TOOL_COMMANDS,
  CLI_HELP_USAGE_GUIDANCE,
} from '@octocodeai/octocode-core/mcp';
import { TOOL_DEFINITIONS } from './tool-command/registry.js';

// Quick (read-first) commands get a rich arg hint; every other command is
// derived from COMMAND_SPECS below so the list never drifts or misses one.
const QUICK_COMMAND_NAMES = new Set(['cache']);

/**
 * Agent instructions block: explains how to drive the CLI (list tools, read a
 * schema, and call a tool. The canonical system prompt and per-tool guidance
 * stay behind `context --full` so top-level help remains a cheap router.
 */
function buildAgentInstructionsBlock(): string[] {
  return [
    `  ${dim('<AGENT_INSTRUCTIONS>')}`,
    ...CLI_HELP_USAGE_GUIDANCE.map(line => `  ${dim(line)}`),
    `  ${dim('</AGENT_INSTRUCTIONS>')}`,
  ];
}

function buildToolBlock(): string[] {
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
        `      ${c('cyan', namePad)} ${dim(formatConciseToolDescription(name, 82))}`
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
  const toolLines = buildToolBlock();
  const agentInstructions = buildAgentInstructionsBlock();

  let isAuthenticated = false;
  try {
    const { getAuthStatus } = await import('../features/github-oauth.js');
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
    `    ${c('yellow', "tools <name> --queries '<json>'".padEnd(31))} ${dim('lean run (minified JSON by default)')}`,
    `    ${c('yellow', 'tools <name> [op] --<field> <value>'.padEnd(31))} ${dim('flag run — schema fields as kebab-case flags')}`,
    ...toolLines,
    '',

    // ── Every other command — an INDEX (short summary), full usage in --help ─
    `  ${bold('MORE COMMANDS')}  ${dim('· full usage:')} ${c('cyan', '<command> --help')}`,
    ...COMMAND_SPECS.filter(s => !QUICK_COMMAND_NAMES.has(s.name)).map(s =>
      commandIndexLine(s.name)
    ),
    '',

    // ── Flags · exit codes · docs (compact, no repetition) ─────────────────
    `  ${bold('FLAGS')}  ${dim('tool output is minified JSON by default ·')} ${c('cyan', '--yaml')} ${dim('human view ·')} ${c('cyan', '--json')} ${dim('pretty ·')} ${c('cyan', '--pretty')} ${dim('readable JSON ·')} ${c('cyan', '--raw')} ${dim('bare file ·')} ${c('cyan', '--no-color')}`,
    `  ${bold('EXIT')}   ${dim('0 ok · 2 input · 3 not-found · 4 auth · 5 tool · 7 rate-limit')}`,
    `  ${bold('DOCS')}   ${underline('https://github.com/bgauryy/octocode/tree/main/docs')}`,
    '',

    // ── Agent protocol — last, so humans reach quick commands first ─────────
    ...agentInstructions,
    '',
  ];

  process.stdout.write(`${lines.join('\n')}\n`);
}
