// Ranked search and entity cards over one data.json (query-index.ts):
//   search <text> [--files|--symbols] [--all]   files and symbols ranked by how well their name matches, then
//                                               by importance (call sites received, exported, PageRank)
//   about <file|symbol>                          one answer with everything adjacent to it: metrics, community,
//                                               direct dependencies and dependents, or callers and callees
// `search` is the one text query: exact names first, then partial and misspelled ones (`find` reads as it).
import { documentedBy } from './query-links.ts';
import {
  QueryError,
  resolveLocation,
  resolveSymbol,
  type QueryIndex,
  type SymbolInfo,
} from './query-index.ts';
import {
  callees,
  callers,
  dependents,
  deps,
  fileRow,
  metricOf,
  plural,
  readCall,
  type NextCalls,
  type OpResult,
  type QueryArgs,
  type Row,
} from './query-ops.ts';
import { fileOf } from '../ids.ts';

/** How a candidate's name matched the query, strongest first. */
export const MATCHES = [
  'exact',
  'prefix',
  'words',
  'substring',
  'terms',
  'path',
  'fuzzy',
] as const;
export type Match = (typeof MATCHES)[number];

/** Rows a search shows unless `--all` asks for every match; the summary names the rest. */
export const SEARCH_SHOWN = 20;

// camelCase, PascalCase, snake_case, kebab-case, dotted and path names → lower-case words.
export function words(text: string) {
  return text
    .split(/[^A-Za-z0-9]+/)
    .flatMap(part =>
      part.split(/(?<=[a-z0-9])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])/)
    )
    .filter(Boolean)
    .map(word => word.toLowerCase());
}

const compact = (text: string) => text.toLowerCase().replace(/[^a-z0-9]/g, '');

interface Query {
  text: string;
  lower: string;
  compact: string;
  terms: string[];
}

// Each query term (or each letter of a single term) starts a name word, in order: `pa`, `p args` → parseArgs,
// `get name` → getUserName.
function wordPrefixes(terms: string[], name: string[]) {
  // Query words are ASCII letters and digits (`words`), so one character is one code unit.
  const letters = terms.length === 1 ? terms[0].split('') : terms;
  let at = 0;
  for (const part of letters) {
    while (at < name.length && !name[at].startsWith(part)) at++;
    if (at === name.length) return false;
    at++;
  }
  return letters.length > 1;
}

function subsequence(needle: string, hay: string) {
  let at = 0;
  for (const c of needle) {
    at = hay.indexOf(c, at) + 1;
    if (!at) return false;
  }
  return true;
}

/**
 * The strongest way `name` matches the query, or null. `path` helps a query of several words (one may name the
 * folder), and alone matches only when `byPath` (a file is found by its folders; a symbol by its own name).
 */
export function matchOf(
  q: Query,
  name: string,
  path: string,
  byPath: boolean
): Match | null {
  const lower = name.toLowerCase();
  const flat = compact(name);
  if (lower === q.lower || (q.compact && flat === q.compact)) return 'exact';
  if (lower.startsWith(q.lower) || (q.compact && flat.startsWith(q.compact)))
    return 'prefix';
  const nameWords = words(name);
  if (wordPrefixes(q.terms, nameWords)) return 'words';
  if (lower.includes(q.lower) || (q.compact && flat.includes(q.compact)))
    return 'substring';
  const pathLower = path.toLowerCase();
  if (
    q.terms.length > 1 &&
    q.terms.every(t => lower.includes(t) || pathLower.includes(t)) &&
    q.terms.some(t => lower.includes(t))
  )
    return 'terms';
  if (byPath && pathLower.includes(q.lower)) return 'path';
  return null;
}

interface Hit {
  row: Row;
  match: Match;
  importance: number[];
  name: string;
}

const rank = (m: Match) => MATCHES.indexOf(m);

