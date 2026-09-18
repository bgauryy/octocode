import type { DatabaseSync } from '../sqlite.js';
import { runAwarenessHistoryOperation } from '../history.js';
import { HistoryError } from '../history-store.js';
import { HISTORY_ROUTE_DESCRIPTORS } from '../schema/definitions-history.js';
import type { ParsedArgs } from './args.js';

export async function runHistoryCommand(db: DatabaseSync, command: string, args: ParsedArgs) {
  const route = HISTORY_ROUTE_DESCRIPTORS.find(entry => entry.schema === command);
  if (!route) throw new Error('Unknown history route.');
  const request: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(args)) {
    if (key === '_') continue;
    if (key === 'limit' || key === 'offset' || key === 'grace_seconds') request[key] = typeof value === 'string' ? Number(value) : value;
    else if (Array.isArray(value) && !(key === 'file' && ['history_capture', 'history_checkpoint', 'history_restore_preview'].includes(command))) {
      if (value.length !== 1) throw new HistoryError('HISTORY_INVALID_REQUEST', `--${key} accepts one value for this command.`);
      request[key] = value[0];
    } else request[key] = value;
  }
  if (args._.length) throw new HistoryError('HISTORY_INVALID_REQUEST', 'History commands do not accept positional arguments.');
  return runAwarenessHistoryOperation(db, route.command.slice('history '.length), request);
}
