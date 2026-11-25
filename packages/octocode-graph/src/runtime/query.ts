// `octocode-graph query`: saved-graph questions from the shell. The first argument is a result directory, its
// data.json, or a project directory: then the newest saved graph of exactly that root under .octocode/octocode-graph
// (of the current directory or the project) answers, else the newest one the per-user graph registry lists for that
// root (registry.ts: a graph built with --out elsewhere), and stderr names it. A run its live viewer serves answers
// through that server, from the graph it already holds; otherwise the graph is read here. Files edited since the
// graph was written are a warning in every answer (withStaleness), so a stale answer is never silent; with
// --refresh they are refreshed first (api.ts openFreshGraph) and the answer comes from the refreshed graph.
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { once } from 'node:events';
import { runHeader } from './lib/run-output.ts';
import { absOf, rootOf } from '../ids.ts';
import {
  formatResult,
  HINT,
  runQuery,
  type QueryData,
  type QueryResult,
} from './query-api.ts';
import { plural } from './query-ops.ts';
import { postServer, serverOf, ServedError } from './lib/served.ts';
import { graphCli, graphCommand, shellArg } from './invocation.ts';
import { canonicalDir, registeredRuns, registryHome } from './registry.ts';
import { withSearchIndex } from './search-sidecar.ts';
import { errorMessage, isRecord } from './lib.ts';
import { withProvider } from './search-provider.ts';
import type { SavedGraph } from './api.ts';
export * from './query-api.ts';

/** Where builds put their results by default (lib/run-output.ts `freshOut`), relative to the workspace. */
const RUNS = join('.octocode', 'octocode-graph');

const isFile = (p: string) =>
  statSync(p, { throwIfNoEntry: false })?.isFile() ?? false;

/** The graph a result directory answers from: data.json, else the stitched checkpoint of a run still finishing. */
function resultGraph(dir: string) {
  const final = join(dir, 'data.json');
  const checkpoint = join(dir, 'stitched.json');
  if (isFile(final)) return final;
  return isFile(checkpoint) ? checkpoint : null;
}

/** The project root a saved run was built from: its events.jsonl header, the first line. */
function runRoot(dir: string) {
  const root = runHeader(dir)?.root;
  return typeof root === 'string' ? root : null;
}

const subdirs = (place: string) => {
  try {
    return readdirSync(place).map(name => join(place, name));
  } catch {
    return [];
  }
};

// The newest of these result directories holding a graph whose run was built from `root` (its events header names
// it, under any spelling of the same directory): a registry entry never answers for another root.
function newestOf(dirs: string[], root: string) {
  const want = canonicalDir(root);
  const runs = dirs.flatMap(dir => {
    const graph = resultGraph(dir);
    const from = graph && runRoot(dir);
    if (!from || canonicalDir(from) !== want) return [];
    return [{ dir, graph, mtime: statSync(graph).mtimeMs }];
  });
  return runs.toSorted((a, b) => b.mtime - a.mtime)[0];
}

/** The newest saved graph of exactly `root`: the default places first, then the graph registry; or why there is none. */
function newestRun(root: string, home: string) {
  const places = [...new Set([resolve(RUNS), join(root, RUNS)])];
  const local = newestOf(places.flatMap(subdirs), root);
  if (local) return { ...local, registered: false };
  const registered = newestOf(registeredRuns(root, home), root);
  if (registered) return { ...registered, registered: true };
  throw new Error(
    `no saved graph of ${root} in ${places.join(' or ')} or the graph registry (${home}): build one with ${graphCli(`build ${shellArg(root)}`)}`
  );
}

/** The graph a path names: its file, the run it came from when one was chosen, and whether the registry named it. */
export interface FoundGraph {
  path: string;
  chosen: string | null;
  registered?: boolean;
}

/**
 * The graph file to load for the first argument, and the run it came from when one was chosen. `home`: the graph
 * registry consulted for a project directory (registry.ts registryHome).
 */
