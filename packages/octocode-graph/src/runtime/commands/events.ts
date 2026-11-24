#!/usr/bin/env node
// `octocode-graph events <result-dir|project-dir> [--json]`: a saved run's events.jsonl summarized (events.ts).
import { readEvents } from '../pipeline.ts';
import { resultDir } from '../query.ts';
import { formatEvents, summarizeEvents } from '../events.ts';
import { errorMessage } from '../lib.ts';
import { graphCommand } from '../invocation.ts';

const usage =
  () => `usage: ${graphCommand()} events <result-dir|project-dir> [--json]
  Summarizes the run's events.jsonl. Each octocode call counts once by its tool id (a start, then done or error):
  calls, cursor pages, failed calls, calls answered with an error row, unfinished calls, rows, bytes and summed call
  time, per tool and in total. Per run (the build, each refresh, then any later events): wall time from its first
  event to its last, its calls and its phases with their durations. Checks that every id started once. A project
  directory names its newest saved graph. --json writes every field.`;

function main(argv: string[]) {
  if (argv.includes('--help') || argv.includes('-h')) {
    process.stdout.write(`${usage()}\n`);
    return 0;
  }
  const json = argv.includes('--json');
  const args = argv.filter(a => a !== '--json');
  if (args.length !== 1 || args[0].startsWith('--')) {
    process.stderr.write(`${usage()}\n`);
    return 2;
  }
  let dir: string;
  try {
    dir = resultDir(args[0]);
  } catch (error) {
    process.stderr.write(`octocode-graph events: ${errorMessage(error)}\n`);
    return 2;
  }
  try {
    const summary = summarizeEvents(readEvents(dir, { rows: false }));
    process.stdout.write(
      `${json ? JSON.stringify({ dir, ...summary }) : formatEvents(summary, dir)}\n`
    );
    return 0;
  } catch (error) {
    process.stderr.write(`octocode-graph events: ${errorMessage(error)}\n`);
    return 1;
  }
}

process.exitCode = main(process.argv.slice(2));
