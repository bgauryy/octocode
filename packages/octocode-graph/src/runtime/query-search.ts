// Ranked search and entity cards over one data.json (query-index.ts):
//   search <text> [--files|--symbols] [--all] [--kind K] [--lang L] [--role R] [--path P] [--exported]
//          [--param T] [--returns T] [--unsigned] [--provider] [--tests]
//       files and symbols ranked by reciprocal rank fusion of how well their name matches (exact and prefix names
//       always first) and BM25F over their documents (name, enclosing scope, path, kind, doc comment, names used,
//       signature: search-index.ts), fused by search-fusion.ts. Test code ranks after production code unless --tests.
//       Ties: call sites
//       received, exported, PageRank. Each row says which signals found it (`found`).
//   about <file|symbol>                          one answer with everything adjacent to it: metrics, community,
//                                               direct dependencies and dependents, or callers and callees
// `search` is the one text query: exact names first, then partial, described and misspelled ones (`find` reads as it).
import { documentedBy } from './query-links.ts';
import {
  QueryError,
  resolveLocation,
  resolveSymbol,
  type QueryIndex,
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
  sourcePath,
  type NextCalls,
  type OpResult,
  type QueryArgs,
  type Row,
} from './query-ops.ts';
import {
  bm25f,
  queryTerms,
  ranked,
  searchIndexOf,
  words,
  type SearchDoc,
  type SearchIndex,
} from './search-index.ts';
import { fuse, type RankedList } from './search-fusion.ts';
import { fileOf } from '../ids.ts';

export { words } from './search-index.ts';

/** How a candidate's name matched the query, strongest first; then the signals that find a document otherwise. */
export const MATCHES = [
  'exact',
  'prefix',
  'words',
  'substring',
  'terms',
  'path',
  'text',
  'related',
  'linked',
  'fuzzy',
] as const;
export type Match = (typeof MATCHES)[number];
/** Rows a search shows unless `--all` asks for every match; the summary names the rest. */
export const SEARCH_SHOWN = 20;

/** How much each ranked list counts in the fusion: a name match most, the widened lists least. */
const LIST_WEIGHT = { name: 2, text: 1 };

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

/** A name as matching reads it: lower case, letters and digits only, its words, and the lower-case path or id. */
interface NameForm {
  lower: string;
  flat: string;
  words: string[];
  path: string;
}
const formOf = (name: string, path: string): NameForm => ({
  lower: name.toLowerCase(),
  flat: compact(name),
  words: words(name),
  path: path.toLowerCase(),
});

/**
 * The strongest way a name matches the query, or null. Its path helps a query of several words (one may name the
 * folder), and alone matches only when `byPath` (a file is found by its folders; a symbol by its own name).
 */
function matchForm(q: Query, f: NameForm, byPath: boolean): Match | null {
  if (f.lower === q.lower || (q.compact && f.flat === q.compact))
    return 'exact';
  if (
    f.lower.startsWith(q.lower) ||
    (q.compact && f.flat.startsWith(q.compact))
  )
    return 'prefix';
  if (wordPrefixes(q.terms, f.words)) return 'words';
  if (f.lower.includes(q.lower) || (q.compact && f.flat.includes(q.compact)))
    return 'substring';
  if (
    q.terms.length > 1 &&
    q.terms.every(t => f.lower.includes(t) || f.path.includes(t)) &&
    q.terms.some(t => f.lower.includes(t))
  )
    return 'terms';
  if (byPath && f.path.includes(q.lower)) return 'path';
  return null;
}

/** The strongest way `name` (of the file or symbol at `path`) matches the query, or null (see matchForm). */
export function matchOf(
  q: Query,
  name: string,
  path: string,
  byPath: boolean
): Match | null {
  return matchForm(q, formOf(name, path), byPath);
}

// Call sites each symbol receives in production code: the strongest sign that a name matters.
const receivedBy = new WeakMap<QueryIndex, Map<string, number>>();
function received(I: QueryIndex) {
  let sites = receivedBy.get(I);
  if (sites) return sites;
  sites = new Map<string, number>();
  for (const [target, calls] of I.callsBy('target'))
    sites.set(
      target,
      calls.reduce((n, c) => n + (c.count ?? 1), 0)
    );
  receivedBy.set(I, sites);
  return sites;
}

