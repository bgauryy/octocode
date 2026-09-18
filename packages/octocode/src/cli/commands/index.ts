import type { CLICommand } from '../types.js';

type CommandLoader = () => Promise<CLICommand>;

const commandLoaders: Record<string, CommandLoader> = {
  skill: async () => (await import('./skill.js')).skillCommand,
};

// Every command the CLI dispatches. Each MUST have a matching spec in
// the package-local command-spec registry (the single source of truth) —
// enforced by tests/cli/command-spec-coverage.test.ts so help never silently
// falls back to a non-core source.
export const REGISTERED_COMMAND_NAMES: readonly string[] = [
  ...Object.keys(commandLoaders),
];

export function isRegisteredCommand(name: string): boolean {
  return Object.hasOwn(commandLoaders, name);
}

export async function loadCommand(
  name: string
): Promise<CLICommand | undefined> {
  const loader = commandLoaders[name];
  return loader ? loader() : undefined;
}