function compareHits(a: Hit, b: Hit) {
  const byMatch = rank(a.match) - rank(b.match);
  if (byMatch) return byMatch;
  for (let i = 0; i < Math.max(a.importance.length, b.importance.length); i++) {
    const d = (b.importance[i] ?? 0) - (a.importance[i] ?? 0);
    if (d) return d;
  }
  return (
    a.name.length - b.name.length ||
    (a.row.id < b.row.id ? -1 : a.row.id > b.row.id ? 1 : 0) ||
    (a.row.symbol ?? '').localeCompare(b.row.symbol ?? '')
  );
}

// Call sites each symbol receives in production code: the strongest sign that a name matters.
function received(I: QueryIndex) {
  const sites = new Map<string, number>();
  for (const [target, calls] of I.callsBy('target'))
    sites.set(
      target,
      calls.reduce((n, c) => n + (c.count ?? 1), 0)
    );
  return sites;
}

const basename = (id: string) => id.slice(id.lastIndexOf('/') + 1);
const stem = (id: string) => basename(id).replace(/\.[^.]+$/, '');

function fileHits(I: QueryIndex, q: Query, fuzzy: boolean): Hit[] {
  return I.ids.flatMap(id => {
    const name = stem(id);
    const match = fuzzy
      ? subsequence(q.compact, compact(name))
        ? 'fuzzy'
        : null
      : (matchOf(q, name, id, true) ??
        (basename(id).toLowerCase() === q.lower ? 'exact' : null));
    if (!match) return [];
    const pagerank = metricOf(I, id, 'pagerank') ?? 0;
    return [
      {
        row: fileRow(I, id, { kind: 'file', match, pagerank }),
        match,
        importance: [pagerank],
        name,
      },
    ];
  });
}

function symbolHits(I: QueryIndex, q: Query, fuzzy: boolean): Hit[] {
  const sites = received(I);
  const exported = new Set(
    (I.D.symbols ?? []).filter(s => s.exported).map(s => s.id)
  );
  const hits: Hit[] = [];
  const add = (s: SymbolInfo, match: Match) => {
    const calls = sites.get(s.id) ?? 0;
    hits.push({
      row: fileRow(I, s.file, {
        line: s.line ?? undefined,
        kind: s.kind,
        symbol: s.id,
        name: s.name,
        match,
        callSites: calls,
        exported: exported.has(s.id),
      }),
      match,
      importance: [
        calls,
        exported.has(s.id) ? 1 : 0,
        metricOf(I, s.file, 'pagerank') ?? 0,
      ],
      name: s.name,
    });
  };
  for (const s of I.symbols().byId.values()) {
    if (!I.file.has(s.file)) continue;
    const short = s.name.split('.').pop() ?? s.name;
    if (fuzzy) {
      if (subsequence(q.compact, compact(short))) add(s, 'fuzzy');
      continue;
    }
    const match =
      matchOf(q, short, s.id, false) ?? matchOf(q, s.name, s.id, false);
    if (match) add(s, match);
  }
  return hits;
}

