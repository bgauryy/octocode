#!/usr/bin/env node
// `octocode-graph diff`: what changed structurally between two saved graphs (diff.ts). With one result directory it
// compares the graph its last refresh replaced (previous.json) with the current one.
import { openGraph, previousGraph } from '../api.ts';
import { formatDiff } from '../diff.ts';
import { graphCommand } from '../invocation.ts';

const usage = (
  cli = graphCommand()
) => `usage: ${cli} diff <result-dir|project-dir> [--json]
       ${cli} diff <before> <after> [--json]
  Compares two saved graphs (result directories, data.json files or project directories) of the same project;
  with one result or project directory, the graph its last refresh replaced (previous.json) with the current one;
  a project directory names its newest saved graph (.octocode/octocode-graph, then the graph registry). Prints the
  summary and the review findings, high first: new or grown cycles, new dependencies between chunks (flagged when
  a more stable package now depends on a less stable one), files with more dependents, files entering the
  PageRank or betweenness top 20, then the dependencies added and removed, renames and files. --json writes every
  list in full.`;

function main(argv: string[]) {
  const json = argv.includes('--json');
  const args = argv.filter(a => a !== '--json');
  if (argv.includes('--help') || argv.includes('-h')) {
    process.stdout.write(`${usage()}\n`);
    return 0;
  }
  if (args.length < 1 || args.length > 2) {
    process.stderr.write(`${usage()}\n`);
    return 2;
  }
  try {
    const [before, after] =
      args.length === 1
        ? [previousGraph(args[0]), openGraph(args[0])]
        : [openGraph(args[0]), openGraph(args[1])];
    const d = after.diffFrom(before);
    process.stdout.write(`${json ? JSON.stringify(d) : formatDiff(d)}\n`);
    return 0;
  } catch (error) {
    process.stderr.write(
      `octocode-graph diff: ${error instanceof Error ? error.message : String(error)}\n`
    );
    return 1;
  }
}

process.exitCode = main(process.argv.slice(2));
