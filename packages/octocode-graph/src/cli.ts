import { delegateToNative, resolveNativeBin } from 'octocode/native';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { graphCli, shellArg } from './runtime/invocation.ts';
import type { ManifestProvider } from './enrich.ts';

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
    'Add provider evidence (coverage, traces, control flow, SCIP, bundler metafiles, CPU and heap profiles) to a data.json',
  locate:
    'Map bundled or minified positions (file:line:col) to source lines and graph symbols through source maps',
  cache:
    'Show or prune the shared graph cache of chunk results, LSP answers and package closures',
  events:
    "Summarize a saved run's events.jsonl: tool calls, cursor pages, errors and time per tool, phases per run",
  check:
    'Check dependency rules (forbidden, allowed, required, reachable, circular) for CI: baseline, SARIF, --since <rev>',
} as const;

// Arguments of the commands this module runs itself; graphCli prefixes the invocation that works here.
const usage = {
  html: ['html <saved-run> <new.html>'],
  enrich: [
    'enrich <data.json> <manifest.json> <new-data.json>',
    'enrich --manifest-from <data.json> <new-manifest.json> --revision <rev> (--provider <format> <file> [--test-id <id>] [--workload <name>] [--source-root <dir>] [--working-directory <dir>] [--module-file <file>])...',
  ],
} as const;
const usageOf = (topic: keyof typeof usage) =>
  `Usage: ${usage[topic].map(line => graphCli(line)).join('\n       ')}`;

// `graphQuery --all-pages`: this package reads the call to its end (runtime/all-pages.ts) and prints the joined
// reply; the engine's own help lists every other option.
const ALL_PAGES = '--all-pages';
const allPagesHelp = () => `
Option of ${graphCli('graphQuery')}:
      --all-pages       Read the call to its end and print one JSON reply without next: every page, row
                        fragment, evidence list and cycle member list joined, each row once, with every page's
                        warnings. Starts from {"queries":[…]}, not a cursor; exit codes as above
`;

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
        `Usage: ${graphCli('channel')}\n  ${COMMANDS.channel}\n`
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
    process.stdout.write(`${usageOf(topic)}\n`);
    return 0;
  }
  if (args[0] === 'html') {
    if (args.length !== 3) throw new Error(usageOf('html'));
    const { exportHtml } = await import('./html.ts');
    await exportHtml(args[1], args[2]);
    return 0;
  }
  if (args[0] === 'enrich' && args[1] === '--manifest-from')
    return writeManifestCli(args.slice(2));
  if (args[0] === 'enrich') {
    if (args.length !== 4) throw new Error(usageOf('enrich'));
    const { enrichExport } = await import('./enrich.ts');
    await enrichExport(args[1], args[2], args[3]);
    return 0;
  }
  const queryHelp =
    topic === 'graphQuery' &&
    (args[0] === 'help' || args.includes('--help') || args.includes('-h'));
  if (args[0] === 'graphQuery' && args.includes(ALL_PAGES) && !queryHelp)
    return allPagesCli(args.slice(1));
  const engine = await resolveNativeBin();
  if ('error' in engine) throw new Error(engine.error);
  // The engine knows no --all-pages: its help is asked without it, and this package's option follows.
  const code = await delegateToNative(engine.bin, [
    '__graph',
    ...(queryHelp ? args.filter(arg => arg !== ALL_PAGES) : args),
  ]);
  if (code === 0 && queryHelp) process.stdout.write(allPagesHelp());
  if (
    code === 0 &&
    args.length === 1 &&
    ['--help', '-h', 'help'].includes(args[0])
  ) {
    const width = Math.max(...Object.keys(COMMANDS).map(name => name.length));
    process.stdout.write(
      `\nSaved graph commands (${graphCli('<command> --help')}):\n${Object.entries(
        COMMANDS
      )
        .map(([name, what]) => `  ${name.padEnd(width)}  ${what}`)
        .join('\n')}\n`
    );
  }
  return code;
}

