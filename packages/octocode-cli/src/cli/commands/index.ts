import type { CLICommand } from '../types.js';
import { installCommand } from './install.js';
import { authCommand, loginCommand, logoutCommand } from './auth.js';
import { tokenCommand } from './token.js';
import { skillsCommand } from './skills.js';
import { statusCommand } from './status.js';

type CommandLoader = () => Promise<CLICommand>;

const lightweightCommands: readonly CLICommand[] = [
  installCommand,
  authCommand,
  loginCommand,
  logoutCommand,
  skillsCommand,
  tokenCommand,
  statusCommand,
];

const commandLoaders: Record<string, CommandLoader> = {
  // Smart commands temporarily unhooked — will be re-added in a future release.
  // get: async () => (await import('./get.js')).getCommand,
  // tree: async () => (await import('./tree.js')).treeCommand,
  // files: async () => (await import('./files.js')).filesCommand,
  // search: async () => (await import('./search.js')).searchCommand,
  // pr: async () => (await import('./pr.js')).prCommand,
  // repo: async () => (await import('./repo.js')).repoCommand,
  // pkg: async () => (await import('./pkg.js')).pkgCommand,
  // symbols: async () => (await import('./symbols.js')).symbolsCommand,
  // lsp: async () => (await import('./lsp.js')).lspCommand,
  install: async () => (await import('./install.js')).installCommand,
  auth: async () => (await import('./auth.js')).authCommand,
  login: async () => (await import('./auth.js')).loginCommand,
  logout: async () => (await import('./auth.js')).logoutCommand,
  skills: async () => (await import('./skills.js')).skillsCommand,
  token: async () => (await import('./token.js')).tokenCommand,
  status: async () => (await import('./status.js')).statusCommand,
};

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
  tokenCommand,
  skillsCommand,
  statusCommand,
};
