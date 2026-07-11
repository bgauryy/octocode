// The registrable `tools` CLICommand.
import type { CLICommand, ParsedArgs } from '../types.js';
import { EXIT } from '../exit-codes.js';
import { executeToolCommand } from './execute.js';

export const toolCommand: CLICommand = {
  name: 'tools',
  options: [
    { name: 'queries', hasValue: true },
    { name: 'query', hasValue: true },
    { name: 'list' },
    { name: 'scheme' },
  ],
  handler: async (args: ParsedArgs) => {
    const success = await executeToolCommand(args);
    if (!success && !process.exitCode) {
      process.exitCode = EXIT.GENERAL;
    }
  },
};
