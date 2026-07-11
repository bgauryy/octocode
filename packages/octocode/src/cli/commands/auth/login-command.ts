import type { CLICommand, ParsedArgs } from '../../types.js';
import { runOctocodeLogin } from './login-flow.js';
import { runAuthMenu } from './menu.js';

export const loginCommand: CLICommand = {
  name: 'login',
  options: [
    { name: 'hostname', hasValue: true },
    { name: 'git-protocol', hasValue: true },
    { name: 'force' },
    { name: 'json' },
  ],
  handler: async (args: ParsedArgs) => {
    const jsonOutput = Boolean(args.options['json']);
    const forceLogin = Boolean(args.options['force']);
    if (!jsonOutput && !forceLogin && process.stdout.isTTY) {
      return runAuthMenu(args);
    }

    return runOctocodeLogin(args);
  },
};
