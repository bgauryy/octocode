// The graph query language over one data.json (pure functions; the server, the channel, this CLI and the offline
// viewer's worker share it).
//   search <text> [--files|--symbols] [--all]   files and symbols ranked by name match, then importance
//   about <file|symbol>            metrics, community, direct dependencies and dependents (or callers and callees)
//   critical functions|points [n|all], critical path [<file>] [--up]   central functions, cut points, longest chain
//   impact <file> [depth N]        every file that transitively depends on <file>
//   deps | dependents <file> [depth N]   forward / reverse neighbors (default depth 1)
//   path <a> <b>                   the strongest dependency path a → b (Dijkstra, cost 1/weight)
//   cycle <file>                   its strongly connected set, ordered along one ring through it
//   community <file|id>            members by PageRank and links to other communities
//   callers | callees <file#symbol|name>   symbol-level calls with provenance
//   top <metric> [n|all]           files ranked by pagerank, betweenness, inWeight, outWeight, core or depth
// `--tests` includes test-only edges; `--imports` walks import edges only; `--weak` adds edges backed only by
// medium/low-confidence name-bound calls (walks follow strong edges by default). Result: { ok, op, text, error?, candidates?, nodes, edges, rows, summary, stats }.
import { links } from './query-links.ts';
import {
  QueryError,
  indexOf,
  resolveFile,
  textOf,
  type QueryData,
} from './query-index.ts';
import * as ops from './query-ops.ts';
import { about, search } from './query-search.ts';
import { critical } from './query-critical.ts';
import { region } from './query-flow.ts';
import { nearest, readQuery } from './query-forgiving.ts';
import type { NextCalls, Op, OpResult, QueryArgs, Row } from './query-ops.ts';

export type { QueryData } from './query-index.ts';
export type { OpResult, Row } from './query-ops.ts';

export const OPS: Record<string, Op> = {
  search,
  about,
  critical,
  region,
  files: ops.fileRecords,
  impact: ops.impact,
  deps: ops.deps,
  dependents: ops.dependents,
  path: ops.path,
  cycle: ops.cycle,
  order: ops.dependencyOrder,
  trace: ops.trace,
  community: ops.community,
  callers: ops.callers,
  callees: ops.callees,
  top: ops.top,
  links,
};
export const HINT =
  'search <text> [--files|--symbols] [--all] · about <file|symbol> · critical functions|points [n|all] · critical path [<file>] [--up] · region <entry> <outcome> · files <file...> · impact <file...> [depth N] · deps|dependents <file...> [depth N] · path <a> <b> [--hops] [--avoid <file|dir>...] · cycle [<file>] · order <file> · trace <file|file#symbol> <pattern> · community <file|id> · callers|callees <file#symbol|name> · top <metric> [n|all] · links [<file>] [--unresolved|--stale] [--rule md-link|md-path|md-cite|md-symbol|manifest|tsconfig|contract|md-contract|agent] · --tests · --imports · --weak';

