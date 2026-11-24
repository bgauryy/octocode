// The query index over one data.json: the weighted file graph (graph-model.ts CSR, production edges unless
// tests), per-pair evidence (import lines, symbol calls) and the symbol table, built once per D and mode. Argument
// resolution lives here too: a file is a root-relative path, a unique suffix or a node id; a symbol is file#name or
// a bare name. Ambiguity is an error carrying every candidate, never a guess.
import { RelationIndex, type Relation } from '../relations.ts';
import {
  callEvidence,
  csr,
  fileEdges,
  type FileEdge,
  type Graph,
} from '../graph-model.ts';
import { scc, type Components } from '../algorithms.ts';
import {
  keeps,
  loadOf,
  loadOfKind,
  strengthOf,
  type Load,
  type LoadMode,
} from '../loads.ts';
import type {
  CallEdge,
  FileNode,
  GraphData,
  GraphEdge,
  WeightedEdge,
} from './types.ts';
import { absOf, fileOf, rootOf } from '../ids.ts';

/** A saved graph as queries read it: an export may predate any field, so every one is optional. */
export type QueryData = Partial<GraphData>;

/**
 * Which edges a query walks: test-only edges, imports only, or also edges backed only by name-bound calls. `load`
 * walks imports that run: `runtime` drops type-only imports, `eager` also drops lazy ones (dynamic import()).
 */
export interface QueryMode {
  tests?: boolean;
  imports?: boolean;
  weak?: boolean;
  load?: LoadMode | null;
}

/** A call between two files, as pair evidence lists it. */
export interface CallEvidence {
  source: string;
  target: string;
  lines: number[];
  via: string;
  confidence: string;
  count: number;
}

export interface PairEvidence {
  imports: number[];
  /** The import kinds behind the pair (static-import, type-import, dynamic-import, …) and when they take effect. */
  kinds: string[];
  /** The lines of each import kind, when the build recorded them. */
  kindLines: Record<string, number[]>;
  /** The load the engine stated per kind, when it did. */
  loads: Record<string, Load>;
  load: Load | null;
  calls: CallEvidence[];
}

export interface SymbolInfo {
  id: string;
  file: string;
  name: string;
  line: number | null;
  endLine?: number;
  kind: string;
}

export interface SymbolTable {
  byId: Map<string, SymbolInfo>;
  byName: Map<string, string[]>;
}

/** Query index for one data object and mode. Members built on first use. */
export interface QueryIndex {
  readonly D: QueryData;
  readonly tests: boolean;
  readonly imports: boolean;
  readonly weak: boolean;
  readonly load: LoadMode | null;
  readonly ids: string[];
  readonly file: Map<string, FileNode>;
  readonly root: string;
  readonly G: Graph;
  readonly edge: Map<string, WeightedEdge>;
  byBasename(): Map<string, string[]>;
  byAbsolutePath(): Map<string, string>;
  relations(): RelationIndex;
  callsBy(endpoint: 'source' | 'target'): Map<string, CallEdge[]>;
  evidence(): Map<string, PairEvidence>;
  symbols(): SymbolTable;
  components(): Components;
}

const byText = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);

/** An argument as text: absent is empty. Arguments come from tokenized query text or validated JSON. */
export const textOf = (value: unknown) =>
  typeof value === 'string'
    ? value
    : value == null
      ? ''
      : JSON.stringify(value);
const pairKey = (u: string, v: string) => `${u}\0${v}`;
const cache = new WeakMap<QueryData, Record<string, QueryIndex>>();
const production = new WeakMap<QueryData, FileEdge[]>();

/** A query the graph cannot answer as asked; `candidates` lists every re-runnable argument. */
export class QueryError extends Error {
  readonly candidates: string[];
  readonly arg: string | null;
  /** The `:line[:column]` suffix of an ambiguous source location, kept on each candidate. */
  anchor: string | undefined;

  constructor(
    message: string,
    {
      candidates = [],
      arg = null,
      anchor,
    }: { candidates?: string[]; arg?: string | null; anchor?: string } = {}
  ) {
    super(message);
    this.candidates = candidates;
    this.arg = arg;
    this.anchor = anchor;
  }
}

function memo<T>(fn: () => T): () => T {
  let value: T;
  let done = false;
  return () => {
    if (!done) {
      value = fn();
      done = true;
    }
    return value;
  };
}

function append<K, V>(map: Map<K, V[]>, key: K, value: V) {
  const items = map.get(key);
  if (items) items.push(value);
  else map.set(key, [value]);
}

const linesOf = (x: { lines?: number[]; line?: number | null }) =>
  x.lines ?? (x.line ? [x.line] : []);
