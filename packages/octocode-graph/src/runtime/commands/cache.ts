#!/usr/bin/env node
// `octocode-graph cache stats|prune`: the shared graph cache (graph-cache.ts) in the Octocode home. `stats` lists its
// entries and bytes by kind; `prune --max-bytes N` evicts the least recently used entries until they hold at most N.
import { parseArgs } from 'node:util';
import { cacheStats, pruneCache } from '../graph-cache.ts';
import { errorMessage } from '../lib.ts';
import { graphCommand } from '../invocation.ts';

const usage = () => `usage: ${graphCommand()} cache stats
       ${graphCommand()} cache prune --max-bytes N
  The shared graph cache holds chunk results, LSP answers and derived data (package closures, search indexes) that
  every build, refresh and result directory reuses, keyed by content. stats prints its entries and bytes by kind as
  JSON; prune evicts the least recently used entries until they hold at most N bytes, and prints what it removed.
  Results in result directories are copies: pruning never changes a saved graph.`;

function main(argv: string[]) {
  let values: { 'max-bytes'?: string; help?: boolean };
  let positionals: string[];
  try {
    ({ values, positionals } = parseArgs({
      args: argv,
      options: {
        'max-bytes': { type: 'string' },
        help: { type: 'boolean', short: 'h' },
      },
      allowPositionals: true,
    }));
  } catch (error) {
    process.stderr.write(`${errorMessage(error)}\n${usage()}\n`);
    return 2;
  }
  if (values.help) {
    process.stdout.write(`${usage()}\n`);
    return 0;
  }
  const [action, ...rest] = positionals;
  if (rest.length || (action !== 'stats' && action !== 'prune')) {
    process.stderr.write(`Expected stats or prune\n${usage()}\n`);
    return 2;
  }
  if (action === 'stats') {
    if (values['max-bytes'] !== undefined) {
      process.stderr.write(`--max-bytes belongs to prune\n${usage()}\n`);
      return 2;
    }
    process.stdout.write(`${JSON.stringify(cacheStats())}\n`);
    return 0;
  }
  const raw = values['max-bytes'];
  if (raw === undefined || !/^\d+$/.test(raw)) {
    process.stderr.write(
      `prune needs --max-bytes N (a nonnegative integer)\n${usage()}\n`
    );
    return 2;
  }
  process.stdout.write(`${JSON.stringify(pruneCache(Number(raw)))}\n`);
  return 0;
}

process.exitCode = main(process.argv.slice(2));