export function graphPath(arg: string, home = registryHome()): FoundGraph {
  const p = resolve(arg);
  const s = statSync(p, { throwIfNoEntry: false });
  // A path that names nothing is refused: its parent directory is never read in its place.
  if (!s)
    throw new Error(
      `no graph at ${p}: name a result directory, its data.json or a project directory`
    );
  if (!s.isDirectory()) return { path: p, chosen: null };
  const own = resultGraph(p);
  if (own) return { path: own, chosen: null };
  const run = newestRun(p, home);
  return { path: run.graph, chosen: run.dir, registered: run.registered };
}

// What only a result directory holds: its event log, graph, checkpoint or the record of the viewer serving it.
const RUN_FILES = ['events.jsonl', 'data.json', 'stitched.json', 'server.json'];

/**
 * The result directory a path names: a result directory itself (any run file, even before its graph is written),
 * the directory of a graph file, or the newest saved graph of a project directory (graphPath).
 */
export function resultDir(arg: string, home = registryHome()) {
  const p = resolve(arg);
  if (RUN_FILES.some(name => isFile(join(p, name)))) return p;
  const found = graphPath(p, home);
  return found.chosen ?? dirname(found.path);
}

/** A graph file that could not be read as one: the CLI reports it as a usage error. */
export class GraphFileError extends Error {}

/**
 * A saved graph file read and indexed for queries. A file that is not JSON, or not a graph object, is a
 * GraphFileError naming it. data.json is this package's own output: the query engine tolerates any missing field.
 */
export function readGraphFile(path: string): QueryData {
  let data: unknown;
  try {
    data = JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    throw new GraphFileError(
      `${path} is not a saved graph (a data.json written by build): ${errorMessage(error)}`
    );
  }
  if (!isRecord(data))
    throw new GraphFileError(
      `${path} is not a saved graph (a data.json written by build): it holds no JSON object`
    );
  return withSearchIndex(data, path);
}

/** Graph files whose content may have changed since `graph` was written (a later modification time). */
export function editedSince(D: QueryData, graph: string) {
  const written = statSync(graph).mtimeMs;
  const root = rootOf(D);
  return (D.files ?? []).flatMap(f => {
    const abs = !root || f.kind === 'package' ? null : absOf(root, f.id);
    const s = abs ? statSync(abs, { throwIfNoEntry: false }) : undefined;
    return abs && (!s || s.mtimeMs > written) ? [f.id] : [];
  });
}

/**
 * An answer with the graph files changed or missing since the graph was written: every one listed (`staleFiles`)
 * and a warning naming the first. The CLI, MCP and the live viewer add it the same way.
 */
export function withStaleness(r: QueryResult, edited: string[]): QueryResult {
  if (!edited.length) return r;
  const warning = `${plural(edited.length, 'graph file')} changed or missing since this graph was written (first: ${edited[0]}); ask again with --refresh (query_graph refresh: true) to refresh the changed chunks first`;
  const warnings = Array.isArray(r.warnings) ? (r.warnings as string[]) : [];
  return { ...r, staleFiles: edited, warnings: [warning, ...warnings] };
}

const usage = (
  cli = graphCommand()
) => `usage: ${cli} query <result-dir|data.json|project-dir> ("<query>" | --stdin) [--json] [--refresh]
  Start with search <name> and about <file|symbol>; then callers, impact, path, cycle, critical or top.
  Natural phrasing is read as one exact query and shown as "read as:" (callers of x, a typo, top pr).
  A project directory selects its newest saved graph under .octocode/octocode-graph, else the newest the
  per-user graph registry lists for it (a build with --out elsewhere); stderr names it. A run its viewer
  serves (${cli} serve) is answered by that server. Every answer warns of graph files changed since it was
  written; --refresh refreshes the chunks holding them first and answers from the refreshed graph (unchanged:
  no refresh). --stdin answers one query per line from one load; --json writes one result per line.
  ${HINT}
`;