const callRow = (c: CallEdge): CallEvidence => ({
  source: c.source,
  target: c.target,
  lines: linesOf(c),
  via: c.via ?? 'graph',
  confidence: c.confidence ?? 'medium',
  count: c.count ?? 1,
});

// u→v evidence: import lines and the symbol-level calls between the two files.
function pairEvidence(D: QueryData, tests: boolean) {
  const by = new Map<string, PairEvidence>();
  const at = (u: string, v: string) => {
    const key = pairKey(u, v);
    let found = by.get(key);
    if (!found)
      by.set(
        key,
        (found = {
          imports: [],
          kinds: [],
          kindLines: {},
          loads: {},
          load: null,
          calls: [],
        })
      );
    return found;
  };
  for (const e of D.imports ?? [])
    if (tests || !e.test) {
      const pair = at(e.source, e.target);
      for (const line of linesOf(e)) pair.imports.push(line);
      for (const kind of e.resolution ?? [])
        if (!pair.kinds.includes(kind)) pair.kinds.push(kind);
      for (const [kind, lines] of Object.entries(e.kindLines ?? {}))
        (pair.kindLines[kind] ??= []).push(...lines);
      Object.assign(pair.loads, e.loads);
      pair.load = loadOf(pair.kinds, pair.loads);
    }
  for (const c of D.calls ?? []) {
    const [u, v] = [fileOf(c.source), fileOf(c.target)];
    if (u !== v && (tests || !c.test)) at(u, v).calls.push(callRow(c));
  }
  return by;
}

// Every symbol: declared rows, plus call endpoints the declarations do not list (stitched or external targets).
function symbolTable(D: QueryData): SymbolTable {
  const byId = new Map<string, SymbolInfo>();
  for (const s of D.symbols ?? [])
    byId.set(s.id, {
      id: s.id,
      file: s.file ?? fileOf(s.id),
      name: s.name ?? s.id.split('#').slice(1).join('#'),
      line: s.line ?? 1,
      ...(s.endLine != null && { endLine: s.endLine }),
      kind: s.kind ?? 'symbol',
    });
  for (const c of D.calls ?? [])
    for (const id of [c.source, c.target]) {
      if (byId.has(id) || !id.includes('#')) continue;
      byId.set(id, {
        id,
        file: fileOf(id),
        name: id.split('#').slice(1).join('#'),
        line: null,
        kind: 'symbol',
      });
    }
  const byName = new Map<string, string[]>();
  for (const s of byId.values()) {
    const short = s.name.split('.').pop() ?? s.name;
    for (const k of new Set([s.name, short])) append(byName, k, s.id);
  }
  return { byId, byName };
}

function* relations(
  D: QueryData,
  files: Map<string, FileNode>,
  { tests, weak, load }: Required<QueryMode>
): Generator<Relation> {
  for (const edge of D.enrichment?.relations ?? [])
    if (tests || !edge.test) yield edge;
  for (const s of D.symbols ?? []) {
    if (files.has(s.file))
      yield {
        relation: 'DEFINES',
        source: s.file,
        target: s.id,
        path: s.file,
        lines: linesOf(s),
      };
  }
  const kinds: [string, GraphEdge[] | undefined][] = [
    ['IMPORTS', D.imports],
    ['CALLS', D.calls],
  ];
  for (const [kind, rows] of kinds)
    for (const edge of rows ?? []) {
      if (
        (!tests && edge.test) ||
        (kind === 'CALLS' && !weak && callEvidence(edge) < 1) ||
        (kind === 'IMPORTS' &&
          load &&
          !keeps(
            load,
            loadOf(edge.resolution, 'loads' in edge ? edge.loads : undefined)
          ))
      )
        continue;
      const path = fileOf(edge.source);
      if (!files.has(path) || !files.has(fileOf(edge.target))) continue;
      yield { ...edge, relation: kind, path, lines: linesOf(edge) };
    }
}

// Languages whose modules load through imports (JS/TS, Python): an import of theirs that loads at run time or never
// (eager, deferred, type, unknown). Every other language binds at compile time (Go packages, Java, C# and Scala
// namespaces, Rust crates, C includes): a resolved call there is a dependency even with no import between the files,
// and a language with no import edge at all (one the engine links no imports for) is among them.
function moduleLoading(D: QueryData, files: Map<string, FileNode>) {
  const languages = new Set<string>();
  for (const e of D.imports ?? []) {
    const load = loadOf(e.resolution, e.loads);
    const language = files.get(e.source)?.language;
    if (language && load !== 'compile' && load !== 'data')
      languages.add(language);
  }
  return languages;
}