const basename = (id: string) => id.slice(id.lastIndexOf('/') + 1);

/** The flags search takes: switches, and the filters that take a value (`--kind function` or `--kind=function`). */
const SWITCHES = [
  '--files',
  '--symbols',
  '--all',
  '--exported',
  '--provider',
  '--unsigned',
];
const FILTERS = [
  '--kind',
  '--lang',
  '--role',
  '--path',
  '--param',
  '--returns',
];

interface SearchArgs {
  text: string;
  switches: Set<string>;
  filters: Map<string, string[]>;
}

function searchArgs(args: string[]): SearchArgs {
  const text: string[] = [];
  const switches = new Set<string>();
  const filters = new Map<string, string[]>();
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (!a.startsWith('--')) {
      text.push(a);
      continue;
    }
    const eq = a.indexOf('=');
    const flag = eq < 0 ? a : a.slice(0, eq);
    if (SWITCHES.includes(flag) && eq < 0) switches.add(flag);
    else if (FILTERS.includes(flag)) {
      const value = eq < 0 ? args[++i] : a.slice(eq + 1);
      if (value === undefined || value === '' || value.startsWith('--'))
        throw new QueryError(`search ${flag} needs a value`);
      filters.set(flag, [...(filters.get(flag) ?? []), value]);
    } else
      throw new QueryError(
        `search takes ${[...SWITCHES, ...FILTERS.map(f => `${f} <value>`)].join(', ')}, not ${a}`
      );
  }
  return { text: text.join(' ').trim(), switches, filters };
}

const lowerList = (values: string[] | undefined) =>
  values
    ?.flatMap(v => v.split(','))
    .map(v => v.trim().toLowerCase())
    .filter(Boolean);

// A type name as a whole word of the signature text: `Node` matches `Node | undefined`, not `NodeArray`.
function typePattern(text: string) {
  const escaped = text.trim().replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const before = /^[A-Za-z0-9_]/.test(text.trim()) ? '(?<![A-Za-z0-9_])' : '';
  const after = /[A-Za-z0-9_]$/.test(text.trim()) ? '(?![A-Za-z0-9_])' : '';
  return new RegExp(`${before}${escaped}${after}`, 'i');
}

type Signature = 'match' | 'unknown';

/**
 * Which documents the filters keep, and for --param/--returns whether the recorded signature matches ('match') or
 * none is recorded ('unknown', kept after the matches): a signature that does not match drops the document.
 */
