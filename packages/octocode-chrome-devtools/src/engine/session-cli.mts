#!/usr/bin/env node
// `session --action start|list|show|close`: see session-command.mts.
import { chromeCommands } from './chrome-contract.mjs';
import { errorMessage } from './error-message.mjs';
import { sessionCommand } from './session-command.mjs';

const args = process.argv.slice(2);
if (args.includes('--help') || args.includes('-h')) {
  process.stdout.write(
    `session: ${chromeCommands.session}\nUsage: session --action start [--port 9222] | --action list | --action show --id <s-...> | --action close --id <s-...>\n`
  );
} else {
  try {
    process.stdout.write(`${JSON.stringify(await sessionCommand(args))}\n`);
  } catch (error) {
    process.stderr.write(`[SESSION] ${errorMessage(error)}\n`);
    process.exitCode = 2;
  }
}
