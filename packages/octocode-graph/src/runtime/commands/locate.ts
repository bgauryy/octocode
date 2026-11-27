#!/usr/bin/env node
// `octocode-graph locate`: map positions in bundled or minified code (stack frames, profile frames) back to source
// lines through the bundle's source map (sourcemap.ts), and with a graph name the enclosing graph symbol.
import { openGraph } from '../api.ts';
import { formatLocated, locatePositions, parsePosition } from '../sourcemap.ts';
import { graphCommand } from '../invocation.ts';

const usage =
  () => `usage: ${graphCommand()} locate <bundle.js:line:col>... [--graph <result-dir|project-dir>] [--json]
  Maps each generated position (1-based line and column, as stack traces print them) to its original
  source:line:column and name through the bundle's source map: its sourceMappingURL (a file or a data: URL), else
  <bundle>.map. With --graph, names the graph symbol whose lines hold the mapped line and its declaration line.
  Without a map, names the function around the generated position from the bundle text and lists the graph
  symbols of that name: one is reported as resolved by name only, several as candidates, none as none.
  --json writes every field. Exit 1 when a bundle cannot be read, 2 for bad arguments.`;

function main(argv: string[]) {
  if (argv.includes('--help') || argv.includes('-h')) {
    process.stdout.write(`${usage()}\n`);
    return 0;
  }
  const json = argv.includes('--json');
  const positions: string[] = [];
  let graphArg: string | undefined;
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--json') continue;
    if (argv[i] === '--graph') graphArg = argv[++i];
    else positions.push(argv[i]);
  }
  const bad = positions.filter(p => !parsePosition(p));
  if (
    !positions.length ||
    bad.length ||
    (argv.includes('--graph') && !graphArg)
  ) {
    if (bad.length)
      process.stderr.write(
        `octocode-graph locate: not <file>:<line>:<column>: ${bad.join(' ')}\n`
      );
    process.stderr.write(`${usage()}\n`);
    return 2;
  }
  let graph;
  try {
    graph = graphArg ? openGraph(graphArg) : undefined;
  } catch (error) {
    process.stderr.write(
      `octocode-graph locate: ${error instanceof Error ? error.message : String(error)}\n`
    );
    return 1;
  }
  if (graph?.run) process.stderr.write(`graph: ${graph.run}\n`);
  const rows = locatePositions(positions, graph?.data);
  process.stdout.write(
    `${json ? JSON.stringify(rows) : formatLocated(rows, process.cwd())}\n`
  );
  return rows.some(r => r.error) ? 1 : 0;
}

process.exitCode = main(process.argv.slice(2));