export function search(I: QueryIndex, { args }: QueryArgs): OpResult {
  const flags = new Set(args.filter(a => a.startsWith('--')));
  for (const flag of flags)
    if (!['--files', '--symbols', '--all'].includes(flag))
      throw new QueryError(
        `search takes --files, --symbols or --all, not ${flag}`
      );
  const text = args
    .filter(a => !a.startsWith('--'))
    .join(' ')
    .trim();
  if (!text) throw new QueryError('search needs text');
  const q: Query = {
    text,
    lower: text.toLowerCase(),
    compact: compact(text),
    terms: words(text),
  };
  const kinds = {
    files: !flags.has('--symbols') || flags.has('--files'),
    symbols: !flags.has('--files') || flags.has('--symbols'),
  };
  const collect = (fuzzy: boolean) => [
    ...(kinds.files ? fileHits(I, q, fuzzy) : []),
    ...(kinds.symbols ? symbolHits(I, q, fuzzy) : []),
  ];
  let hits = collect(false);
  // A misspelled or abbreviated name: letters in order, tried only when no name or path holds the text.
  const fuzzy = !hits.length && q.compact.length >= 3;
  if (fuzzy) hits = collect(true);
  hits.sort(compareHits);
  const all = flags.has('--all');
  const shown = all ? hits : hits.slice(0, SEARCH_SHOWN);
  const rows = shown.map((h, i) => ({ ...h.row, rank: i + 1 }));
  const byMatch: Record<string, number> = {};
  for (const h of hits) byMatch[h.match] = (byMatch[h.match] ?? 0) + 1;
  const files = hits.filter(h => h.row.kind === 'file').length;
  const best = hits[0];
  const rest =
    hits.length > rows.length
      ? `; showing ${rows.length}: \`search ${text}${[...flags].map(f => ` ${f}`).join('')} --all\` lists all ${hits.length}`
      : '';
  return {
    rows,
    edges: [],
    nodes: [...new Set(rows.map(r => r.id))],
    ...(best && {
      next: { read: readCall(I, best.row.symbol ?? best.row.id) },
    }),
    stats: {
      matches: hits.length,
      shown: rows.length,
      files,
      symbols: hits.length - files,
      byMatch,
      fuzzy,
    },
    summary: hits.length
      ? `${count(hits.length, 'match', 'matches')} for "${text}" (${plural(files, 'file')}, ${plural(hits.length - files, 'symbol')})${fuzzy ? ' by letters in order' : ''}; best ${best.row.symbol ?? best.row.id} (${best.match})${rest}`
      : `nothing matches "${text}"`,
  };
}

const count = (n: number, one: string, many: string) =>
  `${n} ${n === 1 ? one : many}`;

const asArgs = (args: string[]): QueryArgs => ({
  args,
  tests: false,
  imports: false,
  weak: false,
  depth: null,
});

// Adjacent rows keep their own fields; a depth-1 walk's `depth` and `through` (the subject itself) are implied.
const tag = (rows: Row[], role: string) =>
  rows.map(({ depth: _depth, through: _through, ...r }) => ({ ...r, role }));

function ordinal(I: QueryIndex, id: string) {
  const own = metricOf(I, id, 'pagerank');
  if (typeof own !== 'number') return null;
  return 1 + I.ids.filter(x => (metricOf(I, x, 'pagerank') ?? -1) > own).length;
}

function aboutFile(I: QueryIndex, id: string, line?: number): OpResult {
  const f = I.file.get(id);
  const m = f?.metrics;
  const communities = I.D.metrics?.communities ?? [];
  const community =
    m?.community == null ? null : communities.find(c => c.id === m.community);
  const declared = (I.D.symbols ?? []).filter(s => s.file === id);
  const q = asArgs([id]);
  const uses = deps(I, q);
  const usedBy = dependents(I, q);
  const sites = received(I);
  const called = declared
    .map(s => ({ s, calls: sites.get(s.id) ?? 0 }))
    .toSorted((a, b) => b.calls - a.calls || (a.s.line ?? 0) - (b.s.line ?? 0));
  const position = ordinal(I, id);
  const self: Row = {
    ...fileRow(I, id, { line }),
    role: 'subject',
    kind: f?.kind ?? 'file',
    language: f?.language ?? null,
    metrics: m ?? null,
    pagerankRank: position,
    community: community
      ? { id: community.id, label: community.label, size: community.size }
      : null,
    check: f?.check?.status ?? null,
  };
  const symbols = called.map(({ s, calls }) => ({
    ...fileRow(I, id, {
      line: s.line ?? undefined,
      kind: s.kind,
      symbol: s.id,
      name: s.name,
      callSites: calls,
      exported: s.exported,
    }),
    role: 'declares',
  }));
  const parts = [
    position ? `PageRank #${position} of ${I.ids.length}` : 'no metrics',
    community ? `community ${community.id} (${community.label})` : null,
    m?.cycle != null ? `in cycle ${m.cycle}` : null,
    count(uses.rows.length, 'direct dependency', 'direct dependencies'),
    plural(usedBy.rows.length, 'direct dependent'),
    `${plural(declared.length, 'declaration')}${called[0]?.calls ? `; most called ${called[0].s.name} (${plural(called[0].calls, 'site')})` : ''}`,
  ].filter(Boolean);
  const docs = documentedBy(I, id);
  if (docs.length) parts.push(`${plural(docs.length, 'doc line')} document it`);
  return {
    rows: [
      self,
      ...tag(uses.rows, 'dependency'),
      ...tag(usedBy.rows, 'dependent'),
      ...symbols,
      ...docs,
    ],
    edges: [...uses.edges, ...usedBy.edges],
    nodes: [...new Set([id, ...uses.nodes, ...usedBy.nodes])],
    stats: {
      dependencies: uses.rows.length,
      dependents: usedBy.rows.length,
      declarations: declared.length,
      weakDependencies: uses.stats?.weakAdds ?? 0,
      weakDependents: usedBy.stats?.weakAdds ?? 0,
      // Only a graph built with doc links can say a file is undocumented.
      ...(I.D.links && { documentedBy: docs.length }),
    },
    next: { read: readCall(I, id) },
    summary: `${id}: ${parts.join(', ')}`,
  };
}

