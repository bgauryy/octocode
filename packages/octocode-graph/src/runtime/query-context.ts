// context <file|symbol...> [--tokens N] [--from K] [--tests]: a ranked map of the files and symbols around what is
// named, cut to a token budget, for an agent's prompt. Aider's repo map (aider/repomap.py) over this graph: PageRank
// personalized to the named files and symbols, over a directed graph of declarations and their evidence-weighted
// uses (graph-model.ts weight rule):
//   caller → callee        call sites × evidence (1 when the language server resolved it, else high 1, medium 0.5,
//                          low 0.25)
//   importer → imported    1 per import; a file → each name it takes from another file, 1
//   file ⇄ its symbols     1 each way: a file's rank reaches its declarations and theirs reaches it
// Each use also links back at half its weight, so callers and importers rank too, below what the named code uses.
// The named items come first, then every other reached item by rank. Rows are added in rank order while their
// estimated tokens (characters / 4 of the row's text) fit the budget; the first row always shows, and the cut is
// stated with the query that continues from the first row left out.
import { callEvidence } from '../graph-model.ts';
import {
  QueryError,
  resolveLocation,
  resolveSymbol,
  type QueryIndex,
} from './query-index.ts';
import {
  fileRow,
  type OpResult,
  type QueryArgs,
  type Row,
} from './query-ops.ts';
import {
  searchIndexOf,
  type SearchDoc,
  type SearchIndex,
} from './search-index.ts';

/** The default budget: Aider's default repo-map size (--map-tokens 1024). */
export const CONTEXT_TOKENS = 1024;
const RESTART = 0.15;
/** Power iteration stops when the L1 change falls below this (as algorithms.ts pagerank). */
const TOLERANCE = 1e-10;
/** A link back (callee → caller, imported → importer) counts this fraction of the use. */
const BACK = 0.5;

interface Weighted {
  offsets: Int32Array;
  targets: Int32Array;
  weights: Float64Array;
  /** Each node's total out-weight. */
  out: Float64Array;
}

const graphs = new WeakMap<SearchIndex, Record<string, Weighted>>();

function weightedGraph(I: QueryIndex, index: SearchIndex): Weighted {
  const key = I.tests ? 'tests' : 'production';
  const cached = graphs.get(index)?.[key];
  if (cached) return cached;
  const at = new Map(index.docs.map((d, i) => [d.id, i]));
  const from: number[] = [],
    to: number[] = [],
    weight: number[] = [];
  const link = (
    u: number | undefined,
    v: number | undefined,
    w: number,
    back = BACK
  ) => {
    if (u === undefined || v === undefined || u === v || !(w > 0)) return;
    from.push(u, v);
    to.push(v, u);
    weight.push(w, w * back);
  };
  index.docs.forEach((d, i) => {
    if (d.symbol) link(at.get(d.file), i, 1, 1);
  });
  for (const c of I.D.calls ?? [])
    if (I.tests || !c.test)
      link(
        at.get(c.source),
        at.get(c.target),
        (c.count ?? 1) * callEvidence(c)
      );
  for (const e of I.D.imports ?? [])
    if (I.tests || !e.test) link(at.get(e.source), at.get(e.target), 1);
  for (const u of I.D.uses ?? [])
    if (I.tests || !u.test) link(at.get(u.source), at.get(u.target), 1);
  const n = index.docs.length;
  const offsets = new Int32Array(n + 1);
  for (const u of from) offsets[u + 1]++;
  for (let i = 0; i < n; i++) offsets[i + 1] += offsets[i];
  const fill = offsets.slice(0, n);
  const targets = new Int32Array(from.length);
  const weights = new Float64Array(from.length);
  const out = new Float64Array(n);
  from.forEach((u, k) => {
    const slot = fill[u]++;
    targets[slot] = to[k];
    weights[slot] = weight[k];
    out[u] += weight[k];
  });
  const built = { offsets, targets, weights, out };
  graphs.set(index, { ...graphs.get(index), [key]: built });
  return built;
}

/** PageRank personalized to `seeds`: a walk restarts at a seed, and a node with no out-weight hands its rank back. */
export function personalizedPageRank(G: Weighted, seeds: number[]) {
  const n = G.out.length;
  const p = new Float64Array(n);
  for (const s of seeds) p[s] += 1 / seeds.length;
  let x = Float64Array.from(p);
  for (;;) {
    const y = new Float64Array(n);
    let dangling = 0;
    for (let u = 0; u < n; u++) {
      if (!x[u]) continue;
      if (!G.out[u]) {
        dangling += x[u];
        continue;
      }
      const share = ((1 - RESTART) * x[u]) / G.out[u];
      for (let k = G.offsets[u]; k < G.offsets[u + 1]; k++)
        y[G.targets[k]] += share * G.weights[k];
    }
    const restart = RESTART + (1 - RESTART) * dangling;
    let change = 0;
    for (let u = 0; u < n; u++) {
      y[u] += restart * p[u];
      change += Math.abs(y[u] - x[u]);
    }
    x = y;
    if (change < TOLERANCE) return x;
  }
}

