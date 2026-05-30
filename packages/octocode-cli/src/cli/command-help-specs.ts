import { findCommand } from './commands.js';
import type { CLICommand, CLICommandSpec } from './types.js';

export function findStaticCommandHelp(
  name: string
): CLICommandSpec | undefined {
  const command = findCommand(name);
  return command ? toCommandSpec(command) : undefined;
}

function toCommandSpec(command: CLICommand): CLICommandSpec {
  const { handler: _handler, ...spec } = command;
  return spec;
}