// Mode: tests adds test-only edges; imports keeps only file pairs with an import; weak adds edges backed only by
// medium/low-confidence calls bound by name (by default walks follow strong edges: an import or a full-evidence call);
// load keeps only file pairs whose strongest import runs (loads.ts): a call alone loads no module.
function buildIndex(D: QueryData, mode: Required<QueryMode>): QueryIndex {
  const { tests, imports, weak, load } = mode;
  const files = D.files ?? [];
  const root = rootOf(D);
  const ids = files.map(f => f.id);
  const file = new Map(files.map(f => [f.id, f]));
  const edges = memo(() => {
    if (load) {
      // Saved fileEdges predate `load`: recount from the import kinds.
      const loading = moduleLoading(D, file);
      return fileEdges(
        { imports: D.imports ?? [], calls: D.calls ?? [] },
        tests
      )
        .flatMap(e => {
          if (e.imports) return e.load && keeps(load, e.load) ? [e] : [];
          // A call alone: a compile-time dependency where the caller's language binds by reference, never a load
          // where modules load through imports. Name-bound calls only with --weak, as in every walk.
          const language = file.get(e.source)?.language ?? '';
          return !loading.has(language) && (weak || e.strong)
            ? [{ ...e, load: 'compile' as const }]
            : [];
        })
        .map(e => (tests ? { ...e, test: false } : e));
    }
    let all = D.fileEdges;
    if (!tests && (D.imports || D.calls)) {
      let rows = production.get(D);
      if (!rows) {
        rows = fileEdges(
          { imports: D.imports ?? [], calls: D.calls ?? [] },
          false
        );
        production.set(D, rows);
      }
      all = rows;
    }
    all ??= fileEdges({ imports: D.imports ?? [], calls: D.calls ?? [] });
    return all
      .filter(e => (imports ? e.imports : weak || e.strong !== false))
      .map(e => (tests ? { ...e, test: false } : e));
  });
  const graph = memo(() => csr(ids, edges()));
  const pairs = memo(
    () =>
      new Map(
        edges()
          .filter(e => !e.test)
          .map(e => [pairKey(e.source, e.target), e])
      )
  );
  const calls = new Map<string, Map<string, CallEdge[]>>();
  return {
    D,
    tests,
    imports,
    weak,
    load,
    ids,
    file,
    byBasename: memo(() =>
      Map.groupBy(ids, id => id.slice(id.lastIndexOf('/') + 1))
    ),
    byAbsolutePath: memo(
      () =>
        new Map(
          files.flatMap(f =>
            root && f.kind !== 'package'
              ? [[absOf(root, f.id), f.id] as const]
              : []
          )
        )
    ),
    get G() {
      return graph();
    },
    get edge() {
      return pairs();
    },
    root,
    relations: memo(
      () =>
        new RelationIndex(
          ['DEFINES', 'IMPORTS', 'CALLS', ...(D.enrichment?.kinds ?? [])],
          relations(D, file, mode)
        )
    ),
    callsBy(endpoint) {
      let bySymbol = calls.get(endpoint);
      if (!bySymbol) {
        bySymbol = new Map();
        for (const call of D.calls ?? [])
          if (tests || !call.test) append(bySymbol, call[endpoint], call);
        calls.set(endpoint, bySymbol);
      }
      return bySymbol;
    },
    evidence: memo(() => pairEvidence(D, tests)),
    symbols: memo(() => symbolTable(D)),
    components: memo(() => scc(graph())),
  };
}

// One index per data object and mode; a reloaded data.json is a new object, so a refresh never reads a stale index.
export function indexOf(
  D: QueryData,
  { tests = false, imports = false, weak = false, load = null }: QueryMode = {}
) {
  let modes = cache.get(D);
  if (!modes) cache.set(D, (modes = {}));
  const walked = load ?? (imports ? 'imports' : weak ? 'weak' : 'strong');
  const key = `${tests ? 'tests' : 'production'}:${walked}`;
  modes[key] ??= buildIndex(D, { tests, imports, weak, load });
  return modes[key];
}

export const edgeOf = (I: QueryIndex, u: string, v: string) =>
  I.edge.get(pairKey(u, v)) ?? null;
export const evidenceOf = (I: QueryIndex, u: string, v: string) =>
  I.evidence().get(pairKey(u, v)) ?? {
    imports: [],
    kinds: [],
    kindLines: {},
    loads: {},
    load: null,
    calls: [],
  };

/** The first line of u→v's imports whose kind passes `keep`; null when the build recorded no lines per kind. */
export function importLine(
  I: QueryIndex,
  u: string,
  v: string,
  keep: (load: Load) => boolean
) {
  const ev = evidenceOf(I, u, v);
  const lines = Object.entries(ev.kindLines).flatMap(([kind, at]) => {
    const load = loadOfKind(kind, ev.loads);
    return load && keep(load) ? at : [];
  });
  return lines.length ? Math.min(...lines) : null;
}

