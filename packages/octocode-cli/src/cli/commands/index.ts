import type { CLICommand } from '../types.js';
import { installCommand } from './install.js';
import { authCommand, loginCommand, logoutCommand } from './auth.js';
import { tokenCommand } from './token.js';
import { skillsCommand } from './skills.js';
import { statusCommand } from './status.js';
import { getCommand } from './get.js';
import { treeCommand } from './tree.js';
import { searchCommand } from './search.js';
import { prCommand } from './pr.js';
import { toolCommand } from '../tool-command.js';

const commands: CLICommand[] = [
  // Agent-friendly unified commands
  getCommand,
  treeCommand,
  searchCommand,
  prCommand,
  // Management commands
  installCommand,
  authCommand,
  loginCommand,
  logoutCommand,
  skillsCommand,
  toolCommand,
  tokenCommand,
  statusCommand,
];

export function findCommand(name: string): CLICommand | undefined {
  return commands.find(cmd => cmd.name === name || cmd.aliases?.includes(name));
}

export {
  getCommand,
  treeCommand,
  searchCommand,
  prCommand,
  installCommand,
  authCommand,
  loginCommand,
  logoutCommand,
  tokenCommand,
  skillsCommand,
  statusCommand,
};