// The text a row stands for in a prompt: where it is, what it is, and its signature when recorded.
function rowText(d: SearchDoc) {
  const where =
    d.symbol && d.line
      ? `${d.file}:${d.line}${d.endLine && d.endLine > d.line ? `-${d.endLine}` : ''}`
      : d.file;
  return `${where} ${d.kind}${d.symbol ? ` ${d.name}` : ''}${d.params ? ` ${d.params}` : ''}${d.returns ? ` → ${d.returns}` : ''}`;
}
/** Estimated tokens of a row: characters / 4, the usual rule for code and English. */
export const tokensOf = (text: string) => Math.ceil((text.length + 1) / 4);

function positive(flag: string, value: string | undefined) {
  const n = Number(value);
  if (
    value === undefined ||
    !/^\d+$/.test(value) ||
    !Number.isSafeInteger(n) ||
    n < 1
  )
    throw new QueryError(
      `context ${flag} takes a positive integer, not ${value ?? 'nothing'}`
    );
  return n;
}

export function context(I: QueryIndex, { args }: QueryArgs): OpResult {
  const named: string[] = [];
  let budget = CONTEXT_TOKENS,
    start = 1;
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    const [flag, inline] = a.includes('=')
      ? [a.slice(0, a.indexOf('=')), a.slice(a.indexOf('=') + 1)]
      : [a, undefined];
    if (flag === '--tokens') budget = positive(flag, inline ?? args[++i]);
    else if (flag === '--from') start = positive(flag, inline ?? args[++i]);
    else if (a.startsWith('--'))
      throw new QueryError(
        `context takes files or symbols, --tokens N and --from K, not ${a}`
      );
    else named.push(a);
  }
  if (!named.length) throw new QueryError('context needs a file or a symbol');
  const index = searchIndexOf(I.D);
  const at = new Map(index.docs.map((d, i) => [d.id, i]));
  const seeds: number[] = [];
  for (const arg of named) {
    let id: string;
    if (arg.includes('#') && !I.file.has(arg)) id = resolveSymbol(I, arg);
    else
      try {
        id = resolveLocation(I, arg).id;
      } catch (error) {
        if (!(error instanceof QueryError) || error.candidates.length)
          throw error;
        id = resolveSymbol(I, arg);
      }
    const doc = at.get(id);
    if (doc === undefined)
      throw new QueryError(`${id} is not in the search index`, { arg });
    if (!seeds.includes(doc)) seeds.push(doc);
  }
  const rank = personalizedPageRank(weightedGraph(I, index), seeds);
  const seeded = new Set(seeds);
  const order = [
    ...seeds,
    ...[...rank.keys()]
      .filter(i => rank[i] > 0 && !seeded.has(i))
      .sort((a, b) => rank[b] - rank[a] || a - b),
  ];
  if (start > order.length)
    throw new QueryError(
      `context --from ${start} is past the last of ${order.length} ranked items`
    );
  const rows: Row[] = [];
  let used = 0;
  for (let k = start - 1; k < order.length; k++) {
    const d = index.docs[order[k]];
    const tokens = tokensOf(rowText(d));
    if (rows.length && used + tokens > budget) break;
    used += tokens;
    rows.push({
      ...(d.symbol
        ? fileRow(I, d.file, {
            line: d.line ?? undefined,
            ...(d.endLine != null && { endLine: d.endLine }),
            kind: d.kind,
            symbol: d.id,
            name: d.name,
            ...(d.params != null && { params: d.params }),
            ...(d.returns != null && { returns: d.returns }),
          })
        : fileRow(I, d.id, { kind: 'file' })),
      rank: k + 1,
      value: Number(rank[order[k]].toPrecision(4)),
      ...(seeded.has(order[k]) && { role: 'named' }),
      tokens,
    });
  }
  const last = start - 1 + rows.length;
  const rest = order.length - last;
  const quote = (t: string) =>
    /[\s"']/.test(t) ? `"${t.replace(/["\\]/g, '\\$&')}"` : t;
  const more = rest
    ? `context ${named.map(quote).join(' ')} --tokens ${budget} --from ${last + 1}${I.tests ? ' --tests' : ''}`
    : null;
  return {
    rows,
    edges: [],
    nodes: [...new Set(rows.map(r => r.id))],
    ...(more && { next: { query: more } }),
    stats: {
      budget,
      tokens: used,
      from: start,
      shown: rows.length,
      ranked: order.length,
      rest,
      ...(rows.length === 1 && used > budget && { oversized: true }),
    },
    summary: `context of ${named.join(', ')}: ${order.length} ranked files and symbols; rows ${start}–${last} in about ${used} of ${budget} tokens${rows.length === 1 && used > budget ? ' (one row, over the budget)' : ''}${more ? `; ${rest} more cut by the budget: \`${more}\`` : '; nothing cut'}`,
  };
}
