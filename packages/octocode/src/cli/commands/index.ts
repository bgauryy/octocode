import type { CLICommand } from '../types.js';
import { installCommand } from './install.js';
import { authCommand, loginCommand, logoutCommand } from './auth.js';
import { statusCommand } from './status.js';

type CommandLoader = () => Promise<CLICommand>;

const lightweightCommands: readonly CLICommand[] = [
  installCommand,
  authCommand,
  loginCommand,
  logoutCommand,
  statusCommand,
];

const commandLoaders: Record<string, CommandLoader> = {
  // Search is the single read-only research surface. Workflow commands below
  // are kept when they provide distinct materialization/cache actions.
  search: async () => (await import('./search.js')).searchCommand,
  skill: async () => (await import('./skill.js')).skillCommand,
  cache: async () => (await import('./cache.js')).cacheCommand,
  clone: async () => (await import('./clone.js')).cloneCommand,
  install: async () => (await import('./install.js')).installCommand,
  auth: async () => (await import('./auth.js')).authCommand,
  login: async () => (await import('./auth.js')).loginCommand,
  logout: async () => (await import('./auth.js')).logoutCommand,
  status: async () => (await import('./status.js')).statusCommand,
  'lsp-server': async () => (await import('./lsp-server.js')).lspServerCommand,
};

// Every command the CLI dispatches. Each MUST have a matching spec in
// octocode-core (the single source of truth for help/usage/description) —
// enforced by tests/cli/command-spec-coverage.test.ts so help never silently
// falls back to a non-core source.
export const REGISTERED_COMMAND_NAMES: readonly string[] = [
  ...lightweightCommands.map(command => command.name),
  ...Object.keys(commandLoaders),
].filter((name, i, all) => all.indexOf(name) === i);

export function findCommand(name: string): CLICommand | undefined {
  return lightweightCommands.find(command => command.name === name);
}

export async function loadCommand(
  name: string
): Promise<CLICommand | undefined> {
  const lightweightCommand = findCommand(name);
  if (lightweightCommand) {
    return lightweightCommand;
  }

  const loader = commandLoaders[name];
  return loader ? loader() : undefined;
}

export {
  installCommand,
  authCommand,
  loginCommand,
  logoutCommand,
  statusCommand,
};
