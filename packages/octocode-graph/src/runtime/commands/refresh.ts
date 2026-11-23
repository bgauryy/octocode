#!/usr/bin/env node
// Headless incremental refresh of a saved run after code edits (refresh.ts): only the chunks holding a change are
// rebuilt, then the whole graph is stitched, checked and measured again, and data.json, the briefs and the wiki are
// rewritten. Its events append to the run's events.jsonl, continuing the sequence, so a replay shows the refresh.
import { parseArgs } from 'node:util';
import { lspMode } from '../lib/run-options.ts';
import { refreshGraph } from '../api.ts';

const USAGE = `usage: octocode-graph refresh <result-dir> [--lsp smart|all|off] [--progress]
  Rebuilds only the chunks whose files changed since <result-dir> was built (an added or removed package manifest
  rebuilds the whole root), then stitches, checks and measures the whole graph again. Prints one JSON line with the
  changed, added and removed files and the rebuilt chunks. A run a live viewer serves refreshes through that viewer,
  so its page updates.`;

async function main(argv: string[]) {
  let values: { lsp?: string; progress?: boolean; help?: boolean };
  let positionals: string[];
  try {
    ({ values, positionals } = parseArgs({
      args: argv,
      options: {
        lsp: { type: 'string' },
        progress: { type: 'boolean' },
        help: { type: 'boolean', short: 'h' },
      },
      allowPositionals: true,
    }));
    if (values.help) {
      process.stdout.write(`${USAGE}\n`);
      return 0;
    }
    if (positionals.length !== 1)
      throw new Error('Expected one result directory');
  } catch (error) {
    process.stderr.write(
      `${error instanceof Error ? error.message : String(error)}\n${USAGE}\n`
    );
    return 2;
  }
  const t0 = Date.now();
  const controller = new AbortController();
  process.once('SIGINT', () => controller.abort());
  try {
    const summary = await refreshGraph(positionals[0], {
      ...(values.lsp !== undefined && { lsp: lspMode(values.lsp) }),
      signal: controller.signal,
      onEvent: event => {
        if (
          values.progress &&
          (event.type === 'phase' || event.type === 'chunk')
        )
          process.stderr.write(
            `${((Date.now() - t0) / 1000).toFixed(1)}s ${event.type} ${typeof event.id === 'string' ? event.id : ''} ${typeof event.state === 'string' ? event.state : ''}${typeof event.note === 'string' ? ` ${event.note}` : ''}\n`
          );
      },
    });
    process.stdout.write(`${JSON.stringify(summary)}\n`);
    return 0;
  } catch (error) {
    process.stderr.write(
      `octocode-graph refresh: ${error instanceof Error ? error.message : String(error)}\n`
    );
    return 1;
  }
}

process.exitCode = await main(process.argv.slice(2));
