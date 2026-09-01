import { findCommandSpec } from './commands/specs.js';
import type { CLICommandSpec } from './types.js';

// Commands removed from this CLI build (still in octocode-core external package).
const REMOVED_COMMANDS = new Set<string>(['search']);

// `context` is implemented by this CLI rather than the external command set,
// so its help contract lives beside its runtime instead of patching stale text.
const CONTEXT_COMMAND_HELP: CLICommandSpec = {
  name: 'context',
  description: 'Print token-sized agent protocol and tool guidance',
  usage: 'context [--full|--minimal] [--json]',
  scheme: [
    'args: none.',
    'output: agent protocol, output semantics, tool descriptions, and on-demand schema commands.',
  ],
  whenToUse: [
    'Use --minimal for the cheapest tool-name orientation.',
    'Use --full for the shared MCP prompt and complete tool descriptions; schemas remain on demand.',
  ],
  examples: [
    'context --minimal',
    'context',
    'context --full',
    'context --json',
  ],
  options: [
    {
      name: 'full',
      description: 'Include the shared MCP prompt and full descriptions',
    },
    {
      name: 'minimal',
      description: 'Print only protocol, commands, output mode, and tool names',
    },
    { name: 'json', description: 'Output as JSON: { context }' },
  ],
};

export function findStaticCommandHelp(
  name: string
): CLICommandSpec | undefined {
  if (REMOVED_COMMANDS.has(name)) return undefined;
  if (name === 'context') return CONTEXT_COMMAND_HELP;
  return findCommandSpec(name);
}
