import { delegateToNative, resolveNativeBin } from 'octocode/native';
import { fileURLToPath } from 'node:url';

// Saved-graph commands, each one line of help; the runtime ones run their entry under src/runtime/commands.
const COMMANDS = {
  build:
    'Build a stitched graph of a file or directory into a new result directory',
  query:
    'Ask a saved graph: search, about, callers, impact, path, cycle, critical, region, top…',
  refresh:
    'Update a saved graph after edits (only changed chunks rebuild); keeps previous.json',
  diff: 'Compare two saved graphs, or a refresh with the graph it replaced: review findings first',
  serve: 'Build or replay a graph in the live browser viewer',
  channel:
    'The mcp server plus the live viewer, its chat and wiki tools, as a Claude Code channel',
  reply:
    'Answer a viewer chat, query or wiki request through the viewer serving a run',
  wiki: 'Build, read, note or lint the code wiki of a saved graph',
  link: "Propose, confirm, remove or list an agent's links between files, each verified against its quotes",
  render: 'Write brief.txt for a data.json',
  preflight:
    'Report what can run here (engine, language servers) and how to enable the rest',
  html: 'Export a saved run as one offline HTML page',
  enrich:
    'Add provider evidence (coverage, traces, control flow, SCIP) to a data.json',
} as const;

const usage = {
  html: 'octocode-graph html <saved-run> <new.html>',
  enrich: 'octocode-graph enrich <data.json> <manifest.json> <new-data.json>',
} as const;

// `mcp` runs this package's MCP server (saved-graph tools beside the engine's graph tools); the engine's help
// already lists it, so the help below does not repeat it.
const runtimeCommands = new Set([
  ...Object.keys(COMMANDS).filter(name => !Object.hasOwn(usage, name)),
  'mcp',
]);

export async function runGraphCli(args: readonly string[]): Promise<number> {
  const topic = args[0] === 'help' ? args[1] : args[0];
  if (runtimeCommands.has(topic)) {
    if (
      topic === 'channel' &&
      (args[0] === 'help' || args.includes('--help') || args.includes('-h'))
    ) {
      process.stdout.write(
        `Usage: octocode-graph channel\n  ${COMMANDS.channel}\n`
      );
      return 0;
    }
    const entry = fileURLToPath(import.meta.resolve(`#graph-runtime/${topic}`));
    return delegateToNative(process.execPath, [
      entry,
      ...(args[0] === 'help' ? ['--help'] : args.slice(1)),
    ]);
  }
  if (
    (topic === 'html' || topic === 'enrich') &&
    args.length === 2 &&
    (args[0] === 'help' || args[1] === '--help' || args[1] === '-h')
  ) {
    process.stdout.write(`Usage: ${usage[topic]}\n`);
    return 0;
  }
  if (args[0] === 'html') {
    if (args.length !== 3) throw new Error(`Usage: ${usage.html}`);
    const { exportHtml } = await import('./html.ts');
    await exportHtml(args[1], args[2]);
    return 0;
  }
  if (args[0] === 'enrich') {
    if (args.length !== 4) throw new Error(`Usage: ${usage.enrich}`);
    const { enrichExport } = await import('./enrich.ts');
    await enrichExport(args[1], args[2], args[3]);
    return 0;
  }
  const engine = await resolveNativeBin();
  if ('error' in engine) throw new Error(engine.error);
  const code = await delegateToNative(engine.bin, ['__graph', ...args]);
  if (
    code === 0 &&
    args.length === 1 &&
    ['--help', '-h', 'help'].includes(args[0])
  ) {
    const width = Math.max(...Object.keys(COMMANDS).map(name => name.length));
    process.stdout.write(
      `\nSaved graph commands (octocode-graph <command> --help):\n${Object.entries(
        COMMANDS
      )
        .map(([name, what]) => `  ${name.padEnd(width)}  ${what}`)
        .join('\n')}\n`
    );
  }
  return code;
}