// The first evidence line of u→v in u, for a row's location: the first import of the kind that decides the pair's
// load (so a hop marked deferred cites its import()), else the first import or call site.
export function evidenceLine(I: QueryIndex, u: string, v: string) {
  const ev = evidenceOf(I, u, v);
  // The import that decides the pair's load (its strongest kind): never an `import type` beside an `import()`.
  const decided = ev.load;
  const deciding = decided
    ? importLine(I, u, v, l => strengthOf(l) === strengthOf(decided))
    : null;
  if (deciding !== null) return deciding;
  let earliest = Infinity;
  for (const line of ev.imports)
    if (Number.isInteger(line) && line < earliest) earliest = line;
  for (const call of ev.calls)
    for (const line of call.lines)
      if (Number.isInteger(line) && line < earliest) earliest = line;
  return earliest === Infinity ? 1 : earliest;
}

function matchFile(I: QueryIndex, path: string, raw: string, anchor = '') {
  if (I.file.has(path)) return path;
  if (path.startsWith('/')) return I.byAbsolutePath().get(path);
  const sameName =
    I.byBasename().get(path.slice(path.lastIndexOf('/') + 1)) ?? [];
  const hits = sameName.filter(id => id.endsWith(`/${path}`));
  if (hits.length <= 1) return hits[0];
  throw new QueryError(
    `${raw} is ambiguous: ${hits.length} files end with it`,
    {
      candidates: hits,
      arg: raw,
      anchor,
    }
  );
}

export interface Location {
  id: string;
  line?: number;
  column?: number;
}

/** Prefer literal paths, including names containing '#' or ':', before interpreting source anchors. */
export function resolveLocation(I: QueryIndex, arg: unknown): Location {
  const raw = textOf(arg).trim();
  if (!raw) throw new QueryError('missing file argument');
  let path = raw.replace(/^\.\//, '');
  if (I.root && path.startsWith(`${I.root}/`))
    path = path.slice(I.root.length + 1);
  let id = matchFile(I, path, raw);
  if (id) return { id };
  const anchor = /:(\d+)(?::(\d+))?$/.exec(path);
  if (anchor) {
    path = path.slice(0, anchor.index);
    id = matchFile(I, path, raw, anchor[0]);
    if (id) {
      const line = Number(anchor[1]);
      const column = anchor[2] ? Number(anchor[2]) : undefined;
      if (![line, column ?? 1].every(n => Number.isSafeInteger(n) && n > 0))
        throw new QueryError(`invalid source location in ${raw}`);
      return { id, line, ...(column && { column }) };
    }
  }
  if (path.includes('#')) {
    path = fileOf(path);
    id = matchFile(I, path, raw);
    if (id) return { id };
  }
  const lower = path.toLowerCase();
  const near = I.ids.filter(candidate =>
    candidate.toLowerCase().includes(lower)
  );
  throw new QueryError(
    `no file is ${raw}${near.length ? `; ${near.length} contain it` : ''}`,
    { candidates: near, arg: raw, anchor: anchor?.[0] }
  );
}

export const resolveFile = (I: QueryIndex, arg: unknown) =>
  resolveLocation(I, arg).id;

function symbolsInFile(I: QueryIndex, fileArg: string, name: string) {
  const file = resolveFile(I, fileArg);
  const { byId } = I.symbols();
  if (byId.has(`${file}#${name}`)) return [`${file}#${name}`];
  return [...byId.values()]
    .filter(
      s => s.file === file && (s.name === name || s.name.endsWith(`.${name}`))
    )
    .map(s => s.id);
}

/** A symbol argument (file#name with any file form, or a bare name) → its symbol id; ambiguity lists candidates. */
export function resolveSymbol(I: QueryIndex, arg: unknown) {
  const raw = textOf(arg).trim();
  if (!raw) throw new QueryError('missing symbol argument');
  const { byId, byName } = I.symbols();
  if (byId.has(raw)) return raw;
  const at = raw.indexOf('#');
  const hits =
    at > 0
      ? symbolsInFile(I, raw.slice(0, at), raw.slice(at + 1))
      : (byName.get(raw) ?? []);
  if (hits.length === 1) return hits[0];
  if (hits.length > 1)
    throw new QueryError(`${raw} is ambiguous: ${hits.length} symbols match`, {
      candidates: hits.toSorted(byText),
      arg: raw,
    });
  const lower = raw.toLowerCase();
  const near = [...byId.keys()]
    .filter(id => id.toLowerCase().includes(lower))
    .toSorted(byText);
  throw new QueryError(
    `no symbol is ${raw}${near.length ? `; ${near.length} contain it` : ''}`,
    { candidates: near, arg: raw }
  );
}