// `graphQuery [JSON]... [--input <FILE|->] [--json] --all-pages`: the input as the engine reads it, read to its end.
async function allPagesCli(args: readonly string[]): Promise<number> {
  const { AllPagesInputError, exitCodeOf, readAllPages, replyExitCode } =
    await import('./runtime/all-pages.ts');
  const pieces: string[] = [];
  let source: string | undefined;
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === ALL_PAGES || arg === '--json') continue;
    if (arg === '--input' || arg.startsWith('--input=')) {
      source = arg.includes('=') ? arg.slice('--input='.length) : args[++i];
      if (!source) return usageError('--input needs a file or -');
    } else if (arg.startsWith('--')) return usageError(`unknown option ${arg}`);
    else pieces.push(arg);
  }
  if (source !== undefined && pieces.length)
    return usageError(
      'give the JSON envelope as an argument or with --input, not both'
    );
  const text =
    source === undefined
      ? pieces.join(' ')
      : readFileSync(source === '-' ? 0 : source, 'utf8');
  let input: unknown;
  try {
    input = JSON.parse(text);
  } catch (error) {
    return usageError(
      `the JSON envelope does not parse: ${error instanceof Error ? error.message : String(error)}`
    );
  }
  try {
    const reply = await readAllPages(input);
    process.stdout.write(
      `${process.stdout.isTTY ? JSON.stringify(reply, null, 2) : JSON.stringify(reply)}\n`
    );
    return replyExitCode(reply);
  } catch (error) {
    if (error instanceof AllPagesInputError) return usageError(error.message);
    process.stderr.write(
      `graphQuery --all-pages: ${error instanceof Error ? error.message : String(error)}\n`
    );
    return exitCodeOf((error as { errorClass?: unknown }).errorClass);
  }
}

function usageError(message: string) {
  process.stderr.write(
    `graphQuery --all-pages: ${message}\nUsage: ${graphCli(`graphQuery '{"queries":[…]}' ${ALL_PAGES}`)}\n`
  );
  return 2;
}

// `enrich --manifest-from <data.json> <new-manifest.json> --revision <rev> --provider <format> <file> [options]...`:
// each provider option after a --provider belongs to that provider.
const PROVIDER_OPTIONS = {
  '--test-id': 'testId',
  '--workload': 'workload',
  '--source-root': 'sourceRoot',
  '--working-directory': 'workingDirectory',
  '--module-file': 'moduleFile',
} as const;

async function writeManifestCli(args: readonly string[]): Promise<number> {
  const { writeManifest } = await import('./enrich.ts');
  const fail = (message: string) => {
    process.stderr.write(
      `enrich --manifest-from: ${message}\n${usageOf('enrich')}\n`
    );
    return 2;
  };
  const [base, manifest, ...rest] = args;
  if (!base || !manifest || base.startsWith('--') || manifest.startsWith('--'))
    return fail('name the graph export and the new manifest file');
  let revision: string | undefined;
  const providers: ManifestProvider[] = [];
  for (let i = 0; i < rest.length; i++) {
    const flag = rest[i];
    const value = rest[i + 1];
    if (flag === '--revision') {
      if (!value) return fail('--revision needs a value');
      revision = value;
      i++;
    } else if (flag === '--provider') {
      const file = rest[i + 2];
      if (!value || !file || file.startsWith('--'))
        return fail('--provider needs a format and a file');
      providers.push({ format: value, file });
      i += 2;
    } else if (Object.hasOwn(PROVIDER_OPTIONS, flag)) {
      const provider = providers.at(-1);
      if (!provider) return fail(`${flag} belongs to a --provider before it`);
      if (!value) return fail(`${flag} needs a value`);
      provider[PROVIDER_OPTIONS[flag as keyof typeof PROVIDER_OPTIONS]] = value;
      i++;
    } else return fail(`unexpected ${flag}`);
  }
  if (!revision)
    return fail('--revision names the revision the files describe');
  if (!providers.length)
    return fail('add at least one --provider <format> <file>');
  const written = await writeManifest(base, manifest, { revision, providers });
  process.stdout.write(
    `wrote ${manifest}: baseSha256 of ${base} and the sha256 of ${written.providers.length === 1 ? '1 provider file' : `${written.providers.length} provider files`}; next: ${graphCli(`enrich ${shellArg(base)} ${shellArg(manifest)} <new-data.json>`)}\n`
  );
  return 0;
}