function aboutSymbol(I: QueryIndex, sym: string): OpResult {
  const info = I.symbols().byId.get(sym);
  const q = asArgs([sym]);
  const into = callers(I, q);
  const out = callees(I, q);
  const file = fileOf(sym);
  const self: Row = {
    ...fileRow(I, file, { line: info?.line ?? undefined }),
    role: 'subject',
    symbol: sym,
    name: info?.name ?? sym.split('#').pop(),
    kind: info?.kind ?? 'symbol',
    exported: (I.D.symbols ?? []).find(s => s.id === sym)?.exported ?? null,
  };
  return {
    symbol: sym,
    ...(into.warnings ? { warnings: into.warnings } : {}),
    next: {
      read: readCall(I, sym),
      ...((into.next as NextCalls | undefined)?.confirm && {
        confirm: (into.next as NextCalls).confirm,
      }),
    },
    rows: [
      self,
      ...tag(into.rows, 'caller'),
      ...tag(out.rows, 'callee'),
      ...documentedBy(I, sym),
    ],
    edges: [...into.edges, ...out.edges],
    nodes: [...new Set([file, ...into.nodes, ...out.nodes])],
    stats: {
      callers: into.stats,
      callees: out.stats,
      ...(I.D.links && { documentedBy: documentedBy(I, sym).length }),
    },
    summary: `${sym}: ${info?.kind ?? 'symbol'} at ${file}${info?.line ? `:${info.line}` : ''}; ${into.summary}; ${out.summary}${documentedBy(I, sym).length ? `; ${plural(documentedBy(I, sym).length, 'doc line')} document it` : ''}`,
  };
}

/** A file (path, suffix, `path:line`) or a symbol (`file#name`, bare name): everything adjacent to it in one answer. */
export function about(I: QueryIndex, { args }: QueryArgs): OpResult {
  const target = args.join(' ').trim();
  if (!target) throw new QueryError('about needs a file or a symbol');
  if (target.includes('#') && !I.file.has(target))
    return aboutSymbol(I, resolveSymbol(I, target));
  let fileError: QueryError;
  try {
    const { id, line } = resolveLocation(I, target);
    return aboutFile(I, id, line);
  } catch (error) {
    if (!(error instanceof QueryError)) throw error;
    fileError = error;
  }
  let symbol: string;
  try {
    symbol = resolveSymbol(I, target);
  } catch (error) {
    if (!(error instanceof QueryError)) throw error;
    // An ambiguous or near-miss symbol, and nothing for a file: the symbol's own answer lists its candidates.
    if (!fileError.candidates.length && error.candidates.length) throw error;
    if (!error.candidates.length && fileError.candidates.length)
      throw fileError;
    const candidates = [
      ...new Set([...fileError.candidates, ...error.candidates]),
    ];
    throw new QueryError(
      `no file or symbol is ${target}${candidates.length ? `; ${plural(candidates.length, 'file or symbol')} contain it` : `; try search ${target}`}`,
      { candidates, arg: target }
    );
  }
  return aboutSymbol(I, symbol);
}