// Words, with "double" or 'single' quotes keeping spaces.
export function tokenize(text: unknown): string[] {
  return [
    ...textOf(text).matchAll(/"((?:\\["\\]|[^"])*)"|'([^']*)'|(\S+)/g),
  ].map(m => m[1]?.replace(/\\(["\\])/g, '$1') ?? m[2] ?? m[3]);
}

const DEPTH = /^(?:--)?depth(?:=(.+))?$/i;
function depthValue(v: string | undefined, allowZero: boolean) {
  if (/^(all|\*|inf)$/i.test(String(v))) return Infinity;
  const n = Number(v);
  if (
    v === undefined ||
    !/^\d+$/.test(v) ||
    !Number.isSafeInteger(n) ||
    n < (allowZero ? 0 : 1)
  )
    throw new QueryError(
      `depth must be a ${allowZero ? 'nonnegative' : 'positive'} integer or all, not ${v ?? 'nothing'}`
    );
  return n;
}

// Ops whose walk takes a depth; elsewhere `depth` is an ordinary word (top depth, find depth).
const WALKS = new Set(['impact', 'deps', 'dependents']);

// Arguments, `--tests`, `--imports`, `--weak`, and (for walks) `depth N` / `--depth N` / `depth=N` anywhere after the op.
export function parseArgs(
  tokens: string[],
  { walk = true, path = false, trace = false } = {}
): QueryArgs {
  const args: string[] = [];
  let hops = false;
  let tests = false,
    imports = false,
    weak = false,
    depth: number | null = null;
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i];
    const m = walk || (trace && t.startsWith('--')) ? DEPTH.exec(t) : null;
    if (t === '--hops' && path) hops = true;
    else if (t === '--tests') tests = true;
    else if (t === '--imports') imports = true;
    else if (t === '--weak') weak = true;
    else if (m) depth = depthValue(m[1] ?? tokens[++i], trace);
    else args.push(t);
  }
  return { args, tests, imports, weak, depth, ...(hops && { hops }) };
}

export interface Candidate {
  id: string;
  query: string;
}

// Each candidate as the same query with the ambiguous argument replaced, so a viewer or agent can re-run it.
function candidateRows(tokens: string[], error: QueryError): Candidate[] {
  const quote = (t: string) =>
    /[\s"']/.test(t) ? `"${t.replace(/["\\]/g, '\\$&')}"` : t;
  return error.candidates.map(id => ({
    id,
    query: tokens
      .map(t => quote(t === error.arg ? id + (error.anchor ?? '') : t))
      .join(' '),
  }));
}

/** What the answer covers: the export root, its snapshot and the coverage it recorded. */
export interface QueryScope {
  root: string | null;
  snapshot: string | null;
  isPartial: boolean | null;
  partialReasons: string[];
  revision?: string;
  baseSha256?: string;
}

/** One answer: `ok` with the op's rows, or `ok: false` with the error and re-runnable candidates. */
export interface QueryResult extends OpResult {
  ok: boolean;
  /** How a loosely typed query was read: the exact query that answered and each change made. */
  readAs?: { query: string; notes: string[] };
  op: string;
  text: string;
  scope: QueryScope;
  error?: string;
  candidates?: Candidate[];
  lspStatus?: 'pending';
  tests?: true;
  imports?: true;
  weak?: true;
}

const empty = (): Pick<OpResult, 'nodes' | 'edges' | 'rows'> => ({
  nodes: [],
  edges: [],
  rows: [],
});

/** Runs one query text over data `D`; never throws for a bad query (ok:false with error and candidates). */
export function runQuery(D: QueryData, text: unknown): QueryResult {
  const q = textOf(text).trim();
  const coverage = D.coverage ?? D.snapshot;
  const scope: QueryScope = {
    root: D.section ?? null,
    snapshot: D.snapshot?.id ?? null,
    isPartial: coverage?.isPartial ?? null,
    partialReasons: coverage?.partialReasons ?? [],
    ...(D.enrichment && {
      revision: D.enrichment.revision,
      baseSha256: D.enrichment.baseSha256,
    }),
  };
  const reading = readQuery(tokenize(q), Object.keys(OPS));
  const tokens = reading.tokens;
  const op = (tokens[0] ?? '').toLowerCase();
  const base = {
    op,
    text: q,
    scope,
    ...(D.callStats?.lsp === 'pending' && { lspStatus: 'pending' as const }),
  };
  if (reading.candidates) {
    const error = `unknown query "${tokenize(q)[0]}": several queries are that close; re-run one`;
    return {
      ok: false,
      ...base,
      error,
      candidates: reading.candidates.map(query => ({
        id: query.split(' ')[0],
        query,
      })),
      ...empty(),
      summary: error,
    };
  }
  if (!Object.hasOwn(OPS, op)) {
    const error = op
      ? `unknown query "${op}"; try: ${HINT}`
      : `empty query; try: ${HINT}`;
    return { ok: false, ...base, error, ...empty(), summary: error };
  }
  const answer = (words: string[], notes: string[]): QueryResult => {
    const p = parseArgs(words.slice(1), {
      walk: WALKS.has(op),
      path: op === 'path',
      trace: op === 'trace',
    });
    const r = OPS[op](
      indexOf(D, { tests: p.tests, imports: p.imports, weak: p.weak }),
      p
    );
    return {
      ok: true,
      ...base,
      ...(notes.length && { readAs: { query: words.join(' '), notes } }),
      ...empty(),
      ...r,
      ...(p.tests ? { tests: true as const } : {}),
      ...(p.imports ? { imports: true as const } : {}),
      ...(p.weak ? { weak: true as const } : {}),
    };
  };
  try {
    return answer(tokens, reading.notes);
  } catch (error) {
    if (!(error instanceof QueryError)) throw error;
    // An unknown name nothing contains may be misspelled: one near name is read in its place, several are offered.
    const near =
      !error.candidates.length && error.arg ? nearNames(D, error.arg) : [];
    if (near.length === 1 && error.arg) {
      const words = tokens.map(t => (t === error.arg ? near[0] : t));
      try {
        return answer(words, [
          ...reading.notes,
          `no name is "${error.arg}": read as ${near[0]}`,
        ]);
      } catch (retry) {
        if (!(retry instanceof QueryError)) throw retry;
      }
    }
    const candidates = error.candidates.length
      ? candidateRows(tokens, error)
      : near.map(id => ({
          id,
          query: tokens.map(t => (t === error.arg ? id : t)).join(' '),
        }));
    const message =
      near.length > 1
        ? `${error.message}; these names are close`
        : error.message;
    return {
      ok: false,
      ...base,
      error: message,
      ...(candidates.length ? { candidates } : {}),
      ...empty(),
      summary: message,
    };
  }
}

// Files (by path or file name) and symbols (by name) nearest a misspelled argument, as ids a query accepts.
function nearNames(D: QueryData, arg: string) {
  const I = indexOf(D);
  const looksLikeFile = /[/.]/.test(arg.replace(/#.*$/, ''));
  const pool = new Map<string, string[]>();
  const add = (name: string, id: string) => {
    const list = pool.get(name) ?? [];
    list.push(id);
    pool.set(name, list);
  };
  if (looksLikeFile)
    for (const id of I.ids) {
      add(id, id);
      add(id.slice(id.lastIndexOf('/') + 1), id);
    }
  else
    for (const s of I.symbols().byId.values())
      add(s.name.split('.').pop() ?? s.name, s.id);
  const { best } = nearest(
    looksLikeFile ? arg.replace(/^\.\//, '') : arg,
    pool.keys()
  );
  return [...new Set(best.flatMap(name => pool.get(name) ?? []))].sort();
}

export interface NodeProblem {
  arg: string;
  error: string;
  candidates: string[];
}

/** Node arguments (paths, suffixes, ids, file#symbol) → ids; anything unknown or ambiguous is listed, not guessed. */
export function resolveNodes(
  D: QueryData,
  list: readonly unknown[] | undefined
) {
  const I = indexOf(D);
  const ids: string[] = [];
  const problems: NodeProblem[] = [];
  for (const arg of list ?? []) {
    try {
      ids.push(resolveFile(I, arg));
    } catch (error) {
      if (!(error instanceof QueryError)) throw error;
      problems.push({
        arg: textOf(arg),
        error: error.message,
        candidates: error.candidates,
      });
    }
  }
  return { ids: [...new Set(ids)], problems };
}

function rowLine(r: Row) {
  const symbol = r.symbol
    ? `${r.name ?? r.symbol} (${r.via ?? r.kind ?? 'symbol'}${r.confidence ? `/${r.confidence}` : ''}${(r.count ?? 1) > 1 ? ` ×${r.count}` : ''})`
    : null;
  const tags = [
    r.role ?? null,
    r.component != null ? `group ${r.component + 1}` : null,
    r.depth != null ? `d${r.depth}` : null,
    r.rank != null ? `#${r.rank}` : null,
    r.order != null ? `ring ${r.order}` : null,
    typeof r.cycleId === 'number' ? `cycle ${r.cycleId}` : null,
    r.weight != null ? `w ${r.weight}` : null,
    r.value != null && !r.parts ? `${r.value}` : null,
    r.pagerank != null && r.match == null ? `pr ${r.pagerank}` : null,
    r.endpoint ?? null,
    symbol,
    r.match != null ? `${r.symbol ? '' : 'file  '}${r.match}` : null,
    r.callSites
      ? `${r.callSites} call site${r.callSites === 1 ? '' : 's'}`
      : null,
    r.callers != null
      ? `${r.callers} caller${r.callers === 1 ? '' : 's'} in ${r.callerFiles ?? '?'} file${r.callerFiles === 1 ? '' : 's'}, ${r.transitiveCallers ?? '?'} transitive`
      : null,
    r.parts ? `cuts off ${r.value}, parts ${r.parts.join('/')}` : null,
    r.fromEntry != null
      ? `${r.mustPass ? 'must pass  ' : ''}+${r.fromEntry}/−${r.toOutcome}`
      : null,
    r.through ? `via ${r.through}` : null,
    r.nextFile ? `→ ${r.nextFile}` : null,
    Array.isArray(r.members)
      ? `${r.members.length} files: ${r.members.join(' ')}`
      : null,
  ].filter(Boolean);
  const location = r.loc
    ? r.loc.path + (r.loc.line == null ? '' : `:${r.loc.line}`)
    : '';
  return `${location}${tags.length ? `  ${tags.join('  ')}` : ''}`;
}

/** Plain text of a result: the summary, every row with its location, links or candidates. */
export function formatResult(r: QueryResult) {
  const lines = [`${r.ok ? '' : 'error: '}${r.summary ?? r.error ?? ''}`];
  if (r.readAs)
    lines.unshift(`read as: ${r.readAs.query}  (${r.readAs.notes.join('; ')})`);
  if (r.scope?.isPartial)
    lines.push(
      `Partial graph coverage. ${r.scope.partialReasons.join(', ')}`.trim()
    );
  if (r.lspStatus === 'pending') lines.push('LSP confirmation pending.');
  for (const w of asList<string>(r.warnings)) lines.push(`warning: ${w}`);
  const next = r.next as NextCalls | undefined;
  if (next?.read)
    lines.push(
      `read: octocode localFetch '${JSON.stringify(next.read.input)}'`
    );
  if (next?.confirm)
    lines.push(
      `confirm: octocode lspSearch '${JSON.stringify(next.confirm.input)}'`
    );
  for (const x of r.rows ?? []) lines.push(`  ${rowLine(x)}`);
  for (const step of asList(r.steps)) lines.push(`  ${JSON.stringify(step)}`);
  for (const c of asList<{ source: string; target: string; evidence: unknown }>(
    r.crossings
  ))
    lines.push(`  ${c.source} → ${c.target}  ${JSON.stringify(c.evidence)}`);
  for (const l of asList<{
    community: number;
    label: string | null;
    weight: number;
    out: number;
    in: number;
  }>(r.links))
    lines.push(
      `  link community ${l.community}${l.label ? ` (${l.label})` : ''}: weight ${l.weight} (out ${l.out}, in ${l.in})`
    );
  if (r.candidates?.length)
    lines.push(
      'candidates (re-run one):',
      ...r.candidates.map(c => `  ${c.query}`)
    );
  return lines.join('\n');
}

// Op-specific lists a result may carry (steps, crossings, links), typed by the op that wrote them.
function asList<T = unknown>(value: unknown): T[] {
  return Array.isArray(value) ? (value as T[]) : [];
}
