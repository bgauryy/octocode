/** Shared repository command handlers; adapters own process and shell behavior. */
import { writeCommandText } from '../command-output.js';
import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname, isAbsolute, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { formatAwarenessQueryResult, queryAwareness } from '../repo-query.js';
import { ParsedArgs } from './args.js';
import { EmitOptions, emit } from '../command-output.js';
import { flagBool } from './args.js';

export function cmdQuery(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const view = String(args['view'] ?? args._[0] ?? 'all');
  const format = String(args['format'] ?? 'json').toLowerCase();
  const workspacePath = args['workspace'] ? String(args['workspace']) : process.cwd();
  const requestedAgentId = args['agent_id'] ? String(args['agent_id']) : null;
  const result = queryAwareness(db, {
    view,
    workspacePath,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    query: args['query'] ? String(args['query']) : null,
    limit: args['limit']
      ? parseInt(String(args['limit']), 10)
      : opts.compact ? (view === 'workboard' ? 1 : 5) : undefined,
    // The host-bound actor identifies workboard ordering and inbox visibility;
    // it must not filter peer work or verification rows out of the shared board.
    agentId: view === 'workboard' ? null : requestedAgentId,
    preferAgentId: view === 'workboard' ? requestedAgentId : null,
    recipientAgentId: view === 'workboard' ? requestedAgentId : null,
    state: Array.isArray(args['state']) ? args['state'].map(String) : args['state'] ? String(args['state']) : null,
    label: Array.isArray(args['label']) ? args['label'].map(String) : args['label'] ? String(args['label']) : null,
    file: args['file'] ? String(Array.isArray(args['file']) ? args['file'][0] : args['file']) : null,
    since: args['since'] ? String(args['since']) : null,
    includeBodies: flagBool(args['include_bodies']),
  });

  const outPath = args['out'] ? String(args['out']) : null;
  if (outPath) {
    const resolvedOutPath = isAbsolute(outPath) ? resolve(outPath) : resolve(workspacePath, outPath);
    mkdirSync(dirname(resolvedOutPath), { recursive: true });
    writeFileSync(resolvedOutPath, formatAwarenessQueryResult(result, format), 'utf8');
    return emit({ db_path: dbPath, path: resolvedOutPath, view: result.view, count: result.count }, 0, opts);
  }

  if (opts.compact && format === 'json' && result.count === 0) {
    return emit({ view: result.view, count: 0, rows: [] }, 0, opts);
  }
  if (format === 'json') return emit({ db_path: dbPath, ...result }, 0, opts);
  writeCommandText(formatAwarenessQueryResult(result, format));
  return 0;
}
