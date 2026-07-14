import { findCommandSpec } from './commands/specs.js';
import type { CLICommandSpec } from './types.js';

// Commands removed from this CLI build (still in octocode-core external package).
const REMOVED_COMMANDS = new Set<string>();

export function findStaticCommandHelp(
  name: string
): CLICommandSpec | undefined {
  if (REMOVED_COMMANDS.has(name)) return undefined;
  return findCommandSpec(name);
}