// How this process answers: through the live viewer that serves the run (it holds the graph loaded), else from the
// graph read once here (or already opened by --refresh). A viewer that cannot be reached, or refuses, falls back to
// the local read.
async function answerer(
  path: string,
  opened: SavedGraph | null
): Promise<(q: string) => Promise<QueryResult>> {
  let local: ((q: string) => QueryResult) | null = opened
    ? q => withStaleness(opened.query(q), opened.edited)
    : null;
  const read = () => {
    const D = readGraphFile(path);
    const edited = editedSince(D, path);
    return (q: string) => withStaleness(runQuery(D, q), edited);
  };
  let server = serverOf(dirname(path));
  return async q => {
    if (server)
      try {
        return (await postServer(server, '/query', {
          query: q,
        })) as unknown as QueryResult;
      } catch (error) {
        if (!(error instanceof ServedError)) throw error;
        process.stderr.write(
          `octocode-graph query: ${error.message}; reading the graph here\n`
        );
        server = null;
      }
    local ??= read();
    // search --provider: the configured classification provider reranks the best rows (search-provider.ts).
    return withProvider(local(q));
  };
}

// Before answering with --refresh: graph files edited since the graph was written are refreshed first (only the
// chunks holding them rebuild); a refresh that fails leaves the graph as written, and every answer still warns.
async function freshened(graph: FoundGraph) {
  const { openFreshGraph } = await import('./api.ts');
  try {
    const opened = await openFreshGraph(graph.path);
    const r = opened.refreshed;
    if (r)
      process.stderr.write(
        `octocode-graph query: refreshed ${plural(r.changed?.length ?? 0, 'changed file')}, ${plural(r.added?.length ?? 0, 'added file')} and ${plural(r.removed?.length ?? 0, 'removed file')} (rebuilt chunks: ${(r.chunks ?? []).join(', ') || 'none'}) in ${(r.ms / 1000).toFixed(1)} s before answering\n`
      );
    return opened;
  } catch (error) {
    process.stderr.write(
      `octocode-graph query: --refresh failed: ${error instanceof Error ? error.message : String(error)}; answering from the graph as written\n`
    );
    return null;
  }
}

const FLAGS = new Set(['--json', '--stdin', '--refresh']);

export async function main(argv: string[]) {
  const json = argv.includes('--json');
  const stdin = argv.includes('--stdin');
  const refresh = argv.includes('--refresh');
  const rest = argv.filter(a => !FLAGS.has(a));
  const help = argv.includes('--help') || argv.includes('-h');
  if ((stdin ? rest.length !== 1 : rest.length < 2) || help) {
    (help ? process.stdout : process.stderr).write(usage());
    process.exitCode = help ? 0 : 2;
    return;
  }
  let graph: FoundGraph;
  try {
    graph = graphPath(rest[0]);
  } catch (error) {
    process.stderr.write(
      `octocode-graph query: ${error instanceof Error ? error.message : String(error)}\n`
    );
    process.exitCode = 2;
    return;
  }
  if (graph.chosen)
    process.stderr.write(
      `octocode-graph query: answering from ${graph.chosen} (${graph.registered ? `registered for ${resolve(rest[0])}, ` : ''}written ${statSync(graph.path).mtime.toISOString()})\n`
    );
  const ask = await answerer(
    graph.path,
    refresh ? await freshened(graph) : null
  );
  const queries = stdin
    ? createInterface({ input: process.stdin, crlfDelay: Infinity })
    : [rest.slice(1).join(' ')];
  for await (const query of queries) {
    if (stdin && !query.trim()) continue;
    let r: QueryResult;
    try {
      r = await ask(query);
    } catch (error) {
      if (!(error instanceof GraphFileError)) throw error;
      process.stderr.write(`octocode-graph query: ${error.message}\n`);
      process.exitCode = 2;
      return;
    }
    if (
      !process.stdout.write(`${json ? JSON.stringify(r) : formatResult(r)}\n`)
    )
      await once(process.stdout, 'drain');
    // A query error, or a rule the query states was broken (reach --forbid): CI fails on either.
    if (!r.ok || r.failed) process.exitCode = 1;
  }
}
