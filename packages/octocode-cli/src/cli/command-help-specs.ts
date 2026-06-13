import { findCommandSpec } from './commands/specs.js';
import type { CLICommandSpec } from './types.js';

export function findStaticCommandHelp(
  name: string
): CLICommandSpec | undefined {
  return findCommandSpec(name);
}