function filterOf(I: QueryIndex, index: SearchIndex, a: SearchArgs) {
  const kinds = lowerList(a.filters.get('--kind'));
  const langs = lowerList(a.filters.get('--lang'));
  const roles = lowerList(a.filters.get('--role'));
  const prefixes = a.filters
    .get('--path')
    ?.map(p => p.replace(/^\.\//, '').replace(/\/+$/, ''));
  const params = a.filters.get('--param')?.map(typePattern);
  const returns = a.filters.get('--returns')?.map(typePattern);
  // A filter value nothing in the graph has is an error that lists what it has, never an empty answer.
  const known = (
    values: string[] | undefined,
    seen: () => Iterable<string>,
    what: string
  ) => {
    if (!values) return;
    const have = new Set(seen());
    for (const v of values)
      if (!have.has(v))
        throw new QueryError(
          `no ${what} is ${v} in this graph; it has ${[...have].sort().join(', ') || 'none'}`
        );
  };
  const graphFiles = I.D.files ?? [];
  known(kinds, () => index.docs.map(d => d.kind.toLowerCase()), 'kind');
  known(
    langs,
    () =>
      graphFiles.flatMap(f => (f.language ? [f.language.toLowerCase()] : [])),
    'language'
  );
  known(
    roles,
    () => graphFiles.flatMap(f => (f.roles ?? []).map(r => r.toLowerCase())),
    'file role'
  );
  const files = !a.switches.has('--symbols') || a.switches.has('--files');
  const symbols = !a.switches.has('--files') || a.switches.has('--symbols');
  const exported = a.switches.has('--exported');
  const signed = Boolean(params || returns);
  return (d: SearchDoc): Signature | null => {
    if (d.symbol ? !symbols : !files) return null;
    if ((exported || signed) && !d.symbol) return null;
    if (exported && !d.exported) return null;
    if (kinds && !kinds.includes(d.kind.toLowerCase())) return null;
    const f = I.file.get(d.file);
    if (langs && !langs.includes((f?.language ?? '').toLowerCase()))
      return null;
    if (
      roles &&
      !roles.some(r => (f?.roles ?? []).some(x => x.toLowerCase() === r))
    )
      return null;
    if (
      prefixes &&
      !prefixes.some(
        p => d.file === p || d.file.startsWith(`${p}/`) || p === '' || p === '.'
      )
    )
      return null;
    if (!signed) return 'match';
    let unknown = false;
    for (const [patterns, text] of [
      [params, d.params],
      [returns, d.returns],
    ] as const) {
      if (!patterns) continue;
      if (text == null) unknown = true;
      else if (!patterns.every(p => p.test(text))) return null;
    }
    return unknown ? 'unknown' : 'match';
  };
}

const forms = new WeakMap<SearchIndex, NameForm[]>();
const formsOf = (index: SearchIndex) => {
  let list = forms.get(index);
  if (!list)
    forms.set(index, (list = index.docs.map(d => formOf(d.name, d.id))));
  return list;
};

// The name match of a document: a symbol by its own name, a file by its stem or its whole base name and its folders.
function nameMatch(q: Query, d: SearchDoc, form: NameForm): Match | null {
  if (d.symbol) return matchForm(q, form, false);
  return (
    matchForm(q, form, true) ??
    (basename(d.id).toLowerCase() === q.lower ? 'exact' : null)
  );
}

/** A localFetch call for a document: its declaration span, or the whole file. */
function readDoc(I: QueryIndex, d: SearchDoc): NonNullable<NextCalls['read']> {
  if (!d.symbol || !d.line) return readCall(I, d.file);
  return {
    tool: 'localFetch',
    input: {
      queries: [
        {
          path: sourcePath(I, d.file),
          ranges: [
            {
              startLine: d.line,
              endLine: Math.max(d.line, d.endLine ?? d.line),
            },
          ],
        },
      ],
    },
  };
}

interface Hit {
  doc: number;
  match: Match;
  /** In a test file: ranked after production code unless --tests asks for test code too. */
  test: boolean;
  found: string[];
  score: number;
  signature: Signature;
  importance: number[];
}

const pinned = (m: Match) => (m === 'exact' ? 0 : m === 'prefix' ? 1 : 2);

function compareHits(index: SearchIndex) {
  return (a: Hit, b: Hit) => {
    const byPin = pinned(a.match) - pinned(b.match);
    if (byPin) return byPin;
    if (a.signature !== b.signature) return a.signature === 'match' ? -1 : 1;
    if (a.test !== b.test) return a.test ? 1 : -1;
    if (a.score !== b.score) return b.score - a.score;
    for (
      let i = 0;
      i < Math.max(a.importance.length, b.importance.length);
      i++
    ) {
      const d = (b.importance[i] ?? 0) - (a.importance[i] ?? 0);
      if (d) return d;
    }
    const x = index.docs[a.doc],
      y = index.docs[b.doc];
    return (
      x.name.length - y.name.length ||
      (x.file < y.file ? -1 : x.file > y.file ? 1 : 0) ||
      (x.id < y.id ? -1 : x.id > y.id ? 1 : 0)
    );
  };
}

export function search(I: QueryIndex, { args, tests }: QueryArgs): OpResult {
  const a = searchArgs(args);
  const signed = a.filters.has('--param') || a.filters.has('--returns');
  if (!a.text && !signed) throw new QueryError('search needs text');
  const index = searchIndexOf(I.D);
  const keep = filterOf(I, index, a);
  const q: Query = {
    text: a.text,
    lower: a.text.toLowerCase(),
    compact: compact(a.text),
    terms: words(a.text),
  };
  const nameForms = formsOf(index);

  // The name list: every name match, by match then importance (today's ranking).
  const names = new Map<number, Match>();
  if (a.text)
    index.docs.forEach((d, i) => {
      const m = nameMatch(q, d, nameForms[i]);
      if (m) names.set(i, m);
    });
  const { terms, unknown } = a.text
    ? queryTerms(index, a.text)
    : { terms: [], unknown: [] };
  const lexical = bm25f(index, terms);

  const sites = received(I);
  const importance = (d: SearchDoc) =>
    d.symbol
      ? [
          sites.get(d.id) ?? 0,
          d.exported ? 1 : 0,
          metricOf(I, d.file, 'pagerank') ?? 0,
        ]
      : [metricOf(I, d.file, 'pagerank') ?? 0];
  const byImportance = (x: number, y: number) => {
    const ix = importance(index.docs[x]),
      iy = importance(index.docs[y]);
    for (let i = 0; i < Math.max(ix.length, iy.length); i++) {
      const d = (iy[i] ?? 0) - (ix[i] ?? 0);
      if (d) return d;
    }
    return x - y;
  };
  const nameList = [...names.keys()].sort(
    (x, y) =>
      MATCHES.indexOf(names.get(x)!) - MATCHES.indexOf(names.get(y)!) ||
      byImportance(x, y)
  );
  const lists: RankedList[] = [
    { signal: 'name', docs: nameList, weight: LIST_WEIGHT.name },
    { signal: 'text', docs: ranked(lexical.scores), weight: LIST_WEIGHT.text },
  ];
  // Signature filters alone: every symbol they keep, by importance.
  if (!a.text)
    lists.push({
      signal: 'signature',
      docs: index.docs.map((_, i) => i).sort(byImportance),
      weight: 1,
    });
  const fused = fuse(lists);

  const testCode = (d: SearchDoc) =>
    !tests && Boolean(I.file.get(d.file)?.roles?.includes('test'));
  const matchFor = (i: number, found: string[]): Match =>
    names.get(i) ??
    (found.includes('text') || found.includes('signature')
      ? 'text'
      : found.includes('related')
        ? 'related'
        : 'linked');
  const hits: Hit[] = [];
  for (const [i, score] of fused.score) {
    const signature = keep(index.docs[i]);
    if (!signature) continue;
    const found = fused.found.get(i) ?? [];
    // A walk alone reaches a document only through the hits: it is a hit when the text or a name found something.
    hits.push({
      doc: i,
      match: matchFor(i, found),
      test: testCode(index.docs[i]),
      found,
      score,
      signature,
      importance: importance(index.docs[i]),
    });
  }
  // A misspelled or abbreviated name: letters in order, tried only when no name, document or path holds the text.
  const fuzzy = !hits.length && q.compact.length >= 3;
  if (fuzzy)
    index.docs.forEach((d, i) => {
      const signature = keep(d);
      if (signature && subsequence(q.compact, nameForms[i].flat))
        hits.push({
          doc: i,
          match: 'fuzzy',
          test: testCode(d),
          found: ['fuzzy'],
          score: 0,
          signature,
          importance: importance(d),
        });
    });
  hits.sort(compareHits(index));
  // A signature filter on a graph that records signatures keeps what matches: a symbol with no written type for the
  // filtered position is left out and counted (--unsigned keeps it after the matches). A graph that records none keeps
  // them all, with a warning, or the filter would answer nothing.
  const strict = signed && index.signatures && !a.switches.has('--unsigned');
  const leftOut = strict
    ? hits.filter(h => h.signature === 'unknown').length
    : 0;
  if (leftOut)
    hits.splice(0, hits.length, ...hits.filter(h => h.signature !== 'unknown'));
  const all = a.switches.has('--all');
  const shown = all ? hits : hits.slice(0, SEARCH_SHOWN);
  const matchedWords = (i: number) =>
    [...new Set(lexical.matched.get(i) ?? [])].map(t => index.surface[t]);
  const rows = shown.map((h, k) => {
    const d = index.docs[h.doc];
    const held = matchedWords(h.doc);
    const extra = {
      match: h.match,
      found: h.found,
      ...(held.length && { words: held }),
    };
    const row = {
      ...(d.symbol
        ? fileRow(I, d.file, {
            line: d.line ?? undefined,
            kind: d.kind,
            symbol: d.id,
            name: d.name,
            ...extra,
            callSites: sites.get(d.id) ?? 0,
            exported: d.exported,
            ...(d.params != null && { params: d.params }),
            ...(d.returns != null && { returns: d.returns }),
            ...(signed && { signature: h.signature }),
          })
        : fileRow(I, d.id, {
            kind: 'file',
            ...extra,
            pagerank: metricOf(I, d.id, 'pagerank') ?? 0,
          })),
      rank: k + 1,
    };
    // The declaration's span, so a reader (or --provider) fetches the whole of it.
    if (row.loc && d.symbol && d.endLine != null) row.loc.endLine = d.endLine;
    return row;
  });
  const byMatch: Record<string, number> = {};
  const byFound: Record<string, number> = {};
  for (const h of hits) {
    byMatch[h.match] = (byMatch[h.match] ?? 0) + 1;
    for (const s of h.found) byFound[s] = (byFound[s] ?? 0) + 1;
  }
  const files = hits.filter(h => !index.docs[h.doc].symbol).length;
  const best = hits[0] && index.docs[hits[0].doc];
  const asked = args
    .filter(x => x !== '--all')
    .map(x => (/\s/.test(x) ? `"${x.replace(/["\\]/g, '\\$&')}"` : x))
    .join(' ');
  const rest =
    hits.length > rows.length
      ? `; showing ${rows.length}: \`search ${asked} --all\` lists all ${hits.length}`
      : '';
  const unknownSigned =
    leftOut || hits.filter(h => h.signature === 'unknown').length;
  const warnings = [
    ...(index.sidecar === 'failed'
      ? [
          `doc comments are not indexed: the search index could not be read or built (${index.sidecarError}); names, paths and calls still rank`,
        ]
      : []),
    ...(index.unread.length
      ? [
          `${plural(index.unread.length, 'file')} changed since the graph was written or could not be read (first: ${index.unread[0].file}): their doc comments are not indexed`,
        ]
      : []),
    ...(signed && !index.signatures
      ? [
          'this graph records no parameter or return types: every symbol is listed as signature unknown; rebuild it with an engine that records them',
        ]
      : []),
  ];
  // The words searched for, quoted; with filters alone, the filters (`returns Response, kind function`).
  const label = a.text
    ? `"${a.text}"`
    : [...a.filters]
        .map(([flag, values]) => `${flag.slice(2)} ${values.join(', ')}`)
        .join(', ') || `"${asked}"`;
  return {
    rows,
    edges: [],
    nodes: [...new Set(rows.map(r => r.id))],
    ...(warnings.length && { warnings }),
    ...(best && { next: { read: readDoc(I, best) } }),
    stats: {
      matches: hits.length,
      shown: rows.length,
      files,
      symbols: hits.length - files,
      byMatch,
      byFound,
      fuzzy,
      ...(unknown.length && { unknownWords: unknown }),
      ...(signed && { signatureUnknown: unknownSigned }),
      index: {
        documents: index.docs.length,
        documented: index.documented,
        sidecar: index.sidecar,
        ...(index.unread.length && { unread: index.unread.length }),
      },
      ...(a.switches.has('--provider') && { provider: 'requested' }),
    },
    summary: hits.length
      ? `${count(hits.length, 'match', 'matches')} for ${label} (${plural(files, 'file')}, ${plural(hits.length - files, 'symbol')})${fuzzy ? ' by letters in order' : ''}; best ${best.id} (${hits[0].match})${unknownSigned ? `; ${plural(unknownSigned, 'symbol')} with no recorded signature ${leftOut ? 'left out (--unsigned keeps them)' : 'listed after the matches'}` : ''}${rest}`
      : `nothing matches ${label}${leftOut ? `; ${plural(leftOut, 'symbol')} with no recorded signature left out (--unsigned keeps them)` : ''}`,
  };
}

const count = (n: number, one: string, many: string) =>
  `${n} ${n === 1 ? one : many}`;

const asArgs = (args: string[]): QueryArgs => ({
  args,
  tests: false,
  imports: false,
  weak: false,
  load: null,
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
