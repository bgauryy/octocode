// The query index over one data.json: the weighted file graph (graph-model.mjs CSR, production edges unless
// tests), per-pair evidence (import lines, symbol calls) and the symbol table, built once per D and mode. Argument
// resolution lives here too: a file is a root-relative path, a unique suffix or a node id; a symbol is file#name or
// a bare name. Ambiguity is an error carrying every candidate, never a guess.
import { csr, fileEdges } from './graph-model.mjs';
import { scc } from './algorithms.mjs';
const byText = (a, b) => (a < b ? -1 : a > b ? 1 : 0);

export const fileOf = (id) => String(id).split('#')[0];
const pairKey = (u, v) => `${u}\0${v}`;
const cache = new WeakMap();

export class QueryError extends Error {
  constructor(message, { candidates = [], arg = null } = {}) {
    super(message);
    this.candidates = candidates;
    this.arg = arg;
  }
}

function memo(fn) {
  let value;
  let done = false;
  return () => {
    if (!done) { value = fn(); done = true; }
    return value;
  };
}

const linesOf = (x) => x.lines ?? (x.line ? [x.line] : []);
const callRow = (c) => ({ source: c.source, target: c.target, lines: linesOf(c), via: c.via ?? 'graph', confidence: c.confidence ?? 'medium', count: c.count ?? 1 });

// u→v evidence: import lines and the symbol-level calls between the two files.
function pairEvidence(D, tests) {
  const by = new Map();
  const at = (u, v) => by.get(pairKey(u, v)) ?? by.set(pairKey(u, v), { imports: [], calls: [] }).get(pairKey(u, v));
  for (const e of D.imports ?? []) if (tests || !e.test) at(e.source, e.target).imports.push(...linesOf(e));
  for (const c of D.calls ?? []) {
    const [u, v] = [fileOf(c.source), fileOf(c.target)];
    if (u !== v && (tests || !c.test)) at(u, v).calls.push(callRow(c));
  }
  return by;
}

// Every symbol: declared rows, plus call endpoints the declarations do not list (stitched or external targets).
function symbolTable(D) {
  const byId = new Map();
  for (const s of D.symbols ?? []) byId.set(s.id, { id: s.id, file: s.file ?? fileOf(s.id), name: s.name ?? s.id.split('#').slice(1).join('#'), line: s.line ?? 1, kind: s.kind ?? 'symbol' });
  for (const c of D.calls ?? []) for (const id of [c.source, c.target]) {
    if (byId.has(id) || !String(id).includes('#')) continue;
    byId.set(id, { id, file: fileOf(id), name: id.split('#').slice(1).join('#'), line: null, kind: 'symbol' });
  }
  const byName = new Map();
  for (const s of byId.values()) {
    const short = s.name.split('.').pop();
    for (const k of new Set([s.name, short])) byName.set(k, [...(byName.get(k) ?? []), s.id]);
  }
  return { byId, byName };
}

// Mode: tests adds test-only edges; imports keeps only file pairs with an import; weak adds edges backed only by
// medium/low-confidence calls bound by name (by default walks follow strong edges: an import or a full-evidence call).
function buildIndex(D, { tests, imports, weak }) {
  const files = D.files ?? [];
  const ids = files.map((f) => f.id);
  const all = D.fileEdges ?? fileEdges({ imports: D.imports ?? [], calls: D.calls ?? [] });
  const edges = all.filter((e) => (imports ? e.imports : weak || e.strong !== false)).map((e) => (tests ? { ...e, test: false } : e));
  const G = csr(ids, edges);
  return {
    D, tests, imports, weak, ids, G,
    root: typeof D.section === 'string' ? D.section.replace(/\/+$/, '') : '',
    file: new Map(files.map((f) => [f.id, f])),
    edge: new Map(edges.filter((e) => !e.test).map((e) => [pairKey(e.source, e.target), e])),
    evidence: memo(() => pairEvidence(D, tests)),
    symbols: memo(() => symbolTable(D)),
    components: memo(() => scc(G)),
  };
}

// One index per data object and mode; a reloaded data.json is a new object, so a refresh never reads a stale index.
export function indexOf(D, { tests = false, imports = false, weak = false } = {}) {
  let modes = cache.get(D);
  if (!modes) cache.set(D, (modes = {}));
  const key = `${tests ? 'tests' : 'production'}:${imports ? 'imports' : weak ? 'weak' : 'strong'}`;
  modes[key] ??= buildIndex(D, { tests, imports, weak });
  return modes[key];
}

export const edgeOf = (I, u, v) => I.edge.get(pairKey(u, v)) ?? null;
export const evidenceOf = (I, u, v) => I.evidence().get(pairKey(u, v)) ?? { imports: [], calls: [] };

// The first evidence line of u→v in u (an import line, else the earliest call site), for a row's location.
export function evidenceLine(I, u, v) {
  const ev = evidenceOf(I, u, v);
  const lines = [...ev.imports, ...ev.calls.flatMap((c) => c.lines)].filter(Number.isInteger);
  return lines.length ? Math.min(...lines) : 1;
}

function normalizePath(I, raw) {
  let p = raw.replace(/^\.\//, '');
  if (I.root && p.startsWith(`${I.root}/`)) p = p.slice(I.root.length + 1);
  if (I.file.has(p)) return p;
  return fileOf(p).replace(/(?::\d+){1,2}$/, '');
}

/** A file argument → its node id. Throws QueryError with every candidate when it is ambiguous or unknown. */
export function resolveFile(I, arg) {
  const raw = String(arg ?? '').trim();
  if (!raw) throw new QueryError('missing file argument');
  if (I.file.has(raw)) return raw;
  const p = normalizePath(I, raw);
  if (I.file.has(p)) return p;
  const hits = I.ids.filter((id) => id.endsWith(`/${p}`));
  if (hits.length === 1) return hits[0];
  if (hits.length > 1) throw new QueryError(`${raw} is ambiguous: ${hits.length} files end with it`, { candidates: hits, arg: raw });
  const lower = p.toLowerCase();
  const near = I.ids.filter((id) => id.toLowerCase().includes(lower));
  throw new QueryError(`no file is ${raw}${near.length ? `; ${near.length} contain it` : ''}`, { candidates: near, arg: raw });
}

function symbolsInFile(I, fileArg, name) {
  const file = resolveFile(I, fileArg);
  const { byId } = I.symbols();
  if (byId.has(`${file}#${name}`)) return [`${file}#${name}`];
  return [...byId.values()].filter((s) => s.file === file && (s.name === name || s.name.endsWith(`.${name}`))).map((s) => s.id);
}

/** A symbol argument (file#name with any file form, or a bare name) → its symbol id; ambiguity lists candidates. */
export function resolveSymbol(I, arg) {
  const raw = String(arg ?? '').trim();
  if (!raw) throw new QueryError('missing symbol argument');
  const { byId, byName } = I.symbols();
  if (byId.has(raw)) return raw;
  const at = raw.indexOf('#');
  const hits = at > 0 ? symbolsInFile(I, raw.slice(0, at), raw.slice(at + 1)) : byName.get(raw) ?? [];
  if (hits.length === 1) return hits[0];
  if (hits.length > 1) throw new QueryError(`${raw} is ambiguous: ${hits.length} symbols match`, { candidates: hits.toSorted(byText), arg: raw });
  const lower = raw.toLowerCase();
  const near = [...byId.keys()].filter((id) => id.toLowerCase().includes(lower)).toSorted(byText);
  throw new QueryError(`no symbol is ${raw}${near.length ? `; ${near.length} contain it` : ''}`, { candidates: near, arg: raw });
}
