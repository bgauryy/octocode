// `octocode-graph query`: saved-graph questions from the shell. The first argument is a result directory, its
// data.json, or a project directory: then the newest saved graph of exactly that root under .octocode/octocode-graph
// (of the current directory or the project) answers, and stderr names it. A run its live viewer serves answers
// through that server, from the graph it already holds; otherwise the graph is read here. Files edited since the
// graph was written are a warning in every answer (withStaleness), so a stale answer is never silent.
import {
  closeSync,
  openSync,
  readFileSync,
  readSync,
  readdirSync,
  statSync,
} from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { once } from 'node:events';
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
  let fd: number | undefined;
  try {
    fd = openSync(join(dir, 'events.jsonl'), 'r');
    const head = Buffer.alloc(64 * 1024);
    const text = head.toString(
      'utf8',
      0,
      readSync(fd, head, 0, head.length, 0)
    );
    const first: unknown = JSON.parse(text.slice(0, text.indexOf('\n')));
    return first &&
      typeof first === 'object' &&
      'root' in first &&
      typeof first.root === 'string'
      ? first.root
      : null;
  } catch {
    return null; // Not a saved run, or one without a readable header.
  } finally {
    if (fd !== undefined) closeSync(fd);
  }
}

/** The newest saved graph of exactly `root`, or why there is none. */
function newestRun(root: string) {
  const places = [...new Set([resolve(RUNS), join(root, RUNS)])];
  const runs = places.flatMap(place => {
    let names: string[];
    try {
      names = readdirSync(place);
    } catch {
      return [];
    }
    return names.flatMap(name => {
      const dir = join(place, name);
      const graph = resultGraph(dir);
      if (!graph || runRoot(dir) !== root) return [];
      return [{ dir, graph, mtime: statSync(graph).mtimeMs }];
    });
  });
  const best = runs.toSorted((a, b) => b.mtime - a.mtime)[0];
  if (best) return best;
  throw new Error(
    `no saved graph of ${root} in ${places.join(' or ')}: build one with octocode-graph build ${root}`
  );
}

/** The graph file to load for the first argument, and the run it came from when one was chosen. */
export function graphPath(arg: string) {
  const p = resolve(arg);
  if (!statSync(p, { throwIfNoEntry: false })?.isDirectory())
    return { path: p, chosen: null };
  const own = resultGraph(p);
  if (own) return { path: own, chosen: null };
  const run = newestRun(p);
  return { path: run.graph, chosen: run.dir };
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
  const warning = `${plural(edited.length, 'graph file')} changed or missing since this graph was written (first: ${edited[0]}); rebuild or refresh for current edges`;
  const warnings = Array.isArray(r.warnings) ? (r.warnings as string[]) : [];
  return { ...r, staleFiles: edited, warnings: [warning, ...warnings] };
}

const USAGE = `usage: octocode-graph query <result-dir|data.json|project-dir> ("<query>" | --stdin) [--json]
  Start with search <name> and about <file|symbol>; then callers, impact, path, cycle, critical or top.
  Natural phrasing is read as one exact query and shown as "read as:" (callers of x, a typo, top pr).
  A project directory selects its newest saved graph under .octocode/octocode-graph; stderr names it. A run
  its viewer serves (octocode-graph serve) is answered by that server. Every answer warns of graph files
  changed since it was written. --stdin answers one query per line from one load; --json writes one result
  per line.
  ${HINT}
`;

// How this process answers: through the live viewer that serves the run (it holds the graph loaded), else from the
// graph read once here. A viewer that cannot be reached, or refuses, falls back to the local read.
async function answerer(
  path: string
): Promise<(q: string) => Promise<QueryResult>> {
  let local: ((q: string) => QueryResult) | null = null;
  const read = () => {
    // data.json is this package's own output: the query engine tolerates any missing field.
    const D = JSON.parse(readFileSync(path, 'utf8')) as QueryData;
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
    return local(q);
  };
}

export async function main(argv: string[]) {
  const json = argv.includes('--json');
  const stdin = argv.includes('--stdin');
  const rest = argv.filter(a => a !== '--json' && a !== '--stdin');
  const help = argv.includes('--help') || argv.includes('-h');
  if ((stdin ? rest.length !== 1 : rest.length < 2) || help) {
    (help ? process.stdout : process.stderr).write(USAGE);
    process.exitCode = help ? 0 : 2;
    return;
  }
  let graph: ReturnType<typeof graphPath>;
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
      `octocode-graph query: answering from ${graph.chosen} (written ${statSync(graph.path).mtime.toISOString()})\n`
    );
  const ask = await answerer(graph.path);
  const queries = stdin
    ? createInterface({ input: process.stdin, crlfDelay: Infinity })
    : [rest.slice(1).join(' ')];
  for await (const query of queries) {
    if (stdin && !query.trim()) continue;
    const r = await ask(query);
    if (
      !process.stdout.write(`${json ? JSON.stringify(r) : formatResult(r)}\n`)
    )
      await once(process.stdout, 'drain');
    if (!r.ok) process.exitCode = 1;
  }
}
