import { defineCli, defineCommand, withCommands } from 'octocode-mcp-cli';
import { z } from 'zod';
import { instructions, mcpCommands } from './generated.js';

const doctor = defineCommand({
  name: 'doctor',
  description: 'Check the local setup.',
  schema: z.object({
    verbose: z.boolean().default(false).describe('Print extra detail'),
  }),
  run: async input => ({ ok: true, verbose: input.verbose }),
});

export const cli = withCommands(
  defineCli({
    name: 'issues',
    instructions,
    commands: mcpCommands,
  }),
  [doctor],
);
