import type { CLICommand, ParsedArgs } from '../types.js';
import { printAuthStatus } from './shared.js';

export const statusCommand: CLICommand = {
  name: 'status',
  aliases: ['s'],
  description: 'Show GitHub authentication status',
  usage: 'octocode-cli status [--hostname <host>]',
  options: [
    {
      name: 'hostname',
      short: 'H',
      description: 'GitHub Enterprise hostname (default: github.com)',
      hasValue: true,
    },
  ],
  handler: async (args: ParsedArgs) => {
    const hostnameOpt = args.options['hostname'] ?? args.options['H'];
    const hostname =
      (typeof hostnameOpt === 'string' ? hostnameOpt : undefined) ||
      'github.com';
    printAuthStatus(hostname);
  },
};
